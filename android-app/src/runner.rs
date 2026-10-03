// ============================================================================
// runner.rs —— 命令执行器
//
// 命令是同步、阻塞、可能跑很久的（解包一个大 RSB 要几十秒）。
// 直接在 UI 线程跑会让界面完全卡死，所以必须丢到后台线程。
//
// 但 Slint 的属性只能在事件循环线程上改，所以后台线程不能碰窗口，
// 只能通过 invoke_from_event_loop 把结果「投递」回来。
//
// 这个模块负责把这三件事串起来：
//   1. 从 UI 收集表单值 → 组装 Params
//   2. 起后台线程执行 → 期间持续把日志投回来
//   3. 结束后把结果状态、耗时、是否成功写回 UI
//
// ★ 本轮对接方式的变化：所有状态/回调都收进了 AppState 全局
//   （ui/state.slint），不再散落在 AppWindow 根组件上。
//   `window.global::<AppState>()` 拿到全局句柄，
//   setter / getter / invoke 的用法跟旧版窗口属性一一对应。
// ============================================================================

use crate::catalog::{self, ParamKind};
use crate::params::Params;
use crate::ui::{AppWindow, AppState};
use slint::{ComponentHandle, Model};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// 是否有任务正在跑（防止用户连点「开始」起一堆线程）
static RUNNING: AtomicBool = AtomicBool::new(false);

/// 请求取消。命令内部不检查这个标志（它们是一次性的纯函数），
/// 所以取消的语义是「不再把后续日志投回 UI，并尽快收尾」。
static CANCELLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// 文本类参数的「最新输入值」。
    ///
    /// ★ 为什么不把文本输入直接写回 params 模型：每次 edited 都
    /// 重建整个 VecModel 的话，`for` 委托里的 TextInput 会被
    /// 销毁重建 —— 焦点丢失、Android 上软键盘收起，用户每打
    /// 一个字都要重新点一下输入框。这是用户实际报过的 bug
    /// （「我每次在文本框里输入一个字，它就断了」）。
    ///
    /// 所以文本输入只更新这张表，模型**完全不动**；
    /// collect() 时以这里的值为最高优先级。
    /// 换功能时整表清空（见 build_form），避免旧值串进新表单。
    static FORM_VALUES: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

pub fn is_running() -> bool {
    RUNNING.load(Ordering::Relaxed)
}

/// 接好 UI 回调。
pub fn wire(window: &AppWindow) {
    let app = window.global::<AppState>();

    // --- 搜索框 ---
    {
        let weak = window.as_weak();
        app.on_search_changed(move |_q| {
            if let Some(w) = weak.upgrade() {
                refresh_list(&w);
            }
        });
    }

    // --- 选中某条命令 → 生成参数表单 ---
    //
    // 界面侧（command_picker.slint）在点击列表行时已经把
    // 选择面板收起（AppState.open-select = ""），这里只管填数据。
    {
        let weak = window.as_weak();
        app.on_select_command(move |id| {
            if let Some(w) = weak.upgrade() {
                build_form(&w, &id);
                // 选了新功能就清掉上次的输出摘要，
                // 免得用户把它当成这次的结果
                w.global::<AppState>().set_last_result("".into());
                // 新表单不允许带着旧的下拉展开态
                w.global::<AppState>().set_open_select("".into());
            }
        });
    }

    // --- 参数值变化 ---
    {
        let weak = window.as_weak();
        app.on_param_value_changed(move |key, value| {
            if let Some(w) = weak.upgrade() {
                update_param(&w, &key, &value);
            }
        });
    }

    // --- 开始执行 ---
    {
        let weak = window.as_weak();
        app.on_run_command(move || {
            if let Some(w) = weak.upgrade() {
                start_run(&w);
            }
        });
    }

    // --- 清空日志（日志页的按钮）---
    {
        let weak = window.as_weak();
        app.on_clear_log(move || {
            if let Some(w) = weak.upgrade() {
                w.global::<AppState>().set_log_text("".into());
            }
        });
    }

    // --- 取消 ---
    {
        let weak = window.as_weak();
        app.on_cancel_run(move || {
            CANCELLED.store(true, Ordering::Relaxed);
            crate::log_line!("已请求取消——当前文件处理完就会停下。");
            if let Some(w) = weak.upgrade() {
                w.global::<AppState>().set_running(false);
            }
        });
    }

    refresh_list(window);
}

/// 按当前搜索词重建命令列表。
pub fn refresh_list(window: &AppWindow) {
    let app = window.global::<AppState>();
    let q = app.get_search_text().to_string();
    let hits = catalog::filter(&q, "全部");

    let items: Vec<crate::ui::CommandItem> = hits
        .iter()
        .map(|c| crate::ui::CommandItem {
            id: c.id.into(),
            name: c.name.into(),
            category: c.category.into(),
            desc: c.desc.into(),
            param_count: c.params.len() as i32,
        })
        .collect();

    app.set_command_count(items.len() as i32);

    // VecModel::from(Vec<T>) 得到 VecModel，再包成 ModelRc。
    // 直接 ModelRc::from(Vec<T>) 是不支持的。
    app.set_commands(slint::ModelRc::new(slint::VecModel::from(items)));
}

