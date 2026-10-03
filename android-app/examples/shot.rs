// ============================================================================
// examples/shot.rs —— 桌面自动截图（视觉走查用）
//
// 用法：
//   SHOT_DIR=/tmp/shots SLINT_BACKEND=software \
//     xvfb-run -a cargo run --features desktop --example shot
//
// 场景脚本：交错「改状态 → 下一拍截图」，保证截图前至少渲染过一帧
// （take_snapshot 需要窗口处于已渲染状态）。覆盖：
//   明/暗 × 工具/设置/日志三页 × baseline 与自定义主题色（蓝）。
// ============================================================================

use pvz2_toolkit_android::ui::{AppWindow, AppState};
use slint::{ComponentHandle, Timer, TimerMode};

fn shot(w: &AppWindow, name: &str, dir: &str) {
    match w.window().take_snapshot() {
        Ok(buf) => {
            let img = image::RgbaImage::from_raw(
                buf.width(),
                buf.height(),
                buf.as_bytes().to_vec(),
            )
            .expect("像素缓冲尺寸不匹配");
            let path = format!("{dir}/{name}.png");
            match img.save(&path) {
                Ok(_) => eprintln!("✓ {path} ({}x{})", img.width(), img.height()),
                Err(e) => eprintln!("✗ 写 {path} 失败：{e}"),
            }
        }
        Err(e) => eprintln!("✗ take_snapshot({name}) 失败：{e}"),
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let out_dir = std::env::var("SHOT_DIR").unwrap_or_else(|_| "/tmp/shots".into());
    std::fs::create_dir_all(&out_dir).unwrap();

    let window = AppWindow::new()?;
    pvz2_toolkit_android::runner::wire(&window);
    pvz2_toolkit_android::settings::wire(&window, &window.as_weak());
    {
        let app = window.global::<AppState>();
        app.set_license_text(pvz2_toolkit_android::LICENSE_TEXT.into());
    }
    // 手机比例窗口（逻辑尺寸走 preferred，物理给 412x860 的两倍方便看清）
    window.show()?;

    let timer = std::rc::Rc::new(Timer::default());
    let weak = window.as_weak();
    let step = std::cell::Cell::new(0u32);
    let dir = out_dir.clone();
    timer.start(
        TimerMode::Repeated,
        std::time::Duration::from_millis(350),
        move || {
            let Some(w) = weak.upgrade() else { return };
            let s = step.get();
            step.set(s + 1);
            let app = w.global::<AppState>();
            match s {
                // —— 明色 baseline ——
                0 => {
                    app.set_current_tab(0);
                }
                1 => {
                    // 诊断：inject 后 Monet 各系取值
                    let m = w.global::<pvz2_toolkit_android::ui::Monet>();
                    let hex = |br: slint::Brush| match br {
                        slint::Brush::SolidColor(c) => format!("#{:08X}", c.as_argb_encoded()),
                        _ => "non-solid".into(),
                    };
                    eprintln!(
                        "DEBUG dark={} surface={} sc-low={} sc={} secondary-container={} primary={}",
                        m.get_dark(),
                        hex(m.get_surface()),
                        hex(m.get_surface_container_low()),
                        hex(m.get_surface_container()),
                        hex(m.get_secondary_container()),
                        hex(m.get_primary()),
                    );
                    shot(&w, "01-home-light-baseline", &dir);
                }
                2 => {
                    app.set_current_tab(2);
                }
                3 => shot(&w, "02-settings-light-baseline", &dir),
                // —— 明色自定义蓝（整套 Monet 变色验证）——
                4 => {
                    app.invoke_set_theme_color("#1565C0".into());
                }
                5 => shot(&w, "03-settings-light-blue", &dir),
                6 => {
                    app.set_current_tab(0);
                }
                7 => shot(&w, "04-home-light-blue", &dir),
                // —— 暗色 ——
                8 => {
                    app.invoke_set_theme_mode("dark".into());
                }
                9 => shot(&w, "05-home-dark-blue", &dir),
                10 => {
                    app.set_current_tab(2);
                }
                11 => shot(&w, "06-settings-dark-blue", &dir),
                12 => {
                    app.set_current_tab(1);
                }
                13 => shot(&w, "07-logs-dark-blue", &dir),
                // —— 收尾：还原明色 baseline，避免污染后续运行 ——
                14 => {
                    app.invoke_set_theme_mode("system".into());
                    app.invoke_set_theme_color("monet".into());
                    app.set_current_tab(0);
                    let _ = slint::quit_event_loop();
                }
                _ => {
                    let _ = slint::quit_event_loop();
                }
            }
        },
    );

    window.run()?;
    Ok(())
}
