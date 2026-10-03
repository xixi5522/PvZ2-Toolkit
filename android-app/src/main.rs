// ============================================================================
// 桌面可执行入口
//
// Android 上整个进程是从 android_main 起的（由 android-activity 提供的
// NativeActivity 桥接进来），这个 main() 根本不会被链接进 .so。
//
// 但 cargo 仍然会尝试编译这个 bin target —— 而 `run_desktop` 被
// #[cfg(not(target_os = "android"))] 关掉了，于是交叉编译到 Android 时
// 报 "cannot find function `run_desktop`"。
//
// 所以这里也加同样的 cfg 闸门：Android 下给一个空的 main，只为让
// bin target 能通过类型检查。
// ============================================================================

#[cfg(not(target_os = "android"))]
fn main() -> Result<(), slint::PlatformError> {
    pvz2_toolkit_android::run_desktop()
}

#[cfg(target_os = "android")]
fn main() {
    // Android 上真正的入口是 lib.rs 里的 android_main。
    // 这个 bin 只是为了满足 Cargo.toml 里声明的 [[bin]]，不会被安装进 APK。
}
