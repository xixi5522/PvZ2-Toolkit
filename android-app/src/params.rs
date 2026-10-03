// ============================================================================
// params.rs —— 参数容器
//
// UI 把用户填的表单回传成一个 (key, value) 列表；这一层负责把它包成
// 一个便于取值、且会给出「人话报错」的容器。
//
// 关键设计：这里是「让错误信息对用户友好」的地方。
// CLI 时代用户是开发者，报个 io::Error 无所谓；到了手机上，
// 用户看到 "No such file or directory" 是不知道该怎么办的，
// 所以下面所有取值失败都带上了参数的中文名。
// ============================================================================

use anyhow::{Result, anyhow, bail};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Params {
    map: HashMap<String, String>,
}

impl Params {
    pub fn new(pairs: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            map: pairs.into_iter().collect(),
        }
    }

    /// 原始取值。空串视同「没填」。
    pub fn raw(&self, key: &str) -> Option<&str> {
        self.map
            .get(key)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
    }

    // ---------------------------------------------------------------- 路径

    /// 必填路径。为空时报错，错误信息里带上中文标签。
    pub fn path_required(&self, key: &str, label: &str) -> Result<PathBuf> {
        let v = self
            .raw(key)
            .ok_or_else(|| anyhow!("「{label}」不能为空，请先选择"))?;
        let p = PathBuf::from(v);
        if !p.exists() {
            bail!("「{label}」指向的路径不存在：\n{v}");
        }
        Ok(p)
    }

    /// 可选路径。
    pub fn path_opt(&self, key: &str) -> Option<PathBuf> {
        self.raw(key).map(PathBuf::from)
    }

    /// 必填的输入文件，额外校验「它确实是个文件」。
    pub fn file_in(&self, key: &str, label: &str) -> Result<PathBuf> {
        let p = self.path_required(key, label)?;
        if !p.is_file() {
            bail!("「{label}」需要是一个文件，但选中的是目录：\n{}", p.display());
        }
        Ok(p)
    }

    /// 必填的输入目录，额外校验「它确实是个目录」。
    pub fn dir_in(&self, key: &str, label: &str) -> Result<PathBuf> {
        let p = self.path_required(key, label)?;
        if !p.is_dir() {
            bail!("「{label}」需要是一个目录，但选中的是文件：\n{}", p.display());
        }
        Ok(p)
    }

    // ---------------------------------------------------------------- 标量

    pub fn bool(&self, key: &str) -> bool {
        matches!(self.raw(key), Some(v) if v == "true" || v == "1")
    }

    pub fn int(&self, key: &str, label: &str, default: i64) -> Result<i64> {
        match self.raw(key) {
            None => Ok(default),
            Some(v) => v
                .parse::<i64>()
                .map_err(|_| anyhow!("「{label}」需要是整数，当前填的是：{v}")),
        }
    }

    pub fn u32_or(&self, key: &str, label: &str, default: u32) -> Result<u32> {
        match self.raw(key) {
            None => Ok(default),
            Some(v) => v
                .parse::<u32>()
                .map_err(|_| anyhow!("「{label}」需要是非负整数，当前填的是：{v}")),
        }
    }

    /// 可选 u32：没填返回 None（区别于「填了 0」）。
    pub fn u32_opt(&self, key: &str, label: &str) -> Result<Option<u32>> {
        match self.raw(key) {
            None => Ok(None),
            Some(v) => {
                let n = v
                    .parse::<u32>()
                    .map_err(|_| anyhow!("「{label}」需要是非负整数，当前填的是：{v}"))?;
                Ok(if n == 0 { None } else { Some(n) })
            }
        }
    }

    /// 可选 i32：填 0 视为「不指定」（对应原 CLI 的 Option<i32> + 0 约定）。
    pub fn i32_opt(&self, key: &str, label: &str) -> Result<Option<i32>> {
        match self.raw(key) {
            None => Ok(None),
            Some(v) => {
                let n = v
                    .parse::<i32>()
                    .map_err(|_| anyhow!("「{label}」需要是整数，当前填的是：{v}"))?;
                Ok(if n == 0 { None } else { Some(n) })
            }
        }
    }

    /// i32，0 是合法值（比如 PTX 的 format ID 可能就是要 0）。
    pub fn i32_raw(&self, key: &str, label: &str, default: i32) -> Result<i32> {
        match self.raw(key) {
            None => Ok(default),
            Some(v) => v
                .parse::<i32>()
                .map_err(|_| anyhow!("「{label}」需要是整数，当前填的是：{v}")),
        }
    }

    pub fn str_or<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.raw(key).unwrap_or(default)
    }

    /// 下拉框取值：必须落在候选项里，否则报错（防止 UI 被改坏后静默走错分支）。
    pub fn choice<'a>(
        &'a self,
        key: &str,
        label: &str,
        allowed: &[&str],
        default: &'a str,
    ) -> Result<&'a str> {
        let v = self.raw(key).unwrap_or(default);
        if !allowed.contains(&v) {
            bail!(
                "「{label}」的取值 {v} 不在允许范围内（{}）",
                allowed.join(" / ")
            );
        }
        Ok(v)
    }

    // ------------------------------------------------------------ 输出路径

    /// 解析输出路径，并保证父目录存在。
    ///
    /// 手机上这点特别重要：用户经常选一个还不存在的子目录当输出，
    /// 如果直接写文件会得到「No such file or directory」这种莫名报错。
    /// 这里统一帮忙建好。
    pub fn resolve_out(
        &self,
        key: &str,
        default: impl FnOnce() -> PathBuf,
        fallback_ext: &str,
    ) -> Result<PathBuf> {
        let mut p = match self.raw(key) {
            Some(v) => PathBuf::from(v),
            None => {
                let mut d = default();
                if !fallback_ext.is_empty() {
                    d.set_extension(fallback_ext.trim_start_matches('.'));
                }
                d
            }
        };

        if p.as_os_str().is_empty() {
            bail!("无法推断输出路径，请手动填写");
        }

        // 有扩展名但父目录不存在 → 建目录
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| anyhow!("无法创建输出目录 {}：{e}", parent.display()))?;
            }
        }
        Ok(std::mem::take(&mut p))
    }
}

