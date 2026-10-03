// ============================================================================
// interaction.rs —— 无头（headless）交互回归测试
//
// 目的：把「切页 → 选功能 → 参数区出现 → 跑完自动进日志页」
// 这条链路钉死。
//
// 本轮（多文件重构 + 底部导航栏回归）之后的关键约定：
//   * 所有状态/回调都在 AppState 全局（ui/state.slint），
//     测试通过 w.global::<AppState>() 读写；
//   * 底部导航栏回来了：current-tab 0=工具 1=日志 2=设置；
//   * 运行结束/开始自动切到日志页（旧版是自动弹日志弹窗）；
//   * 主题配色进了 Theme 全局。
//
// ★ 为什么所有运行时检查都挤在一个 #[test] 里：
//   winit 的事件循环一个进程只能建一次，而 libtest 给每个
//   #[test] 起独立线程 —— 所以运行时场景必须共用同一个窗口、
//   顺序执行。静态结构检查则不建窗口，任何环境都能跑。
//
// ★ 无图形后端时运行时测试会「跳过」而不是失败（环境限制）。
// ============================================================================

#![allow(non_snake_case)]

use pvz2_toolkit_android::ui::AppWindow;
use slint::{ComponentHandle, Model};

/// 让 invoke_from_event_loop 排队的回调真正跑掉。
fn tick() {
    let _ = slint::invoke_from_event_loop(|| {});
}

/// 把每个场景关心的状态复位，避免互相污染（窗口是共用的）。
fn reset(w: &AppWindow) {
    let app = w.global::<pvz2_toolkit_android::ui::AppState>();
    app.set_current_tab(0);
    app.set_open_select("".into());
    app.set_selected_id("".into());
    app.set_search_text("".into());
    app.set_has_unread_logs(false);
    app.invoke_search_changed("".into());
    tick();
}

