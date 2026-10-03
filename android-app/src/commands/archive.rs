// ============================================================================
// commands/archive.rs —— 资源包类命令
//
// RSB / RSGP / RSBP 补丁 / PAK / DZip 五种容器格式的解包、打包、补丁。
// 全部基于 ed1ths-pvz-toolkit 的格式库：
//   * rsb-archive / rsb-patch / pak-archive / dzip（formats/ 内置拷贝）
// ============================================================================

use crate::params::{human_size, short_path, Params};
use anyhow::{anyhow, bail, Context, Result};
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

/// 读整个文件（带上下文报错）。
fn read_all(p: &Path) -> Result<Vec<u8>> {
    std::fs::read(p).with_context(|| format!("无法读取文件 {}", p.display()))
}

/// 输出目录：填了就用填的，没填就在输入文件旁边建一个。
fn out_dir_or_sibling(ps: &Params, key: &str, input: &Path, tag: &str) -> Result<PathBuf> {
    if let Some(d) = ps.path_opt(key) {
        std::fs::create_dir_all(&d)?;
        return Ok(d);
    }
    let base = input
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    let d = base.join(format!("{stem}_{tag}"));
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

/// 输出文件：填了就用填的，没填就按「输入名.新扩展名」生成。
fn out_file_or_sibling(ps: &Params, key: &str, input: &Path, ext: &str, tag: &str) -> Result<PathBuf> {
    if let Some(p) = ps.path_opt(key) {
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        return Ok(p);
    }
    let base = input
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    Ok(base.join(format!("{stem}_{tag}.{ext}")))
}

/// 写文件（带上下文报错）。
fn write_file(p: &Path, data: &[u8]) -> Result<()> {
    std::fs::write(p, data).with_context(|| format!("无法写入 {}", p.display()))
}

/// 安全落盘相对路径（拒绝 .. / 绝对路径逃逸）。
fn safe_join(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel = rel.replace('\\', "/");
    let p = Path::new(&rel);
    if p.is_absolute() {
        bail!("包内出现绝对路径，拒绝解包：{rel}");
    }
    let mut out = root.to_path_buf();
    for comp in p.components() {
        match comp {
            std::path::Component::Normal(c) => out.push(c),
            std::path::Component::CurDir => {}
            _ => bail!("包内出现可疑路径，拒绝解包：{rel}"),
        }
    }
    Ok(out)
}

// ============================================================================
// RSB 回包（rsb.pack）
// ============================================================================

/// PtxFormat 的稳定显示名（textures.json 与 ptx.decode/encode 共用）。
pub fn ptx_format_name(f: rsb_archive::PtxFormat) -> String {
    use rsb_archive::PtxFormat;
    match f {
        PtxFormat::Rgba8888 => "Rgba8888".into(),
        PtxFormat::Rgba4444 => "Rgba4444".into(),
        PtxFormat::Rgb565 => "Rgb565".into(),
        PtxFormat::Rgba5551 => "Rgba5551".into(),
        PtxFormat::Rgba4444Block => "Rgba4444Block".into(),
        PtxFormat::Rgb565Block => "Rgb565Block".into(),
        PtxFormat::Rgba5551Block => "Rgba5551Block".into(),
        PtxFormat::Pvrtc4BppRgba => "Pvrtc4BppRgba".into(),
        PtxFormat::Etc1 => "Etc1".into(),
        PtxFormat::Pvrtc4BppRgbaA8 => "Pvrtc4BppRgbaA8".into(),
        PtxFormat::Etc1A8 => "Etc1A8".into(),
        PtxFormat::Etc1CompressedAlpha => "Etc1CompressedAlpha".into(),
        PtxFormat::Etc1Palette => "Etc1Palette".into(),
        PtxFormat::Astc {
            block_width,
            block_height,
        } => format!("Astc{block_width}x{block_height}"),
        PtxFormat::A8 => "A8".into(),
        PtxFormat::L8 => "L8".into(),
        PtxFormat::La88 => "La88".into(),
        PtxFormat::Al88 => "Al88".into(),
        PtxFormat::La44 => "La44".into(),
        PtxFormat::Al44 => "Al44".into(),
        PtxFormat::Rgb332 => "Rgb332".into(),
        PtxFormat::Rgb888 => "Rgb888".into(),
        PtxFormat::Argb8888 => "Argb8888".into(),
        PtxFormat::Argb4444 => "Argb4444".into(),
        PtxFormat::Argb1555 => "Argb1555".into(),
        PtxFormat::Unknown(code) => format!("Unknown({code})"),
    }
}

/// 名称 → PopCap 数字格式码（ptx_format_name 的逆映射，Unknown 除外）。
fn ptx_format_code_from_name(name: &str) -> Option<i32> {
    Some(match name.to_ascii_lowercase().as_str() {
        "rgba8888" => 0,
        "rgba4444" => 1,
        "rgb565" => 2,
        "rgba5551" => 3,
        "rgba4444block" => 21,
        "rgb565block" => 22,
        "rgba5551block" => 23,
        "pvrtc4bpprgba" => 30,
        "etc1" => 147,
        "pvrtc4bpprgbaa8" => 148,
        "astc4x4" => 160,
        "astc5x5" => 161,
        "astc6x6" => 162,
        "astc8x8" => 163,
        "a8" => 200,
        "l8" => 201,
        "la88" => 202,
        "al88" => 203,
        "la44" => 204,
        "al44" => 205,
        "rgb332" => 206,
        "rgb888" => 207,
        "argb8888" => 208,
        "argb4444" => 209,
        "argb1555" => 210,
        _ => return None,
    })
}

/// 解包时输出的贴图元数据清单（textures.json 的单条记录）。
/// 用户做独立 PTX 互转或回包时，可在这里直接抄到宽高与格式参数。
#[derive(serde::Serialize)]
struct PtxMetaRecord {
    /// 包内 .ptx 相对路径
    ptx: String,
    /// 转出的 PNG 相对路径（解码失败/未转换时为 null）
    png: Option<String>,
    width: u32,
    height: u32,
    /// PopCap 数字格式码（回填参数用）
    format_code: i32,
    /// 解析后的格式名（带调色板/Alpha 消歧）
    format: String,
    alpha_size: Option<i32>,
    alpha_format: Option<i32>,
    pitch: Option<u32>,
}

/// 用 PTX 解码器原路解码一张 PNG 对比内容是否改动。
/// 相同则直接复用原始 PTX 字节 —— 既省一次有损 ETC1/PVRTC 重编码，
/// 也避免「解包后没改直接回包」时画质二次劣化。
/// 返回 (最终 PTX 字节, 是否真的改动)。
fn encode_ptx_if_changed(
    original_ptx: &[u8],
    png_path: &Path,
    descriptor: rsb_archive::PtxDescriptor,
) -> Result<(Vec<u8>, bool)> {
    use rsb_archive::{AstcQuality, PtxDecoder, PtxEncodeOptions, PtxEncoder};

    // 新图：用户改过的 PNG
    let new_img = image::open(png_path)
        .with_context(|| format!("无法读取贴图 {}", png_path.display()))?
        .to_rgba8();
    if new_img.width() != descriptor.width || new_img.height() != descriptor.height {
        bail!(
            "贴图尺寸必须保持 {}×{}（当前 PNG 是 {}×{}）——回包暂不支持改变贴图尺寸",
            descriptor.width,
            descriptor.height,
            new_img.width(),
            new_img.height()
        );
    }

    // 原 PTX 解码出来与 PNG 全等 → 没改过，复用原字节（免有损重编码）
    if let Ok(orig_img) = PtxDecoder::decode_with_descriptor(original_ptx, descriptor) {
        if orig_img.as_raw() == new_img.as_raw() {
            return Ok((original_ptx.to_vec(), false));
        }
    }

    let options = PtxEncodeOptions {
        format: descriptor.format,
        row_pitch: descriptor.row_pitch,
        channel_order: descriptor.channel_order,
        astc_quality: AstcQuality::MEDIUM,
    };
    let data = PtxEncoder::encode_image(&new_img, options)
        .map_err(|e| anyhow!("贴图 {} 编码回 PTX 失败：{e}", png_path.display()))?;
    Ok((data, true))
}

/// PNG 快速编码：image 的 `save()` 走默认压缩档（zlib level 高），
/// 批量转图时每张图几十到几百毫秒，是解包慢的大头之一。
/// 这里固定 Fast 压缩 + 自适应过滤 —— 单图快 3~5 倍，体积只大几个百分点。
/// 入参直接收 RGBA8 缓冲（PTX 解码器的输出本就是 RgbaImage，免二次转换）。
fn save_png_fast(rgba: &image::RgbaImage, path: &Path) -> Result<()> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::{ExtendedColorType, ImageEncoder};

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let f = std::fs::File::create(path)
        .with_context(|| format!("无法创建 {}", path.display()))?;
    let enc = PngEncoder::new_with_quality(f, CompressionType::Fast, FilterType::Adaptive);
    enc.write_image(rgba.as_raw(), rgba.width(), rgba.height(), ExtendedColorType::Rgba8)
        .with_context(|| format!("写 PNG 失败：{}", path.display()))
}

