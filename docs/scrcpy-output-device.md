# scrcpy 选择电脑音频输出设备：补丁、Windows 构建与实测

> 任务：上游 scrcpy v5.0.1 没有选择**电脑音频输出设备**的选项。本文件记录
> 为它新增 `--audio-output-device` / `--list-audio-devices` 的补丁设计、
> Windows 构建过程、实测输出、分发清单、CI 复用方式和已知限制。
>
> 结论：**构建成功，实测通过**。产物在
> `vendor/scrcpy-patched/win64/`，补丁在
> `vendor/scrcpy-patched/scrcpy-5.0.1-phone-audio.patch`。

## 1. 摘要

| 项 | 结果 |
| --- | --- |
| 上游版本 | scrcpy **v5.0.1**（客户端为 C + **SDL3**，不是 SDL2） |
| 上游是否支持选择输出设备 | 否。`app/src/audio_player.c` 固定 `SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK` |
| 补丁规模 | 8 个文件 +123/−5 行，仅客户端，不动 server/协议 |
| Windows 构建 | 成功。MSYS2 UCRT64 + meson/ninja，release + LTO + strip |
| `scrcpy.exe --version` | 退出码 0，见 §5.1 |
| `scrcpy.exe --list-audio-devices` | 退出码 0，列出本机 5 个真实输出设备，见 §5.2 |
| `scrcpy.exe --help` | 两个新选项均出现，见 §5.3 |
| 端到端（连手机播音频到指定设备） | **未测**（任务要求不连手机）；用独立 SDL 测试验证了"设备名→设备 ID→打开"路径，见 §5.5 |

## 2. 上游事实核对

- scrcpy 5.0.1 客户端使用 **SDL3**（`#include <SDL3/SDL_audio.h>`，
  `SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, ...)`），
  任务背景里"SDL2 的 `SDL_OpenAudioDevice(NULL, ...)` / `SDL_GetNumAudioDevices`"
  在 5.0.1 已不适用：SDL3 中枚举播放设备的 API 是
  `SDL_GetAudioPlaybackDevices(int *count)` + `SDL_GetAudioDeviceName(devid)`，
  默认设备常量是 `SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK`。
- `app/src/scrcpy.c` 在 `audio_playback` 时才 `SDL_Init(SDL_INIT_AUDIO)`，
  且整个流程先启动 server/adb，所以"枚举本机设备"必须在 `main.c` 里提前处理，
  不能走 `options->list`（那个会启动 adb/server）。
- 官方 win64 发行包（`scrcpy-win64-v5.0.1.zip`）自带
  `SDL3.dll 3.4.18`、`avcodec-63/avformat-63/avutil-61/swresample-7`（FFmpeg 9.0.2）、
  `libusb-1.0.dll 1.0.30`；MSYS2 UCRT64 当前包版本与之**完全一致**
  （见 §5.1 的 `Dependencies (compiled / linked)`），因此可以用官方 DLL 做运行时分发。

## 3. 补丁设计

补丁文件：`vendor/scrcpy-patched/scrcpy-5.0.1-phone-audio.patch`
（针对 v5.0.1 源码树的 `git diff`，`git apply -p1` 或 `patch -p1` 均可）。
修改说明（Apache-2.0 §4）：`vendor/scrcpy-patched/CHANGES-phone-audio.md`。

### 3.1 行为

- `--audio-output-device=<name>`：按 SDL 设备名精确匹配当前播放设备，
  用匹配到的 `SDL_AudioDeviceID` 调 `SDL_OpenAudioDeviceStream()`；
  不传 / 传空串时行为与上游完全一致（默认设备）。
  名字不存在时报错并使音频播放器打开失败（不会静默回退到默认设备）。
- `--list-audio-devices`：`SDL_Init(SDL_INIT_AUDIO)` →
  枚举 → 每行打印一个设备名 → `SDL_Quit()`；不启动 adb/server，
  不需要连接手机，退出码 0/1。
- 其余逻辑（设备端采集、编码、传输、音频调节器）不变；server 无需重新构建。

### 3.2 修改的文件

