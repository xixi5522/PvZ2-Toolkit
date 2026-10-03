// ============================================================================
// commands/anim.rs —— 动画类命令
//
// PAM / ReAnim / 粒子特效 三种动画资源格式的解码与编码。
// JSON 结构直接来自各格式库的 serde 类型（与上游桌面版互通）。
// ============================================================================

use crate::params::{short_path, Params};
use anyhow::{Context, Result};
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

/// "pc" / "phone32" / "phone64" → 库的版本枚举。
fn reanim_version(s: &str) -> reanim_codec::ReanimVersion {
    match s {
        "phone32" => reanim_codec::ReanimVersion::Phone32,
        "phone64" => reanim_codec::ReanimVersion::Phone64,
        _ => reanim_codec::ReanimVersion::PC,
    }
}

fn particles_version(s: &str) -> particle_codec::ParticlesVersion {
    match s {
        "phone32" => particle_codec::ParticlesVersion::Phone32,
        "phone64" => particle_codec::ParticlesVersion::Phone64,
        _ => particle_codec::ParticlesVersion::PC,
    }
}

// ============================================================================
// PAM（pam-codec）
// ============================================================================

pub fn pam_decode(ps: &Params) -> Result<String> {
    use pam_codec::decode_pam;
    use std::io::Cursor;

    let input = ps.file_in("input", "PAM 文件")?;
    let output = derive_out(ps, "output", &input, "decoded", "json")?;

    let data = read_all(&input)?;
    let mut reader = Cursor::new(&data);
    let pam = decode_pam(&mut reader).context("PAM 解析失败")?;
    let json = serde_json::to_string_pretty(&pam).context("转 JSON 失败")?;
    write_file(&output, json.as_bytes())?;

    Ok(format!(
        "{} → {}（{} 个图元 / {} 个精灵）",
        short_path(&input),
        short_path(&output),
        pam.image.len(),
        pam.sprite.len()
    ))
}

pub fn pam_encode(ps: &Params) -> Result<String> {
    use pam_codec::encode_pam;
    use std::io::Cursor;

    let input = ps.file_in("input", "JSON 文件")?;
    let output = derive_out(ps, "output", &input, "encoded", "pam")?;

    let text = read_all(&input)?;
    let pam: pam_codec::PamInfo =
        serde_json::from_slice(&text).context("JSON 解析失败：不是合法的 PAM 结构")?;
    let mut buf = Cursor::new(Vec::new());
    encode_pam(&pam, &mut buf).context("PAM 编码失败")?;
    let data = buf.into_inner();
    write_file(&output, &data)?;

    Ok(format!(
        "{} → {}（{} 字节）",
        short_path(&input),
        short_path(&output),
        data.len()
    ))
}

// ============================================================================
// ReAnim（reanim-codec）
// ============================================================================

pub fn reanim_decode(ps: &Params) -> Result<String> {
    use reanim_codec::decode_with_version;

    let input = ps.file_in("input", "ReAnim 文件")?;
    let output = derive_out(ps, "output", &input, "decoded", "json")?;

    let data = read_all(&input)?;
    let (reanim, version) = decode_with_version(&data)
        .context("ReAnim 解析失败：不是有效的 .reanim 文件")?;
    let json = serde_json::to_string_pretty(&reanim).context("转 JSON 失败")?;
    write_file(&output, json.as_bytes())?;

    let vname = match version {
        reanim_codec::ReanimVersion::PC => "PC",
        reanim_codec::ReanimVersion::Phone32 => "Phone32",
        reanim_codec::ReanimVersion::Phone64 => "Phone64",
    };
    Ok(format!(
        "{} → {}（{} 条轨道，版本 {vname}）",
        short_path(&input),
        short_path(&output),
        reanim.tracks.len()
    ))
}

pub fn reanim_encode(ps: &Params) -> Result<String> {
    use reanim_codec::encode;

    let input = ps.file_in("input", "JSON 文件")?;
    let version = reanim_version(ps.str_or("version", "pc"));
    let output = derive_out(ps, "output", &input, "encoded", "reanim")?;

    let text = read_all(&input)?;
    let reanim: reanim_codec::Reanim =
        serde_json::from_slice(&text).context("JSON 解析失败：不是合法的 ReAnim 结构")?;
    let data = encode(&reanim, version).context("ReAnim 编码失败")?;
    write_file(&output, &data)?;

    Ok(format!(
        "{} → {}（{} 条轨道，{} 字节）",
        short_path(&input),
        short_path(&output),
        reanim.tracks.len(),
        data.len()
    ))
}

