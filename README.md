# PvZ2 Toolkit —— 安卓 GUI（Slint + Rust）

把 [LambdaEd1th/ed1ths-pvz-toolkit](https://github.com/LambdaEd1th/ed1ths-pvz-toolkit)
的 14 个格式库包装成一个安卓应用。

- **处理层**：ed1ths-pvz-toolkit 的 14 个格式库（14 类共 **35 条命令**）
- **UI 框架**：Slint 1.18.1，**官方 std-widgets 的 Material Design 3 样式**（fork material 样式源码，色彩数据源整体换成本地 Monet 引擎）
- **语言**：Rust（不用 C++）
- **界面**：三页 + 底部导航栏 —— 工具页（选功能 + 填参数 + 运行）、日志页、设置页
- **主题色**：**自定义** —— 「跟随系统莫奈」（Android 12+ 从壁纸提取主色）/ 12 个预设种子色 / 手输 hex，由 material-color-utilities（Google 官方 Monet 参考实现）生成完整 MD3 tonal 色板
- **性能**：解包/回包 rayon 多核并行 + PNG Fast 编码 + 日志节流 + release `opt-level=3`（2.1 相比 2.0 大幅提速）
- **许可证**：**AGPL-3.0-or-later**（全文见 [LICENSE](LICENSE)）
- **作者**：嘻嘻哈哈嘿嘿

> 本项目是上游 ed1ths-pvz-toolkit 的 GUI 壳，处理逻辑全部来自上游
> `formats/` 层。上游项目同样以 AGPL-3.0-or-later 发布，在此特别致谢。
> 应用内「设置 → 开源与许可」也可查看致谢名单与许可证全文。

---

## 目录结构

```
pvz2-toolkit-android/
├── Cargo.toml                     # workspace 根（edition 2021, version 2.2.0）
├── .cargo/config.toml             # ★ 交叉编译的关键配置，注释很详细
├── LICENSE                        # AGPL-3.0 官方全文（编译期嵌入 APK）
├── formats/                       # ★ ed1ths-pvz-toolkit 的 14 个格式库（原样搬迁）
│   ├── bnk-archive/               #   BNK 音频库
│   ├── compiled-text/             #   编译文本 (LawnStrings 等)
│   ├── crypt-data/                #   Crypt-Data
│   ├── dzip-archive/              #   DZip 资源包
│   ├── newton-manifest/           #   Newton 清单
│   ├── pak-archive/               #   PAK 资源包
│   ├── pam-codec/                 #   PAM 动画
│   ├── particle-codec/            #   PopFX 粒子特效
│   ├── reanim-codec/              #   ReAnim 动画
│   ├── rsb-archive/               #   RSB/RSG 资源包（含 PTX 纹理）
│   ├── rsb-patch/                 #   RSB 补丁
│   ├── serde-rton/                #   RTON 数据
│   ├── smf-container/             #   SMF 容器
│   └── wem-audio/                 #   WEM 音频
├── android-app/
│   ├── Cargo.toml
│   ├── build.rs                   # ★ Slint 编译配置（material 样式/字体/资源嵌入）
│   ├── fonts/
│   │   └── NotoSansSC-Regular.otf # 中文字体（见 docs/FONT-NOTES.md）
│   ├── ui/
│   │   ├── app.slint              # 三页布局 + 底部导航栏 + 感谢浮层
│   │   ├── theme/                 # 配色镜像官方 Palette + 自绘骨架组件
│   │   ├── components/            # 参数表单、命令选择器
│   │   └── screens/               # 工具页 / 日志页 / 设置页
│   ├── src/
│   │   ├── lib.rs                 # 入口装配 + 日志管线 + LICENSE 嵌入
│   │   ├── main.rs                # 桌面入口（Android 下为空壳）
│   │   ├── catalog.rs             # 35 个命令的声明式目录
│   │   ├── params.rs              # 参数读取封装（友好的中文错误）
│   │   ├── runner.rs              # 后台执行 + 进度回传
│   │   ├── settings.rs            # 设置持久化 + 明暗模式桥接
│   │   ├── platform.rs            # Android JNI 层（权限/Toast/Prefs）
│   │   └── commands/              # 35 条命令 → 14 个格式库的适配层
│   │       ├── archive.rs         #   PAK / DZip / SMF
│   │       ├── audio.rs           #   BNK / WEM
│   │       ├── anim.rs            #   PAM / ReAnim / PopFX
│   │       ├── data.rs            #   RTON / 编译文本 / Crypt-Data / Newton
│   │       └── rsb.rs             #   RSB / RSG / 补丁
│   ├── tests/
│   │   └── interaction.rs         # 无头交互回归测试
│   └── android/app/src/main/
│       ├── AndroidManifest.xml
│       └── res/                   # 图标、主题、字符串
├── scripts/
│   ├── build-android.sh           # 编译各 ABI + 校验依赖
│   ├── package-apk.sh             # 组装签名 APK + 多层校验
│   └── package-src.sh             # 打包源码 ZIP
└── docs/
    ├── FONT-NOTES.md              # 字体问题的完整排查记录
    └── DEX-EXPLAINED.md           # 为什么一定有 dex
```

---

## 1. 功能列表（28 条命令）

全部命令来自上游 14 个格式库，按格式分为 14 类：

| 分类 | 命令 |
|---|---|
| **RSB 资源包** | RSB 解包（内嵌 PTX 自动转 PNG）、**RSB 回包**（改过的 PNG 自动按原格式编回 PTX）、RSB PTX 导出、RSG 分包解包、**RSG 分包打包**、**PTX 转 PNG**（独立贴图互转）、**PNG 转 PTX** |
| **RSB 补丁** | 创建补丁、应用补丁 |
| **PAK 资源包** | PAK 解包、PAK 打包、PAK 列表 |
| **DZip 资源包** | DZip 解包、DZip 打包 |
| **RTON 数据** | RTON 解码、RTON 编码 |
| **SMF 容器** | SMF 解码、SMF 编码 |
| **编译文本** | 编译文本解码、编译文本编码 |
| **Crypt-Data** | Crypt-Data 解码、Crypt-Data 编码 |
| **Newton 清单** | Newton 解码、Newton 编码 |
| **PAM 动画** | PAM 解码、PAM 编码 |
| **ReAnim 动画** | ReAnim 解码、ReAnim 编码、**XFL 工程导入**、**ReAnim 转 XFL 工程** |
| **粒子特效** | PopFX 解码、PopFX 编码 |
| **BNK 音频库** | BNK 提取 |
| **WEM 音频** | WEM 解码、**音频转 WEM**（WAV/OGG → Wwise WEM） |

目录是**声明式**的（`catalog.rs` 里一张表），加功能只需要：

1. 在 `commands/` 里写个 `fn(...) -> Result<String>`
2. 在 `catalog.rs` 的 `COMMANDS` 加一条记录
3. 在 `commands/mod.rs` 的 `dispatch` 加一个分支

界面不用动 —— 选择器、搜索、分类、参数表单、计数徽标全靠目录数据驱动。

---

## 2. 界面

三个页面，底部导航栏切换（自绘 TouchArea，带选中态与红点提示）：

* **工具页**：功能选择器（搜索 + 分类 + 单列可滚动列表）→ 参数表单
  （文本输入用官方 LineEdit、开关用官方 Switch、下拉用官方 ComboBox）→
  运行/取消按钮（官方 Button）。运行中底部显示官方 ProgressIndicator。
* **日志页**：运行日志常驻面板，等宽字体、自动滚到底、可清空。
* **设置页**：作者信息、明暗模式（跟随系统/浅色/深色）、
  **主题色（跟随系统莫奈 / 12 个预设种子色 / 手输 hex，整套 MD3 配色即时换装）**、
  存储权限申请、线程数、版本信息，以及 **「开源与许可」卡片** —— 里面可以打开
  **全屏「开源致谢」浮层**：逐一致谢上游格式库与 Slint / Noto Sans CJK SC，
  并展示 AGPL-3.0 许可证全文（编译期 `include_str!("../../LICENSE")` 嵌入）。

### 官方 MD3 组件化 + Monet 主题引擎（2.0/2.1 的核心改动）

1.x 的控件全是手绘的（自绘按钮、自绘开关、自绘下拉、自研种子色调色板）。
2.0 整体换成 **Slint 官方 `std-widgets.slint` 的 material 样式**：
Button、LineEdit、ComboBox、Switch、ProgressIndicator、ScrollView 全部官方。

**2.1 的关键演进：fork material 样式 + 自建 Monet 引擎**。
官方 material 的 `accentify()` 只把强调色注入部分角色，surface 系永远是
固定紫灰 tint —— 换主题色时组件和背景会「打架」。2.1 直接把
`i-slint-compiler` 的 `widgets/{material,common,interfaces}` fork 进项目
（`android-app/ui/widgets/`），改动只有两处：

1. `material/styling.slint` 的 `MaterialPalette` 数据源整体换成本地
   `Monet` global（`ui/widgets/monet.slint`，32 个 MD3 角色全部由 Rust
   侧写入）；
2. `material/color-scheme.slint` 的明暗数据源从编译器内置
   `SlintInternal` 改为 `Monet.dark`（不再依赖 backend 回调）。

组件外观 100% 官方，颜色 100% 来自 Monet 引擎 —— 设置页选个种子色，
官方组件和自绘骨架同时换装。取色用
[material-color-utilities](https://crates.io/crates/material-color-utilities)
（Google 官方 Monet 参考实现）：Android 12+ 读壁纸主色当种子
（`WallpaperManager.getWallpaperColors(FLAG_SYSTEM)`，无需权限），
也支持 12 个预设色 / 手输 hex；`build_scheme` 用 TonalSpot + Spec 2021，
container 系角色带一层修正（绕过上游 dev.18 的明暗反转 bug，
见 `src/colors.rs` 注释）。

fork 同时解锁了性能修复：release profile 从 `opt-level="z"` 提到 3，
解包/回包改 rayon 并行（按文件粒度吃满多核）、PNG 用 Fast 压缩档
编码、日志投递 120ms 节流 —— 这是 2.1 「解包回包快了很多」的全部原因。

2.0→2.1 过程中验证过的三件事，写在这里避免下一个人再踩：

* **官方 `Palette` global 是只读的**。`material/style-base.slint` 里十个颜色
  角色全是 `out`，只有 `color-scheme` 是 `in-out`。应用侧唯一能改的就是
  明暗（`Palette.set_color_scheme(...)`），**没有注入自定义颜色的口子** ——
  这是 2.1 选择 fork 样式源码而不是继续调 API 的直接原因。

* **fork 的 widgets 源码要用 `SLINT_ENABLE_EXPERIMENTAL_FEATURES=1` 编译**。
  上游源码里用了 `export interface`（`interfaces/lineedit.slint`）和
  `@shadowable` 注解，这些只在编译器「内部编译」时放行；fork 后它们
  变成用户代码会被拒。build.rs 设置该环境变量即可（见
  `android-app/build.rs`），同时 fork 文件内的裸 import 全部改成了
  显式相对路径。

* **官方 material 组件内嵌 SVG 图标 → 桌面构建需要手动补 feature**。
  ComboBox 的下拉箭头等图标经 `load_image_from_embedded_data` 解码，
  该函数在 `i-slint-core` 的 `image-decoders` feature 下；Android 后端
  会自动启用它，但桌面链路（slint 顶层没有转发该 feature 的入口）不会，
  所以 `Cargo.toml` 对非 Android target 显式补了一条
  `i-slint-core = { features = ["image-decoders"] }`。

另有一条从 1.x 保留的核心教训：**Slint 里绝不能在输入回调里重建模型**。
文本输入只更新 Rust 侧的 `FORM_VALUES` 表，模型完全不动，运行时再合并；
否则每打一个字整个 `for` 委托被销毁重建，焦点丢失、软键盘收起。

---

## 3. 交叉编译的三个关键点

全部集中在 `.cargo/config.toml`，每一处都有详细注释。这里只列结论：

### 3.1 linker 必须写绝对路径

cargo 的 `linker` 配置**不走 PATH 查找** —— 它把字符串原样交给 exec。
写裸名字 `aarch64-linux-android26-clang` 会得到
`linker not found: No such file or directory`，即使那个文件确实在 PATH 上。

### 3.2 `ANDROID_JAR` / `ANDROID_HOME` 写进 `[env]` 段

Slint Android 后端的 build.rs 要找 `android.jar`，找不到直接 panic。
写进 `.cargo/config.toml` 的 `[env]` 段（`force = false`）最稳：
cargo 会注入给所有 build script，不依赖父 shell 的 export。

### 3.3 `CC_<target>` 是必需的，不是可选优化

`cc-rs` 的查找顺序最后一档会拼出 `aarch64-linux-android-clang` ——
**NDK 里没这个名字**（NDK 的叫 `aarch64-linux-android26-clang`，带 API 等级）。
所以 `[env]` 里四个 `CC_*` / `AR_*` 变量都得显式给。

### 3.4 Skia 预编译库

`skia-bindings` 默认去 GitHub releases 下载 prebuilt（Android 三个架构都有）。
网络受限环境下可用 `SKIA_BINARIES_URL` 指向本地镜像：

```toml
SKIA_BINARIES_URL = { value = "file:///缓存/serve/{tag}/skia-binaries-{key}.tar.gz" }
```

**注意占位符是 `{tag}` 和 `{key}`，不是 `{}`** —— 写成 `{}` 不会被替换，
会静默退化成「从源码编译 skia」。

---

## 4. 关于「没有 dex 文件」

APK 内**没有独立的 `classes*.dex` 文件**（打包脚本有硬断言），但严格说
「完全没有 dex」做不到。证据链完整记录在 `docs/DEX-EXPLAINED.md`，结论：

Slint 的 Android 后端需要一个 Java 辅助类 `SlintAndroidJavaHelper`
（软键盘、剪贴板、深色模式检测、刘海/键盘尺寸计算）。它在**编译期**被
javac + D8 编译成 `classes.dex`，然后 `include_bytes!` 嵌进 `.so`，
运行时用 `InMemoryDexClassLoader` 加载。`AndroidManifest.xml` 里
`android:hasCode="false"` 是准确的 —— **应用自身的 Java 代码为零**，
那个 dex 是 Slint 后端的实现细节。

---

## 5. 构建

### 5.1 前置要求

- Android NDK r27c（默认 `/opt/android-sdk/android-ndk-r27c`，可用环境变量覆盖）
- Android SDK platforms 35 的 `android.jar` + build-tools（aapt2 / zipalign / apksigner）
- JDK（build script 里 D8 用）
- rustup target：`aarch64-linux-android`、`x86_64-linux-android`（`i686` 可选）

路径默认值都写在 `.cargo/config.toml` 的 `[env]` 段，`force = false`
意味着环境里已有的值优先 —— 换机器时 export 一下即可覆盖。

### 5.2 编译

```bash
# 编译全部可用 ABI（只编一个：./scripts/build-android.sh aarch64-linux-android）
./scripts/build-android.sh
```

脚本对每个 ABI 做两项增量校验：

1. **DT_NEEDED 逐项解析** —— 每个依赖的 `.so` 都要能在 NDK sysroot 里找到。
   这是 C++ 版翻车的地方：依赖库出现在 DT_NEEDED 里但没打进 apk，
   安装后 `dlopen` 失败 → 一启动就闪退。
2. **入口符号导出** —— `ANativeActivity_onCreate` 与 `android_main` 都必须在。

### 5.3 打包

```bash
VERSION=2.1.0 ./scripts/package-apk.sh
# 产物：dist/pvz2-toolkit-2.1.0-<时间戳>.apk
```

打包脚本流程（✅ 为硬性校验，不通过就退出）：

| 步骤 | 校验点 |
|---|---|
| 1 | 收集各 ABI 的 `.so` |
| 2–3 | `aapt2 compile` / `link` 生成 apk 骨架 |
| 4 | 以 **STORED（不压缩）** 写入 `lib/<abi>/` |
| 4b | ✅ 断言 apk 内**没有独立的 `classes*.dex`** |
| 5 | ✅ 断言每个 `.so` 的压缩方法确实是 `Stored` |
| 6 | `zipalign -f -p 4` 4KB 页对齐 |
| 6b | ✅ 逐个复核 `.so` 的**数据偏移 mod 4096 == 0** |
| 7 | `apksigner` 签名（v1 + v2），首次运行自动生成 debug keystore |
| 8 | ✅ 最终验证：签名、对齐、内容清单、manifest 属性、dex 检查 |

### 5.4 打包源码 ZIP

```bash
./scripts/package-src.sh
# 产物：dist/pvz2-toolkit-android-src-<时间戳>.zip
```

脚本会做两轮硬性检查后才打包：**必须有** `Cargo.toml` / `LICENSE` /
`formats/` / `android-app/`（ui、src、fonts、android）/ `scripts/` / `docs/`；
**必须没有** `target/`、`dist/`、`.git/`、keystore、`*.apk`。

### 5.5 桌面调试（可选）

```bash
cargo build --release -p pvz2-toolkit-android --features desktop
./target/release/pvz2-toolkit
```

桌面用软件渲染器（`x86_64-unknown-linux-gnu` 没有 Skia prebuilt）。
它的价值是在没有真机时验证 `.slint` 的运行期行为。无头环境配 Xvfb
可截图（详见 1.x 记录，方法不变）。

---

## 6. 只支持两个 ABI（1.x 的三个变两个）

| ABI | rust target | 覆盖率 |
|---|---|---|
| `arm64-v8a` | `aarch64-linux-android` | 2017 年后几乎所有真机 |
| `x86_64` | `x86_64-linux-android` | 现代模拟器 |

两个 32 位 ABI 均不可用，原因是硬限制：

* **armv7**：Skia 官方 prebuilt 从来没发布过（实测 404），而
  `renderer-skia` 在 Android 上是唯一可用的 GPU 渲染器
  （femtovg 需要 fontconfig，被 Slint 上游显式禁用）。
* **i686**：Slint 1.18.1 → skia-bindings 0.153.3 起上游 CI 不再发布
  i686-linux-android 的 prebuilt（实测 404），回退源码编译需要直连
  codeload.github.com（受限网络走不通）。

影响面很小：arm64-v8a 覆盖 2017 年之后几乎所有真机。

---

## 7. 权限与设置

Manifest 声明：`READ/WRITE_EXTERNAL_STORAGE`（后者 maxSdkVersion=32）、
`MANAGE_EXTERNAL_STORAGE`（Android 11+ 访问任意目录需用户手动开
「所有文件访问」）、`requestLegacyExternalStorage`（兼容 Android 10）。
设置页显示权限状态，点击发起申请。

设置持久化：Android 走 `SharedPreferences("pvz2_toolkit")`（JNI），
桌面走 `~/.config/pvz2-toolkit/settings.json`，格式统一为
`key=value` 行文本。设置项：明暗模式（跟随系统/浅色/深色）、后台线程数。
明暗判断会同时驱动两处：自绘骨架（`Theme.effective-dark`）与官方组件
（`Palette.color-scheme`），同一份判断保证两边永远一致。

---

## 8. 许可证

本项目以 **AGPL-3.0-or-later** 发布（SPDX 标识：`AGPL-3.0-or-later`），
许可证全文见 [LICENSE](LICENSE)，亦可从
<https://www.gnu.org/licenses/> 获取。

致谢（排名不分先后）：

* [LambdaEd1th/ed1ths-pvz-toolkit](https://github.com/LambdaEd1th/ed1ths-pvz-toolkit)
  —— 全部 14 个格式库（bnk-archive、compiled-text、crypt-data、dzip-archive、
  newton-manifest、pak-archive、pam-codec、particle-codec、reanim-codec、
  rsb-archive、rsb-patch、serde-rton、smf-container、wem-audio）
* [Slint](https://slint.dev) —— 界面框架与官方 material 组件
* Noto Sans CJK SC —— 中文字体（Google, SIL Open Font License）

依 AGPL-3.0 第 7 条，本项目的版权与许可声明已在应用内「设置 →
开源与许可」中显著展示。

---

## 9. 已知限制

1. **必须有 dex**（见第 4 节），这是 Slint 上游的架构决定
2. **不支持 32 位 ABI**（见第 6 节），Skia prebuilt 缺失
3. **未在真机上验证过** —— 开发环境没有 `adb` 也没有模拟器。
   已做的验证：交叉编译通过、依赖可解析、入口符号导出、
   `cargo test` 全绿、桌面 Xvfb 截图走查（工具页/设置页/日志页/
   深色模式/功能面板/参数表单全部通过）。
   注：Xvfb 合成事件无法触发官方 Button 的点击（StateLayer 的已知
   兼容限制），真机不受影响。
4. **签名是 debug 证书**，不是发布证书 —— 自己装没问题，
   要上架得换正式密钥重新签。
5. **莫奈取色需要 Android 12+** —— 壁纸主色（Material You）是
   API 31 起的能力；低版本或取色失败时自动回退 MD3 baseline 紫
   `#6750A4`，设置页手选种子色不受版本限制。