/// 并行落盘前的「待写文件」。
///
/// ★ 两段式设计：extract_packet / unpack_rsg 需要 `&mut Rsb`（串行），
/// 而写盘 + PTX 解码 + PNG 编码是 CPU 大头 —— 先串行把数据切出来，
/// 再交 rayon 按文件粒度吃满多核。PngJob 在收集期就把描述符参数
/// 算好，并行闭包里不做任何需要外部上下文的判断。
struct PendingFile {
    data: Vec<u8>,
    /// Some = 尝试解码成 PNG（失败自动回退写原始 .ptx）
    png: Option<PngJob>,
    /// 原样落盘的完整路径（png 走 fallback 时也用它）
    target: PathBuf,
}

struct PngJob {
    png_path: PathBuf,
    /// PNG 相对路径（textures.json 记录用）
    png_rel: String,
    descriptor: rsb_archive::PtxDescriptor,
}

/// 每个并行单元的结果，用于末尾统一汇总日志（并行时绝不逐条打日志 ——
/// 打日志走 UI 投递，比文件 IO 还贵）。
enum WriteOutcome {
    Written,
    PngConverted,
    PngFailed,
}

/// 解包一个 RSB：逐个 RSG packet → unpack_rsg → 并行落盘。
/// 返回 (写出文件数, description.json 文本, 贴图元数据清单)。
struct RsbUnpackResult {
    files: usize,
    desc_json: Option<String>,
    ptx_meta: Vec<PtxMetaRecord>,
}