| 文件 | 修改 |
| --- | --- |
| `app/src/audio_player.h` | 新增 `output_device_name` 字段；`sc_audio_player_init()` 加参数；声明 `sc_audio_player_list_devices()` |
| `app/src/audio_player.c` | 实现枚举/按名查找/指定设备打开；新增 `<stdio.h>`、`<string.h>`、`SDL3/SDL_init.h` |
| `app/src/options.h` | 新增 `const char *audio_output_device` |
| `app/src/options.c` | 默认 `NULL` |
| `app/src/cli.h` | `struct scrcpy_cli_args` 新增 `bool list_audio_devices` |
| `app/src/cli.c` | 两个新选项的枚举/选项表/帮助文本/解析（空串→NULL） |
| `app/src/main.c` | 在 `--version` 之后、网络/server 初始化之前处理 `--list-audio-devices` |
| `app/src/scrcpy.c` | 把 `options->audio_output_device` 传给音频播放器 |

关键代码（`app/src/audio_player.c`）：

```c
bool
sc_audio_player_list_devices(void) {
    if (!SDL_Init(SDL_INIT_AUDIO)) { ... return false; }
    int count;
    SDL_AudioDeviceID *devices = SDL_GetAudioPlaybackDevices(&count);
    ...
    for (int i = 0; i < count; ++i) {
        const char *name = SDL_GetAudioDeviceName(devices[i]);
        if (name) printf("%s\n", name);
    }
    SDL_free(devices);
    SDL_Quit();
    return true;
}
```

```c
    SDL_AudioDeviceID device = SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK;
    if (ap->output_device_name) {
        if (!sc_audio_player_find_device(ap->output_device_name, &device)) {
            LOGE("Audio output device not found: %s", ap->output_device_name);
            LOGE("List the available devices with --list-audio-devices");
            ... return false;
        }
    }
    ap->stream = SDL_OpenAudioDeviceStream(device, &spec,
                                           sc_audio_player_stream_callback, ap);
```

## 4. 构建（可复现）

### 4.1 本机构建环境

- Windows 11 x64，无 MSVC；MSYS2 安装于 `D:\Program Files\msys2`。
- UCRT64 工具链（实测版本）：

| 包 | 版本 |
| --- | --- |
| mingw-w64-ucrt-x86_64-gcc | 16.2.0-6 |
| mingw-w64-ucrt-x86_64-pkgconf | 1~3.0.10-1 |
| mingw-w64-ucrt-x86_64-meson | 1.12.1-1 |
| mingw-w64-ucrt-x86_64-ninja | 1.13.2-1 |
| mingw-w64-ucrt-x86_64-make | 4.4.1-5 |
| mingw-w64-ucrt-x86_64-sdl3 | 3.4.18-1 |
| mingw-w64-ucrt-x86_64-ffmpeg | 9.0.2-1（libavcodec 63.1.102） |
| mingw-w64-ucrt-x86_64-libusb | 1.0.30-1 |
| msys2-runtime | 3.6.11-4 |

### 4.2 一次性环境准备（pacman 镜像）

MSYS2 默认源在国内较慢，已在
`D:\Program Files\msys2\etc\pacman.d\` 把清华源放到最前面（其余行保留）：

```
# mirrorlist.msys
Server = https://mirrors.tuna.tsinghua.edu.cn/msys2/msys/$arch/
# mirrorlist.mingw / mirrorlist.mingw64 / mirrorlist.ucrt64
Server = https://mirrors.tuna.tsinghua.edu.cn/msys2/mingw/$repo/
```

改动前备份（4 个文件）：
`mirrorlist.msys.bak-20261023-234213`、`mirrorlist.mingw.bak-20261023-234213`、
`mirrorlist.mingw64.bak-20261023-234213`、`mirrorlist.ucrt64.bak-20261023-234213`。
随后执行过一次 `pacman -Syu --noconfirm` 全量升级，再安装上表依赖。

### 4.3 构建命令（实测）

```powershell
$env:MSYSTEM='UCRT64'
& 'D:\Program Files\msys2\usr\bin\bash.exe' -lc 'cd /g/phone-audio/vendor/scrcpy-patched && ./build-windows.sh'
```

`build-windows.sh` 等价于（照抄实测命令；构建产物在 `vendor/build-ucrt64/`）：

```bash
# MSYS2 UCRT64 shell 内
pacman -S --needed --noconfirm \
  mingw-w64-ucrt-x86_64-gcc mingw-w64-ucrt-x86_64-pkgconf \
  mingw-w64-ucrt-x86_64-meson mingw-w64-ucrt-x86_64-ninja \
  mingw-w64-ucrt-x86_64-make mingw-w64-ucrt-x86_64-sdl3 \
  mingw-w64-ucrt-x86_64-ffmpeg mingw-w64-ucrt-x86_64-libusb