/// 为选中的命令生成参数表单。
pub fn build_form(window: &AppWindow, id: &str) {
    let Some(cmd) = catalog::find(id) else {
        crate::log_line!("内部错误：找不到命令 {id}");
        return;
    };

    // 换功能就清掉上一功能的手输值 ——
    // 不同功能常有同名参数（input / output），
    // 不清的话旧值会悄悄串进新表单。
    FORM_VALUES.with(|m| m.borrow_mut().clear());

    let fields: Vec<crate::ui::ParamField> = cmd
        .params
        .iter()
        .map(|p| crate::ui::ParamField {
            key: p.key.into(),
            label: p.label.into(),
            kind: kind_to_int(p.kind),
            value: p.default.into(),
            hint: p.hint.into(),
            required: p.required,
            choices: {
                // 显式构造 VecModel 再包成 ModelRc ——
                // ModelRc 没有 From<Vec<T>> 的实现，只有 From<T>（单个模型）
                let v: Vec<slint::SharedString> =
                    p.choices.iter().map(|c| (*c).into()).collect();
                slint::ModelRc::new(slint::VecModel::from(v))
            },
            choice_index: p
                .choices
                .iter()
                .position(|c| *c == p.default)
                .map(|i| i as i32)
                .unwrap_or(-1),
        })
        .collect();

    let app = window.global::<AppState>();
    app.set_selected_id(cmd.id.into());
    app.set_selected_name(cmd.name.into());
    app.set_selected_desc(cmd.desc.into());
    app.set_params(slint::ModelRc::new(slint::VecModel::from(fields)));
}

fn kind_to_int(k: ParamKind) -> i32 {
    match k {
        ParamKind::InputFile => 0,
        ParamKind::InputDir => 1,
        ParamKind::OutputFile => 2,
        ParamKind::OutputDir => 3,
        ParamKind::Bool => 4,
        ParamKind::Int => 5,
        ParamKind::Text => 6,
        ParamKind::Choice => 7,
    }
}

/// 更新某个参数的值。
///
/// ★ 这是「每打一个字就失焦」bug 的修复点。旧实现每次 edited
/// 都 `set_params(新 VecModel)` —— 整个模型被替换，`for` 委托
/// 全部销毁重建，正在打字的 TextInput 连同焦点一起被销毁。
/// 现在文本值只落 FORM_VALUES，模型不碰；只有开关 / 下拉
/// （它们没有「正在打字」的状态）才做**单行** set_row_data 更新。
fn update_param(window: &AppWindow, key: &str, value: &str) {
    // 1) 文本值总是先落到 FORM_VALUES —— collect() 的第一优先级
    FORM_VALUES.with(|m| {
        m.borrow_mut().insert(key.to_string(), value.to_string());
    });

    // 2) 开关(4) / 下拉(7) 把新值立刻同步到模型（显示状态从模型读）。
    //    ★ 只更新匹配的那一行，绝不重建模型。
    let app = window.global::<AppState>();
    let model = app.get_params();
    for i in 0..model.row_count() {
        let Some(f) = model.row_data(i) else { continue };
        if f.key.as_str() != key {
            continue;
        }
        if f.kind == 4 || f.kind == 7 {
            let mut nf = f.clone();
            nf.value = value.into();
            if nf.kind == 7 {
                // 顺手把 choice_index 也对上 —— 只改 value 不改
                // 索引的话，collect() 按索引取值时会拿默认项覆盖
                // 用户实际选中的项。
                nf.choice_index = (0..nf.choices.row_count())
                    .filter_map(|j| nf.choices.row_data(j))
                    .position(|c| c.as_str() == value)
                    .map(|j| j as i32)
                    .unwrap_or(nf.choice_index);
            }
            if let Some(vm) = model
                .as_any()
                .downcast_ref::<slint::VecModel<crate::ui::ParamField>>()
            {
                vm.set_row_data(i, nf);
            }
        }
        break;
    }
}

/// 收集表单值。
fn collect(window: &AppWindow) -> Params {
    let app = window.global::<AppState>();
    let overrides = FORM_VALUES.with(|m| m.borrow().clone());
    let model = app.get_params();
    let pairs: Vec<(String, String)> = (0..model.row_count())
        .filter_map(|i| model.row_data(i))
        .map(|f| {
            // 用户手动改过的值优先 —— 文本输入不回写模型（防失焦），
            // 这里才是它真正生效的地方。
            if let Some(v) = overrides.get(f.key.as_str()) {
                return (f.key.to_string(), v.clone());
            }
            // 下拉框：把索引还原成候选项文本
            let value = if f.kind == 7 && f.choice_index >= 0 {
                f.choices
                    .row_data(f.choice_index as usize)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| f.value.to_string())
            } else {
                f.value.to_string()
            };
            (f.key.to_string(), value)
        })
        .collect();
    Params::new(pairs)
}

