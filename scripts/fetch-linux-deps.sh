#!/usr/bin/env bash
# 下载 Linux 版 scrcpy（官方静态包）与 Android platform-tools 的 adb 到 bin/。
#
# 适用：Ubuntu 24.04 等 x86_64 Linux。Windows 请用 scripts/fetch-windows-deps.ps1，
#       不要用本脚本，也不要让两套平台的依赖混在同一个 bin/ 里。
#
# 用法：
#   bash scripts/fetch-linux-deps.sh
#
# 可选环境变量（一般不需要）：
#   SCRCPY_VERSION        scrcpy 版本号，默认固定为 5.0.1（不带 v）。
#                         设为 latest 时通过 GitHub Releases 的重定向解析最新版本。
#   SCRCPY_TARBALL_URL    直接指定静态包 URL（镜像/离线，支持 file://）。
#   SCRCPY_SHA256         静态包 SHA-256；默认内置 5.0.1 的校验和。
#   PLATFORM_TOOLS_URL    platform-tools-latest-linux.zip 的 URL。
set -euo pipefail

cd "$(dirname "$0")/.."

die() { echo "错误：$*" >&2; exit 1; }
warn() { echo "警告：$*" >&2; }

# ---- 环境检查 -------------------------------------------------------------
# 本脚本下载的是 Linux 二进制，在 Windows/MSYS2 上运行会把 Linux 可执行文件写进
# bin/，和 Windows 依赖混在一起。因此明确拒绝非 Linux 环境。
if [ "$(uname -s)" != "Linux" ]; then
  die "此脚本只用于 Linux（当前 $(uname -s)）。Windows 请运行 scripts/fetch-windows-deps.ps1；不要在 Windows 上执行 Linux 二进制。"
fi

case "$(uname -m)" in
  x86_64 | amd64) ;;
  *) die "官方只发布 linux-x86_64 静态包，当前架构 $(uname -m)。请自行构建 scrcpy 或使用发行版包。" ;;
esac

for cmd in curl tar unzip sha256sum; do
  command -v "$cmd" >/dev/null 2>&1 \
    || die "缺少命令：$cmd（Debian/Ubuntu: sudo apt install curl tar unzip coreutils）"
done

mkdir -p bin

# WSL / 共享目录场景下 bin/ 里可能已有 Windows 依赖，提示一下（不视为错误）。
if [ -e bin/scrcpy.exe ] || [ -e bin/adb.exe ]; then
  warn "bin/ 中检测到 Windows 依赖（*.exe）。Linux 与 Windows 依赖建议使用不同目录，避免混淆。"
fi

# ---- 解析下载地址 ---------------------------------------------------------
version="${SCRCPY_VERSION:-5.0.1}"
if [ "$version" = "latest" ]; then
  echo "查询 scrcpy 最新 release..."
  latest_url="$(curl -fsSL -o /dev/null -w '%{url_effective}' \
    -H "User-Agent: phone-audio" \
    "https://github.com/Genymobile/scrcpy/releases/latest")" \
    || die "无法查询 GitHub Releases（可改用 SCRCPY_VERSION=<版本号> 固定版本）"
  tag="${latest_url##*/}"
  version="${tag#v}"
  [ -n "$version" ] || die "无法从重定向地址解析版本号：$latest_url"
fi

tarball_url="${SCRCPY_TARBALL_URL:-https://github.com/Genymobile/scrcpy/releases/download/v${version}/scrcpy-linux-x86_64-v${version}.tar.gz}"
platform_tools_url="${PLATFORM_TOOLS_URL:-https://dl.google.com/android/repository/platform-tools-latest-linux.zip}"

# 内置 v5.0.1 校验和（取自该 release 的 SHA256SUMS.txt），可被 SCRCPY_SHA256 覆盖。
expected_sha="${SCRCPY_SHA256:-}"
if [ -z "$expected_sha" ] && [ "$version" = "5.0.1" ]; then
  expected_sha="9f969d30cc574816077edecda65719c1c70b0aee1df1ba5fa00779ac07b8fd6e"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# ---- 下载并校验 scrcpy ----------------------------------------------------
# 全部内容先落到临时目录，最后才写入 bin/，避免中途失败留下“半套”依赖。
echo "下载 scrcpy $version..."
curl -fsSL --retry 3 --retry-delay 2 -H "User-Agent: phone-audio" \
  "$tarball_url" -o "$tmp/scrcpy.tar.gz" \
  || die "scrcpy 下载失败：$tarball_url"

