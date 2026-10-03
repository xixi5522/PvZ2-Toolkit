// ============================================================================
// settings.rs —— 应用设置
//
// 设置的读写分两层：
//   1. 持久化：Android 上用 SharedPreferences（走 JNI），桌面调试时退回一个
//      JSON 文件。这样同一套代码两端都能跑。
//   2. 应用：把设置值反映到 UI 上（明暗模式 + 主题色）。
//
// 2.1 变化：**配色注入回归，但引擎换成 Google 官方 Monet 算法**。
//   fork material 样式后（ui/widgets/material/），官方组件的颜色数据源
//   是我们的 Monet global —— 由 material-color-utilities（Material You
//   参考实现）从种子色算出完整 MD3 tonal palette 注入：
//     * theme_color = "monet"   → Android 读壁纸主色当种子（真·莫奈取色），
//       桌面回退 baseline 紫
//     * theme_color = "#RRGGBB" → 用户自定义种子色（预设色板 / 手输 hex）
//   * 明暗：Theme.effective-dark（<=> Monet.dark）与 Monet.color-scheme
//     由 colors::inject 一并写入，官方组件与自绘永远同源。
// ============================================================================

use crate::colors;
use crate::ui::{AppWindow, AppState, ColorPreset};
use slint::ComponentHandle;
use std::sync::RwLock;

/// 明暗模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    /// 跟随系统
    System,
    Light,
    Dark,
}

impl ThemeMode {
    fn as_str(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::System,
        }
    }
}

/// 应用设置。
#[derive(Debug, Clone)]
pub struct Settings {
    pub theme_mode: ThemeMode,
    /// 主题色："monet"（跟随系统莫奈取色）或 "#RRGGBB"（自定义种子色）
    pub theme_color: String,
    /// 后台线程数（0 = 自动，按 CPU 核心数）
    pub threads: u32,
    /// 上次使用的输入/输出目录，方便下次直接跳过去
    pub last_input_dir: String,
    pub last_output_dir: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme_mode: ThemeMode::System,
            theme_color: "monet".into(),
            threads: 0,
            last_input_dir: String::new(),
            last_output_dir: String::new(),
        }
    }
}

/// 当前生效的种子色（ARGB）。
///
/// "monet" 模式：Android 读系统 Material You 强调色（壁纸主色），
/// 读不到或桌面环境回退 MD3 baseline 紫（与官方 backend 注入值同族）。
pub fn seed_color() -> u32 {
    let s = current();
    match s.theme_color.as_str() {
        "monet" => {
            #[cfg(target_os = "android")]
            {
                crate::platform::system_accent_color().unwrap_or(colors::DEFAULT_SEED)
            }
            #[cfg(not(target_os = "android"))]
            {
                colors::DEFAULT_SEED
            }
        }
        hex => colors::parse_hex(hex).unwrap_or(colors::DEFAULT_SEED),
    }
}

static CURRENT: RwLock<Option<Settings>> = RwLock::new(None);

/// 读取当前设置（惰性加载）。
pub fn current() -> Settings {
    if let Ok(g) = CURRENT.read() {
        if let Some(s) = g.as_ref() {
            return s.clone();
        }
    }
    load()
}

/// 从持久化存储加载。
fn load() -> Settings {
    let s = {
        #[cfg(target_os = "android")]
        {
            load_android()
        }
        #[cfg(not(target_os = "android"))]
        {
            load_file()
        }
    };

    if let Ok(mut g) = CURRENT.write() {
        *g = Some(s.clone());
    }
    s
}

/// 保存并立即应用。
pub fn save(s: &Settings) -> Result<(), String> {
    {
        #[cfg(target_os = "android")]
        {
            save_android(s)?;
        }
        #[cfg(not(target_os = "android"))]
        {
            save_file(s)?;
        }
    }

    if let Ok(mut g) = CURRENT.write() {
        *g = Some(s.clone());
    }
    Ok(())
}

