#!/usr/bin/env bash
# ============================================================================
# package-apk.sh —— 把编译产物组装成可安装的 apk
#
# 为什么手写这一步而不用 gradle：
#   1) 没有 Java 源码、没有 gradle 工程，aapt2 + zip + apksigner 就够
#   2) 需要精确控制 so 的压缩方式 —— 这是闪退高发区，自己掌控更可靠
#   3) 省掉 gradle 网络下载（本环境访问 dl.google.com 不通）
#
# ---- 三条硬性约束（Android 12+ 全部是强制的）----
#
#   A. android:extractNativeLibs="false" 时，apk 里的 .so 必须：
#        · 以 STORED（压缩方法 0，即不压缩）存储
#        · 起始偏移 4KB 对齐（zipalign -p 4）
#      否则安装时直接失败：INSTALL_FAILED_INVALID_APK
#      / "extractNativeLibs=false requires uncompressed and aligned"。
#
#   B. 所有 native 库都得在 lib/<abi>/ 下，目录名必须是
#      arm64-v8a / x86_64 / x86（不是 rust 的 target triple）。
#
#   C. apk 必须签名（v2 方案）。未签名只能 adb install -r 到 userdebug 版，
#      正常手机装不上。这里自动生成一个 debug keystore。
#
# 用法：
#   ./scripts/package-apk.sh                # 用 target/ 下已有的 so
#   ./scripts/package-apk.sh --clean        # 先清理再打包
# 产物：dist/pvz2-toolkit-<版本>-<时间戳>.apk
# ============================================================================
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# ---------------------------------------------------------------- 配置
BUILD_TOOLS="${BUILD_TOOLS:-/opt/android-sdk/build-tools/34.0.0}"
ANDROID_JAR="${ANDROID_JAR:-/opt/android-sdk/platforms/android-35/android.jar}"
AAPT2="$BUILD_TOOLS/aapt2"
ZIPALIGN="$BUILD_TOOLS/zipalign"
APKSIGNER="$BUILD_TOOLS/apksigner"

APP_DIR="android-app/android/app/src/main"
MANIFEST="$APP_DIR/AndroidManifest.xml"
RES_DIR="$APP_DIR/res"

# 版本号：优先取 git describe，没有 git 就退回日期
VERSION="${VERSION:-1.0.0}"
TIMESTAMP="$(date +%Y%m%d-%H%M)"

DIST_DIR="$REPO_ROOT/dist"
WORK_DIR="$REPO_ROOT/target/apk-work"
KEYSTORE="$REPO_ROOT/.android-debug.keystore"

# ABI 映射：rust target triple → apk 的 lib/<abi> 目录名
ABI_MAP=(
  "aarch64-linux-android:arm64-v8a"
  "x86_64-linux-android:x86_64"
  "i686-linux-android:x86"
)

MIN_SDK=26   # Slint 后端用 InMemoryDexClassLoader 需要 26+；
             # 更早的版本会退回 DexClassLoader（会往 cache 目录写文件），
             # 26 是更省心的下界，且覆盖了 Android 8.0 起的设备
TARGET_SDK=35

if [ "${1:-}" = "--clean" ]; then
  echo "==> 清理 $WORK_DIR 与 $DIST_DIR"
  rm -rf "$WORK_DIR" "$DIST_DIR"
fi

mkdir -p "$DIST_DIR" "$WORK_DIR"

# ---------------------------------------------------------------- 前置检查
echo "==> 检查工具链"
for tool in "$AAPT2" "$ZIPALIGN" "$APKSIGNER"; do
  if [ ! -x "$tool" ]; then
    echo "错误：找不到 $tool" >&2
    exit 1
  fi
done
if [ ! -f "$ANDROID_JAR" ]; then
  echo "错误：找不到 android.jar：$ANDROID_JAR" >&2
  exit 1
fi
echo "    aapt2    : $AAPT2"
echo "    zipalign : $ZIPALIGN"
echo "    apksigner: $APKSIGNER"

# ---------------------------------------------------------------- 1. 收集 so
echo ""
echo "==> 收集 native 库"
LIBS_DIR="$WORK_DIR/lib"
rm -rf "$LIBS_DIR"
any_lib=0
for entry in "${ABI_MAP[@]}"; do
  rust_target="${entry%%:*}"
  abi="${entry##*:}"
  so="target/$rust_target/release/libpvz2_toolkit_android.so"
  if [ -f "$so" ]; then
    mkdir -p "$LIBS_DIR/$abi"
    cp "$so" "$LIBS_DIR/$abi/libpvz2_toolkit_android.so"
    printf "    %-12s %-60s %s\n" "$abi" "$so" "$(du -h "$so" | cut -f1)"
    any_lib=1
  else
    echo "    跳过 $abi（$so 不存在，先跑 scripts/build-android.sh）"
  fi