if [ -n "$expected_sha" ]; then
  actual_sha="$(sha256sum "$tmp/scrcpy.tar.gz" | awk '{print $1}')"
  if [ "$actual_sha" != "$expected_sha" ]; then
    die "scrcpy 校验和不匹配（期望 $expected_sha，实际 $actual_sha）。请检查下载源，或通过 SCRCPY_SHA256 指定正确值。"
  fi
  echo "SHA-256 校验通过"
else
  warn "未提供 SCRCPY_SHA256，跳过完整性校验（建议固定版本并配置校验和）"
fi

mkdir -p "$tmp/scrcpy"
tar -xzf "$tmp/scrcpy.tar.gz" -C "$tmp/scrcpy"

# 官方静态包结构（v5.0.1，实际下载核对）：
#   scrcpy-linux-x86_64-v5.0.1/
#     scrcpy(ELF, ~34MB) scrcpy-server(APK/jar, 733930B) adb(ELF, 10642368B)
#     scrcpy.1 scrcpy.png disconnected.png LICENSE
# 注意：scrcpy-server 与 scrcpy 同级（不在 share/ 下）；v5.0.1 包内已经自带
# adb，且与 platform-tools-latest-linux.zip 里的 adb 字节完全一致。有内置 adb
# 时直接使用，省掉一次 9MB 下载；老版本/未来版本若没有 adb，再回退到 platform-tools。
src="$(dirname "$(find "$tmp/scrcpy" -type f -name scrcpy -print -quit)")"
if [ -z "$src" ] || [ ! -f "$src/scrcpy" ]; then
  die "静态包结构异常：解压后没有找到 scrcpy 可执行文件"
fi
if [ ! -f "$src/scrcpy-server" ]; then
  die "静态包结构异常：$src 下没有 scrcpy-server（预期与 scrcpy 同级）"
fi

# ---- 准备 adb -------------------------------------------------------------
if [ -f "$src/adb" ]; then
  echo "静态包已自带 adb，跳过 platform-tools 下载"
  adb_src="$src/adb"
else
  echo "静态包没有 adb，下载 Android platform-tools..."
  curl -fsSL --retry 3 --retry-delay 2 -H "User-Agent: phone-audio" \
    "$platform_tools_url" -o "$tmp/platform-tools.zip" \
    || die "platform-tools 下载失败：$platform_tools_url"

  mkdir -p "$tmp/platform-tools"
  unzip -q "$tmp/platform-tools.zip" -d "$tmp/platform-tools" || die "platform-tools 解压失败"
  if [ ! -f "$tmp/platform-tools/platform-tools/adb" ]; then
    die "platform-tools 结构异常：压缩包内缺少 platform-tools/adb"
  fi
  adb_src="$tmp/platform-tools/platform-tools/adb"
fi

# ---- 安装到 bin/ ----------------------------------------------------------
# 只复制运行所需的文件，保持 bin/ 干净（不需要 man page、图标等）。
cp -f "$src/scrcpy" bin/scrcpy
cp -f "$src/scrcpy-server" bin/scrcpy-server
cp -f "$adb_src" bin/adb
chmod 0755 bin/scrcpy bin/adb
chmod 0644 bin/scrcpy-server
# scrcpy 是 Apache-2.0，分发时需带上游 LICENSE（THIRD_PARTY_LICENSES.md 也引用了它）。
if [ -f "$src/LICENSE" ]; then
  cp -f "$src/LICENSE" bin/LICENSE.scrcpy
fi

# ---- 校验 -----------------------------------------------------------------
echo "校验版本："
scrcpy_version="$(bin/scrcpy --version 2>&1 | awk '/^scrcpy / { if (v == "") v = $2 } END { print v }')"
adb_version="$(bin/adb version 2>&1 | sed -n '1p')"
echo "  ${scrcpy_version:-(未知)}"
echo "  ${adb_version:-(未知)}"

[ -n "$scrcpy_version" ] || die "无法解析 bin/scrcpy --version 输出"
if ! awk -v v="$scrcpy_version" 'BEGIN {
      split(v, a, ".");
      if (a[1] > 2 || (a[1] == 2 && a[2] >= 1)) exit 0;
      exit 1;
    }'; then
  die "scrcpy 版本过旧（$scrcpy_version），需要 >= 2.1（旧版不支持 --audio-source 等参数）"
fi

echo "完成。scrcpy $scrcpy_version 与 adb 已放到 bin/："
echo "  bin/scrcpy"
echo "  bin/scrcpy-server"
echo "  bin/adb"