fn unpack_rsb_to(
    input: &Path,
    out_root: &Path,
    ptx_png: bool,
    only_ptx: bool,
) -> Result<RsbUnpackResult> {
    use rayon::prelude::*;
    use rsb_archive::{unpack_rsg, PtxDecoder, PtxDescriptor, PtxRsbMetadata, Rsb, ChannelOrder};
    use std::io::Cursor;

    let data = read_all(input)?;
    let mut rsb = Rsb::open(Cursor::new(&data))
        .with_context(|| format!("{} 不是有效的 RSB v4 资源包", input.display()))?;

    let packets = rsb.read_rsg_info()?;
    if packets.is_empty() {
        bail!("包内没有 RSG packet（可能是空包或不支持的变体）");
    }

    // part2 的贴图信息表：packet 内第 i 张 PTX → 全局表 ptx_before_number + i
    let ptx_infos = rsb.read_ptx_info()?;

    // description.json（顺手在这一次打开里读掉 —— 旧版会整个重读一遍包）
    let desc_json: Option<String> = rsb
        .read_resources_description("")
        .ok()
        .and_then(|desc| serde_json::to_string_pretty(&desc).ok());

    // ---- 阶段 1（串行）：切出所有文件，算好每份的落盘计划 ----
    let mut pending: Vec<PendingFile> = Vec::new();
    let mut ptx_meta: Vec<PtxMetaRecord> = Vec::new();

    for info in &packets {
        let packet_bytes = rsb
            .extract_packet(info)
            .with_context(|| format!("提取分包 {} 失败", info.name))?;
        let mut cursor = Cursor::new(&packet_bytes);
        let unpacked = unpack_rsg(&mut cursor)
            .with_context(|| format!("解包分包 {} 失败", info.name))?;

        // 该 packet 在贴图信息表中的窗口
        let ptx_begin = usize::try_from(info.ptx_before_number).unwrap_or(usize::MAX);
        let ptx_end = if info.ptx_number > 0 {
            ptx_begin.saturating_add(info.ptx_number as usize)
        } else {
            ptx_begin
        };
        let mut ptx_ordinal = 0usize;

        for f in unpacked {
            let rel = f.path.replace('\\', "/");
            let is_ptx = rel.to_ascii_lowercase().ends_with(".ptx");

            if only_ptx && !is_ptx {
                continue;
            }

            let target = safe_join(out_root, &rel)?;

            // PTX → PNG 计划（带全局贴图表 + 允许时）；解码失败回退原始 .ptx。
            // from_rsb_payload 会用数据长度校验候选布局，30/147 系格式自动消歧。
            let png = if is_ptx && ptx_png {
                let gi = ptx_begin.saturating_add(ptx_ordinal);
                ptx_infos.get(gi).filter(|_| gi < ptx_end || info.ptx_number == 0).and_then(|pi| {
                    let meta = PtxRsbMetadata {
                        format_code: rsb_archive::PtxFormatCode::new(pi.format),
                        alpha_size: pi.alpha_size.and_then(|v| u32::try_from(v).ok()),
                        alpha_format: pi.alpha_format,
                        row_pitch: u32::try_from(pi.pitch).ok().filter(|p| *p > 0),
                        channel_order: ChannelOrder::Rgba,
                    };
                    PtxDescriptor::from_rsb_payload(pi.width as u32, pi.height as u32, meta, &f.data)
                        .ok()
                        .map(|descriptor| PngJob {
                            png_path: target.with_extension("png"),
                            png_rel: Path::new(&rel)
                                .with_extension("png")
                                .to_string_lossy()
                                .into_owned(),
                            descriptor,
                        })
                })
            } else {
                None
            };

            if is_ptx {
                // textures.json 记录：无论是否转 PNG 都抄下原始参数，
                // 供 ptx.decode / ptx.encode / 回包时照抄
                let gi = ptx_begin.saturating_add(ptx_ordinal);
                let record = ptx_infos
                    .get(gi)
                    .filter(|_| gi < ptx_end || info.ptx_number == 0)
                    .map(|pi| {
                        let meta = PtxRsbMetadata {
                            format_code: rsb_archive::PtxFormatCode::new(pi.format),
                            alpha_size: pi.alpha_size.and_then(|v| u32::try_from(v).ok()),
                            alpha_format: pi.alpha_format,
                            row_pitch: u32::try_from(pi.pitch).ok().filter(|p| *p > 0),
                            channel_order: ChannelOrder::Rgba,
                        };
                        let format = PtxDescriptor::from_rsb_payload(
                            pi.width as u32,
                            pi.height as u32,
                            meta,
                            &f.data,
                        )
                        .map(|d| ptx_format_name(d.format))
                        .unwrap_or_else(|_| format!("Unknown({})", pi.format));
                        PtxMetaRecord {
                            ptx: rel.clone(),
                            png: png.as_ref().map(|j| j.png_rel.clone()),
                            width: pi.width.max(0) as u32,
                            height: pi.height.max(0) as u32,
                            format_code: pi.format,
                            format,
                            alpha_size: pi.alpha_size,
                            alpha_format: pi.alpha_format,
                            pitch: u32::try_from(pi.pitch).ok().filter(|p| *p > 0),
                        }
                    })
                    .unwrap_or_else(|| PtxMetaRecord {
                        ptx: rel.clone(),
                        png: None,
                        width: 0,
                        height: 0,
                        format_code: 0,
                        format: "Unknown".into(),
                        alpha_size: None,
                        alpha_format: None,
                        pitch: None,
                    });
                ptx_meta.push(record);
                ptx_ordinal += 1;
            }

            pending.push(PendingFile { data: f.data, png, target });
        }
    }

    // ---- 阶段 2（并行）：写盘 + 解码 + PNG 编码，按文件粒度吃满核 ----
    let outcomes: Vec<WriteOutcome> = pending
        .par_iter()
        .map(|pf| -> Result<WriteOutcome> {
            match &pf.png {
                Some(job) => {
                    match PtxDecoder::decode_with_descriptor(&pf.data, job.descriptor) {
                        Ok(img) => {
                            save_png_fast(&img, &job.png_path)?;
                            Ok(WriteOutcome::PngConverted)
                        }
                        Err(_) => {
                            // 解码失败就保留原始 .ptx，别把数据弄丢
                            if let Some(parent) = pf.target.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            write_file(&pf.target, &pf.data)?;
                            Ok(WriteOutcome::PngFailed)
                        }
                    }
                }
                None => {
                    if let Some(parent) = pf.target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    write_file(&pf.target, &pf.data)?;
                    Ok(WriteOutcome::Written)
                }
            }
        })
        .collect::<Result<Vec<_>>>()?;

    // 每个并行单元都成功写出了一份文件；PNG 转换/失败只是细分统计
    let files = outcomes.len();
    let ptx_converted = outcomes
        .iter()
        .filter(|o| matches!(o, WriteOutcome::PngConverted))
        .count();
    let ptx_failed = outcomes
        .iter()
        .filter(|o| matches!(o, WriteOutcome::PngFailed))
        .count();

    if ptx_png && ptx_failed > 0 {
        crate::log_line!("提示：{ptx_failed} 张贴图解码失败，已保留原始 .ptx 数据。");
    }
    if ptx_png {
        crate::log_line!("贴图：{ptx_converted} 张已转为 PNG。");
    }

    Ok(RsbUnpackResult { files, desc_json, ptx_meta })
}

/// 写 textures.json（解包输出的贴图参数清单，独立互转/回包时照抄用）。
fn write_ptx_meta(out_root: &Path, meta: &[PtxMetaRecord]) {
    if meta.is_empty() {
        return;
    }
    if let Ok(json) = serde_json::to_string_pretty(meta) {
        let _ = write_file(&out_root.join("textures.json"), json.as_bytes());
    }
}

pub fn rsb_unpack(ps: &Params) -> Result<String> {
    let input = ps.file_in("input", "RSB 文件")?;
    let out_root = out_dir_or_sibling(ps, "output", &input, "unpacked")?;
    let ptx_png = ps.bool("ptx-png");

    crate::log_line!("资源包：{}", short_path(&input));
    let r = unpack_rsb_to(&input, &out_root, ptx_png, false)?;

    // description.json —— 包内资源分组描述，方便后续工具用
    if let Some(json) = r.desc_json {
        let path = out_root.join("description.json");
        let _ = write_file(&path, json.as_bytes());
    }
    write_ptx_meta(&out_root, &r.ptx_meta);

    Ok(format!(
        "已解包 {} 个文件 → {}",
        r.files,
        short_path(&out_root)
    ))
}