// ============================================================================
// 粒子特效（particle-codec）
// ============================================================================

pub fn popfx_decode(ps: &Params) -> Result<String> {
    use particle_codec::xml::format_particles_xml;
    use particle_codec::{decode_pc, decode_phone32, decode_phone64};

    let input = ps.file_in("input", "粒子文件")?;
    let output = derive_out(ps, "output", &input, "decoded", "xml")?;
    let version = ps.str_or("version", "auto").to_string();

    let data = read_all(&input)?;

    // auto 模式按 PC → Phone32 → Phone64 依次尝试，直到有一个能解开
    let (particles, vname) = match version.as_str() {
        "pc" => (decode_pc(&data).context("PC 版解码失败")?, "PC"),
        "phone32" => (
            decode_phone32(&data).context("Phone32 版解码失败")?,
            "Phone32",
        ),
        "phone64" => (
            decode_phone64(&data).context("Phone64 版解码失败")?,
            "Phone64",
        ),
        _ => {
            let mut last: Option<anyhow::Error> = None;
            let mut hit = None;
            for (label, f) in [
                ("PC", decode_pc(&data)),
                ("Phone32", decode_phone32(&data)),
                ("Phone64", decode_phone64(&data)),
            ] {
                match f {
                    Ok(p) => {
                        hit = Some((p, label));
                        break;
                    }
                    Err(e) => last = Some(e.into()),
                }
            }
            match hit {
                Some(v) => v,
                None => {
                    return Err(last.unwrap_or_else(|| anyhow::anyhow!("解码失败")))
                        .context("自动识别失败：三种版本都无法解析");
                }
            }
        }
    };

    let xml = format_particles_xml(&particles).context("转 XML 失败")?;
    write_file(&output, xml.as_bytes())?;

    Ok(format!(
        "{} → {}（版本 {vname}）",
        short_path(&input),
        short_path(&output)
    ))
}

pub fn popfx_encode(ps: &Params) -> Result<String> {
    use particle_codec::xml::parse_particles_xml;
    use particle_codec::encode;

    let input = ps.file_in("input", "XML 文件")?;
    let version = particles_version(ps.str_or("version", "pc"));
    let output = derive_out(ps, "output", &input, "encoded", "popfx")?;

    let text = read_all(&input)?;
    let xml = std::str::from_utf8(&text).context("XML 文件不是 UTF-8 文本")?;
    let particles = parse_particles_xml(xml).context("XML 解析失败")?;
    let data = encode(&particles, version).context("粒子库编码失败")?;
    write_file(&output, &data)?;

    Ok(format!(
        "{} → {}（{} 字节）",
        short_path(&input),
        short_path(&output),
        data.len()
    ))
}

// ============================================================================
// XFL 工程（reanim-codec::xfl）—— PC 版动画的 Flash 工程形态
// ============================================================================

pub fn reanim_xfl_decode(ps: &Params) -> Result<String> {
    use reanim_codec::decode_xfl;

    let input = ps.dir_in("input", "XFL 工程目录")?;
    let output = derive_out(ps, "output", &input, "decoded", "json")?;

    let reanim = decode_xfl(&input)
        .context("XFL 解析失败：目录里缺少有效的 DOMDocument.xml？")?;
    let json = serde_json::to_string_pretty(&reanim).context("转 JSON 失败")?;
    write_file(&output, json.as_bytes())?;

    Ok(format!(
        "{} → {}（{} 条轨道）",
        short_path(&input),
        short_path(&output),
        reanim.tracks.len()
    ))
}

pub fn reanim_xfl_encode(ps: &Params) -> Result<String> {
    use reanim_codec::encode_xfl;

    let input = ps.file_in("input", "ReAnim JSON")?;

    // 输出是目录：填了就用，没填在 JSON 旁边建 stem_xfl 目录
    let output = match ps.path_opt("output") {
        Some(d) => d,
        None => input
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .join(format!(
                "{}_xfl",
                input
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "output".into())
            )),
    };

    let data = read_all(&input)?;
    let reanim: reanim_codec::Reanim =
        serde_json::from_slice(&data).context("JSON 解析失败：需要 reanim.decode 输出的结构")?;
    encode_xfl(&reanim, &output)
        .context("写出 XFL 工程失败")?;

    Ok(format!(
        "{} → {}（{} 条轨道）",
        short_path(&input),
        short_path(&output),
        reanim.tracks.len()
    ))
}