# 关键坑：GNU ld 的 LTO wrapper 无法处理含空格的临时目录
# （MSYS2 默认 TMPDIR = "D:\Program Files\msys2\tmp"），必须换掉
export TMPDIR=/g/phone-audio/vendor/tmp TMP=$TMPDIR TEMP=$TMPDIR
mkdir -p "$TMPDIR"

cd /g/phone-audio/vendor/scrcpy-src
meson setup /g/phone-audio/vendor/build-ucrt64 \
    --buildtype=release --strip -Db_lto=true \
    -Dcompile_server=false -Dportable=true
ninja -C /g/phone-audio/vendor/build-ucrt64
```

- `-Dcompile_server=false -Dportable=true` 与官方 `release/build_windows.sh`
  一致：不编译 Java server，客户端从 exe 同目录找 `scrcpy-server`。
- 干净构建耗时（本机）：**meson setup 11s + ninja 17s = 28s**；
  首次环境（pacman 下载安装）约 15 分钟（含清华源）。
- 产物 `vendor/build-ucrt64/app/scrcpy.exe`：483,327 字节，
  PE32+ CUI（console）子系统，导入表只有
  `SDL3.dll / avcodec-63 / avformat-63 / avutil-61 / swresample-7 / libusb-1.0`
  + Windows 系统 DLL，不依赖 `libwinpthread-1.dll` 等 MSYS2 运行时。

### 4.4 运行时分发（用官方 DLL）

`win64/` 由 `build-windows.sh` 组装：我们的 `scrcpy.exe` + 官方
`scrcpy-win64-v5.0.1.zip` 里的 DLL 与 `scrcpy-server`（**该 zip 已下载到
`vendor/downloads/scrcpy-win64-v5.0.1.zip` 并解压到
`vendor/scrcpy-patched/official-win64-v5.0.1/`**）。

这样做的原因：MSYS2 的 ffmpeg DLL 依赖大量额外 codec DLL（x264/x265/…），
逐个收集进分发目录很麻烦；官方包里的 FFmpeg/SDL3/libusb DLL 是自包含的，
且版本与编译期头文件/导入库完全一致（FFmpeg 9.0.2、SDL 3.4.18、libusb 1.0.30），
不存在 ABI 风险。已实测：把 PATH 限制为 `win64` + `%SystemRoot%\System32` 时
所有命令正常，证明分发目录自包含。

### 4.5 构建踩坑记录（原始错误）

1. **git clone 失败 / raw.githubusercontent SSL 失败**
   - `git clone https://github.com/Genymobile/scrcpy.git` 报
     `Failed to connect to github.com:443 over proxy 127.0.0.1`（全局 git
     `http.proxy=http://127.0.0.1:7897` 已失效）；清空代理后直连也超时。
   - `raw.githubusercontent.com` 报 `The SSL connection could not be established`。
   - 解决：用 `Invoke-WebRequest` 下载 codeload tarball
     `https://codeload.github.com/Genymobile/scrcpy/tar.gz/refs/tags/v5.0.1`
     （可用）；官方 release zip 用 `curl.exe -L --ssl-no-revoke`
     （curl/Schannel 可访问，.NET 不行）。源码包 SHA-256：
     `A24B996AC23D0F674D3237C00B39A97829D8ACBE9E1657A6C216FD51EA488EE5`。
2. **LTO 链接失败：路径含空格**
   - 原始错误：
     `lto1.exe: error: D:\Program Files\msys2\tmp\cciMI8vE.ltrans2.o: file too short`
     / `make: *** [.../Files\msys2\tmp/...] 错误 1` /
     `lto-wrapper.exe: fatal error: make returned 2 exit status`。
   - 解决：`export TMPDIR=/g/phone-audio/vendor/tmp`（无空格路径）后链接成功。
3. **SDL3 API 名称**
   - 初版按记忆写了 `SDL_GetAudioOutputDevices()`，SDL 3.4.18 头文件里不存在；
     正确 API 是 `SDL_GetAudioPlaybackDevices()`（自 SDL 3.2.0 起）。
