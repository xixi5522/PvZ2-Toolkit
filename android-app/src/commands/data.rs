// ============================================================================
// commands/data.rs —— 数据类命令
//
// RTON / SMF / 编译文本 / Crypt-Data / Newton 清单 五种数据格式的
// 解码与编码。全部基于 ed1ths-pvz-toolkit 的格式库。
// ============================================================================

use crate::params::{short_path, Params};
use anyhow::{bail, Context, Result};
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

/// 输出文件：填了就用，没填就按「输入名_操作.新扩展名」放在输入旁边。
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
// RTON（serde-rton）
// ============================================================================

pub fn rton_decode(ps: &Params) -> Result<String> {
    use serde_rton::{decrypt_data, from_bytes, Value};

    let input = ps.file_in("input", "RTON 文件")?;
    let output = derive_out(ps, "output", &input, "decoded", "json")?;

    let mut data = read_all(&input)?;
    if ps.bool("encrypted") {
        data = decrypt_data(&data).context("解密失败：文件可能不是加密 RTON")?;
        crate::log_line!("已先对加密 RTON 解密（{} 字节）", data.len());
    }

    let value: Value = from_bytes(&data).context("RTON 解析失败")?;
    let json = serde_json::to_string_pretty(&value).context("转 JSON 失败")?;
    write_file(&output, json.as_bytes())?;

    Ok(format!(
        "{} → {}（{} 节点）",
        short_path(&input),
        short_path(&output),
        human_nodes(&json)
    ))
}

pub fn rton_encode(ps: &Params) -> Result<String> {
    use serde_rton::{encrypt_data, to_bytes, Value};

    let input = ps.file_in("input", "JSON 文件")?;
    let output = derive_out(ps, "output", &input, "encoded", "rton")?;

    let text = read_all(&input)?;
    let value: Value = serde_json::from_slice(&text).context("JSON 解析失败")?;
    let mut data = to_bytes(&value).context("RTON 编码失败")?;
    if ps.bool("encrypt") {
        data = encrypt_data(&data).context("RTON 加密失败")?;
    }
    write_file(&output, &data)?;

    Ok(format!(
        "{} → {}（{} 字节）",
        short_path(&input),
        short_path(&output),
        data.len()
    ))
}

/// 粗略数一下 JSON 里的节点数，日志里给个量级。
fn human_nodes(json: &str) -> String {
    let n = json.matches(['{', '[']).count();
    format!("约 {n} 层结构")
}

// ============================================================================
// SMF（smf-container）
// ============================================================================

pub fn smf_decode(ps: &Params) -> Result<String> {
    use smf_container::decode;
    use std::io::Cursor;

    let input = ps.file_in("input", "SMF 文件")?;
    let output = derive_out(ps, "output", &input, "decoded", "txt")?;

    let data = read_all(&input)?;
    let payload = decode(Cursor::new(&data)).context("SMF 解码失败")?;
    write_file(&output, &payload)?;

    Ok(format!(
        "{} → {}（{} 字节）",
        short_path(&input),
        short_path(&output),
        payload.len()
    ))
}

pub fn smf_encode(ps: &Params) -> Result<String> {
    use smf_container::{encode, EncodeOptions};

    let input = ps.file_in("input", "输入文件")?;
    let output = derive_out(ps, "output", &input, "encoded", "smf")?;

    let data = read_all(&input)?;
    let wrapped = encode(&data, EncodeOptions::default()).context("SMF 编码失败")?;
    write_file(&output, &wrapped)?;

    Ok(format!(
        "{} → {}（{} 字节）",
        short_path(&input),
        short_path(&output),
        wrapped.len()
    ))
}

// ============================================================================
// 编译文本（compiled-text）
// ============================================================================

pub fn ctext_decode(ps: &Params) -> Result<String> {
    use compiled_text::decode;

    let input = ps.file_in("input", "编译文本")?;
    let seed = ps
        .raw("seed")
        .ok_or_else(|| anyhow::anyhow!("「SEED 密钥」不能为空，请填写加密种子"))?;
    let output = derive_out(ps, "output", &input, "decoded", "txt")?;

    let data = read_all(&input)?;
    let plain = decode(&data, seed).context("解码失败：SEED 不对或文件格式不符？")?;
    write_file(&output, &plain)?;

    Ok(format!(
        "{} → {}（{} 字节）",
        short_path(&input),
        short_path(&output),
        plain.len()
    ))
}

