#!/bin/bash
# Build the phone-audio patched scrcpy 5.0.1 Windows x86_64 client.
#
# Requirements:
#   - MSYS2 UCRT64 environment (https://www.msys2.org/)
#   - network access to the MSYS2 mirrors (for pacman) on the first run
#   - the official scrcpy-win64-v5.0.1 package extracted in
#     ./official-win64-v5.0.1 (used for the runtime DLLs and scrcpy-server,
#     they are self-contained, unlike the MSYS2 FFmpeg package DLLs)
#
# Usage (PowerShell):
#   $env:MSYSTEM='UCRT64'
#   & 'D:\Program Files\msys2\usr\bin\bash.exe' -lc 'cd /g/phone-audio/vendor/scrcpy-patched && ./build-windows.sh'
#
# Or from an "MSYS2 UCRT64" shell:
#   cd /g/phone-audio/vendor/scrcpy-patched && ./build-windows.sh
#
# The patched sources live in ../scrcpy-src (upstream v5.0.1 + patch).
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
SRC_DIR="${SRC_DIR:-$HERE/../scrcpy-src}"
BUILD_DIR="${BUILD_DIR:-$HERE/../build-ucrt64}"
OUT_DIR="${OUT_DIR:-$HERE/win64}"
DEPS_DIR="${DEPS_DIR:-$HERE/official-win64-v5.0.1}"
DOWNLOADS_DIR="${DOWNLOADS_DIR:-$HERE/../downloads}"

SCRCPY_VERSION="5.0.1"
SOURCE_SHA256="A24B996AC23D0F674D3237C00B39A97829D8ACBE9E1657A6C216FD51EA488EE5"
OFFICIAL_WIN64_SHA256="B12A2C4EE8BE317422451FC7DCF8EE20A71B5EA7EF9AD73DDD825A25316227A5"

# --- Prepare pristine sources / official package ----------------------------
# Downloads are cached in vendor/downloads (git-ignored); a clean checkout can
# rebuild by fetching the exact upstream files and verifying their SHA-256.
if [ ! -d "$SRC_DIR/app" ]; then
    tarball="$DOWNLOADS_DIR/scrcpy-$SCRCPY_VERSION.tar.gz"
    mkdir -p "$DOWNLOADS_DIR"
    if [ ! -f "$tarball" ]; then
        echo "downloading scrcpy $SCRCPY_VERSION sources..."
        curl -fsSL --retry 3 \
            "https://codeload.github.com/Genymobile/scrcpy/tar.gz/refs/tags/v$SCRCPY_VERSION" \
            -o "$tarball" \
            || { echo "error: failed to download $tarball" >&2; exit 1; }
    fi
    echo "$SOURCE_SHA256  $tarball" | sha256sum -c - >/dev/null \
        || { echo "error: source tarball SHA-256 mismatch" >&2; exit 1; }
    echo "extracting scrcpy $SCRCPY_VERSION sources..."
    mkdir -p "$SRC_DIR"
    tar -xzf "$tarball" -C "$SRC_DIR" --strip-components=1
    echo "applying phone-audio patch..."
    (
        cd "$SRC_DIR"
        patch -p1 --batch < "$HERE/scrcpy-$SCRCPY_VERSION-phone-audio.patch"
    )
fi

if [ ! -d "$DEPS_DIR" ]; then
    zip="$DOWNLOADS_DIR/scrcpy-win64-v$SCRCPY_VERSION.zip"
    mkdir -p "$DOWNLOADS_DIR"
    if [ ! -f "$zip" ]; then
        echo "downloading official scrcpy win64 package..."
        curl -fsSL --retry 3 \
            "https://github.com/Genymobile/scrcpy/releases/download/v$SCRCPY_VERSION/scrcpy-win64-v$SCRCPY_VERSION.zip" \
            -o "$zip" \
            || { echo "error: failed to download $zip" >&2; exit 1; }
    fi
    echo "$OFFICIAL_WIN64_SHA256  $zip" | sha256sum -c - >/dev/null \
        || { echo "error: official win64 package SHA-256 mismatch" >&2; exit 1; }
    echo "extracting official scrcpy win64 package..."
    mkdir -p "$DEPS_DIR"
    unzip -q "$zip" -d "$DEPS_DIR.tmp"
    inner="$(find "$DEPS_DIR.tmp" -mindepth 1 -maxdepth 1 -type d | head -n1)"
    if [ -n "$inner" ]; then
        mv "$inner"/* "$DEPS_DIR"/
    else
        mv "$DEPS_DIR.tmp"/* "$DEPS_DIR"/
    fi
    rm -rf "$DEPS_DIR.tmp"
fi

if [ ! -d "$SRC_DIR/app" ]; then
    echo "error: patched scrcpy source tree not found: $SRC_DIR" >&2
    exit 1
fi
if [ ! -d "$DEPS_DIR" ]; then
    echo "error: official scrcpy-win64-v5.0.1 package not found: $DEPS_DIR" >&2
    echo "       see docs/scrcpy-output-device.md for the download URL" >&2
    exit 1
fi

# Dependencies, see scrcpy doc/build.md ("In MSYS2").
pacman -S --needed --noconfirm \
    patch unzip \
    mingw-w64-ucrt-x86_64-gcc \
    mingw-w64-ucrt-x86_64-pkgconf \
    mingw-w64-ucrt-x86_64-meson \
    mingw-w64-ucrt-x86_64-ninja \
    mingw-w64-ucrt-x86_64-make \
    mingw-w64-ucrt-x86_64-sdl3 \
    mingw-w64-ucrt-x86_64-ffmpeg \
    mingw-w64-ucrt-x86_64-libusb

# GNU ld's LTO wrapper breaks when the temporary directory contains spaces
# (MSYS2's default TMPDIR is "D:\Program Files\msys2\tmp"), so point TMPDIR to
# a space-free directory.
TMP_DIR="$HERE/../tmp"
mkdir -p "$TMP_DIR"
export TMPDIR="$TMP_DIR"
export TMP="$TMP_DIR"
export TEMP="$TMP_DIR"

start=$(date +%s)
cd "$SRC_DIR"
rm -rf "$BUILD_DIR"
meson setup "$BUILD_DIR" \
    --buildtype=release --strip -Db_lto=true \
    -Dcompile_server=false -Dportable=true
mid=$(date +%s)
ninja -C "$BUILD_DIR"
end=$(date +%s)

mkdir -p "$OUT_DIR"
cp "$BUILD_DIR/app/scrcpy.exe" "$OUT_DIR/scrcpy.exe"
cp "$SRC_DIR/LICENSE" "$OUT_DIR/LICENSE.txt"

# Runtime dependencies. The official release DLLs are exactly the versions used
# at build time (SDL 3.4.18 / FFmpeg 9.0.2 / libusb 1.0.30) and are
# self-contained; using the MSYS2 package DLLs instead would also require
# shipping their extra codec dependencies.
for f in SDL3.dll avcodec-63.dll avformat-63.dll avutil-61.dll \
         swresample-7.dll libusb-1.0.dll scrcpy-server; do
    cp "$DEPS_DIR/$f" "$OUT_DIR/$f"
done

echo
echo "built: $OUT_DIR/scrcpy.exe"
echo "meson setup: $((mid - start))s, ninja: $((end - mid))s"
"$OUT_DIR/scrcpy.exe" --version
