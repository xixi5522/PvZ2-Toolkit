// ============================================================================
// commands/audio.rs —— 音频类命令
//
// BNK 音频库（Wwise SoundBank）与 WEM 音频的解析/导出/转码。
// ============================================================================

use crate::params::{human_size, short_path, Params};
use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};

fn read_all(p: &Path) -> Result<Vec<u8>> {
    std::fs::read(p).with_context(|| format!("无法读取文件 {}", p.display()))
}

fn write_file(p: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(p, data).with_context(|| format!("无法写入 {}", p.display()))
}

fn derive_out(ps: &Params, key: &str, input: &Path, tag: &str, ext: &str) -> Result<PathBuf> {
    if let Some(p) = ps.path_opt(key) {
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

// ============================================================================
// BNK（bnk-archive）
// ============================================================================

pub fn bnk_extract(ps: &Params) -> Result<String> {
    use bnk_archive::{BankChunk, SoundBankIndex};

    let input = ps.file_in("input", "BNK 文件")?;
    let out_root = if let Some(d) = ps.path_opt("output") {
        std::fs::create_dir_all(&d)?;
        d
    } else {
        let base = input
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = input
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "bnk".into());
        let d = base.join(format!("{stem}_audio"));
        std::fs::create_dir_all(&d)?;
        d
    };

    let data = read_all(&input)?;
    let bank = bnk_archive::from_bytes(&data)
        .with_context(|| format!("{} 不是有效的 BNK 音频库", input.display()))?;
    let index = SoundBankIndex::new(&bank);

    // 遍历所有 DIDX 条目，从 DATA 块切出每条 WEM
    let mut exported = 0usize;
    let mut total_bytes = 0u64;
    let mut ids: Vec<_> = Vec::new();

    for chunk in &bank.chunks {
        if let BankChunk::MediaIndex(entries) = chunk {
            for entry in entries {
                ids.push(entry.id);
            }
        }
    }
    ids.sort();
    ids.dedup();

    for id in &ids {
        if let Some(media) = index.embedded_media(&bank, *id)? {
            let path = out_root.join(format!("{}.wem", id));
            write_file(&path, media.data)?;
            exported += 1;
            total_bytes += media.data.len() as u64;
        }
    }

    // 顺带导出一份层级对象信息，方便对照
    {
        use std::fmt::Write as _;
        let mut info = String::new();
        let _ = writeln!(info, "# {}", input.display());
        let _ = writeln!(info, "# 导出 {} 条 WEM（{}）", exported, human_size(total_bytes));
        for chunk in &bank.chunks {
            if let BankChunk::Hierarchy(objects) = chunk {
                for obj in objects {
                    let _ = writeln!(info, "object {:?} kind={:?}", obj.id, obj.kind);
                }
            }
        }
        write_file(&out_root.join("bank_info.txt"), info.as_bytes())?;
    }

    Ok(format!(
        "已导出 {exported} 条 WEM（{}）→ {}",
        human_size(total_bytes),
        short_path(&out_root)
    ))
}

// ============================================================================
// WEM（wem-audio）
// ============================================================================

pub fn wem_decode(ps: &Params) -> Result<String> {
    use wem_audio::{decode_wem_to_wav, WemDecodeOptions};
    use std::io::BufWriter;

    let input = ps.file_in("input", "WEM 文件")?;
    let output = derive_out(ps, "output", &input, "audio", "wav")?;

    let file = std::fs::File::open(&input)
        .with_context(|| format!("无法打开 {}", input.display()))?;
    let out = std::fs::File::create(&output)
        .with_context(|| format!("无法创建 {}", output.display()))?;
    decode_wem_to_wav(file, BufWriter::new(out), &WemDecodeOptions::default())
        .with_context(|| "WEM 解码失败：编码器不支持或数据损坏")?;

    let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    Ok(format!(
        "{} → {}（{}）",
        short_path(&input),
        short_path(&output),
        human_size(size)
    ))
}

pub fn wem_encode(ps: &Params) -> Result<String> {
    use std::io::{Cursor, Read};

    let input = ps.file_in("input", "音频文件")?;
    let codec = ps.str_or("codec", "pcm");
    let output = derive_out(ps, "output", &input, "encoded", "wem")?;

    // 嗅探容器头，防止把 WAV 喂给 Vorbis 编码器之类的错配
    let mut head = [0u8; 4];
    {
        let mut f = std::fs::File::open(&input)
            .with_context(|| format!("无法打开 {}", input.display()))?;
        f.read_exact(&mut head).context("文件太小，不是有效的音频")?;
    }

    let data = match codec {
        "vorbis" => {
            if &head != b"OggS" {
                bail!("Vorbis 编码需要 OGG 输入（当前文件不是 Ogg 容器）");
            }
            let f = std::fs::File::open(&input)?;
            let mut out = Cursor::new(Vec::new());
            wem_audio::VorbisWemEncoder::new(std::io::BufReader::new(f))
                .encode(&mut out)
                .map_err(|e| anyhow!("Vorbis WEM 编码失败：{e}"))?;
            out.into_inner()
        }
        codec @ ("pcm" | "adpcm") => {
            if &head != b"RIFF" {
                bail!("PCM/ADPCM 编码需要 WAV 输入（当前文件不是 WAV）");
            }
            let f = std::fs::File::open(&input)?;
            let mut out = Cursor::new(Vec::new());
            let result = match codec {
                "adpcm" => wem_audio::AdpcmWemEncoder::new(std::io::BufReader::new(f))
                    .and_then(|enc| enc.encode(&mut out)),
                _ => wem_audio::PcmWemEncoder::new(std::io::BufReader::new(f))
                    .and_then(|enc| enc.encode(&mut out)),
            };
            result.map_err(|e| anyhow!("WEM 编码失败：{e}"))?;
            out.into_inner()
        }
        _ => bail!("未知编码方式：{codec}"),
    };
    write_file(&output, &data)?;

    Ok(format!(
        "已编码 {}（{}）——替换 .wem 后记得用 RSB 回包写回资源包",
        short_path(&output),
        human_size(data.len() as u64)
    ))
}
