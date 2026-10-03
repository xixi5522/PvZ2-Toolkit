// ============================================================================
// lib.rs —— Android 入口 + 桌面入口 + 模块装配
//
// 这个文件是「一套代码，两处编译」的关键：
//
//   Android：NativeActivity 通过 dlsym 找 ANativeActivity_onCreate
//            （由 android-activity crate 提供），它再调进 android_main。
//            我们先 slint::android::init(app) 装好 Slint 的 Android 后端，
//            然后跑 UI。
//
//   桌面：  直接 main() 跑 UI，方便在开发机上调试界面。
//
// 和 C++ 版最大的差别就在这里：
//   slint-cpp 没有 Android 后端，得靠一个 Rust shim 去补；
//   而 slint（Rust）本身就是后端的所有者，一行 init 就能用。
// ============================================================================

pub mod catalog;
pub mod colors;
pub mod commands;
pub mod params;
pub mod runner;
pub mod settings;

/// AGPL-3.0 许可证全文（编译期嵌入，二进制里自带完整许可文本）。
pub const LICENSE_TEXT: &str = include_str!("../../LICENSE");

pub mod ui {
    slint::include_modules!();
}

// `ComponentHandle` 必须显式 use —— `as_weak()` / `run()` 都是它的方法，
// 不在 trait 作用域里就调不到（编译器给的报错会很误导，像是方法不存在）。
use slint::ComponentHandle;

#[cfg(target_os = "android")]
pub mod platform;

// ============================================================================
// 日志管线
//
// Android 上没有 stdout，println! 的内容会直接丢掉。所以所有进度的输出
// 都经这里转到 UI 的日志面板（桌面调试时也同时打到 stderr）。
//
// 日志文本由 Rust 侧用一个 String 全权累积，每次把完整内容 set 给 UI。
// 原因是 Slint 侧没有可被 Rust 调用的「追加」入口（public function 不导出），
// 而且累加本身是有状态的，放 Rust 里最直观。
//
// 行数上限的存在是因为日志面板是一个 Text 元素，
// 内容长了会拖慢布局与渲染，几千行足够看，多了就从头截掉。
// ============================================================================
use std::sync::{Mutex, OnceLock};

/// 日志缓冲区
static LOG_BUFFER: Mutex<Option<String>> = Mutex::new(None);
/// 日志投递点（把累积后的文本推给 UI）
type LogSink = Box<dyn Fn(&str) + Send + Sync + 'static>;
static LOG_SINK: OnceLock<Mutex<Option<LogSink>>> = OnceLock::new();

const LOG_MAX_LINES: usize = 3000;

/// 日志节流间隔。
///
/// ★ 2.1 优化：原来每行日志都 clone 全量缓冲并投给 UI（set_log_text
/// 触发整段文本重排），批量解包几千个文件时是 O(n²) —— UI 比磁盘还忙。
/// 现在缓冲照常追加，但投递按 120ms 节流；任务收尾时 flush_log()
/// 强制投一次，最后一行不会丢。
const LOG_FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(120);
static LAST_FLUSH: Mutex<Option<std::time::Instant>> = Mutex::new(None);

/// 安装日志投递点。由 UI 层在启动时调用一次。
pub fn set_log_sink(f: impl Fn(&str) + Send + Sync + 'static) {
    let slot = LOG_SINK.get_or_init(|| Mutex::new(None));
    if let Ok(mut g) = slot.lock() {
        *g = Some(Box::new(f));
    }
}

/// 清空日志缓冲。
pub fn clear_log_buffer() {
    if let Ok(mut g) = LOG_BUFFER.lock() {
        *g = Some(String::new());
    }
}

/// 给命令实现用的日志宏。
///
/// 用法和 println! 完全一样，但会走到 UI 日志面板。
#[macro_export]
macro_rules! log_line {
    () => { $crate::log_line!("") };
    ($($arg:tt)*) => {{
        let s = format!($($arg)*);
        $crate::emit_log(&s);
    }};
}