// ---------------------------------------------------------------- 桌面实现
//
// 桌面下用一个 JSON 文件代替 SharedPreferences，
// 路径在 ~/.config/pvz2-toolkit/settings.json

#[cfg(not(target_os = "android"))]
fn settings_path() -> std::path::PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
                .join(".config")
        });
    base.join("pvz2-toolkit").join("settings.json")
}

#[cfg(not(target_os = "android"))]
fn load_file() -> Settings {
    let p = settings_path();
    let Ok(text) = std::fs::read_to_string(&p) else {
        return Settings::default();
    };
    parse_settings(&text).unwrap_or_default()
}

#[cfg(not(target_os = "android"))]
fn save_file(s: &Settings) -> Result<(), String> {
    let p = settings_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败：{e}"))?;
    }
    std::fs::write(&p, serialize_settings(s)).map_err(|e| format!("写入配置失败：{e}"))
}

// ---------------------------------------------------------------- Android
//
// 用 SharedPreferences（单个 XML 文件，系统管理），
// 名字 "pvz2_toolkit"，MODE_PRIVATE = 0

#[cfg(target_os = "android")]
const PREFS_NAME: &str = "pvz2_toolkit";
#[cfg(target_os = "android")]
const MODE_PRIVATE: i32 = 0;

#[cfg(target_os = "android")]
fn load_android() -> Settings {
    let text = crate::platform::prefs_get_all(PREFS_NAME).unwrap_or_default();
    parse_settings(&text).unwrap_or_default()
}

#[cfg(target_os = "android")]
fn save_android(s: &Settings) -> Result<(), String> {
    let pairs = [
        ("theme_mode", s.theme_mode.as_str().to_string()),
        ("theme_color", s.theme_color.clone()),
        ("threads", s.threads.to_string()),
        ("last_input_dir", s.last_input_dir.clone()),
        ("last_output_dir", s.last_output_dir.clone()),
    ];
    crate::platform::prefs_put_all(PREFS_NAME, MODE_PRIVATE, &pairs)
}

// ---------------------------------------------------------------- 序列化
//
// 手写一个极简的键值序列化，避免为 4 个字段引入 serde 依赖。
// 格式：key=value，一行一个；值里的换行/反斜杠转义掉。

#[cfg(not(target_os = "android"))]
fn serialize_settings(s: &Settings) -> String {
    let esc = |v: &str| v.replace('\\', "\\\\").replace('\n', "\\n");
    format!(
        "theme_mode={}\ntheme_color={}\nthreads={}\nlast_input_dir={}\nlast_output_dir={}\n",
        s.theme_mode.as_str(),
        s.theme_color,
        s.threads,
        esc(&s.last_input_dir),
        esc(&s.last_output_dir),
    )
}

fn parse_settings(text: &str) -> Option<Settings> {
    let mut s = Settings::default();
    let mut any = false;

    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.replace("\\n", "\n").replace("\\\\", "\\");
        match k.trim() {
            "theme_mode" => {
                s.theme_mode = ThemeMode::from_str(v.trim());
                any = true;
            }
            "theme_color" => {
                // 只接受 "monet" 或合法 hex；别的值回退 monet
                let v = v.trim();
                if v == "monet" || colors::parse_hex(v).is_some() {
                    s.theme_color = v.to_string();
                } else {
                    s.theme_color = "monet".into();
                }
                any = true;
            }
            "threads" => {
                s.threads = v.trim().parse().unwrap_or(0);
                any = true;
            }
            "last_input_dir" => {
                s.last_input_dir = v;
                any = true;
            }
            "last_output_dir" => {
                s.last_output_dir = v;
                any = true;
            }
            // 旧版本的 seed_color 键直接忽略 —— 配色已交给官方动态取色
            _ => {}
        }
    }

    any.then_some(s)
}