/// 开始执行当前命令。
pub fn start_run(window: &AppWindow) {
    let app = window.global::<AppState>();
    let id = app.get_selected_id().to_string();
    if id.is_empty() {
        crate::log_line!("请先在列表里选一个功能。");
        return;
    }

    if RUNNING.swap(true, Ordering::SeqCst) {
        crate::log_line!("已有任务在跑，请等它结束或点「取消」。");
        return;
    }
    CANCELLED.store(false, Ordering::Relaxed);

    let ps = collect(window);
    let name = app.get_selected_name().to_string();

    app.set_running(true);
    app.set_last_ok(false);
    app.set_last_result("".into());
    app.set_status_text(format!("正在执行：{name}…").into());
    // 切到日志页看进度 —— 等价于旧版「运行开始自动弹日志弹窗」，
    // 只是载体从弹窗换成了底部导航栏的日志页。
    app.set_current_tab(1);
    app.set_has_unread_logs(false);

    let weak = window.as_weak();

    // 日志面板用纯文本累加。行数多了会拖慢渲染，
    // 超过阈值就从头截掉一部分。
    const MAX_LINES: usize = 2000;

    std::thread::spawn(move || {
        let started = Instant::now();
        crate::log_line!("──────────────────────────────");
        crate::log_line!("▶ 开始：{name}");

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::commands::dispatch(&id, &ps)
        }));

        let elapsed = started.elapsed();
        let cancelled = CANCELLED.load(Ordering::Relaxed);

        let (ok, summary) = match result {
            Ok(Ok(msg)) => (true, msg),
            Ok(Err(e)) => (false, format!("{e}")),
            Err(p) => {
                let msg = if let Some(s) = p.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = p.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "(内部错误)".to_string()
                };
                (false, format!("程序内部出错：{msg}"))
            }
        };

        // 按量级切换单位
        let dur = if elapsed.as_secs() >= 1 {
            format!("{:.1} 秒", elapsed.as_secs_f64())
        } else {
            format!("{} 毫秒", elapsed.as_millis())
        };

        if ok {
            crate::log_line!("✓ 完成：{summary}　（耗时 {dur}）");
        } else {
            crate::log_line!("✗ 失败：{summary}　（耗时 {dur}）");
        }
        if cancelled {
            crate::log_line!("（用户请求了取消）");
        }
        crate::log_line!("──────────────────────────────");

        RUNNING.store(false, Ordering::SeqCst);
        // 强制投递最终缓冲 —— 节流模式下最后一行可能还攒着没上屏
        crate::flush_log();

        let _ = slint::invoke_from_event_loop(move || {
            if let Some(w) = weak.upgrade() {
                let app = w.global::<AppState>();
                app.set_running(false);
                app.set_last_ok(ok);
                app.set_status_text(
                    if ok {
                        format!("完成：{summary}")
                    } else {
                        format!("失败：{summary}")
                    }
                    .into(),
                );
                // 工具页操作栏上方那一行摘要。只取首行 ——
                // 完整内容在日志页里。
                let one_line = summary.lines().next().unwrap_or("").to_string();
                app.set_last_result(one_line.into());

                // 截断过长的日志
                let text = app.get_log_text().to_string();
                let lines: Vec<&str> = text.lines().collect();
                if lines.len() > MAX_LINES {
                    let kept = lines[lines.len() - MAX_LINES..].join("\n");
                    app.set_log_text(kept.into());
                }

                // 记一下用户这次用的目录，下次选文件直接跳过去
                if ok {
                    remember_dirs();
                }
            }
        });
    });
}

/// 把本次用到的输入/输出目录存进设置，方便下次开始时定位。
fn remember_dirs() {
    // 逻辑不复杂，出错也不影响主流程，全部忽略错误。
    let mut s = crate::settings::current();
    let changed = if let Some(home) = dirs_candidate() {
        if s.last_input_dir != home {
            s.last_input_dir = home.clone();
            true
        } else {
            false
        }
    } else {
        false
    };
    if changed {
        let _ = crate::settings::save(&s);
    }
}

/// 猜一个合理的「上次目录」。Android 上用外部存储根，
/// 桌面用当前目录。
fn dirs_candidate() -> Option<String> {
    #[cfg(target_os = "android")]
    {
        crate::platform::external_storage_dir()
    }
    #[cfg(not(target_os = "android"))]
    {
        std::env::current_dir()
            .ok()
            .map(|p| p.display().to_string())
    }
}