/// 内部：把一行日志追加到缓冲区，并按节流间隔投递给 UI。
#[doc(hidden)]
pub fn emit_log(line: &str) {
    #[cfg(not(target_os = "android"))]
    eprintln!("{line}");

    // 1) 追加到缓冲区（截断逻辑同旧版）
    {
        let mut g = match LOG_BUFFER.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let buf = g.get_or_insert_with(String::new);
        if !buf.is_empty() {
            buf.push('\n');
        }
        buf.push_str(line);

        // 超长就从头截掉，只保留最后 LOG_MAX_LINES 行
        let lines = buf.matches('\n').count();
        if lines > LOG_MAX_LINES {
            let cut = lines - LOG_MAX_LINES;
            if let Some(pos) = buf
                .match_indices('\n')
                .nth(cut - 1)
                .map(|(i, _)| i + 1)
            {
                *buf = buf[pos..].to_string();
            }
        }
    }

    // 2) 节流投递：120ms 内的后续行攒着，到点把整份快照推给 UI
    let now = std::time::Instant::now();
    let due = {
        let mut last = match LAST_FLUSH.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        match *last {
            Some(t) if now.duration_since(t) < LOG_FLUSH_INTERVAL => false,
            _ => {
                *last = Some(now);
                true
            }
        }
    };
    if due {
        flush_log();
    }
}

/// 强制把当前缓冲投递给 UI（任务收尾时调用，保证最后一行不丢）。
pub fn flush_log() {
    let full = {
        let mut g = match LOG_BUFFER.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        g.get_or_insert_with(String::new).clone()
    };
    if let Ok(mut last) = LAST_FLUSH.lock() {
        *last = Some(std::time::Instant::now());
    }

    let slot = LOG_SINK.get_or_init(|| Mutex::new(None));
    if let Ok(g) = slot.lock() {
        if let Some(f) = g.as_ref() {
            f(&full);
        }
    }
}

// ============================================================================
// 主窗口装配
// ============================================================================
pub fn run() -> Result<(), slint::PlatformError> {
    // 注意：Slint 的 Rust API 是 `AppWindow::new()`，
    // 不是 C++ 那边的 `AppWindow::create()`。两者名字不同。
    let window = ui::AppWindow::new()?;

    // 把日志管线接到 UI 上（AppState 全局的 log-text 属性）。
    // 用 Weak 避免闭包把窗口引用形成环。
    {
        let weak = window.as_weak();
        set_log_sink(move |full_text| {
            // 从后台线程推 UI 必须走 invoke_from_event_loop，
            // 直接改属性会触发 Slint 的线程检查而 panic。
            let text = full_text.to_string();
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(w) = weak.upgrade() {
                    let app = w.global::<ui::AppState>();
                    app.set_log_text(text.into());
                    // 用户不在日志页时来了新日志 → 导航栏红点提醒
                    if app.get_current_tab() != 1 {
                        app.set_has_unread_logs(true);
                    }
                }
            });
        });
    }

    runner::wire(&window);
    settings::wire(&window, &window.as_weak());

    // AGPL-3.0 合规要素：许可证全文编译期打包进应用，
    // 设置页 →「开源致谢与许可证全文」浮层里滚动展示。
    {
        let app = window.global::<ui::AppState>();
        app.set_license_text(LICENSE_TEXT.into());
    }

    // ----------------------------------------------------------------------------
    // 窗口 / Insets 诊断（排查「底部导航栏被系统栏挡住」）
    //
    // 启动 2 秒后（布局与 inset 上报都应已稳定）把三组数据写进日志页：
    //   1. surface 物理尺寸与 scale —— 换算出的逻辑尺寸就是 UI 布局空间
    //   2. Window.safe-area-insets —— 后端上报的系统栏避让量
    //   3. 若逻辑高度接近 preferred(860) 而 safe-area 全 0、真机却仍被
    //      遮挡，说明后端 resize/inset 链路没跑起来，需要换别的避让方案。
    // ----------------------------------------------------------------------------
    {
        let weak = window.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_secs(2), move || {
            let Some(w) = weak.upgrade() else { return };
            let scale = w.window().scale_factor();
            let phys = w.window().size();
            let msg = format!(
                "窗口诊断：物理 {}x{}，scale {:.2}，逻辑 {:.0}x{:.0}，safe-area top={}px bottom={}px",
                phys.width,
                phys.height,
                scale,
                phys.width as f64 / f64::from(scale),
                phys.height as f64 / f64::from(scale),
                w.get_dbg_safe_top(),
                w.get_dbg_safe_bottom()
            );
            // 记录到全局缓冲（桌面顺带打印）；注意不能在这里依赖
            // invoke_from_event_loop 把它转回 UI —— Timer 回调本来就
            // 运行在事件循环线程上，个别后端对「循环内再投递」的重入
            // 处理不可靠，直接同步 set 最稳。
            crate::emit_log(&msg);
            w.global::<ui::AppState>().set_log_text(msg.into());
        });
    }

    window.run()
}