pub fn rsb_ptx_export(ps: &Params) -> Result<String> {
    let input = ps.file_in("input", "RSB 文件")?;
    let out_root = out_dir_or_sibling(ps, "output", &input, "textures")?;

    crate::log_line!("资源包：{}", short_path(&input));
    let r = unpack_rsb_to(&input, &out_root, true, true)?;
    write_ptx_meta(&out_root, &r.ptx_meta);

    Ok(format!(
        "已导出贴图相关文件 {} 个 → {}",
        r.files,
        short_path(&out_root)
    ))
}

// ============================================================================
// RSB 回包 / PTX 独立互转
// ============================================================================

/// 回包时一个文件的「新内容从哪来」。
enum ReloadSource {
    /// 普通文件或没转 PNG 的 .ptx：直接读磁盘
    Disk(PathBuf),
    /// 贴图有同名 PNG：内容一致复用原字节，改动过才重编码
    PtxFromPng(PathBuf, rsb_archive::PtxDescriptor),
}

struct ReloadJob {
    rel: String,
    is_part1: bool,
    part1_extra: Option<rsb_archive::Part1Extra>,
    original: Vec<u8>,
    source: ReloadSource,
}

struct PacketPlan {
    packet_index: usize,
    name: String,
    version: u32,
    flags: u32,
    original_paths: Vec<String>,
    jobs: Vec<ReloadJob>,
}

/// 执行单个替换计划，返回 (最终数据, 是否有改动)。
fn run_reload_job(job: &ReloadJob) -> Result<(Vec<u8>, bool)> {
    match &job.source {
        ReloadSource::Disk(path) => {
            let data = read_all(path)
                .with_context(|| format!("解包目录里缺少文件 {}", path.display()))?;
            let changed = data != job.original;
            Ok((data, changed))
        }
        ReloadSource::PtxFromPng(png, descriptor) => {
            encode_ptx_if_changed(&job.original, png, *descriptor)
        }
    }
}

/// RSB 回包：原包提供结构基准（分包表/贴图参数表/资源索引），
/// 解包目录提供新内容。没动的分包按原字节拷贝，改动的分包重新打包。
pub fn rsb_pack(ps: &Params) -> Result<String> {
    use rayon::prelude::*;
    use rsb_archive::{unpack_rsg, Rsb, RsbArchiveEdit, RsgHeader, RsgPacketEdit, UnpackedFile};
    use std::io::Cursor;

    let input = ps.file_in("input", "原版 RSB")?;
    let dir = ps.dir_in("dir", "修改后的解包目录")?;
    let output = out_file_or_sibling(ps, "output", &input, "rsb", "packed")?;

    crate::log_line!("原包：{}", short_path(&input));
    let data = read_all(&input)?;
    let mut rsb = Rsb::open(Cursor::new(&data))
        .with_context(|| format!("{} 不是有效的 RSB 资源包", input.display()))?;
    let packets = rsb.read_rsg_info()?;
    if packets.is_empty() {
        bail!("包内没有 RSG packet");
    }
    let ptx_infos = rsb.read_ptx_info()?;

    // ---- 阶段 1（串行）：逐 packet 切出文件，确定磁盘替换计划 ----
    let mut plans: Vec<PacketPlan> = Vec::new();
    for (packet_index, info) in packets.iter().enumerate() {
        let packet_bytes = rsb
            .extract_packet(info)
            .with_context(|| format!("提取分包 {} 失败", info.name))?;
        let header = RsgHeader::read_from(&mut Cursor::new(&packet_bytes))
            .with_context(|| format!("分包 {} 头部损坏", info.name))?;
        let unpacked = unpack_rsg(&mut Cursor::new(&packet_bytes))
            .with_context(|| format!("解包分包 {} 失败", info.name))?;

        // 该 packet 的贴图在全局表中的窗口：ptx_before_number + 出现序号
        let ptx_begin = usize::try_from(info.ptx_before_number).unwrap_or(usize::MAX);
        let ptx_end = if info.ptx_number > 0 {
            ptx_begin.saturating_add(info.ptx_number as usize)
        } else {
            ptx_begin
        };
        let mut ptx_ordinal = 0usize;

        let mut jobs: Vec<ReloadJob> = Vec::with_capacity(unpacked.len());
        let mut original_paths = Vec::with_capacity(unpacked.len());

        for f in unpacked {
            let rel = f.path.replace('\\', "/");
            original_paths.push(rel.clone());
            let is_ptx = rel.to_ascii_lowercase().ends_with(".ptx");

            let source = if is_ptx {
                let gi = ptx_begin.saturating_add(ptx_ordinal);
                ptx_ordinal += 1;
                let disk = safe_join(&dir, &rel)?;
                ptx_infos
                    .get(gi)
                    .filter(|_| gi < ptx_end || info.ptx_number == 0)
                    .and_then(|pi| {
                        let meta = rsb_archive::PtxRsbMetadata {
                            format_code: rsb_archive::PtxFormatCode::new(pi.format),
                            alpha_size: pi.alpha_size.and_then(|v| u32::try_from(v).ok()),
                            alpha_format: pi.alpha_format,
                            row_pitch: u32::try_from(pi.pitch).ok().filter(|p| *p > 0),
                            channel_order: rsb_archive::ChannelOrder::Rgba,
                        };
                        rsb_archive::PtxDescriptor::from_rsb_payload(
                            pi.width as u32,
                            pi.height as u32,
                            meta,
                            &f.data,
                        )
                        .ok()
                        .map(|descriptor| (disk.with_extension("png"), descriptor))
                    })
                    .filter(|(png, _)| png.exists())
                    .map(|(png, descriptor)| ReloadSource::PtxFromPng(png, descriptor))
                    .unwrap_or(ReloadSource::Disk(disk))
            } else {
                ReloadSource::Disk(safe_join(&dir, &rel)?)
            };

            jobs.push(ReloadJob {
                rel,
                is_part1: f.is_part1,
                part1_extra: f.part1_info,
                original: f.data,
                source,
            });
        }

        plans.push(PacketPlan {
            packet_index,
            name: info.name.clone(),
            version: header.version,
            flags: header.flags,
            original_paths,
            jobs,
        });
    }

    // ---- 阶段 2（并行）：读磁盘 / PNG→PTX 内容比对与重编码 ----
    let flat: Vec<&ReloadJob> = plans.iter().flat_map(|p| p.jobs.iter()).collect();
    let results: Vec<Result<(Vec<u8>, bool)>> =
        flat.par_iter().map(|job| run_reload_job(job)).collect();

    // ---- 阶段 3：组装编辑表，只有真正改动的分包才重新打包 ----
    let mut edits: Vec<RsgPacketEdit> = Vec::new();
    let mut changed_files = 0usize;
    let mut consumed = 0usize;
    let total_files = flat.len();
    for plan in &plans {
        let n = plan.jobs.len();
        let slice = &results[consumed..consumed + n];
        consumed += n;

        let mut files: Vec<UnpackedFile> = Vec::with_capacity(n);
        let mut packet_changed = false;
        for (job, result) in plan.jobs.iter().zip(slice) {
            let (data, changed) = match result {
                Ok(v) => v.clone(),
                Err(e) => bail!("分包 {} 的文件 {}：{e}", plan.name, job.rel),
            };
            if changed {
                packet_changed = true;
                changed_files += 1;
            }
            files.push(UnpackedFile {
                path: job.rel.clone(),
                data,
                is_part1: job.is_part1,
                part1_info: job.part1_extra.clone(),
            });
        }
        if packet_changed {
            edits.push(RsgPacketEdit {
                packet_index: plan.packet_index,
                original_paths: plan.original_paths.clone(),
                name: plan.name.clone(),
                version: plan.version,
                compression_flags: plan.flags,
                files,
            });
        }
    }

    if edits.is_empty() {
        bail!("解包目录与原包内容完全一致，没有需要回包的改动");
    }
    crate::log_line!(
        "检测到 {changed_files}/{total_files} 个文件有改动，重建 {} 个分包…",
        edits.len()
    );

    let edit = RsbArchiveEdit {
        packets: edits,
        ..Default::default()
    };
    let out = rsb_archive::rebuild_rsb(&data, &edit).map_err(|e| anyhow!("回包失败：{e}"))?;
    write_file(&output, &out)?;

    Ok(format!(
        "已回包 {changed_files} 个改动文件 → {}（{}）",
        short_path(&output),
        human_size(out.len() as u64)
    ))
}