/// 把一个路径平铺成「文件名（父目录）」的紧凑形式，日志面板里省地方。
pub fn short_path(p: &Path) -> String {
    match (p.file_name(), p.parent()) {
        (Some(n), Some(d)) if !d.as_os_str().is_empty() => {
            format!("{}/{}", d.file_name().unwrap_or_default().to_string_lossy(), n.to_string_lossy())
        }
        (Some(n), _) => n.to_string_lossy().into_owned(),
        _ => p.to_string_lossy().into_owned(),
    }
}

/// 人类可读的体积。
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pairs: &[(&str, &str)]) -> Params {
        Params::new(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn empty_string_counts_as_missing() {
        let ps = p(&[("a", ""), ("b", "   ")]);
        assert!(ps.raw("a").is_none());
        assert!(ps.raw("b").is_none());
    }

    #[test]
    fn int_parsing() {
        let ps = p(&[("n", "42")]);
        assert_eq!(ps.int("n", "数量", 0).unwrap(), 42);
        assert_eq!(ps.int("missing", "数量", 7).unwrap(), 7);

        let bad = p(&[("n", "abc")]);
        let e = bad.int("n", "数量", 0).unwrap_err().to_string();
        assert!(e.contains("数量"), "报错里应该有中文标签，实际是：{e}");
    }

    #[test]
    fn zero_means_unspecified_for_optional_numbers() {
        let ps = p(&[("a", "0"), ("b", "5")]);
        assert_eq!(ps.u32_opt("a", "宽").unwrap(), None);
        assert_eq!(ps.u32_opt("b", "宽").unwrap(), Some(5));
    }

    #[test]
    fn choice_rejects_unknown_value() {
        let ps = p(&[("fmt", "weird")]);
        assert!(ps.choice("fmt", "格式", &["json", "yaml"], "json").is_err());
        assert_eq!(
            ps.choice("nope", "格式", &["json", "yaml"], "json").unwrap(),
            "json"
        );
    }

    #[test]
    fn human_size_renders() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2.0 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }
}