/// 判断当前该用深色还是浅色。
///
/// ThemeMode::System 时问系统（Android 走 Configuration.uiMode）。
pub fn effective_dark(mode: ThemeMode) -> bool {
    match mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => {
            #[cfg(target_os = "android")]
            {
                crate::platform::system_dark_mode().unwrap_or(false)
            }
            #[cfg(not(target_os = "android"))]
            {
                false
            }
        }
    }
}

// ============================================================================
// 与 UI 接线
// ============================================================================

/// 把设置同步到全局属性上。
///
/// ★ 配色注入一处搞定（fork 样式后官方组件与自绘同源）：
///   colors::inject  → Monet global（官方 material 组件 + Theme 镜像）
///   Monet.dark      → Theme.effective-dark（palette.slint 双向绑定）
/// 其余设置值进 AppState 全局，所有界面组件直接读。
pub fn apply_to_window(window: &AppWindow) {
    let s = current();
    let dark = effective_dark(s.theme_mode);

    // 种子色 → 完整 MD3 色板 → 官方组件 + 自绘骨架
    let seed = seed_color();
    colors::inject(window, seed, dark);

    let app = window.global::<AppState>();
    app.set_author("嘻嘻哈哈嘿嘿".into());
    app.set_theme_mode(s.theme_mode.as_str().into());
    app.set_theme_color(s.theme_color.clone().into());
    app.set_threads(s.threads as i32);
}

/// 接好设置界面的回调。
pub fn wire(window: &AppWindow, weak: &slint::Weak<AppWindow>) {
    apply_to_window(window);

    let app = window.global::<AppState>();

    // --- 预设主题色样注入（色值由 Rust 算好，UI 只渲染色块）---
    // 拆成两行（各 6 个）：400dp 窄屏一行 12 个放不下。
    // hex 必须掩掉 alpha 存 6 位 —— 设置页选中态靠 theme-color == hex 判断。
    {
        let all: Vec<ColorPreset> = colors::COLOR_PRESETS
            .iter()
            .map(|(name, argb)| ColorPreset {
                name: (*name).into(),
                hex: format!("#{:06X}", argb & 0xFFFFFF).into(),
                brush: slint::Color::from_argb_encoded(*argb).into(),
            })
            .collect();
        let mut row1 = all;
        let row2 = row1.split_off(6.min(row1.len()));
        app.set_color_presets_row1(slint::ModelRc::new(slint::VecModel::from(row1)));
        app.set_color_presets_row2(slint::ModelRc::new(slint::VecModel::from(row2)));
    }

    // --- 改主题色（"monet" 或 "#RRGGBB"）---
    {
        let weak = weak.clone();
        app.on_set_theme_color(move |val: slint::SharedString| {
            let v = val.to_string();
            let v = if v == "monet" {
                "monet".to_string()
            } else {
                match colors::parse_hex(&v) {
                    // 掩掉 alpha 位，统一存成 6 位 #RRGGBB
                    //（argb 是 32 位，:06X 只是最小宽度，不掩码会输出 8 位）
                    Some(argb) => format!("#{:06X}", argb & 0xFF_FFFF),
                    None => return, // 非法输入直接忽略
                }
            };
            let mut s = current();
            s.theme_color = v;
            let _ = save(&s);
            if let Some(w) = weak.upgrade() {
                apply_to_window(&w);
                w.window().request_redraw();
            }
        });
    }

    // --- 改明暗模式 ---
    {
        let weak = weak.clone();
        app.on_set_theme_mode(move |mode: slint::SharedString| {
            let m = ThemeMode::from_str(&mode);
            let mut s = current();
            s.theme_mode = m;
            let _ = save(&s);
            if let Some(w) = weak.upgrade() {
                apply_to_window(&w);
                // 桌面下改完要重新渲染一次
                w.window().request_redraw();
            }
        });
    }

    // --- 改线程数 ---
    {
        let weak = weak.clone();
        app.on_set_threads(move |n: i32| {
            let n = n.clamp(0, 64) as u32;
            let mut s = current();
            s.threads = n;
            let _ = save(&s);
            if let Some(w) = weak.upgrade() {
                apply_to_window(&w);
            }
        });
    }

    // --- 申请存储权限 ---
    {
        let weak = weak.clone();
        app.on_request_storage_permission(move || {
            #[cfg(target_os = "android")]
            {
                crate::platform::request_storage_permission();
                crate::log_line!("已发起存储权限申请，请在系统弹窗里选择「允许」。");
                // 系统弹窗是异步的，稍后再刷新状态
                let weak2 = weak.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1200));
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak2.upgrade() {
                            refresh_permission_state(&w);
                        }
                    });
                });
            }
            #[cfg(not(target_os = "android"))]
            {
                crate::log_line!("桌面环境不需要申请权限。");
                let _ = &weak;
            }
        });
    }

    // --- 复制诊断信息到剪贴板 ---
    //
    // 这个回调只往剪贴板写、再记一条日志，不需要访问窗口，
    // 所以既不用 weak 也不用 upgrade。
    app.on_copy_diagnostics(move || {
        let text = diagnostics_text();
        #[cfg(target_os = "android")]
        {
            if let Err(e) = crate::platform::set_clipboard(&text) {
                crate::log_line!("复制失败：{e}");
                return;
            }
        }
        #[cfg(not(target_os = "android"))]
        {
            // 桌面没有系统剪贴板集成，把内容打到日志里方便直接看
            crate::log_line!("（桌面调试）诊断信息：\n{text}");
        }
        crate::log_line!("诊断信息已复制到剪贴板。");
    });

    refresh_permission_state(window);
}