// ============================================================================
// 运行时场景总入口
// ============================================================================
#[test]
fn 运行时场景总入口() {
    let w = match AppWindow::new() {
        Ok(w) => w,
        Err(e) => {
            eprintln!(
                "⚠ 跳过运行时交互检查：本环境没有可用的 Slint 平台后端（{e}）。\
                 需要 xvfb-run -a cargo test --features desktop --test interaction 才能跑。\
                 静态结构检查不受影响，仍然全部执行。"
            );
            return;
        }
    };
    // 两套接线都要装上，缺一不可
    pvz2_toolkit_android::runner::wire(&w);
    pvz2_toolkit_android::settings::wire(&w, &w.as_weak());

    let app = w.global::<pvz2_toolkit_android::ui::AppState>();

    // ---- 场景 1：列表本身要有内容，默认停在工具页 ----------------------------
    reset(&w);
    assert!(
        app.get_command_count() > 0,
        "命令列表为空 —— catalog::filter 或 refresh_list 有问题"
    );
    assert!(
        app.get_commands().row_count() > 0,
        "功能列表 commands 是空的 —— refresh_list 没生效"
    );
    assert_eq!(app.get_current_tab(), 0, "默认应该停在工具页");
    assert_eq!(app.get_selected_id().to_string(), "", "初始不该有选中功能");

    // ---- 场景 2：★ 选中一条命令 → selected-id 必须被设上 --------------------
    {
        let first = app.get_commands().row_data(0).expect("列表第一条取不到");
        let id = first.id.to_string();
        let name = first.name.to_string();
        assert!(!id.is_empty(), "第一条命令的 id 是空的");

        app.invoke_select_command(id.clone().into());
        tick();

        assert_eq!(
            app.get_selected_id().to_string(),
            id,
            "选中「{name}」后 selected-id 仍是空的 —— 参数区不会出现"
        );
        assert_eq!(app.get_selected_name().to_string(), name, "selected-name 没有被设置");
        assert!(
            !app.get_selected_desc().to_string().is_empty(),
            "selected-desc 是空的，功能说明卡会是空白"
        );
        assert_eq!(
            app.get_params().row_count(),
            first.param_count as usize,
            "参数条数 ({}) 与列表标注的 ({}) 不一致 —— build_form 映射漏了",
            app.get_params().row_count(),
            first.param_count
        );
        assert_eq!(
            app.get_last_result().to_string(),
            "",
            "选中新功能后 last-result 应当被清空"
        );
    }

    // ---- 场景 3：**每一条**命令都要能选中且参数数对得上 ----------------------
    {
        let model = app.get_commands();
        let n = model.row_count();
        for i in 0..n {
            let item = model.row_data(i).unwrap();
            let id = item.id.to_string();
            let name = item.name.to_string();

            app.set_selected_id("".into());
            app.invoke_select_command(id.clone().into());
            tick();

            assert_eq!(
                app.get_selected_id().to_string(),
                id,
                "第 {i} 条「{name}」({id}) 选中失败"
            );
            assert_eq!(
                app.get_params().row_count(),
                item.param_count as usize,
                "第 {i} 条「{name}」的参数条数不匹配（列表标注 {}，实际 {}）",
                item.param_count,
                app.get_params().row_count()
            );
        }
    }

    // ---- 场景 4：★ 底部导航栏切页（本轮回归的核心交互）----------------------
    {
        reset(&w);
        app.set_current_tab(2);
        tick();
        assert_eq!(app.get_current_tab(), 2, "切到设置页失败");

        app.set_current_tab(1);
        tick();
        assert_eq!(app.get_current_tab(), 1, "切到日志页失败");

        app.set_current_tab(0);
        tick();
        assert_eq!(app.get_current_tab(), 0, "切回工具页失败");
    }

    // ---- 场景 5：Monet 取色引擎（明暗两端 surface 必须拉开层次）--------------
    {
        let theme = w.global::<pvz2_toolkit_android::ui::Theme>();
        let monet = w.global::<pvz2_toolkit_android::ui::Monet>();

        let mean = |c: slint::Color| {
            (u32::from(c.red()) + u32::from(c.green()) + u32::from(c.blue())) / 3
        };

        // 浅色：Monet.dark = false → Theme.current 跟随翻转（声明式绑定）
        monet.set_dark(false);
        tick();
        let light = theme.get_current();
        let l_surface = mean(light.surface.color());
        assert!(light.on_surface != light.surface, "亮色 on-surface 与 surface 同色");

        // 深色：Monet.dark = true
        monet.set_dark(true);
        tick();
        let dark = theme.get_current();
        let d_surface = mean(dark.surface.color());
        assert!(dark.on_surface != dark.surface, "深色 on-surface 与 surface 同色");

        // 还原浅色（Monet.dark 与 Theme.effective-dark 同源）
        monet.set_dark(false);
        tick();

        assert!(
            l_surface > d_surface + 100,
            "亮色 surface (均值 {l_surface}) 应当显著亮于深色 surface (均值 {d_surface})"
        );

        assert_eq!(
            app.get_author().to_string(),
            "嘻嘻哈哈嘿嘿",
            "作者署名被改动了"
        );
        assert!(
            !app.get_theme_color().to_string().is_empty(),
            "theme-color 是空的"
        );
        // 预设色板拆两行注入（400dp 窄屏一行 12 个放不下），合计仍是 12
        assert_eq!(
            app.get_color_presets_row1().row_count()
                + app.get_color_presets_row2().row_count(),
            12,
            "预设色板应共 12 个种子色"
        );
    }

    // ---- 场景 6：明暗模式开关 ------------------------------------------------
    {
        app.invoke_set_theme_mode("dark".into());
        tick();
        assert_eq!(app.get_theme_mode().to_string(), "dark");
        assert!(
            w.global::<pvz2_toolkit_android::ui::Theme>().get_effective_dark(),
            "选「深色」后 effective-dark 应为 true"
        );

        app.invoke_set_theme_mode("light".into());
        tick();
        assert_eq!(app.get_theme_mode().to_string(), "light");

        app.invoke_set_theme_mode("system".into());
        tick();
        assert_eq!(app.get_theme_mode().to_string(), "system");
    }

    // ---- 场景 7：主题色（种子色）改动要能生效 --------------------------------
    {
        app.invoke_set_theme_color("#1976D2".into());
        tick();
        assert_eq!(
            app.get_theme_color().to_string(),
            "#1976D2",
            "合法的十六进制颜色没有被接受（或没有规范化成 6 位）"
        );

        app.invoke_set_theme_color("not-a-color".into());
        tick();
        assert_eq!(
            app.get_theme_color().to_string(),
            "#1976D2",
            "非法颜色被接受了 —— 输入校验失效"
        );

        // 收尾还原默认，避免影响后续场景的取色上下文
        app.invoke_set_theme_color("monet".into());
        tick();
        assert_eq!(app.get_theme_color().to_string(), "monet");
    }

    // ---- 场景 8：搜索过滤 ----------------------------------------------------
    {
        app.set_search_text("".into());
        app.invoke_search_changed("".into());
        tick();
        let all = app.get_command_count();

        app.set_search_text("RSB".into());
        app.invoke_search_changed("RSB".into());
        tick();
        let some = app.get_command_count();

        assert!(
            some < all && some > 0,
            "搜索「RSB」应当缩小结果集（全部 {all} 条，命中 {some} 条）"
        );

        app.set_search_text("".into());
        app.invoke_search_changed("".into());
        tick();
        assert_eq!(app.get_command_count(), all, "清空搜索后条数应当恢复");
    }

    // ---- 场景 9：★ 批量命令必须在列表里且有目录类参数 ------------------------
    {
        app.set_search_text("".into());
        app.invoke_search_changed("".into());
        tick();

        let model = app.get_commands();
        let ids: Vec<String> = (0..model.row_count())
            .filter_map(|i| model.row_data(i))
            .map(|c| c.id.to_string())
            .collect();

        for batch_id in ["ptx_encode_batch", "ptx_decode_batch"] {
            assert!(
                ids.iter().any(|i| i == batch_id),
                "找不到批量命令 {batch_id} —— catalog 里没注册，UI 就不会出现它"
            );
        }

        app.invoke_select_command("ptx_encode_batch".into());
        tick();
        let params = app.get_params();
        let input_kind = (0..params.row_count())
            .filter_map(|i| params.row_data(i))
            .find(|p| p.key.as_str() == "input")
            .map(|p| p.kind)
            .expect("ptx_encode_batch 找不到 input 参数");
        assert_eq!(
            input_kind, 1,
            "批量命令的 input 参数应当是目录类型（kind == 1），实际是 {input_kind}"
        );
    }

    // ---- 场景 10：★ 运行开始自动切到日志页 ----------------------------------
    //
    // 旧版是「运行结束自动弹日志弹窗」，本轮日志升级成了独立页面，
    // 自动弹出的等价物 = current-tab 被切到 1（日志）。
    // 这里不真跑命令（会写文件），只验证 start_run 的前置切页逻辑
    // 在 selected-id 为空时的守卫路径，以及未读标记的置位。
    {
        reset(&w);
        // selected-id 为空时 start_run 应当只记日志、不动页面
        app.invoke_run_command();
        tick();
        assert_eq!(app.get_current_tab(), 0, "未选功能时不应切页");
        assert!(!app.get_running(), "未选功能时不应进入运行态");
    }
}