/// 独立 PTX → PNG。
pub fn ptx_decode(ps: &Params) -> Result<String> {
    use rsb_archive::{ChannelOrder, PtxDecoder, PtxDescriptor, PtxFormatCode, PtxRsbMetadata};

    let input = ps.file_in("input", "PTX 文件")?;
    let width = ps.u32_or("width", "宽度", 0)?;
    let height = ps.u32_or("height", "高度", 0)?;
    if width == 0 || height == 0 {
        bail!("宽度和高度必须填写（PTX 没有文件头，无法自动推断）");
    }
    let format_raw = ps.raw("format").unwrap_or("").trim().to_string();
    if format_raw.is_empty() {
        bail!("必须填写格式码或格式名（如 0 / 147 / Etc1A8）");
    }

    let order = match ps.str_or("order", "Rgba") {
        "Bgra" => ChannelOrder::Bgra,
        _ => ChannelOrder::Rgba,
    };
    let row_pitch = ps.u32_opt("pitch", "行距")?;

    let data = read_all(&input)?;
    // 两条解析路径：数字码走 RSB 元数据链（30/147 系用数据长度自动消歧）；
    // 名称直接给格式（调色板/Alpha 变体名称里已写明）
    let descriptor = match format_raw.parse::<i32>() {
        Ok(code) => {
            let meta = PtxRsbMetadata {
                format_code: PtxFormatCode::new(code),
                alpha_size: ps.i32_opt("alpha-size", "Alpha 大小")?.filter(|v| *v > 0).map(|v| v as u32),
                alpha_format: ps.i32_opt("alpha-format", "Alpha 格式")?,
                row_pitch,
                channel_order: order,
            };
            PtxDescriptor::from_rsb_payload(width, height, meta, &data).map_err(|e| {
                anyhow!(
                    "参数与数据不匹配：{e}\n提示：核对宽高、格式码与文件大小——\
                     解包输出目录里的 textures.json 有每张贴图的准确参数"
                )
            })?
        }
        Err(_) => {
            let format = parse_ptx_format_name(&format_raw).ok_or_else(|| {
                anyhow!(
                    "无法识别格式「{format_raw}」：可填数字码（0=Rgba8888 2=Rgb565 \
                     30=Pvrtc4 147=Etc1 160=Astc4x4）或格式名（如 Etc1A8）"
                )
            })?;
            PtxDescriptor::new(width, height, format)
                .map_err(|e| anyhow!("参数错误：{e}"))?
                .with_row_pitch(row_pitch)
                .with_channel_order(order)
        }
    };

    let img = PtxDecoder::decode_with_descriptor(&data, descriptor)
        .map_err(|e| anyhow!("解码失败：{e}"))?;
    let output = out_file_or_sibling(ps, "output", &input, "png", "decoded")?;
    save_png_fast(&img, &output)?;

    Ok(format!(
        "已解码 {}×{} {} → {}",
        width,
        height,
        ptx_format_name(descriptor.format),
        short_path(&output)
    ))
}

