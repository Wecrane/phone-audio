# Third-party licenses and distribution notes

## scrcpy — Apache License 2.0

Upstream: <https://github.com/Genymobile/scrcpy>

This project **modifies** scrcpy v5.0.1 for Windows to add computer audio output
device selection:

- modified files: `app/src/audio_player.c`, `app/src/audio_player.h`,
  `app/src/cli.c`, `app/src/options.c`, `app/src/options.h`, `app/src/scrcpy.c`,
  `app/src/scrcpy.h`, `app/meson.build` (see the exact diff);
- patch: `vendor/scrcpy-patched/scrcpy-5.0.1-phone-audio.patch`;
- change description: `vendor/scrcpy-patched/CHANGES-phone-audio.md`;
- reproducible build script: `vendor/scrcpy-patched/build-windows.sh`;
- the modified binary is `vendor/scrcpy-patched/win64/scrcpy.exe` and is bundled
  into the Windows release (`bin/scrcpy.exe`), with the patch and change notes
  also shipped under `scrcpy-patched/`.

Apache-2.0 obligations that are met:

- include scrcpy's `LICENSE` (bundled at `licenses/Apache-2.0.txt` and shipped
  as `bin/LICENSE.txt` / `scrcpy-patched/LICENSE.scrcpy`);
- keep copyright and attribution notices (see `NOTICE`);
- mark modified files and state the changes (patch + CHANGES + this file);
- do not use the scrcpy name or logo in a way that implies endorsement
  (Apache-2.0 §6); the product is called Phone Audio, not scrcpy.

## adb / Android SDK Platform-Tools — Apache License 2.0

Upstream: <https://developer.android.com/tools/releases/platform-tools>

The scrcpy Windows release bundles `adb.exe`, `AdbWinApi.dll` and
`AdbWinUsbApi.dll`; keep Google's license notices when repackaging.

The Linux release uses the `adb` binary that ships inside the official scrcpy
Linux static tarball (or platform-tools when the tarball lacks it); the same
Apache-2.0 notice applies.

## FFmpeg — LGPL-2.1-or-later

Upstream: <https://ffmpeg.org/legal.html>

- **Windows release**: scrcpy links FFmpeg dynamically. The release ships
  `avcodec-63.dll`, `avformat-63.dll`, `avutil-61.dll`, `swresample-7.dll`
  taken from the *official scrcpy win64 5.0.1 package*, so LGPL dynamic-linking
  obligations are the same as distributing upstream scrcpy.
- **Linux release**: the official scrcpy Linux static tarball links FFmpeg (and
  SDL) statically into a single `scrcpy` executable. If you redistribute that
  Linux package, LGPL-2.1 §6 requires allowing users to relink with a modified
  FFmpeg — provide the corresponding source or a written offer, and keep the
  upstream source archive/URL documented before publishing. This note is a
  reminder, not legal advice.

LGPL-2.1 text: `licenses/LGPL-2.1.txt`.
FFmpeg source: <https://ffmpeg.org/download.html>.

## SDL3 — zlib license

Upstream: <https://www.libsdl.org/license.php>

The Windows build links SDL3 dynamically (`SDL3.dll`, from the official scrcpy
package). The Linux static build links SDL statically (zlib license, attribution
required).

## libusb — LGPL-2.1-or-later

Upstream: <https://libusb.info/>

scrcpy uses libusb for USB/AOA features. The Windows package ships
`libusb-1.0.dll`; the Linux static scrcpy may link it statically.

## Opus — BSD 3-Clause

Upstream: <https://opus-codec.org/license/>

Used by scrcpy/Android for the Opus audio codec.

## Rust dependencies

`eframe`, `egui`, `serde`, `serde_json`, `directories` and their transitive
dependencies are licensed under MIT / Apache-2.0 / BSD-style licenses.
`Cargo.lock` is committed for reproducible builds; run `cargo install
cargo-license && cargo license` (or `cargo about`) before a public release and
include the generated report.