done
if [ "$any_lib" -eq 0 ]; then
  echo "错误：一个 .so 都没有，无法打包。先运行 scripts/build-android.sh" >&2
  exit 1
fi

# ---------------------------------------------------------------- 2. 编译资源
echo ""
echo "==> 编译资源 (aapt2 compile)"
COMPILED_RES="$WORK_DIR/res.zip"
rm -f "$COMPILED_RES"
"$AAPT2" compile --dir "$RES_DIR" -o "$COMPILED_RES"
echo "    $(unzip -l "$COMPILED_RES" | tail -1 | awk '{print $2}') 个资源文件"

# ---------------------------------------------------------------- 3. 链接资源 + 生成 R.java / 基础 apk
echo ""
echo "==> 链接资源并生成基础 apk (aapt2 link)"
UNSIGNED_APK="$WORK_DIR/unsigned.apk"
rm -f "$UNSIGNED_APK"
"$AAPT2" link \
  -o "$UNSIGNED_APK" \
  -I "$ANDROID_JAR" \
  --manifest "$MANIFEST" \
  --min-sdk-version "$MIN_SDK" \
  --target-sdk-version "$TARGET_SDK" \
  --version-code "$(echo "$VERSION" | tr -d '.')" \
  --version-name "$VERSION" \
  --java "$WORK_DIR/gen" \
  "$COMPILED_RES"

echo "    apk 骨架已生成"

# ---------------------------------------------------------------- 4. 塞入 so
# 关键：用 zip -X -0（不压缩）把 so 放进 lib/<abi>/
# -0 = STORED，-X = 不写额外的扩展字段（保持偏移整洁，对齐更好做）
#
# ★ zip 的路径语义（踩过坑）：
#   zip 把「命令行给出的路径」原样当成 archive 内的条目名，
#   并且会去**当前工作目录**找这个文件。
#   所以必须：
#     · cwd = $WORK_DIR（即 lib/ 的父目录）
#     · 传相对路径 "lib/<abi>/xxx.so"
#   如果 cwd 已经是 lib/ 里面，再传 "lib/<abi>/..." 就找不到文件，
#   zip 会报 "Nothing to do!" 然后什么都不做（退出码 12）。
echo ""
echo "==> 写入 native 库（STORED，不压缩）"
for abi in arm64-v8a x86_64 x86; do
  [ -d "$LIBS_DIR/$abi" ] || continue
  ( cd "$WORK_DIR" && zip -X -0 -q "$UNSIGNED_APK" "lib/$abi/libpvz2_toolkit_android.so" )
  echo "    + lib/$abi/libpvz2_toolkit_android.so"
done

# classes.dex 不应该作为独立文件存在于 apk 里 —— 它在 .so 内部。
# 这里做一次断言，防止将来有人误加 dex 导致 android:hasCode="false" 与
# 实际内容不一致（那会引发启动时 ClassNotFound）。
if unzip -l "$UNSIGNED_APK" | grep -qE 'classes[0-9]*\.dex'; then
  echo "错误：apk 里出现了独立的 classes.dex，与 android:hasCode=\"false\" 冲突" >&2
  exit 1
fi
echo "    确认：apk 内无独立 dex 文件（Slint 的 dex 已嵌在 .so 内）"

# ---------------------------------------------------------------- 5. 校验 so 压缩方式
echo ""
echo "==> 校验 so 压缩方式"
bad=0
for abi in "${ABI_MAP[@]}"; do
  abi="${abi##*:}"
  [ -d "$LIBS_DIR/$abi" ] || continue
  entry="lib/$abi/libpvz2_toolkit_android.so"
  # zipinfo -v 输出里 "compression method" 那行；STORED 对应 "none"
  method="$(unzip -v "$UNSIGNED_APK" | awk -v e="$entry" '$0 ~ e {print $2}')"
  if [ "$method" = "Stored" ]; then
    echo "    OK  $entry  → Stored"
  else
    echo "    !!  $entry  → $method（应为 Stored）" >&2
    bad=1
  fi
done
if [ "$bad" -ne 0 ]; then
  echo "错误：存在被压缩的 native 库，extractNativeLibs=false 时会安装失败" >&2
  exit 1
