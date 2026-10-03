// ============================================================================
// build.rs —— 把 ui/*.slint 编译成 Rust 代码
//
// 关键点一：with_style("material")
//
//   Slint 的「样式」（style）决定了 std-widgets 里 Button/CheckBox/Slider
//   这些内置控件长什么样。可选值：fluent / material / cupertino / cosmic /
//   qt / native。
//
//   这不是 cargo feature，而是在编译 .slint 时通过 CompilerConfiguration
//   传给 slint-compiler 的 —— 所以必须写在这里，不能写在 Cargo.toml。
//
//   选了 "material" 之后，.slint 里的
//       import { Button, CheckBox } from "std-widgets.slint";
//   拿到的就是 Material Design 3 版本，自动跟随 Theme.color-scheme
//   和 Palette（明暗模式）变化。官方实现源码在
//       third_party/slint/internal/compiler/widgets/material/。
//
// 关键点二：embed_resources
//
//   这个选项决定「.slint 里 import 的外部资源（字体、图片）怎么进二进制」。
//   三个可选值：
//     AsAbsolutePath          —— 只写绝对路径，运行时按路径读文件
//     EmbedFiles              —— 把文件字节 include 进来，运行时按时注册
//     EmbedForSoftwareRenderer—— 把字体的字形位图在编译期就烘焙好
//
//   ★ 必须用 EmbedFiles，不能用 EmbedForSoftwareRenderer：
//
//     EmbedForSoftwareRenderer 只烘焙「编译期看得见的字形」，也就是
//     .slint 文件里字面写出来的那些字符。我们的界面里大量文本来自
//     Rust 侧运行时 set 进去的字符串（命令名、描述、日志、错误信息），
//     编译期完全看不到 —— 这些字符没有字形，渲染出来就是空白。
//
//     实测：用 EmbedForSoftwareRenderer 时生成的 BitmapFont 只有
//     253 个字形（正好等于 .slint 里出现的不同字符数），
//     界面上命令名/描述大面积缺字。
//
//     EmbedFiles 则把整份字体字节 include 进来，运行时按 family 名注册，
//     任何字符都能渲染。代价是 so 体积 +约 5MB。
//
//   ★ 也不能用 AsAbsolutePath：
//
//     它生成的是
//         register_font_from_path(&PathBuf::from("/编译机/绝对/路径.otf"))
//     这个路径在 Android 设备上不存在，而 Slint 生成的调用末尾是
//     .unwrap() —— 一启动就 panic。
//
//   所以 EmbedFiles 是唯一同时满足「Android 可用」和「动态文本有字形」的选择。
//
// 环境变量 STYLE 可以覆盖，方便临时切样式对比。
// ============================================================================

use slint_build::EmbedResourcesKind;

fn main() {
    let style = std::env::var("STYLE").unwrap_or_else(|_| "material".to_string());

    // ------------------------------------------------------------------------
    // SLINT_DEFAULT_FONT —— 编译期的「默认字体」来源
    //
    // 这是必需的，不是可选优化。原因在 i-slint-common/sharedfontdb.rs 的
    // init_fontdb()：
    //
    //   * Linux（编译机）：走 load_system_fonts()，然后用 fontconfig 去找
    //     "sans-serif" 这个通用族名，设成默认。但 fontconfig 是通过
    //     dlopen("libfontconfig.so.1") 动态加载的 —— 某些精简容器里
    //     这个查找会失败，接着 font_db.query(SansSerif) 返回 None，
    //     于是 embed_glyphs 报：
    //         internal error: fontdb could not determine a default font for sans-serif
    //
    //   * Android（交叉编译目标）：代码里直接把 fontconfig 那段 cfg 掉了，
    //     只尝试 load_fonts_dir("/system/fonts") —— 那是**设备**上的路径，
    //     编译机上根本不存在。所以 Android 构建时编译器的 fontdb 里
    //     一个可用的 sans-serif 都没有，必然报同样的错。
    //
    // SLINT_DEFAULT_FONT 在 init_fontdb 的最前面被检查，优先级最高：
    // 只要它指向的文件能加载，就直接拿它的所有字族当默认族，
    // 后面那些系统字体/fontconfig 的探测全部跳过。
    //
    // 指到我们自己打包的字体子集最合适：既解决了编译期报错，
    // 又保证「编译期参与排版选择的字体」和「运行时实际渲染的字体」
    // 是同一个，字形度量不会在编译期和运行期之间出现偏差。
    //
    // 用 set_var 直接写进当前进程环境 —— build.rs 和 slint-compiler
    // 在同一个进程里跑（compile_with_config 是同进程调用），所以能生效。
    // ------------------------------------------------------------------------
    let font_path = std::path::Path::new("fonts/NotoSansSC-Regular.otf");
    if font_path.exists() {
        let abs = std::fs::canonicalize(font_path).unwrap_or_else(|_| font_path.into());
        // SAFETY: build.rs 是单线程的，此处设置环境变量不会与其他线程竞争
        unsafe { std::env::set_var("SLINT_DEFAULT_FONT", &abs) };
        println!("cargo:warning=SLINT_DEFAULT_FONT = {}", abs.display());
    } else {
        println!(
            "cargo:warning=未找到 {}，将回退到系统字体；\
             若编译报 \"could not determine a default font for sans-serif\"，\
             请确认该文件存在",
            font_path.display()
        );
    }

    // ------------------------------------------------------------------------
    // SLINT_ENABLE_EXPERIMENTAL_FEATURES —— 放行编译器内部实验特性
    //
    // 我们把官方 material 样式的 widgets/ 源码 fork 进了 ui/widgets/（目的：
    // 把 MaterialPalette 的数据源换成自建 Monet global，实现全组件跟随
    // 自定义主题色）。fork 的文件里包含两处只在「编译器内部编译」时合法
    // 的语法：
    //   * common/std-widget-interfaces.slint → ../interfaces/lineedit.slint
    //     里的 `export interface LineEdit { ... }`（interface 是实验特性）
    //   * material/scrollview.slint 等处的 @shadowable 注解（已手动清理，
    //     这里留作兜底，避免将来同步上游文件时再踩）
    //
    // 编译器在 CompilerConfiguration::default() 里读这个环境变量
    // （i-slint-compiler lib.rs: enable_experimental = env var is_some()），
    // 设置后 reject_experimental_feature 直接放行。项目自身代码不使用
    // 任何实验语法，影响范围仅限 fork 的 widgets 文件。
    // ------------------------------------------------------------------------
    // SAFETY: build.rs 是单线程的，此处设置环境变量不会与其他线程竞争
    unsafe { std::env::set_var("SLINT_ENABLE_EXPERIMENTAL_FEATURES", "1") };

    let config = slint_build::CompilerConfiguration::new()
        .with_style(style.clone())
        .embed_resources(EmbedResourcesKind::EmbedFiles);

    println!("cargo:warning=Slint style = {style}");

    slint_build::compile_with_config("ui/app.slint", config)
        .expect("Slint 编译失败");

    // 让 cargo 在 .slint 或字体改动时重新跑 build.rs
    println!("cargo:rerun-if-changed=ui/");
    println!("cargo:rerun-if-changed=fonts/");
    println!("cargo:rerun-if-env-changed=STYLE");
}