/// 格式名 → PtxFormat（ptx.decode / ptx.encode 共用，ptx_format_name 的逆映射）。
fn parse_ptx_format_name(name: &str) -> Option<rsb_archive::PtxFormat> {
    use rsb_archive::PtxFormat;
    Some(match name.to_ascii_lowercase().as_str() {
        "rgba8888" => PtxFormat::Rgba8888,
        "rgba4444" => PtxFormat::Rgba4444,
        "rgb565" => PtxFormat::Rgb565,
        "rgba5551" => PtxFormat::Rgba5551,
        "rgba4444block" => PtxFormat::Rgba4444Block,
        "rgb565block" => PtxFormat::Rgb565Block,
        "rgba5551block" => PtxFormat::Rgba5551Block,
        "pvrtc4bpprgba" => PtxFormat::Pvrtc4BppRgba,
        "etc1" => PtxFormat::Etc1,
        "pvrtc4bpprgbaa8" => PtxFormat::Pvrtc4BppRgbaA8,
        "etc1a8" => PtxFormat::Etc1A8,
        "etc1compressedalpha" => PtxFormat::Etc1CompressedAlpha,
        "etc1palette" => PtxFormat::Etc1Palette,
        "astc4x4" => PtxFormat::Astc {
            block_width: 4,
            block_height: 4,
        },
        "astc5x5" => PtxFormat::Astc {
            block_width: 5,
            block_height: 5,
        },
        "astc6x6" => PtxFormat::Astc {
            block_width: 6,
            block_height: 6,
        },
        "astc8x8" => PtxFormat::Astc {
            block_width: 8,
            block_height: 8,
        },
        "a8" => PtxFormat::A8,
        "l8" => PtxFormat::L8,
        "la88" => PtxFormat::La88,
        "al88" => PtxFormat::Al88,
        "la44" => PtxFormat::La44,
        "al44" => PtxFormat::Al44,
        "rgb332" => PtxFormat::Rgb332,
        "rgb888" => PtxFormat::Rgb888,
        "argb8888" => PtxFormat::Argb8888,
        "argb4444" => PtxFormat::Argb4444,
        "argb1555" => PtxFormat::Argb1555,
        _ => return None,
    })
}

/// 独立 PNG → PTX。
pub fn ptx_encode(ps: &Params) -> Result<String> {
    use rsb_archive::{AstcQuality, ChannelOrder, PtxEncodeOptions, PtxEncoder};

    let input = ps.file_in("input", "PNG 文件")?;
    let format_name = ps.choice("format", "目标格式", &[
        "Rgba8888", "Rgba4444", "Rgb565", "Rgba5551",
        "Etc1", "Etc1A8", "Etc1CompressedAlpha", "Etc1Palette",
        "Pvrtc4BppRgba", "Pvrtc4BppRgbaA8",
        "Astc4x4", "Astc5x5", "Astc6x6", "Astc8x8",
    ], "Rgba8888")?;
    let format =
        parse_ptx_format_name(format_name).ok_or_else(|| anyhow!("暂不支持编码为 {format_name}"))?;
    let order = match ps.str_or("order", "Rgba") {
        "Bgra" => ChannelOrder::Bgra,
        _ => ChannelOrder::Rgba,
    };
    let options = PtxEncodeOptions {
        format,
        row_pitch: ps.u32_opt("pitch", "行距")?,
        channel_order: order,
        astc_quality: AstcQuality::MEDIUM,
    };

    let img = image::open(&input)
        .with_context(|| format!("无法读取图片 {}", input.display()))?
        .to_rgba8();
    let data = PtxEncoder::encode_image(&img, options).map_err(|e| anyhow!("编码失败：{e}"))?;
    let output = out_file_or_sibling(ps, "output", &input, "ptx", "encoded")?;
    write_file(&output, &data)?;

    Ok(format!(
        "已编码 {}×{} {} → {}（{}）",
        img.width(),
        img.height(),
        format_name,
        short_path(&output),
        human_size(data.len() as u64)
    ))
}

// ============================================================================
// RSGP（独立分包）
// ============================================================================

pub fn rsgp_unpack(ps: &Params) -> Result<String> {
    use rsb_archive::unpack_rsg;
    use std::io::Cursor;

    let input = ps.file_in("input", "RSGP 文件")?;
    let out_root = out_dir_or_sibling(ps, "output", &input, "unpacked")?;

    let data = read_all(&input)?;
    let mut cursor = Cursor::new(&data);
    let unpacked = unpack_rsg(&mut cursor)
        .with_context(|| format!("{} 不是有效的 RSG 分包", input.display()))?;

    // 文件粒度并行写盘（大分包里可能有几千个小文件）
    use rayon::prelude::*;
    unpacked
        .par_iter()
        .map(|f| {
            let target = safe_join(&out_root, &f.path)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            write_file(&target, &f.data)?;
            Ok(())
        })
        .collect::<Result<()>>()?;
    let files = unpacked.len();

    // manifest.json：记录 part1 贴图的 id/宽高 —— rsgp.pack 回包时按它重建分包结构
    #[derive(serde::Serialize)]
    struct ManifestEntry<'a> {
        path: &'a str,
        is_part1: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        width: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        height: Option<u32>,
    }
    let manifest: Vec<ManifestEntry> = unpacked
        .iter()
        .map(|f| ManifestEntry {
            path: &f.path,
            is_part1: f.is_part1,
            id: f.part1_info.as_ref().map(|e| e.id),
            width: f.part1_info.as_ref().map(|e| e.width),
            height: f.part1_info.as_ref().map(|e| e.height),
        })
        .collect();
    if let Ok(json) = serde_json::to_string_pretty(&manifest) {
        let _ = write_file(&out_root.join("manifest.json"), json.as_bytes());
    }

    Ok(format!(
        "已解包 {} 个文件 → {}",
        files,
        short_path(&out_root)
    ))
}