/// 刷新权限状态与设备信息显示。
pub fn refresh_permission_state(window: &AppWindow) {
    let app = window.global::<AppState>();
    #[cfg(target_os = "android")]
    {
        let granted = crate::platform::has_storage_permission();
        app.set_storage_granted(granted);
        app.set_storage_status(if granted {
            "已授予存储权限".into()
        } else {
            "尚未授予存储权限——工具需要它来读写 .rsb / .pak 等文件".into()
        });
        app.set_device_abi(crate::platform::primary_abi().into());
        app.set_data_dir(crate::platform::files_dir().into());
    }
    #[cfg(not(target_os = "android"))]
    {
        app.set_storage_granted(true);
        app.set_storage_status("桌面环境（无需权限）".into());
        app.set_device_abi(std::env::consts::ARCH.into());
        app.set_data_dir(
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".into())
                .into(),
        );
    }
}

/// 生成一段给用户/开发者看的诊断信息。
pub fn diagnostics_text() -> String {
    let s = current();
    let mut out = String::new();
    out.push_str("PvZ2 Toolkit 诊断信息\n");
    out.push_str("──────────────────────\n");
    out.push_str(&format!("版本：{}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("作者：嘻嘻哈哈嘿嘿\n"));
    out.push_str(&format!(
        "设置：模式={}　主题色={}　线程={}\n",
        s.theme_mode.as_str(),
        if s.theme_color == "monet" { "跟随系统莫奈" } else { &s.theme_color },
        if s.threads == 0 { "自动".to_string() } else { s.threads.to_string() }
    ));

    #[cfg(target_os = "android")]
    {
        out.push_str(&format!("ABI：{}\n", crate::platform::primary_abi()));
        out.push_str(&format!("数据目录：{}\n", crate::platform::files_dir()));
        out.push_str(&format!(
            "存储权限：{}\n",
            if crate::platform::has_storage_permission() { "已授予" } else { "未授予" }
        ));
        out.push_str(&format!(
            "系统深色模式：{}\n",
            if crate::platform::system_dark_mode().unwrap_or(false) { "是" } else { "否" }
        ));
    }
    #[cfg(not(target_os = "android"))]
    {
        out.push_str(&format!("平台：桌面 / {}\n", std::env::consts::ARCH));
    }

    out.push_str("\n支持的命令：\n");
    for c in crate::catalog::COMMANDS {
        out.push_str(&format!("  · {} —— {}\n", c.name, c.desc));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_mode_parsing() {
        assert_eq!(ThemeMode::from_str("light"), ThemeMode::Light);
        assert_eq!(ThemeMode::from_str("dark"), ThemeMode::Dark);
        assert_eq!(ThemeMode::from_str("system"), ThemeMode::System);
        // 未知值退回跟随系统
        assert_eq!(ThemeMode::from_str("乱写的"), ThemeMode::System);
    }

    #[test]
    fn settings_roundtrip() {
        let s = Settings {
            theme_mode: ThemeMode::Dark,
            theme_color: "#1565C0".into(),
            threads: 7,
            last_input_dir: "/a/b".into(),
            last_output_dir: "/c/d".into(),
        };
        let text = serialize_settings(&s);
        let back = parse_settings(&text).expect("应该能解析回来");
        assert_eq!(back.theme_mode, s.theme_mode);
        assert_eq!(back.theme_color, s.theme_color);
        assert_eq!(back.threads, s.threads);
        assert_eq!(back.last_input_dir, s.last_input_dir);
        assert_eq!(back.last_output_dir, s.last_output_dir);
    }

    #[test]
    fn malformed_settings_do_not_panic() {
        assert!(parse_settings("").is_none());
        assert!(parse_settings("garbage").is_none());
        let s = parse_settings("threads=abc\ntheme_mode=nonsense\n").unwrap_or_default();
        assert_eq!(s.threads, 0);
        assert_eq!(s.theme_mode, ThemeMode::System);
    }

    #[test]
    fn theme_color_parsing_falls_back_to_monet() {
        // 合法 hex 原样保留
        let s = parse_settings("theme_color=#2E7D32\n").unwrap_or_default();
        assert_eq!(s.theme_color, "#2E7D32");
        // 非法值回退 monet
        let s = parse_settings("theme_color=不合法\n").unwrap_or_default();
        assert_eq!(s.theme_color, "monet");
        // 旧版本（2.0 及更早）没有这个键 → 默认 monet
        let s = parse_settings("threads=2\n").unwrap_or_default();
        assert_eq!(s.theme_color, "monet");
        // 1.x 时代的 seed_color 键照旧忽略
        let s = parse_settings("seed_color=#4C8B3C\nthreads=2\n").unwrap_or_default();
        assert_eq!(s.threads, 2);
        assert_eq!(s.theme_color, "monet");
    }

    #[test]
    fn seed_color_custom_hex_takes_effect() {
        // 桌面环境（无 JNI）：monet 回退 baseline；hex 直通
        let s = Settings {
            theme_color: "#2E7D32".into(),
            ..Settings::default()
        };
        *CURRENT.write().unwrap() = Some(s);
        assert_eq!(seed_color(), 0xFF2E7D32);

        let s = Settings {
            theme_color: "monet".into(),
            ..Settings::default()
        };
        *CURRENT.write().unwrap() = Some(s);
        #[cfg(not(target_os = "android"))]
        assert_eq!(seed_color(), colors::DEFAULT_SEED);

        // 还原
        *CURRENT.write().unwrap() = None;
    }

    #[test]
    fn effective_dark_respects_explicit_modes() {
        assert!(effective_dark(ThemeMode::Dark));
        assert!(!effective_dark(ThemeMode::Light));
        // System 在桌面环境固定为浅色；Android 上由系统决定
        #[cfg(not(target_os = "android"))]
        assert!(!effective_dark(ThemeMode::System));
    }

    #[test]
    fn license_text_is_embedded_and_complete() {
        // AGPL-3.0 合规：许可证全文必须编译进二进制（设置页展示用），
        // 且是完整的官方文本而不是占位片段。
        let t = crate::LICENSE_TEXT;
        assert!(t.contains("GNU AFFERO GENERAL PUBLIC LICENSE"));
        assert!(t.contains("Version 3, 19 November 2007"));
        assert!(t.len() > 30_000, "许可证全文似乎不完整：{} 字节", t.len());
    }
}