4. **`--version` / `--list-audio-devices` 首次测试“无效”**
   - 原因是测试脚本把 PowerShell 自动变量 `$args` 当函数参数用，参数没传给
     exe，与构建无关；改用 `Start-Process -ArgumentList` + 独立变量后正常。
   - 教训：验证脚本里不要用 `$args`、`$input` 等自动变量名。

## 5. 实测输出（原始）

测试方式：`PATH` 只保留 `vendor/scrcpy-patched/win64` 与系统目录，
工作目录 `win64/`，输出原样保存到 `vendor/scrcpy-patched/verify/`。

### 5.1 `scrcpy.exe --version`（退出码 0）

文件：`verify/version.out.txt`

```
scrcpy 5.0.1 <https://github.com/Genymobile/scrcpy>

Dependencies (compiled / linked):
 - SDL: 3.4.18 / 3.4.18
 - libavcodec: 63.1.102 / 63.1.102
 - libavformat: 63.1.102 / 63.1.102
 - libavutil: 61.1.102 / 61.1.102
 - libusb: - / 1.0.30
```

stderr 为空。

### 5.2 `scrcpy.exe --list-audio-devices`（退出码 0）

文件：`verify/list-audio-devices.out.txt`，stderr 为空。本机真实输出
（第一行是 scrcpy 常规 banner，其后才是设备名；UTF-8，CRLF 行尾）：

```
scrcpy 5.0.1 <https://github.com/Genymobile/scrcpy>
耳机 (Lian II)
扬声器 (Realtek(R) Audio)
G27H2Max (NVIDIA High Definition Audio)
Virtual Speakers (Virtual Speakers for AudioRelay)
扬声器 (ToDesk Virtual Audio)
```

即 5 个真实播放设备（含中文名），不是空列表。
主程序解析时应跳过 banner 行（详见 §7.1）。

### 5.3 `scrcpy.exe --help` 中的新选项（退出码 0）

文件：`verify/help.out.txt`（27,706 字节），`grep` 到的行号：

```
73:     --audio-output-device=name
75:         The available device names can be listed by --list-audio-devices.
242:     --list-audio-devices
```

对应帮助正文：

```
    --audio-output-device=name
        Select the audio output device on the computer (SDL audio device name).
        The available device names can be listed by --list-audio-devices.
        If not set (or set to an empty name), the default audio output device is
        used.
...
    --list-audio-devices
        List the audio output devices available on the computer (this does not
        require any Android device).
```

### 5.4 参数解析/退出码补充测试

| 命令 | 退出码 | 说明 |
| --- | --- | --- |
| `scrcpy.exe --audio-output-device="扬声器 (Realtek(R) Audio)" --version` | 0 | 中文+空格设备名可正常解析（`verify/device-option-parse.out.txt`） |
| `scrcpy.exe --audio-output-device= --version` | 0 | 空串 = 默认设备（`verify/empty-device-option.out.txt`） |
| `scrcpy.exe --not-a-real-option` | 1 | getopt 报 `unknown option`（`verify/bad-option.err.txt`） |

未连接手机时正常运行（不带 `--version`/`--list-audio-devices`）仍是上游行为：
`ERROR: Could not find any ADB device`，退出码 1；未执行 `adb kill-server`，
未做任何 adb server 操作。

### 5.5 辅助验证：按设备名打开 SDL 输出流

因为不连手机无法走到 scrcpy 的音频播放器 open 路径，另写了独立小测试
（`verify/sdl_device_open_test.c`），复刻补丁"枚举 → 按名字找 ID →
`SDL_OpenAudioDeviceStream(id)`"的 SDL 调用。实测结果
（`verify/sdl-device-open.out.txt`，退出码 0）：

```
enumerated 5 playback device(s)
device[0] id=15 name=耳机 (Lian II)
  open OK (stream device id=49 name=耳机 (Lian II))
device[1] id=19 name=扬声器 (Realtek(R) Audio)
  open OK (stream device id=53 name=扬声器 (Realtek(R) Audio))
device[2] id=23 name=G27H2Max (NVIDIA High Definition Audio)
  open OK (stream device id=57 name=G27H2Max (NVIDIA High Definition Audio))
device[3] id=27 name=Virtual Speakers (Virtual Speakers for AudioRelay)
  open OK (stream device id=61 name=Virtual Speakers (Virtual Speakers for AudioRelay))
device[4] id=31 name=扬声器 (ToDesk Virtual Audio)
  open OK (stream device id=65 name=扬声器 (ToDesk Virtual Audio))
```

