#!/usr/bin/env bash
# 在 Linux 上把 Rust release 二进制、bin/ 依赖、licenses/ 等打包成 tar.gz。
#
# 用法：
#   bash scripts/fetch-linux-deps.sh   # 先准备 bin/scrcpy、bin/scrcpy-server、bin/adb
#   bash scripts/package-linux.sh
set -euo pipefail

cd "$(dirname "$0")/.."

die() { echo "错误：$*" >&2; exit 1; }

# 本脚本打的是 Linux 包，依赖 bin/ 里的 Linux 可执行文件，只能在 Linux 上运行。
if [ "$(uname -s)" != "Linux" ]; then
  die "此脚本只能用于 Linux 打包（当前 $(uname -s)）。Windows 请使用 scripts/package-windows.ps1。"
fi

case "$(uname -m)" in
  x86_64 | amd64) artifact_arch="x86_64" ;;
  *) die "目前只发布 x86_64 包，当前架构为 $(uname -m)" ;;
esac

# ---- 检查依赖 -------------------------------------------------------------
# 必须三个都在：scrcpy 与 adb 可执行，scrcpy-server 由应用通过
# SCRCPY_SERVER_PATH 传给 scrcpy；缺了它运行时会失败。
for f in bin/scrcpy bin/adb bin/scrcpy-server; do
  [ -f "$f" ] || die "缺少 $f：请先运行 bash scripts/fetch-linux-deps.sh（若 bin/ 中混入了 Windows 依赖，请清理后重试）"
done
[ -x bin/scrcpy ] || die "bin/scrcpy 不可执行，请重新运行 bash scripts/fetch-linux-deps.sh"
[ -x bin/adb ] || die "bin/adb 不可执行，请重新运行 bash scripts/fetch-linux-deps.sh"

version="$(bin/scrcpy --version 2>&1 | awk '/^scrcpy / { if (v == "") v = $2 } END { print v }')"
[ -n "$version" ] || die "无法解析 bin/scrcpy --version 输出"
if ! awk -v v="$version" 'BEGIN {
      split(v, a, ".");
      if (a[1] > 2 || (a[1] == 2 && a[2] >= 1)) exit 0;
      exit 1;
    }'; then
  die "scrcpy 版本过旧（$version），需要 >= 2.1"
fi

# ---- 构建 -----------------------------------------------------------------
cargo build --release
[ -f target/release/phone-audio ] || die "构建产物 target/release/phone-audio 不存在"

# ---- 组装包 ---------------------------------------------------------------
out="dist/phone-audio-linux"
rm -rf "$out"
mkdir -p "$out/bin"

cp target/release/phone-audio "$out/"
chmod 0755 "$out/phone-audio"

# 只带运行所需的三个依赖，并显式设置权限（与 tools.rs 的 <exe dir>/bin 布局一致）。
cp -f bin/scrcpy bin/adb "$out/bin/"
chmod 0755 "$out/bin/scrcpy" "$out/bin/adb"
cp -f bin/scrcpy-server "$out/bin/"
chmod 0644 "$out/bin/scrcpy-server"
# fetch-linux-deps.sh 会把包内 scrcpy 的 Apache-2.0 LICENSE 放成 bin/LICENSE.scrcpy，
# 这里一并带上（Windows 包里是 bin/LICENSE.txt，做法一致）。
if [ -f bin/LICENSE.scrcpy ]; then
  cp -f bin/LICENSE.scrcpy "$out/bin/LICENSE.scrcpy"
  chmod 0644 "$out/bin/LICENSE.scrcpy"
fi

[ -d licenses ] || die "缺少 licenses/ 目录"
cp -r licenses "$out/"
if [ -d assets ]; then
  cp -r assets "$out/"
fi
cp README.md LICENSE NOTICE THIRD_PARTY_LICENSES.md "$out/"

# ---- 打包并输出校验和 -----------------------------------------------------
tarball="dist/phone-audio-linux-${artifact_arch}.tar.gz"
tar -C dist -czf "$tarball" phone-audio-linux

echo "已生成 $tarball"
sha256sum "$tarball"
