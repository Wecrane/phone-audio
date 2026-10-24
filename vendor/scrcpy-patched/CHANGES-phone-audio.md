# scrcpy 5.0.1 phone-audio patch — changes

本项目对 [scrcpy](https://github.com/Genymobile/scrcpy) **v5.0.1** 的 Windows 客户端
做了最小修改，目的是让宿主机（电脑）可以选择音频播放的输出设备，并能在不连接
Android 设备的情况下枚举这些设备。

上游 scrcpy 5.0.1 **没有** `--audio-output-device` 选项：`app/src/audio_player.c`
里固定使用 `SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK`（系统默认输出设备）。

## 上游来源

| 项目 | 值 |
| --- | --- |
| 上游 tag | `v5.0.1` |
| 源码包 | `https://codeload.github.com/Genymobile/scrcpy/tar.gz/refs/tags/v5.0.1` |
| 源码包 SHA-256 | `A24B996AC23D0F674D3237C00B39A97829D8ACBE9E1657A6C216FD51EA488EE5` |
| 补丁文件 | `scrcpy-5.0.1-phone-audio.patch`（相对上游 v5.0.1 的 `git diff`） |
| 许可证 | Apache-2.0（见 `LICENSE`，随源码和分发目录一起保留） |

补丁对应 `git apply -p1`（`patch -p1` 亦可）可直接应用到 v5.0.1 源码树。

## 新增 CLI 选项

### `--audio-output-device=<name>`

- 指定 SDL 音频输出设备名（即 `--list-audio-devices` 打印的名字，原样传入，
  含空格、括号、中文等，需要按 shell/程序参数规则加引号）。
- 不指定、或传入空字符串（`--audio-output-device=`）时，使用系统默认输出设备，
  与上游行为完全一致。
- 设备名精确匹配 `SDL_GetAudioDeviceName()` 返回的字符串（大小写敏感）。
- 设备不存在时：音频播放器打开失败，客户端报错
  `Audio output device not found: <name>`，并提示用 `--list-audio-devices` 查看。
  （纯视频模式或 `--no-audio-playback` 不受影响。）

### `--list-audio-devices`

- 初始化 SDL 音频子系统，枚举当前已连接的物理播放设备并打印，每行一个设备名。
- **不需要连接 Android 设备**，不会启动 adb server；退出码 0 表示成功，非 0 表示
  SDL 音频初始化/枚举失败。
- 注意：stdout 第一行是 scrcpy 一贯的 banner
  `scrcpy 5.0.1 <https://github.com/Genymobile/scrcpy>`，设备名从第二行开始。
  解析示例（Windows PowerShell）：
  `scrcpy.exe --list-audio-devices | Select-Object -Skip 1`
  或过滤掉以 `scrcpy ` 开头且包含 `github.com/Genymobile/scrcpy` 的行。

## 修改的文件

| 文件 | 修改内容 |
| --- | --- |
| `app/src/audio_player.h` | `struct sc_audio_player` 新增 `output_device_name` 字段；`sc_audio_player_init()` 增加设备名参数；声明 `sc_audio_player_list_devices()`。 |
| `app/src/audio_player.c` | 新增 `sc_audio_player_list_devices()`（`SDL_Init(SDL_INIT_AUDIO)` + `SDL_GetAudioPlaybackDevices()` + `SDL_GetAudioDeviceName()` + `SDL_Quit()`）；新增 `sc_audio_player_find_device()`（按名字查找 SDL 设备 ID）；打开音频流时若指定了设备名，用查到的设备 ID 替换 `SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK`；新增 `#include <stdio.h>`、`<string.h>`、`SDL3/SDL_init.h`。 |
| `app/src/options.h` | `struct scrcpy_options` 新增 `const char *audio_output_device`。 |
| `app/src/options.c` | 默认值 `.audio_output_device = NULL`。 |
| `app/src/cli.h` | `struct scrcpy_cli_args` 新增 `bool list_audio_devices`（该动作不经过 server）。 |
| `app/src/cli.c` | 新增 `OPT_AUDIO_OUTPUT_DEVICE` / `OPT_LIST_AUDIO_DEVICES` 枚举、选项表条目与帮助文本；解析 `--audio-output-device`（空串 → NULL）并设置 `list_audio_devices`。 |
| `app/src/main.c` | 新增 `#include "audio_player.h"`；在 `--version` 之后、网络/server 初始化之前处理 `--list-audio-devices`，直接枚举本机设备并退出。 |
| `app/src/scrcpy.c` | 调用 `sc_audio_player_init()` 时把 `options->audio_output_device` 传给音频播放器。 |

未修改：`server/`（Java 端）、scrcpy 客户端/服务端协议、adb 相关逻辑、构建系统。
因此补丁后的客户端与官方 v5.0.1 的 `scrcpy-server` 完全兼容，可以直接使用官方
`scrcpy-server`（SHA-256 `764EB6F79811D5211FE9DF341120882BA9994C7A61B897D7BF3FB662E53BC536`）。

## 许可证声明（Apache-2.0 §4）

- 本补丁基于 Apache-2.0 许可的 scrcpy 修改而来，修改后的代码继续适用 Apache-2.0。
- 原版权声明与 `LICENSE` 文件已保留（`vendor/scrcpy-src/LICENSE`、`win64/LICENSE.txt`）。
- 修改过的文件已在上表逐一列出；补丁文件本身即完整 diff，可审计。

## 构建环境与产物

- 构建机：Windows 11 x64 + MSYS2 UCRT64，
  gcc 16.2.0 / meson 1.12.1 / ninja 1.13.2 / SDL3 3.4.18 /
  FFmpeg 9.0.2 (libavcodec 63.1.102) / libusb 1.0.30。
- 产物：`win64/scrcpy.exe`（CUI 子系统，portable 构建，
  与官方 scrcpy-win64-v5.0.1 的 DLL 完全匹配）。
- 可复现构建脚本：`build-windows.sh`；构建/验证细节见
  `docs/scrcpy-output-device.md`。