说明在本机所有枚举到的输出设备都可以按 ID 成功打开（包括中文名设备）。

## 6. 分发清单（`vendor/scrcpy-patched/win64/`）

| 文件 | 大小（字节） | SHA-256 |
| --- | --- | --- |
| `scrcpy.exe` | 483327 | `20CE66F347D77A6784005918798517574EEFA729728991380A3FCF547F057F99` |
| `scrcpy-server` | 733930 | `764EB6F79811D5211FE9DF341120882BA9994C7A61B897D7BF3FB662E53BC536` |
| `SDL3.dll` | 5384859 | `288AAF5B4F246BD84C276351867E3132CFCD9B0A9D25FB679C8D94B30CDF333E` |
| `avcodec-63.dll` | 8685568 | `159928E3BBE7BEACA046CCF831FDF44F1E5559C0E63417F007889A6BB2C5DE88` |
| `avformat-63.dll` | 743424 | `4DBB0C1C8262F7AE45E7EDC4866E26F53B358BF4FC7A0B971F15D1F2D7BAF22F` |
| `avutil-61.dll` | 1047552 | `52EB504CED0EA6D9E936D348DF51C8AAEEC1CAE8FA4A7DBA339FEA87D7CCE725` |
| `swresample-7.dll` | 139776 | `7B57E891BF09C43422D79F98CF9D1830C4B1D48A251423CE5FB8C2048EB8FC1E` |
| `libusb-1.0.dll` | 227005 | `0C82E969C046D816507A2FE1E57253A334CB203BD6EA223194AD17C007154F32` |
| `adb.exe` | 8273560 | `B4A6B455702684652CCCF7B46258B29E653538904359A58FD4931CF3EF286B3F` |
| `AdbWinApi.dll` | 108184 | `C1D653030B4BDE65D3E07E4D0B0979E17BE56DF1436CDD15528630F27808050D` |
| `AdbWinUsbApi.dll` | 73368 | `0710E894D9B40F71A670C13C694079D564C92C1279DA382CFE4850983AAEBE1B` |
| `scrcpy.png` / `disconnected.png` | 6530 / 4703 | （图标，同官方包） |
| `scrcpy-noconsole.vbs` / `open_a_terminal_here.bat` | 212 / 5 | （同官方包） |
| `LICENSE.txt` | 11387 | scrcpy 的 Apache-2.0 LICENSE |
| `CHANGES-phone-audio.md` | — | 本补丁修改说明（Apache-2.0 §4） |

说明：

- `scrcpy.exe` 是**我们补丁构建**的（比官方包同版本 exe 小，LTO+strip 差异），
  `scrcpy-server` 与 6 个 DLL 来自官方 `scrcpy-win64-v5.0.1.zip`
  （原始 zip 保存在 `vendor/downloads/`，解压参考在
  `vendor/scrcpy-patched/official-win64-v5.0.1/`）。
- `adb.exe`/`AdbWin*.dll` 一并放入是为了让 win64 目录与官方发行包布局一致、
  可直接独立运行；主程序若已从 `bin/` 分发 adb，可忽略这三个文件
  （scrcpy 客户端在 Windows 上通过 `PATH` 或 `ADB` 环境变量找 adb）。
- 官方 `scrcpy-server` 与补丁客户端版本匹配（协议未改），可用
  `SCRCPY_SERVER_PATH` 覆盖路径，portable 构建默认找 exe 同目录。
- `scrcpy.exe` 的哈希在两次干净构建间不同（PE 时间戳等导致非位级可复现），
  大小与行为一致；`scrcpy-server`/DLL 哈希稳定。

## 7. 供主程序（phone-audio）集成

### 7.1 列出设备

调用 `scrcpy.exe --list-audio-devices`，退出码 0 后按行解析：

- 跳过第一行 banner：形如 `scrcpy 5.0.1 <https://github.com/Genymobile/scrcpy>`
  （判断：行以 `scrcpy ` 开头且包含 `<https://github.com/Genymobile/scrcpy>`）。