// ============================================================================
// 字体
//
// 字体通过 .slint 里的 @font-face 注册（见 ui/app.slint 顶部），
// 由 Slint 编译器自动生成 register_font_from_path / by_memory 调用。
// 不需要在这里手写代码 —— 而且也不该手写：
// Renderer::register_font_from_memory 只在 internal 层面公开，
// 从应用侧调用要走 platform API，各后端行为不一致。
// 用 @font-face 则两端（Skia / software）都由 Slint 自己保证。
//
// 为什么需要自带字体：
//   * Android：Skia 的 FontMgr 会读 /system/etc/fonts.xml，正常设备上
//     能找到中文字体。但精简 ROM 可能缺 CJK，中文会渲染成空白方块。
//     自带一份完整的 CJK 子集（21090 个字形，约 10.2MB）就能兜住。
//     注意不能用「按本应用出现的字符裁剪」的小子集 ——
//     Rust 侧动态 set 的文本不在编译期字符集里，运行到就缺字，
//     详见 docs/FONT-NOTES.md。
//   * 桌面：开发机 fontconfig 没配好时，默认字体族匹配不到任何字体，
//     渲染结果就是「有布局、没字」。
// ============================================================================

// ============================================================================
// Android 入口
// ============================================================================
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: slint::android::AndroidApp) {
    // 把 JavaVM / Activity 存起来，后面 JNI 层（权限、Toast、Prefs）要用
    crate::platform::init(&app);

    init_logging();

    log::info!("android_main: 启动");

    if let Err(e) = slint::android::init(app) {
        log::error!("Slint Android 后端初始化失败: {e:?}");
        return;
    }
    log::info!("android_main: 后端就绪");

    // catch_unwind 能生效的前提是 profile 里 panic = "unwind"（见根 Cargo.toml）。
    // 用 "abort" 的话，这里根本不会走到 Err 分支，进程直接没了。
    match std::panic::catch_unwind(|| run()) {
        Ok(Ok(())) => log::info!("android_main: 正常退出"),
        Ok(Err(e)) => log::error!("android_main: 运行出错 {e:?}"),
        Err(p) => log::error!("android_main: panic: {}", panic_msg(&p)),
    }
}

#[cfg(target_os = "android")]
fn init_logging() {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("Pvz2Toolkit"),
    );
}

#[cfg(target_os = "android")]
fn panic_msg(p: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "(非字符串 panic)".into()
    }
}

// ============================================================================
// 桌面入口
//
// 就是为了方便在开发机上 cargo run 调界面，不用每次都推到手机上。
// 注意桌面需要 backend-default；Android 构建时走的是另一个 target，
// 不会互相影响。
// ============================================================================
#[cfg(not(target_os = "android"))]
pub fn run_desktop() -> Result<(), slint::PlatformError> {
    run()
}