pub fn rsgp_pack(ps: &Params) -> Result<String> {
    use rsb_archive::{pack_rsg, Part1Extra, UnpackedFile};
    use std::io::Cursor;

    let input = ps.dir_in("input", "输入目录")?;
    let version = ps.u32_or("version", "RSG 版本", 4)?;
    if version != 3 && version != 4 {
        bail!("RSG 版本只支持 3 或 4");
    }
    let flags = ps.u32_or("flags", "压缩方式", 3)?;
    if flags > 3 {
        bail!("压缩方式只支持 0~3");
    }
    let output = out_file_or_sibling(ps, "output", &input, "rsgp", "packed")?;

    // manifest.json（rsgp.unpack 输出）：part1 贴图的 id/宽高基准
    let manifest_path = input.join("manifest.json");
    #[derive(serde::Deserialize)]
    struct ManifestEntry {
        path: String,
        #[serde(default)]
        is_part1: bool,
        #[serde(default)]
        id: Option<u32>,
        #[serde(default)]
        width: Option<u32>,
        #[serde(default)]
        height: Option<u32>,
    }
    let manifest: Option<Vec<ManifestEntry>> = if manifest_path.exists() {
        let bytes = read_all(&manifest_path)?;
        Some(serde_json::from_slice(&bytes)
            .with_context(|| "manifest.json 解析失败：请使用 rsgp.unpack 的原始输出")?)
    } else {
        None
    };

    let mut paths: Vec<PathBuf> = Vec::new();
    for p in walkdir::WalkDir::new(&input).sort_by_file_name() {
        let p = p?;
        if p.file_type().is_file() && p.path() != manifest_path {
            paths.push(p.path().to_path_buf());
        }
    }
    if paths.is_empty() {
        bail!("目录里没有文件：{}", input.display());
    }

    // 并行读文件 + 组装条目（part1 信息优先取 manifest，缺 manifest 的 .ptx 直接报错）
    use rayon::prelude::*;
    let files: Vec<UnpackedFile> = paths
        .par_iter()
        .map(|p| -> Result<UnpackedFile> {
            let rel = p
                .strip_prefix(&input)
                .map_err(|_| anyhow!("内部路径错误"))?
                .to_string_lossy()
                .replace('\\', "/");
            let data = read_all(p)?;
            let m = manifest.as_ref().and_then(|ms| {
                ms.iter()
                    .find(|e| e.path.replace('\\', "/") == rel)
            });
            let is_part1 = m.map(|e| e.is_part1).unwrap_or(false);
            let part1_info = if is_part1 {
                match (m.and_then(|e| e.id), m.and_then(|e| e.width), m.and_then(|e| e.height)) {
                    (Some(id), Some(width), Some(height)) => Some(Part1Extra { id, width, height }),
                    _ => bail!(
                        "贴图 {rel} 在 manifest.json 里缺少 id/width/height，无法打包；\
                         请用 rsgp.unpack 的输出目录回包，或手动补全 manifest 条目"
                    ),
                }
            } else {
                None
            };
            Ok(UnpackedFile { path: rel, data, is_part1, part1_info })
        })
        .collect::<Result<Vec<_>>>()?;

    let part1_count = files.iter().filter(|f| f.is_part1).count();
    let mut cursor = Cursor::new(Vec::new());
    pack_rsg(&mut cursor, &files, version, flags).map_err(|e| anyhow!("打包失败：{e}"))?;
    let bytes = cursor.into_inner();
    write_file(&output, &bytes)?;

    Ok(format!(
        "已打包 {} 个文件（含 {part1_count} 张贴图）→ {}（{}）",
        files.len(),
        short_path(&output),
        human_size(bytes.len() as u64)
    ))
}

// ============================================================================
// RSBP 补丁
// ============================================================================

pub fn rsbp_create(ps: &Params) -> Result<String> {
    use rsb_patch::{ArchiveEncodeOptions, PacketPatchMode, RsbPatch};

    let before = ps.file_in("before", "旧版 RSB")?;
    let after = ps.file_in("after", "新版 RSB")?;
    let output = out_file_or_sibling(ps, "output", &after, "rsbp", "patch")?;
    let mode = match ps.str_or("mode", "stored") {
        "raw" => PacketPatchMode::Raw,
        _ => PacketPatchMode::Stored,
    };

    let before_bytes = read_all(&before)?;
    let after_bytes = read_all(&after)?;
    crate::log_line!(
        "对比 {}（{}）→ {}（{}）",
        short_path(&before),
        human_size(before_bytes.len() as u64),
        short_path(&after),
        human_size(after_bytes.len() as u64)
    );

    let options = match mode {
        PacketPatchMode::Raw => ArchiveEncodeOptions::raw(),
        _ => ArchiveEncodeOptions::stored(),
    };
    let patch = RsbPatch::create(&before_bytes, &after_bytes, options)
        .context("生成补丁失败：两个包的结构差异无法编码")?;
    let bytes = patch.to_bytes()?;
    write_file(&output, &bytes)?;

    let ratio = if after_bytes.len() > 0 {
        bytes.len() as f64 / after_bytes.len() as f64 * 100.0
    } else {
        0.0
    };
    Ok(format!(
        "补丁 {} 个分包差异 → {}（{}，为完整包的 {:.1}%）",
        patch.packets.len(),
        short_path(&output),
        human_size(bytes.len() as u64),
        ratio
    ))
}

pub fn rsbp_apply(ps: &Params) -> Result<String> {
    use rsb_patch::{ArchiveDecodeOptions, PacketPatchMode, RsbPatch};

    let base = ps.file_in("base", "基础 RSB")?;
    let patch = ps.file_in("patch", "补丁文件")?;
    let output = out_file_or_sibling(ps, "output", &base, "rsb", "patched")?;
    let mode = match ps.str_or("mode", "stored") {
        "raw" => PacketPatchMode::Raw,
        _ => PacketPatchMode::Stored,
    };

    let base_bytes = read_all(&base)?;
    let patch_bytes = read_all(&patch)?;
    let parsed = RsbPatch::read(std::io::Cursor::new(&patch_bytes))
        .context("不是有效的 RSBP 补丁文件")?;
    crate::log_line!("补丁包含 {} 个分包差异。", parsed.packets.len());

    let options = match mode {
        PacketPatchMode::Raw => ArchiveDecodeOptions::raw(),
        _ => ArchiveDecodeOptions::stored(),
    };
    let patched = parsed
        .apply_to_archive(&base_bytes, options)
        .context("应用补丁失败：基础包与补丁不匹配？")?;
    write_file(&output, &patched)?;

    Ok(format!(
        "已合成新包 {}（{}）",
        short_path(&output),
        human_size(patched.len() as u64)
    ))
}

// ============================================================================
// PAK
// ============================================================================