- 其余非空行原样作为设备名（保留中文、空格、括号；不要 trim 内部空格）。

Rust 侧建议（伪代码）：

```rust
let out = Command::new(scrcpy_exe).arg("--list-audio-devices").output()?;
let text = String::from_utf8_lossy(&out.stdout);
let devices: Vec<String> = text
    .lines()
    .filter(|l| !l.is_empty())
    .filter(|l| !(l.starts_with("scrcpy ") && l.contains("github.com/Genymobile/scrcpy")))
    .map(str::to_owned)
    .collect();
```

### 7.2 启动时指定输出设备

在原有 scrcpy 参数里追加：

```
--audio-output-device="<设备名>"
```

- 用户选择"默认"时不要传该参数（或传 `--audio-output-device=`）。
- 传了不存在的设备名：scrcpy 会在打开音频时失败并输出
  `Audio output device not found: <name>`，此时应提示用户重新选择
  （注意 `--require-audio` 语义：视频仍可继续时 scrcpy 默认只镜像视频，
  行为取决于现有参数组合）。
- 设备名由 `--list-audio-devices` 原样取得最可靠（精确匹配）。

### 7.3 其他参数兼容性备注

- 该补丁只增加选项，不改变任何既有参数名/语义，原 Rust 侧参数无需改动。
- 上游 5.0.1 已有 `--no-window`、`--no-video-playback`、`--audio-source`、
  `--audio-buffer`、`--audio-output-buffer`、`--audio-dup` 等（由探测任务核对）。

## 8. CI 复用

### 8.1 Windows runner（推荐，与本次构建一致）

```yaml
- name: Setup MSYS2
  uses: msys2/setup-msys2@v2
  with:
    msystem: UCRT64
    update: true
    install: >-
      mingw-w64-ucrt-x86_64-gcc
      mingw-w64-ucrt-x86_64-pkgconf
      mingw-w64-ucrt-x86_64-meson
      mingw-w64-ucrt-x86_64-ninja
      mingw-w64-ucrt-x86_64-sdl3
      mingw-w64-ucrt-x86_64-ffmpeg
      mingw-w64-ucrt-x86_64-libusb
      curl
      unzip

- name: Build patched scrcpy
  shell: msys2 {0}
  run: |
    export TMPDIR="$RUNNER_TEMP/scrcpy-tmp"   # avoid spaces (LTO wrapper bug)
    mkdir -p "$TMPDIR"
    cd vendor/scrcpy-src
    meson setup build --buildtype=release --strip -Db_lto=true \
        -Dcompile_server=false -Dportable=true
    ninja -C build

- name: Assemble win64 artifact
  shell: msys2 {0}
  run: |
    # 官方发行包提供运行时 DLL + scrcpy-server（也可从项目 bin/ 复用）
    curl -L --ssl-no-revoke -o scrcpy-win64.zip \
      https://github.com/Genymobile/scrcpy/releases/download/v5.0.1/scrcpy-win64-v5.0.1.zip
    unzip -o scrcpy-win64.zip -d official
    mkdir -p vendor/scrcpy-patched/win64
    cp vendor/scrcpy-src/build/app/scrcpy.exe vendor/scrcpy-patched/win64/
    cp official/scrcpy-win64-v5.0.1/{SDL3.dll,avcodec-63.dll,avformat-63.dll,avutil-61.dll,swresample-7.dll,libusb-1.0.dll,scrcpy-server,LICENSE.txt} vendor/scrcpy-patched/win64/
```

- 若 runner 上 MSYS2 的 TMPDIR 无空格（通常是 `C:\Users\...\AppData\Local\Temp`），
  可省略 TMPDIR 覆盖；保险起见建议显式设置。
- 校验产物：`vendor/scrcpy-patched/win64/scrcpy.exe --version`、
  `--list-audio-devices`（CI 无声卡时可能返回空列表，但退出码应为 0；
  也可只断言 `--help` 含新选项）。
- 本次环境里 `pacman -Syu` 全量升级过一次；CI 用 `msys2/setup-msys2` 的
  `update: true` 即可。清华源仅为本机加速，不写入项目 CI。

### 8.2 Linux 交叉编译（可选）

