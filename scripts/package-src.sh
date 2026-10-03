#!/usr/bin/env bash
# 打包源码 ZIP —— 交付给用户的「改后的源码」
#
#   用法： ./scripts/package-src.sh
#   产物：   dist/pvz2-toolkit-android-src-<时间戳>.zip
#
# 排除：target/  dist/  密钥  临时参考目录  编辑器/系统垃圾
# 保留：formats/（上游 14 个格式库）
#       android-app/（含 ui/ src/ fonts/ android/）
#       scripts/  docs/  README.md  LICENSE  Cargo.toml  Cargo.lock  .cargo/config.toml

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

STAMP="$(date +%Y%m%d-%H%M)"
OUT_DIR="$ROOT/dist"
NAME="pvz2-toolkit-android-src-$STAMP"
ZIP="$OUT_DIR/$NAME.zip"

# 暂存目录必须放在源码树之外 —— 否则 cp -a "$ROOT/." 会把自己复制进自己。
# 用 mktemp 顺便保证并发运行时不打架。
STAGE_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/pvz2src.XXXXXX")"
STAGE="$STAGE_ROOT/$NAME"
trap 'rm -rf "$STAGE_ROOT"' EXIT

mkdir -p "$OUT_DIR"
mkdir -p "$STAGE"

echo "==> 1/4 复制文件到暂存目录"

# 用 rsync 的 --exclude 保证规则集中、可读
if command -v rsync >/dev/null 2>&1; then
    rsync -a \
        --exclude 'target/' \
        --exclude 'dist/' \
        --exclude '.git/' \
        --exclude '_commands_ref/' \
        --exclude '.android-debug.keystore' \
        --exclude '*.apk' \
        --exclude '*.idsig' \
        --exclude '__pycache__/' \
        --exclude '.DS_Store' \
        --exclude '*.swp' \
        --exclude '*~' \
        --exclude 'core.[0-9]*' \
        "$ROOT/" "$STAGE/"
else
    echo "    (没装 rsync，退回 cp + find 清理)"
    cp -a "$ROOT/." "$STAGE/"
    rm -rf "$STAGE/target" "$STAGE/dist" "$STAGE/.git" "$STAGE/_commands_ref" \
           "$STAGE/.android-debug.keystore"
    find "$STAGE" -name '__pycache__' -type d -prune -exec rm -rf {} +
    find "$STAGE" -name '.DS_Store' -delete
    find "$STAGE" -name '*.swp' -delete
    find "$STAGE" -name '*~' -delete
    # core dump：进程崩溃（SIGBUS/SIGSEGV）时内核会往 cwd 写 core.<pid>，
    # 单个可能上百 MB。曾经漏排过一次 —— ZIP 从 8MB 涨到 478MB。
    find "$STAGE" -maxdepth 3 -name 'core.[0-9]*' -delete
fi

echo "==> 2/4 硬性检查：关键文件必须存在"

check() {
    if [[ ! -e "$STAGE/$1" ]]; then
        echo "    ✗ 缺失：$1" >&2
        return 1
    fi
    echo "    ✓ $1"
}

check "Cargo.toml"
check "Cargo.lock"
check ".cargo/config.toml"
check "README.md"
# AGPL-3.0 项目：LICENSE 必须随源码分发（lib.rs 还会 include_str! 嵌进 APK）
check "LICENSE"
# 上游 14 个格式库一个都不能少
for lib in bnk-archive compiled-text crypt-data dzip-archive newton-manifest \
           pak-archive pam-codec particle-codec reanim-codec rsb-archive \
           rsb-patch serde-rton smf-container wem-audio; do
    check "formats/$lib/src/lib.rs"
done
check "android-app/Cargo.toml"
check "android-app/build.rs"
check "android-app/ui/app.slint"
check "android-app/ui/theme/palette.slint"
check "android-app/ui/theme/widgets.slint"
check "android-app/ui/components/param_form.slint"
check "android-app/ui/screens/settings_screen.slint"
check "android-app/fonts/NotoSansSC-Regular.otf"
check "android-app/src/lib.rs"
check "android-app/src/platform.rs"
check "android-app/src/catalog.rs"
check "android-app/android/app/src/main/AndroidManifest.xml"
check "android-app/android/app/src/main/res/values/strings.xml"
check "scripts/build-android.sh"
check "scripts/package-apk.sh"
check "docs/FONT-NOTES.md"
check "docs/DEX-EXPLAINED.md"

echo "==> 3/4 硬性检查：不该有的东西一个都不能有"

LEAK=0
for bad in target dist .git _commands_ref .android-debug.keystore; do
    if [[ -e "$STAGE/$bad" ]]; then
        echo "    ✗ 不该出现：$bad" >&2
        LEAK=1
    fi
done
if find "$STAGE" -name '*.apk' -o -name '*.idsig' | grep -q .; then
    echo "    ✗ 不该出现：apk / idsig" >&2
    LEAK=1
fi
# core dump 单独再兜一道：它不在上面那串路径里，
# 但单个能到几百 MB，混进去会让 ZIP 体积失控。
if find "$STAGE" -name 'core.[0-9]*' | grep -q .; then
    echo "    ✗ 不该出现：core dump（进程崩溃残留）" >&2
    LEAK=1
fi
if (( LEAK != 0 )); then
    echo "    ✗ 暂存目录里有不该出现的文件，中止打包" >&2
    exit 1
fi
echo "    ✓ 干净（无 target/ dist/ .git/ 密钥/ apk/ core dump/ 临时参考目录）"

# 体积闸门。
#
# 这个源码树正常在 10MB 上下（大头是 10MB 的中文字体，
# zip -9 之后大概 7~8MB）。超过 60MB 就一定是混进了不该有的东西 ——
# 上一次就是 core dump 混进来把包撑到 478MB，
# 而且因为 zip 命令本身"成功"了，不主动设闸门根本发现不了。
MAX_BYTES=$((60 * 1024 * 1024))
RAWSZ=$(du -sb "$STAGE" | cut -f1)
if (( RAWSZ > MAX_BYTES )); then
    echo "    ✗ 暂存目录 ${RAWSZ} 字节，超过 60MB 上限 —— 疑似混入大文件：" >&2
    find "$STAGE" -type f -size +5M -printf '        %s  %p\n' >&2
    exit 1
fi
echo "    ✓ 体积正常（暂存 ${RAWSZ} 字节）"

# 统计一下规模，便于交付说明
N_FILES=$(find "$STAGE" -type f | wc -l)
N_RS=$(find "$STAGE" -name '*.rs' | wc -l)
N_SLINT=$(find "$STAGE" -name '*.slint' | wc -l)
echo "    规模：$N_FILES 个文件，其中 $N_RS 个 .rs、$N_SLINT 个 .slint"

echo "==> 4/4 生成 zip"
rm -f "$ZIP"
( cd "$STAGE_ROOT" && zip -q -r -9 "$ZIP" "$NAME" )

SIZE=$(stat -c%s "$ZIP")
echo
echo "==> 完成"
echo "    $ZIP"
echo "    $(numfmt --to=iec-i --suffix=B "$SIZE" 2>/dev/null || echo "$SIZE 字节")"
echo
echo "==> 校验（列出前 30 项）"
unzip -l "$ZIP" | head -35