fi

# ---------------------------------------------------------------- 6. 对齐
# -p 4 = 以 4KB 页为界对齐未压缩的 .so（对应 -p 的 page-align 语义）
echo ""
echo "==> 4KB 页对齐 (zipalign -p 4)"
ALIGNED_APK="$WORK_DIR/aligned.apk"
rm -f "$ALIGNED_APK"
"$ZIPALIGN" -f -p 4 "$UNSIGNED_APK" "$ALIGNED_APK"

# 对齐后复查一遍偏移
echo "    校验 lib/ 条目的对齐"
python3 - "$ALIGNED_APK" <<'PY'
import sys, zipfile, struct

apk = sys.argv[1]
zf = zipfile.ZipFile(apk)
ok = True
for info in zf.infolist():
    if not info.filename.startswith("lib/") or not info.filename.endswith(".so"):
        continue
    # 本地文件头起始偏移 = 数据偏移 - 头部长度 - 文件名长度 - 扩展字段长度
    off = info.header_offset
    with open(apk, "rb") as f:
        f.seek(off)
        sig = f.read(4)
        if sig != b"PK\x03\x04":
            print(f"    !! {info.filename}: 本地头签名异常 {sig!r}")
            ok = False
            continue
        f.seek(off + 26)
        nlen, xlen = struct.unpack("<HH", f.read(4))
    data_off = off + 30 + nlen + xlen
    aligned = (data_off % 4096) == 0
    mark = "OK " if aligned else "!! "
    print(f"    {mark} {info.filename}  data_off={data_off}  {'4096 对齐' if aligned else f'不对齐 (mod 4096 = {data_off % 4096})'}")
    if not aligned:
        ok = False
sys.exit(0 if ok else 1)
PY
if [ $? -ne 0 ]; then
  echo "错误：native 库未 4KB 对齐" >&2
  exit 1
fi

# ---------------------------------------------------------------- 7. 签名
echo ""
echo "==> 签名 (apksigner, v1 + v2)"
if [ ! -f "$KEYSTORE" ]; then
  echo "    首次运行，生成 debug keystore"
  keytool -genkeypair \
    -keystore "$KEYSTORE" \
    -alias pvz2toolkit \
    -keyalg RSA \
    -keysize 2048 \
    -validity 10000 \
    -storepass android \
    -keypass android \
    -dname "CN=PvZ2 Toolkit, OU=Dev, O=PvZ2Toolkit, L=, ST=, C=CN" \
    2>/dev/null
fi

FINAL_APK="$DIST_DIR/pvz2-toolkit-$VERSION-$TIMESTAMP.apk"
rm -f "$FINAL_APK"
"$APKSIGNER" sign \
  --ks "$KEYSTORE" \
  --ks-key-alias pvz2toolkit \
  --ks-pass pass:android \
  --key-pass pass:android \
  --v1-signing-enabled true \
  --v2-signing-enabled true \
  --out "$FINAL_APK" \
  "$ALIGNED_APK"

echo "    签名完成"

# ---------------------------------------------------------------- 8. 最终校验
echo ""
echo "==> 最终校验"
echo "  [1] 签名验证"
"$APKSIGNER" verify --verbose "$FINAL_APK" 2>&1 | sed 's/^/      /' | head -12

echo ""
echo "  [2] 对齐验证"
"$ZIPALIGN" -c -p 4 -v "$FINAL_APK" 2>&1 | sed 's/^/      /' | head -8

echo ""
echo "  [3] apk 内容"
unzip -l "$FINAL_APK" | sed 's/^/      /'

echo ""
echo "  [4] manifest 关键属性"
"$AAPT2" dump xmltree --file AndroidManifest.xml "$FINAL_APK" 2>/dev/null \
  | grep -E "hasCode|extractNativeLibs|lib_name|permission|package|minSdk|targetSdk" \
  | sed 's/^/      /' | head -20

echo ""
echo "  [5] dex 检查"
if unzip -l "$FINAL_APK" | grep -qE 'classes[0-9]*\.dex'; then
  echo "      !! apk 内存在独立 dex" >&2
else
  echo "      OK   apk 内无独立 dex（Slint 的 dex 嵌在 so 内，不单独存在）"
fi

echo ""
echo "============================================================"
echo "  打包完成"
echo "============================================================"
ls -la "$FINAL_APK" | sed 's/^/  /'
echo ""
echo "  安装：adb install -r \"$FINAL_APK\""