补丁是纯 C、无平台相关代码；上游官方发行包就是 Linux 交叉编译的。
若 CI 已有 Linux 交叉环境，可复用上游 `release/build_windows.sh`
（需要 mingw-w64 + 源码里的 `app/deps/*.sh` 构建依赖），
但耗时更长，且需要自建 FFmpeg/SDL3/libusb，不建议在本项目 CI 里做。
当前 Windows CI 方案已验证。

## 9. 已知限制

1. **设备名精确匹配**：`--audio-output-device` 与
   `SDL_GetAudioDeviceName()` 返回值逐字节比较（区分大小写）；
   设备重命名/热插拔后旧名字会失效，需重新 `--list-audio-devices`。
2. **同名设备**：枚举结果若有完全同名的多个端点，按名匹配只能命中第一个
   （SDL3 没有独立的稳定 ID 暴露在 CLI 层；本次 5 个设备名字唯一）。
3. **热插拔/运行中改默认设备**：音频流只在开始时打开一次；
   设备拔出或默认设备改变不会自动切换（上游行为亦如此），需重启会话。
4. **失败不回退**：指定设备名无效时不会静默用默认设备，而是报错打开失败，
   以便上层明确提示用户（这是有意设计）。
5. **banner 行**：`--list-audio-devices` 沿用 scrcpy 惯例先打印
   `scrcpy 5.0.1 <https://github.com/Genymobile/scrcpy>`，
   解析时必须跳过（见 §7.1）。若希望纯设备列表，可再小改 `main.c`，
   但会偏离上游输出习惯，当前未做。
6. **未做端到端验证**：按任务要求未连接手机、未触发真实音频转发；
   "指定设备打开音频流"用独立 SDL 测试覆盖（§5.5），
   scrcpy 内部的完整链路（server→decoder→audio regulator→SDL）未在真机验证。
7. **仅构建/验证 Windows x64 UCRT64**；同源码在 Linux 上应可编译（未验证）。
8. **分发许可**：官方 scrcpy 的 FFmpeg DLL 按其构建脚本为 LGPL-2.1+ 动态链接
   （未启用 `--enable-gpl`），SDL3 为 zlib、libusb 为 LGPL-2.1+；
   打包时请把这些第三方许可证加入项目的 `THIRD_PARTY_LICENSES.md`/`licenses/`
   （本项目已有对应目录，由 lead 统一处理）。
9. **非位级可复现**：两次干净构建的 `scrcpy.exe` SHA-256 不同（PE 时间戳等），
   大小/行为一致；如果需要可复现构建，需加 `-Wl,--no-insert-timestamp`。

## 10. 产物路径速查

```
vendor/
├── scrcpy-src/                              # 上游 v5.0.1 + 补丁源码（git 基线=上游）
├── scrcpy-patched/
│   ├── scrcpy-5.0.1-phone-audio.patch       # 补丁 diff（git apply -p1）
│   ├── CHANGES-phone-audio.md               # Apache-2.0 §4 修改说明
│   ├── LICENSE                              # Apache-2.0
│   ├── build-windows.sh                     # 可复现构建脚本
│   ├── win64/                               # 分发目录（scrcpy.exe + DLL + server）
│   ├── official-win64-v5.0.1/               # 官方发行包解压参考（DLL 来源）
│   └── verify/                              # 实测原始输出 + 辅助 SDL 测试
├── build-ucrt64/                            # meson/ninja 构建树
├── downloads/                               # tarball / 官方 zip / 构建与 pacman 日志
└── tmp/                                     # 供 LTO 使用的无空格 TMPDIR
```

## 11. 参考

- scrcpy v5.0.1 源码：<https://github.com/Genymobile/scrcpy/tree/v5.0.1>
  （本机通过 codeload tarball 获取，见 §4.5）
- scrcpy 构建文档：<https://github.com/Genymobile/scrcpy/blob/v5.0.1/doc/build.md>
- scrcpy win64 v5.0.1 发行包：
  <https://github.com/Genymobile/scrcpy/releases/download/v5.0.1/scrcpy-win64-v5.0.1.zip>
- SDL3 音频设备 API：`SDL_GetAudioPlaybackDevices` /
  `SDL_GetAudioDeviceName` / `SDL_OpenAudioDeviceStream`
  （<https://wiki.libsdl.org/SDL3/SDL_GetAudioPlaybackDevices>）
- MSYS2：<https://www.msys2.org/>