// ============================================================================
// 静态检查 —— 不需要图形后端，cargo test 在纯命令行环境也能跑
// ============================================================================

/// 剥掉 // 行注释。
fn strip_comments(src: &str) -> String {
    src.lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// ★ 底部导航栏必须存在（用户本轮的核心诉求）。
#[test]
fn 底部导航栏必须存在且有三个目的地() {
    let app = strip_comments(include_str!("../ui/app.slint"));
    let nav = strip_comments(include_str!("../ui/components/navigation_bar.slint"));

    // 导航栏组件存在且被根窗口使用
    assert!(
        nav.contains("export component NavigationBar inherits Rectangle"),
        "components/navigation_bar.slint 里找不到 NavigationBar"
    );
    assert!(
        app.contains("NavigationBar {"),
        "app.slint 没有挂 NavigationBar —— 底部导航栏没了"
    );

    // 三个目的地
    for label in ["\"工具\"", "\"日志\"", "\"设置\""] {
        assert!(
            nav.contains(&format!("label: {label}")),
            "导航栏缺少目的地 {label}"
        );
    }

    // 切页回调要接上
    assert!(
        app.contains("AppState.current-tab = t;"),
        "导航栏的 changed 没有更新 AppState.current-tab，点了不会切页"
    );
}

/// 每个界面必须独立成文件 —— 「一个界面的累加」不许回来。
#[test]
fn 三个界面必须各自独立成文件() {
    let home = strip_comments(include_str!("../ui/screens/home_screen.slint"));
    let logs = strip_comments(include_str!("../ui/screens/logs_screen.slint"));
    let settings = strip_comments(include_str!("../ui/screens/settings_screen.slint"));

    assert!(
        home.contains("export component HomeScreen"),
        "home_screen.slint 里找不到 HomeScreen 组件"
    );
    assert!(
        logs.contains("export component LogsScreen"),
        "logs_screen.slint 里找不到 LogsScreen 组件"
    );
    assert!(
        settings.contains("export component SettingsScreen"),
        "settings_screen.slint 里找不到 SettingsScreen 组件"
    );

    // 根窗口按 current-tab 三选一
    let app = strip_comments(include_str!("../ui/app.slint"));
    assert!(app.contains("if (AppState.current-tab == 0) : HomeScreen"), "工具页没有按 current-tab 显示");
    assert!(app.contains("if (AppState.current-tab == 1) : LogsScreen"), "日志页没有按 current-tab 显示");
    assert!(app.contains("if (AppState.current-tab == 2) : SettingsScreen"), "设置页没有按 current-tab 显示");
}

/// 工具页：选择栏 + 参数区 + 常驻操作栏。
#[test]
fn 工具页必须有选择栏和常驻运行按钮() {
    let home = strip_comments(include_str!("../ui/screens/home_screen.slint"));
    let picker = strip_comments(include_str!("../ui/components/command_picker.slint"));
    let widgets = strip_comments(include_str!("../ui/theme/widgets.slint"));

    // 选择栏两件套存在
    assert!(picker.contains("export component PickerBar"), "找不到 PickerBar");
    assert!(picker.contains("export component PickerPanel"), "找不到 PickerPanel");

    // ★ 运行按钮必须「常驻」：操作栏的 y 定位是 parent.height - action-h，
    //   不依赖任何 if —— 选择面板浮层只能盖住参数区，不能挤走操作栏。
    assert!(
        home.contains("y: parent.height - root.action-h"),
        "操作栏不是钉在屏幕底部的（y: parent.height - root.action-h）——\n\
         会被内容挤出屏幕，用户就看不到运行按钮了"
    );
    assert!(
        home.contains("AppState.run-command()"),
        "运行按钮没有接 run-command 回调"
    );

    // 运行按钮 52px 大号（用户反馈过「运行按钮也好小」）
    // 2.1 起按钮换成官方 material Button（min-height 模式，外部可抬高度）
    assert!(
        home.contains("min-height: 52px"),
        "运行按钮没有用 52px 的大号尺寸"
    );

    // 选择面板必须以浮层形式定位（不占操作栏空间）
    assert!(
        home.contains(": PickerPanel {"),
        "选择面板没有用浮层（PickerPanel）—— 会把操作栏顶出屏幕"
    );

    // 参数表单组件化
    assert!(
        widgets.contains("export component OutlinedField"),
        "widgets.slint 里找不到 OutlinedField —— 紧凑输入框没了"
    );
}

/// 参数行必须紧凑：标签骑边框，不再有单独的「必填」标签行。
#[test]
fn 参数行必须紧凑且必填缩为星号() {
    let widgets = strip_comments(include_str!("../ui/theme/widgets.slint"));
    let form = strip_comments(include_str!("../ui/components/param_form.slint"));

    // OutlinedField 的标签骑在边框上（y 为负）
    let fld_at = widgets
        .find("export component OutlinedField inherits Rectangle")
        .expect("找不到 OutlinedField");
    let fld = &widgets[fld_at..];
    assert!(
        fld.contains("y: -8px"),
        "OutlinedField 的标签没有骑在边框上 —— 每个参数会多出一整行标签"
    );
    // 必填 = 星号
    assert!(
        fld.contains("text: \"*\""),
        "OutlinedField 缺少必填星号"
    );

    // 表单里不允许再出现整行的「必填/可选」标签
    assert!(
        !form.contains("p.required ? \"必填\" : \"可选\""),
        "param_form.slint 里还有「必填/可选」整行标签 —— 用户反馈它占了太多屏幕"
    );

    // 路径参数仍是手输框 + edited 回调
    assert!(
        form.contains("edited(v) => { AppState.param-value-changed(p.key, v); }"),
        "参数输入框没有接 edited 回调 —— 输入的值不会生效"
    );
}

/// 选择面板展开时必须可以单列滚动，且 TouchArea 声明顺序正确。
#[test]
fn 功能列表必须单列可滚动且命中区在内容之后() {
    let picker = strip_comments(include_str!("../ui/components/command_picker.slint"));

    assert!(
        picker.contains("ListView {"),
        "功能列表没有用 ListView —— 几十个功能需要虚拟化滚动"
    );
    assert!(
        picker.contains("for item in AppState.commands : Rectangle"),
        "找不到功能列表的渲染循环"
    );

    let list_at = picker
        .find("for item in AppState.commands : Rectangle")
        .expect("找不到功能列表循环");
    let row_block = &picker[list_at..(list_at + 4500).min(picker.len())];
    let touch_at = row_block
        .find("row-touch := TouchArea")
        .expect("功能列表行里找不到 row-touch := TouchArea");
    let layout_at = row_block
        .find("HorizontalLayout {")
        .expect("功能列表行里找不到内容 HorizontalLayout");
    assert!(
        touch_at > layout_at,
        "列表行的 TouchArea 被声明在内容之前 —— 点击会落空"
    );
}

/// 全局状态文件必须承载共享状态；组件不再逐个传 theme 属性。
#[test]
fn 状态必须收进全局且组件不再手传主题() {
    let state = strip_comments(include_str!("../ui/state.slint"));
    let widgets = strip_comments(include_str!("../ui/theme/widgets.slint"));

    assert!(
        state.contains("export global AppState"),
        "state.slint 里找不到 AppState 全局"
    );
    assert!(
        strip_comments(include_str!("../ui/theme/palette.slint")).contains("export global Theme"),
        "palette.slint 里找不到 Theme 全局"
    );

    // 新组件库不允许再出现 `in property <Palette> theme` 这种逐层传参
    assert!(
        !widgets.contains("in property <Palette> theme"),
        "widgets.slint 里还有逐层传主题的旧写法 —— 组件应当直接读 Theme 全局"
    );
}
