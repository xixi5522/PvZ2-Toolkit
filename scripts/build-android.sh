#!/usr/bin/env bash
# ============================================================================
# build-android.sh —— 编译 Android 目标
#
# 覆盖的 ABI 说明（硬限制而不是偷懒）：
#   * armv7：Skia 官方 prebuilt 从来没发布过，实测 404。而 renderer-skia
#     在 Android 上是唯一可用的 GPU 渲染器（femtovg 因为需要 fontconfig，
#     在 android target 上被 slint 上游显式禁用），所以 armv7 编不出来。
#   * i686：Slint 1.18.1 → skia-bindings 0.153.3 起，上游 CI 不再发布
#     i686-linux-android 的 prebuilt，实测：
#       skia-binaries-<key>-i686-linux-android-*.tar.gz  → HTTP 404
#     回退源码全量编译又需要直连 codeload.github.com（网络受限环境走不通），
#     因此 32 位老模拟器 ABI 暂停提供；脚本逻辑保留了它，
#     若将来上游恢复发布或网络可用，直接重跑即可。
#
#   影响面很小：arm64-v8a 覆盖 2017 年之后几乎所有真机，
#   x86_64 覆盖现代模拟器。只有 32 位老模拟器装不上。
#
# 用法：
#   ./scripts/build-android.sh              # 编全部 ABI
#   ./scripts/build-android.sh aarch64-linux-android   # 只编一个
# ============================================================================
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# ABI 对应关系：rust target → android abi 目录名 → NDK clang 前缀
# （i686 保留在列表里，so 缺失时只跳过不报错）
ALL_TARGETS=(
  "aarch64-linux-android:arm64-v8a:aarch64-linux-android"
  "x86_64-linux-android:x86_64:x86_64-linux-android"
  "i686-linux-android:x86:i686-linux-android"
)

if [ $# -gt 0 ]; then
  TARGETS=()
  for arg in "$@"; do
    found=""
    for entry in "${ALL_TARGETS[@]}"; do
      if [[ "${entry%%:*}" == "$arg" ]]; then
        TARGETS+=("$entry")
        found=1
      fi
    done
    if [ -z "$found" ]; then
      echo "错误：不支持的 target '$arg'" >&2
      echo "可选：aarch64-linux-android  x86_64-linux-android  i686-linux-android" >&2
      exit 1
    fi
  done
else
  TARGETS=("${ALL_TARGETS[@]}")
fi

echo "==> 将编译 ${#TARGETS[@]} 个 ABI"
for entry in "${TARGETS[@]}"; do
  rust_target="${entry%%:*}"
  rest="${entry#*:}"
  android_abi="${rest%%:*}"

  echo ""
  echo "============================================================"
  echo "  $rust_target   (android: $android_abi)"
  echo "============================================================"

  # 确保 rust std 已装
  if ! rustup target list --installed 2>/dev/null | grep -qx "$rust_target"; then
    echo "    -> 安装 rust target $rust_target"
    rustup target add "$rust_target"
  fi

  # i686 的 Skia prebuilt 上游不再发布（见文件头），编译必然失败 ——
  # 非关键 ABI 失败只警告跳过；arm64 / x86_64 失败则照常终止构建。
  if ! cargo build \
    --release \
    -p pvz2-toolkit-android \
    --target "$rust_target"; then
    if [ "$rust_target" = "i686-linux-android" ]; then
      echo "    跳过 i686-linux-android（上游无 Skia prebuilt，编译失败属预期）"
      continue
    fi
    echo "错误：$rust_target 编译失败" >&2
    exit 1
  fi

  so="target/$rust_target/release/libpvz2_toolkit_android.so"
  if [ ! -f "$so" ]; then
    echo "    跳过 $rust_target（$so 未生成 —— 见文件头说明的 ABI 限制）"
    continue
  fi

  # ---- 增量校验：so 的每个 DT_NEEDED 都必须能在 NDK sysroot 里找到 ----
  # 这是上一版 C++ 实现翻车的地方：libkernel.so 出现在 DT_NEEDED 里，
  # 但打包时没被打进 apk，安装后 dlopen 失败 → 一启动就闪退。
  # 所以每一步都校验一次，不要等到最后。
  ndk_sysroot="/opt/android-sdk/android-ndk-r27c/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/$rust_target"
  [ -d "$ndk_sysroot" ] || ndk_sysroot="$ndk_sysroot/26"

  echo "    -> 校验 DT_NEEDED"
  while read -r lib; do
    [ -z "$lib" ] && continue
    if [ -e "$ndk_sysroot/$lib" ] || [ -e "/opt/android-sdk/android-ndk-r27c/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/26/$lib" ]; then
      echo "       OK   $lib"
    else
      echo "       !!   $lib  <-- 无法解析，打包后可能闪退" >&2
    fi
  done < <(readelf -dW "$so" | grep NEEDED | sed 's/.*\[\(.*\)\]/\1/')

  # ---- 校验入口符号 ----
  # NativeActivity 需要 ANativeActivity_onCreate；
  # android_main 是 android-activity crate 的入口，两个都得在。
  #
  # 注意匹配方式：readelf 的输出是「编号: 地址 大小 类型 绑定 可见性 段 名字」，
  # 名字在最后一列，但不同版本的 readelf 可能在名字后面粘上额外的标记。
  # 之前用 grep " $sym$" 会漏匹配（明明符号在，却报「未导出」），
  # 所以改成「按列取值后精确比较」。
  syms="$(readelf -sW --dyn-syms "$so" | awk '{print $NF}')"
  for sym in ANativeActivity_onCreate android_main; do
    if printf '%s\n' "$syms" | grep -qx "$sym"; then
      echo "       OK   $sym 已导出"
    else
      echo "       !!   $sym 未导出  <-- NativeActivity 会启动失败" >&2
    fi
  done

  printf "    -> %s  (%s)\n" "$so" "$(du -h "$so" | cut -f1)"
done

echo ""
echo "==> 全部 ABI 编译完成"