pub fn pak_unpack(ps: &Params) -> Result<String> {

    let input = ps.file_in("input", "PAK 文件")?;
    let out_root = out_dir_or_sibling(ps, "output", &input, "unpacked")?;

    let data = read_all(&input)?;
    let archive = pak_archive::from_bytes(&data)
        .with_context(|| format!("{} 不是有效的 PAK 包", input.display()))?;

    let mut files = 0u64;
    for entry in archive.entries() {
        let rel = String::from_utf8_lossy(entry.path().as_ref()).into_owned();
        let target = safe_join(&out_root, &rel)?;
        if entry.data().is_empty() && rel.ends_with('/') {
            // 目录条目：只建目录
            std::fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_file(&target, entry.data())?;
        files += 1;
    }

    Ok(format!(
        "已解包 {files} 个条目 → {}",
        short_path(&out_root)
    ))
}

pub fn pak_pack(ps: &Params) -> Result<String> {
    use pak_archive::{EncodeOptions, PakArchive, PakEntry, PakPath};

    let input = ps.dir_in("input", "输入目录")?;
    let output = out_file_or_sibling(ps, "output", &input, "pak", "packed")?;

    // 目录树 → 条目（保持相对路径，Windows 分隔符归一）
    let mut paths: Vec<PathBuf> = Vec::new();
    for p in walkdir::WalkDir::new(&input).sort_by_file_name() {
        let p = p?;
        if p.file_type().is_file() {
            paths.push(p.path().to_path_buf());
        }
    }
    if paths.is_empty() {
        bail!("目录里没有文件：{}", input.display());
    }

    // 读文件按路径粒度并行（打包前的大头是磁盘 IO + 解压内存拷贝）
    let entries: Vec<PakEntry> = {
        use rayon::prelude::*;
        paths
            .par_iter()
            .map(|p| -> Result<PakEntry> {
                let rel = p
                    .strip_prefix(&input)
                    .map_err(|_| anyhow!("内部路径错误"))?
                    .to_string_lossy()
                    .replace('\\', "/");
                let data = read_all(p)?;
                Ok(PakEntry::new(PakPath::new(rel), data))
            })
            .collect::<Result<Vec<_>>>()?
    };

    let archive = PakArchive::new(EncodeOptions::default(), entries)
        .context("打包失败")?;
    let bytes = pak_archive::to_bytes(&archive).context("序列化 PAK 失败")?;
    write_file(&output, &bytes)?;

    Ok(format!(
        "已打包 {} 个文件 → {}（{}）",
        paths.len(),
        short_path(&output),
        human_size(bytes.len() as u64)
    ))
}

pub fn pak_list(ps: &Params) -> Result<String> {

    let input = ps.file_in("input", "PAK 文件")?;
    let output = out_file_or_sibling(ps, "output", &input, "txt", "list")?;

    let data = read_all(&input)?;
    let archive = pak_archive::from_bytes(&data)
        .with_context(|| format!("{} 不是有效的 PAK 包", input.display()))?;

    let mut lines = Vec::new();
    lines.push(format!("# {} — {} 个条目", input.display(), archive.len()));
    for entry in archive.entries() {
        let rel = String::from_utf8_lossy(entry.path().as_ref()).into_owned();
        lines.push(format!("{}\t{}", human_size(entry.data().len() as u64), rel));
    }
    let text = lines.join("\n") + "\n";
    write_file(&output, text.as_bytes())?;

    Ok(format!(
        "共 {} 个条目 → {}",
        archive.len(),
        short_path(&output)
    ))
}

// ============================================================================
// DZip
// ============================================================================

pub fn dzip_unpack(ps: &Params) -> Result<String> {
    use dzip::{ExtractOptions, FileSystemVolumeManager};

    let input = ps.file_in("input", "DZip 文件")?;
    let out_root = out_dir_or_sibling(ps, "output", &input, "unpacked")?;

    // 卷文件（GAME.00x / 同名 .00x）默认与索引文件同目录，也可以手动指定
    let volumes_dir = ps
        .path_opt("volumes")
        .or_else(|| input.parent().map(|p| p.to_path_buf()))
        .ok_or_else(|| anyhow!("无法确定卷文件目录"))?;

    // 卷 0 永远是索引文件自身（DZip 格式里索引和数据开头在同一个文件里），
    // 之后按 PvZ2 惯例探测续卷：同名 main.001… 或 ASSETS/GAME.001…
    let base = volumes_dir.join(
        input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "main.dzip".into()),
    );
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".into());
    let mut file_list: Vec<String> = vec![
        base.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    ];
    for name in [format!("{stem}.001"), format!("{stem}.002"), format!("{stem}.003"), "ASSETS/GAME.001".into(), "ASSETS/GAME.002".into(), "ASSETS/GAME.003".into()] {
        if volumes_dir.join(&name).exists() {
            file_list.push(name);
        }
    }
    crate::log_line!("卷文件：{}", file_list.join(", "));

    let file = std::fs::File::open(&input)
        .with_context(|| format!("无法打开 {}", input.display()))?;
    let volumes = FileSystemVolumeManager::new(volumes_dir, file_list);
    let mut opened = dzip::Archive::open_with_volumes(std::io::BufReader::new(file), volumes)
        .context("不是有效的 DZip 索引或卷文件挂载失败")?;

    let report = opened
        .extract_to(&out_root, ExtractOptions { overwrite: true })
        .context("解包失败")?;

    Ok(format!(
        "已解包 {} 个文件（{}）→ {}",
        report.files,
        human_size(report.bytes),
        short_path(&out_root)
    ))
}

pub fn dzip_pack(ps: &Params) -> Result<String> {
    use dzip::{ArchiveBuilder, EntryOptions};

    let input = ps.dir_in("input", "输入目录")?;
    let output = out_file_or_sibling(ps, "output", &input, "dzip", "packed")?;

    let mut builder = ArchiveBuilder::new();
    let mut count = 0usize;
    for p in walkdir::WalkDir::new(&input).sort_by_file_name() {
        let p = p?;
        if !p.file_type().is_file() {
            continue;
        }
        let rel = p
            .path()
            .strip_prefix(&input)
            .map_err(|_| anyhow!("内部路径错误"))?
            .to_string_lossy()
            .replace('\\', "/");
        builder
            .add_path(&rel, p.path(), EntryOptions::default())
            .with_context(|| format!("加入条目失败：{rel}"))?;
        count += 1;
    }
    if count == 0 {
        bail!("目录里没有文件：{}", input.display());
    }

    // 上游 dzip-tool 的语义：第一个卷就是索引文件本身，
    // write_to_path 会把卷 0 命名成输出文件名，剩余数据全在里面。
    let report = builder.write_to_path(&output).context("写出 DZip 失败")?;
    Ok(format!(
        "已打包 {count} 个文件 → {}（{}）",
        short_path(&output),
        human_size(report.stored_bytes)
    ))
}

// Read/Seek 约束占位：保持 trait 在作用域里供库泛型使用
#[allow(dead_code)]
fn _assert_traits<R: Read + Seek>() {}