pub fn ctext_encode(ps: &Params) -> Result<String> {
    use compiled_text::encode;

    let input = ps.file_in("input", "明文文件")?;
    let seed = ps
        .raw("seed")
        .ok_or_else(|| anyhow::anyhow!("「SEED 密钥」不能为空，请填写加密种子"))?;
    let output = derive_out(ps, "output", &input, "encoded", "bin")?;

    let data = read_all(&input)?;
    let cipher = encode(&data, seed).context("编码失败")?;
    write_file(&output, &cipher)?;

    Ok(format!(
        "{} → {}（{} 字节）",
        short_path(&input),
        short_path(&output),
        cipher.len()
    ))
}

// ============================================================================
// Crypt-Data（crypt-data）
// ============================================================================

/// 按用户选择的格式解析密钥。
fn parse_key(ps: &Params) -> Result<Vec<u8>> {
    let key_text = ps
        .raw("key")
        .ok_or_else(|| anyhow::anyhow!("「密钥」不能为空"))?;
    match ps.str_or("key-format", "utf8") {
        "hex" => {
            let t: String = key_text.chars().filter(|c| !c.is_whitespace()).collect();
            if t.len() % 2 != 0 {
                bail!("hex 密钥长度应为偶数，当前 {} 个字符", t.len());
            }
            (0..t.len())
                .step_by(2)
                .map(|i| {
                    u8::from_str_radix(&t[i..i + 2], 16)
                        .map_err(|e| anyhow::anyhow!("hex 解析失败于位置 {}：{e}", i))
                })
                .collect()
        }
        _ => Ok(key_text.as_bytes().to_vec()),
    }
}

pub fn cdat_decode(ps: &Params) -> Result<String> {
    use crypt_data::decrypt;

    let input = ps.file_in("input", "加密数据")?;
    let key = parse_key(ps)?;
    let output = derive_out(ps, "output", &input, "decoded", "bin")?;

    let data = read_all(&input)?;
    let plain = decrypt(&data, &key).context("解密失败：密钥不对或文件格式不符？")?;
    write_file(&output, &plain)?;

    Ok(format!(
        "{} → {}（{} 字节）",
        short_path(&input),
        short_path(&output),
        plain.len()
    ))
}

pub fn cdat_encode(ps: &Params) -> Result<String> {
    use crypt_data::encrypt;

    let input = ps.file_in("input", "明文数据")?;
    let key = parse_key(ps)?;
    let output = derive_out(ps, "output", &input, "encoded", "bin")?;

    let data = read_all(&input)?;
    let cipher = encrypt(&data, &key).context("加密失败")?;
    write_file(&output, &cipher)?;

    Ok(format!(
        "{} → {}（{} 字节）",
        short_path(&input),
        short_path(&output),
        cipher.len()
    ))
}

// ============================================================================
// Newton（newton-manifest）
// ============================================================================

pub fn newton_decode(ps: &Params) -> Result<String> {
    use newton_manifest::{from_bytes, ResourceManifest};

    let input = ps.file_in("input", "Newton 文件")?;
    let output = derive_out(ps, "output", &input, "decoded", "json")?;

    let data = read_all(&input)?;
    let manifest: ResourceManifest = from_bytes(&data).context("Newton 解析失败")?;
    let json = serde_json::to_string_pretty(&manifest).context("转 JSON 失败")?;
    write_file(&output, json.as_bytes())?;

    Ok(format!(
        "{} → {}（{} 个资源组）",
        short_path(&input),
        short_path(&output),
        manifest.groups.len()
    ))
}

pub fn newton_encode(ps: &Params) -> Result<String> {
    use newton_manifest::{to_bytes, ResourceManifest};

    let input = ps.file_in("input", "JSON 文件")?;
    let output = derive_out(ps, "output", &input, "encoded", "newton")?;

    let text = read_all(&input)?;
    let manifest: ResourceManifest =
        serde_json::from_slice(&text).context("JSON 解析失败：不是合法的 Newton 清单结构")?;
    let data = to_bytes(&manifest).context("Newton 编码失败")?;
    write_file(&output, &data)?;

    Ok(format!(
        "{} → {}（{} 个资源组，{} 字节）",
        short_path(&input),
        short_path(&output),
        manifest.groups.len(),
        data.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_out_builds_sibling_path() {
        let input = Path::new("/sdcard/game/main.rton");
        let ps = Params::new(Vec::new());
        let out = derive_out(&ps, "output", input, "decoded", "json").unwrap();
        assert_eq!(out, PathBuf::from("/sdcard/game/main_decoded.json"));
    }
}
