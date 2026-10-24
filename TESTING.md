# 测试与验证记录

> 原则：**只记录实际跑过的结果**。没有真机、没有 Linux 机器的部分明确标为「未验证」。

## 1. 验证环境

| 项目 | 值 |
|---|---|
| 操作系统 | Windows 11（`Microsoft Windows NT 10.0.26300.0`，x86_64，显示缩放 125%） |
| Rust | `rustc 1.99.1 (58cf8f3 2026-10-13)`，`x86_64-pc-windows-gnu`（GNU 工具链） |
| 链接器 | MSYS2 UCRT64 `gcc 16.2.0`（`D:\Program Files\msys2\ucrt64\bin`） |
| 依赖 | scrcpy 5.0.1 官方 win64（SDL 3.4.18 / FFmpeg 63.1.102）+ 本项目补丁版 `scrcpy.exe`；adb 1.0.41（platform-tools 37.0.1） |
| 手机 | vivo 测试机（Android 17 / SDK 37），**无线调试** adb `192.168.1.100:45661`（USB 线未测试） |
| Linux | **无**（本机没有 Linux/WSL；只做了交叉 `cargo check`） |
| 时间 | 2026-10-23 ~ 2026-10-24（Asia/Shanghai） |

> 环境注意事项：本机没有 MSVC/Windows SDK，因此使用 Rust GNU 工具链 + MSYS2 UCRT64 gcc。
> `cargo build`/`cargo test` 正常；交叉检查 Linux target 时需要
> `RUSTFLAGS="-Clink-self-contained=yes"`（否则 host build script 链接找不到 rustc 自带的 MinGW CRT）。

### 1.1 组件版本核对（2026-10-24 查询结果）

| 组件 | 项目使用版本 | 上游最新版 | 说明 |
|---|---|---|---|
| Rust stable | 1.99.1（2026-10-13） | 1.99.1 | `rustup check`：up to date ✅ |
| rustup | 1.29.2 | 1.29.2 | up to date ✅ |
| scrcpy | 5.0.1 | 5.0.1（2026-10-08） | 最新 release ✅；补丁基于该版本 |
| adb / platform-tools | 1.0.41（37.0.1） | 37.0.1 | 一致 ✅（scrcpy 官方包自带） |
| SDL3 | 3.4.18 | 跟随 scrcpy 官方 win64 包 | 直接复用官方 DLL，未自行升级 |
| FFmpeg | avcodec 63.1.102（FFmpeg 9.0.2） | 跟随 scrcpy 官方 win64 包 | 直接复用官方 DLL，未自行升级 |
| eframe / egui | **0.37.0** | 0.37.0 | 已升级 ✅；显式使用 `glow` 渲染后端（见 §1.2） |
| directories | 6.0.0 | 6.0.0 | 已从 5.0.1 升级并重新跑通 check/test/clippy ✅ |
| serde / serde_json | 1.0.229 / 1.0.154 | 1.0.229 / 1.0.154 | 最新 ✅ |
| Rust edition | 2021 | 2024（需 Rust 1.85+） | 不影响功能；未迁移 |
| GitHub Actions | checkout@v7, upload-artifact@v7, rust-cache@v2, dtolnay/rust-toolchain@stable | checkout v7.0.1 / upload-artifact v7.0.2 / rust-cache v2.9.3 | 已更新到当前最新 major ✅（未在 CI 实跑） |
| MSYS2 gcc | 16.2.0 | 16.2.0（MSYS2 仓库） | 最新 ✅ |

> 结论：**所有可升级的运行时依赖都已是最新版**（eframe/egui 0.37、directories 6、serde 等），scrcpy/SDL/FFmpeg/adb 则跟随 scrcpy 5.0.1 官方发行版。

### 1.2 eframe 0.29 → 0.37 迁移记录（2026-10-24）

- `eframe`/`egui` 从 0.29.1 升到 **0.37.0**，`rust-version` 从 1.80 提到 **1.95**（0.37 的 MSRV）。
- eframe 0.37 默认渲染后端改成了 **wgpu**；但 wgpu 30.0.1 + gpu-allocator 0.28 在 Windows 上会解析出两个不同版本的 `windows` crate（0.58/0.62），导致 `wgpu-hal` 编译失败。
  本项目不需要 3D，因此改为 `default-features = false` + 显式启用 **glow**（OpenGL）后端，绕开该上游依赖冲突，二进制也更小。
- egui 0.37 API 迁移：`eframe::App::update` → `App::ui(&mut self, ui: &mut egui::Ui, ...)`；
  `TopBottomPanel` → `egui::Panel`（`default_height/min_height` → `default_size/min_size`）；
  `Context::screen_rect()` → `content_rect()`；`FontDefinitions.font_data` 的值改为 `Arc<FontData>`（加 `.into()`）。
- 升级后重新执行：`cargo fmt --check` / `cargo clippy -D warnings` / `cargo test`（14 passed）/ `cargo build --release` 全部通过，并重新截图、重新打包（见 §3/§4）。

## 2. 编译与静态检查

在项目根目录执行（PATH 加上 `%USERPROFILE%\.cargo\bin` 和 MSYS2 UCRT64）：

| 命令 | 结果 |
|---|---|
| `cargo fmt --all --check` | ✅ 通过（0 diff） |
| `cargo check` | ✅ 通过 |
| `cargo clippy --all-targets -- -D warnings` | ✅ 通过（0 warning） |
| `cargo test` | ✅ **14 passed / 0 failed** |
| `cargo build --release` | ✅ 通过，`target\release\phone-audio.exe`（eframe 0.37/glow，约 8.9 MB） |
| `cargo check --target x86_64-unknown-linux-gnu` | ✅ 通过（Linux cfg 代码可编译；未链接 Linux 二进制） |

单元测试覆盖：

- `adb::parse_devices`：USB/无线/未授权/离线/no permissions/CRLF、过滤 `* daemon`、`adb server`、`error:` 噪声
- `adb::parse_mdns_services`：按 IP 过滤、IPv4、无匹配
- `scrcpy::parse_version`、`Info::supports`（不误判描述文本、区分 `--audio-output-buffer` / `--audio-output-device`）
- `scrcpy::build_args`：最小参数、raw 不带码率、输出设备仅在补丁版支持时传入、旧版本/playback 拒绝
- `settings`：未知值归一化、JSON round-trip、缺字段默认值

## 3. Windows 本机运行验证

| 检查项 | 结果 | 证据 |
|---|---|---|
| 应用可以启动并渲染中文 UI | ✅ | `docs/screenshots/app-main.png` |
| 自动探测 scrcpy / adb 版本 | ✅ | 日志头部显示 `scrcpy 5.0.1`；「复制诊断信息」包含完整 adb 版本 |
| 自动探测补丁能力 | ✅ | 日志 `输出设备选择：支持`；顶部提示可切换为「可在音频里选择电脑输出设备」 |
| `--list-audio-devices` 枚举电脑输出设备 | ✅ | 实测返回 5 个设备：`耳机 (Lian II)`、`扬声器 (Realtek(R) Audio)`、`G27H2Max (NVIDIA High Definition Audio)` 等；首行 banner 已在 Rust 侧过滤 |
| 应用能加载系统 CJK 字体 | ✅ | 日志 `中文字体：C:\Windows\Fonts\simhei.ttf` |
| 配置写入/读取 | ✅ | `%APPDATA%\PhoneAudio\PhoneAudio\config\settings.json` 正常生成 |
| 关闭窗口后无残留 scrcpy/adb 子进程 | ✅ | 关闭后 `Get-Process scrcpy,adb` 无 scrcpy；adb server 保持运行（设计如此） |
| 崩溃日志路径 | ✅（代码路径） | `config_dir/crash.log`、`start-error.log`；未人为触发 panic |

截图（Windows，补丁版 scrcpy 已生效）：

![应用主界面](docs/screenshots/app-main.png)

## 3.1 真机验证（2026-10-24）

**设备**：vivo 测试机，Android 17（SDK 37），通过**无线调试**连接（`adb devices` 同时出现 `192.168.1.100:45661` 与 mDNS 服务名两条记录，同一台设备）。手机已解锁，保持在媒体播放状态。

| 检查项 | 结果 | 证据 |
|---|---|---|
| 设备发现 / 授权状态 | ✅ | 应用设备下拉显示 `示例测试机 · 192.168.1.100:45661 · 已连接` |
| 点击「开始串流」后 scrcpy 启动 | ✅ | 状态变为 **正在串流**；日志：`ADB device found` → `scrcpy-server 1 file pushed` → `音频已开始` → `[server] INFO: Device: [vivo] vivo 测试机 (Android 17)` |
| 音频确实从电脑默认输出（用户有线耳机）播放 | ✅ | **用户确认能听到手机（抖音）声音**；输出设备为「跟随系统默认」 |
| Android 版本预检 | ✅ | `getprop ro.build.version.sdk` = 37 ≥ 30，未触发拦截 |
| 补丁版 scrcpy 能力 | ✅ | 应用日志 `输出设备选择：支持`；`--list-audio-devices` 枚举到 5 个设备 |
| 串流期间 UI 状态 / 设置同步 | ✅ | 音频设置被禁用并提示「修改需先停止串流」；停止按钮可用 |
| 停止 / 关闭后无残留 | ✅（此前在无设备时验证） | 关窗后 `scrcpy` 进程 0 残留 |

**未验证**（用户选择「不测试」，未做中断性操作）：

- 输出设备切换的端到端验证（选择 `扬声器 (Realtek(R) Audio)` 后声音是否改从扬声器输出）
- 断线重连（`adb disconnect` / 拔线 → Reconnecting → 自动恢复）；无线设备断开后需要重新 `adb connect`，当前自动重连逻辑**只等待设备重新出现在 `adb devices` 中**，TPC/IP 设备断线不会自动重连，这是一个已知缺口
- 应用内无线配对流程（本次连接由用户事先完成）
- USB 有线连接、Android 11 锁屏、DRM App、通话/铃声

> 在此之前，用户已手动启动 dist 包中的 `phone-audio.exe`（eframe 0.37 构建）完成上述串流；本次验证同时确认了 **eframe 0.37 升级后的发布构建在真机上工作正常**。

## 3.2 UI 重做与调整验证（v0.3.0–v0.5.1，2026-10-24）

本轮把 UI 重做为卡片式布局（`src/theme.rs` + `main.rs`），并在 v0.3.2 按反馈合并卡片、调整主题：

| 检查项 | 结果 | 证据 |
|---|---|---|
| 主题选择：跟随系统（默认）/ 深色 / 浅色 | ✅ | 顶部下拉框；`settings.json` 的 `theme` 可持久化；默认 `system` |
| 深色主题 | ✅ | [docs/screenshots/app-main.png](docs/screenshots/app-main.png)（当前系统为深色时「跟随系统」渲染为深色） |
| 浅色主题 | ✅（`settings.theme=light`） | [docs/screenshots/app-light.png](docs/screenshots/app-light.png) |
| 设备 + 开始/停止串流合并为一张卡片，按钮在右侧 | ✅ | 见 app-main / app-running；已去掉「准备就绪 / 点击开始… / Opus · 256 kbps…」等文字 |
| 标题栏多余横线 | ✅ | v0.3.3 移除了 header 自绘分隔线（只保留面板自身的单线） |
| 「已连接」徽标位置 | ✅ | v0.3.3 从卡片右上角移到「设备」标题旁边 |
| 左右两栏底部对齐 / 边距对称 | ✅ | v0.3.5 双栏改用 `ui.columns(2)`（等宽、外边距一致），右下卡按左卡剩余高度拉伸，两栏底边齐平 |
| 开始按钮位置 | ✅ | v0.3.5 放在设备卡标题行右侧，不再沉在卡片底部 |
| 去掉「已检测到 N 个输出设备」 | ✅ | 输出卡片只保留输出设备下拉 + 刷新 |
| 「工具与诊断」卡替换为「音源」卡 | ✅ | v0.3.6 右侧为「电脑输出 + 音源」；音源下拉、保留手机外放集中在这里 |
| 设备区改一行 | ✅ | v0.3.7 设备/状态/下拉/刷新/自动串流/开始按钮同一行，垂直居中 |
| 音源卡紧凑 | ✅ | v0.3.7 来源下拉与「保留手机外放」同一行，删掉多余说明文字 |
| 设备行高度 | ✅ | v0.3.7 已连接徽标缩小，开始按钮高度 46→40，按钮靠右对齐 |
| 双栏等高 | ✅ | v0.3.8 固定高度并按行间距校准（左 278.4 / 右 152 + 8 + 116），两栏底边对齐 |
| 右侧两卡间距 | ✅ | v0.3.8 去掉额外的 12px spacer，只保留默认 8px 行距 |
| 双栏底边对齐 | ✅ | v0.3.7 先画右栏，再把左栏音频卡拉到同高，两栏底边完全齐平 |
| 诊断入口 | ✅ | v0.3.6 收进运行日志右上角「诊断」菜单（配置目录 / 声音设置 / 复制诊断信息） |
| 滑杆宽度 | ✅ | v0.3.6 缓冲滑杆与下面三个延迟预设等宽 |
| 开始按钮对齐 | ✅ | v0.3.6 按钮在「启动后自动串流」右侧同排，垂直居中对齐 |
| 滚动条 | ✅ | v0.3.6 主内容区隐藏滚动条（滚轮仍可滚动） |
| 双栏高度 | ✅ | v0.3.6 先画右栏（输出 + 音源），再把左栏音频卡拉到同高，底边齐平 |
| 正在串流的运行态 | ✅ | [docs/screenshots/app-running.png](docs/screenshots/app-running.png)：状态胶囊变绿、按钮变红「停止串流」、音频设置变灰并提示「停止后可修改」、日志显示 server 推送与「音频已开始」 |
| 「启动后自动串流」关闭时不自动开始 | ✅ | 实测启动应用后 `scrcpy` 进程数为 0；打开开关/手动 `Ctrl+Enter` 才会启动 |
| 设备发现/无线自动连接不受该开关影响 | ✅ | 关闭时设备仍显示「已连接」（adb mDNS 自动发现） |
| `Ctrl+Enter` 快捷键启动/停止 | ✅ | 按键后 scrcpy 进程出现、UI 进入运行态；再按一次后进程退出 |
| 窄窗口单列自适应（640×520 逻辑分辨率） | ✅ | 已人工验证（截图不再随仓库分发） |
| 关闭窗口无残留 | ✅ | 关闭后 `scrcpy` 进程 0 残留 |
| 100% DPI 下的视觉效果 | ❌ 未验证 | 本机当前为 150% 缩放（窗口 920×800 逻辑 → 1402×1256 物理）；窄窗口测试用了 960×780 物理 |

> 说明：真机音频链路此前已由用户确认可听；运行态截图验证 UI/进程状态与快捷键，未重新做听感确认。

## 3.3 无线端口变化自动恢复验证（v0.3.1，2026-10-24）

**背景**：Android 无线调试在手机重启/切换开关后会分配新的 `ip:port`；如果配置里保存的是旧端口（本次现场：保存 `192.168.1.100:45661`，实际已变为 `192.168.1.100:42359`），旧版会一直等待不存在的 serial。

**测试步骤**：先 `adb disconnect` 掉当前 TCP 连接（只保留 mDNS 发现记录），把 `settings.json` 的 `serial` 故意改回过期端口 `192.168.1.100:45661`，设置 `auto_connect=true`，然后启动应用。

| 检查项 | 结果 | 证据 |
|---|---|---|
| 自动通过 mDNS 发现新端口并 `adb connect` | ✅ | 应用日志出现「无线调试发现并尝试连接：192.168.1.100:42359」 |
| 按 IP 匹配旧 serial 并自动开始串流 | ✅ | `scrcpy` 命令行实际为 `--serial 192.168.1.100:42359`；UI 设备显示 `示例测试机 · 192.168.1.100:42359`，状态「正在串流」 |
| 新 serial 写回配置 | ✅ | 关闭后 `settings.json` 的 `serial` 已变为 `192.168.1.100:42359` |
| 正常关闭无残留 | ✅ | 关闭后 `scrcpy` 进程 0 残留 |
| 无需重新配对/手动输入 IP:端口 | ✅ | 整个过程只在启动前改配置文件，未在 UI 输入任何地址 |

> 本次未保留包含设备地址的截图，测试步骤与结果如上。

> 注意：本次验证的是「启动时发现端口已变化」；串流进行中断线（exit code 2）后的自动重连逻辑也已改为同样的 mDNS 发现 + 按 IP 匹配，但**未做断线实测**。

## 3.5 开机自启动验证（v0.4.0，2026-10-24）

| 检查项 | 结果 | 证据 |
|---|---|---|
| 「设置 → 开机自启动」勾选后写入注册表 | ✅ | UI 自动点击验证：`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 出现 `PhoneAudio = "G:\phone-audio\target\release\phone-audio.exe"` |
| 取消勾选 / `--disable-autostart` 删除 | ✅ | 再点一次或命令后 `reg query` 返回“找不到指定的注册表项或值” |
| `--enable-autostart` / `--disable-autostart` 退出码 | ✅ | 均为 0；失败时返回 1 |
| 路径包含空格 | ✅ | 在 `...\Phone Audio Test\phone-audio.exe` 下开启，注册表值带引号且完整 |
| Linux autostart | ❌ 未验证 | 代码写 `~/.config/autostart/phone-audio.desktop`，未在 Linux 实机运行 |
| 真实开机/登录触发 | ❌ 未重启验证 | 注册表位置与值已核对，未实际重启登录测试 |

> 自启动只负责拉起应用；是否登录后自动开始串流仍由「启动后自动串流」开关决定。
## 3.6 关闭到系统托盘验证（v0.5.0，2026-10-24）

| 检查项 | 结果 | 证据 |
|---|---|---|
| 首次点击窗口关闭按钮弹出选择框 | ✅ | 用 `WM_CLOSE` 触发后进程存活、窗口仍可见，弹出「最小化到托盘 / 退出应用」 |
| 选择「最小化到托盘」 | ✅ | 进程存活，`IsWindowVisible=false`，日志记录「已最小化到系统托盘」 |
| 记住选择后再次关闭 | ✅ | `tray_choice_made=true, close_to_tray=true` 时直接隐藏，不再弹窗 |
| 设为「退出应用」后关闭 | ✅ | `close_to_tray=false` 时进程正常退出，`scrcpy` 无残留 |
| 托盘左键恢复 / 右键菜单退出 | ❌ 未自动化 | 托盘图标创建成功（隐藏路径依赖它），但未用鼠标点击系统托盘验证恢复/退出 |
| Linux 托盘 | ❌ 未实现 | 非 Windows 平台保持普通关窗；设置里提示「系统托盘不可用」 |

> 首次关闭弹窗截图见 `target/smoke/close-prompt.png`（本机测试产物，未随包分发）。
## 3.7 窗口大小记忆与关闭弹窗视觉（v0.5.1–v0.5.2，2026-10-24）

| 检查项 | 结果 | 证据 |
|---|---|---|
| 启动时恢复上次窗口大小 | ✅ | 配置 `1000×700` → 窗口物理 `1268×922`（150% DPI）；关闭后配置仍为 `1000×700`；再次启动尺寸一致 |
| 关闭/隐藏到托盘时保存 | ✅ | `Drop` 与 `hide_to_tray` 都会 `settings.save()`，尺寸在每帧记录 |
| 运行中调整窗口大小 | ✅ | v0.5.2 每 0.8 秒防抖写盘；实测启动 4 秒后 `settings.json` 已出现 `window_width/height` |
| 版本识别 | ✅ | 「设置」菜单底部显示 `Phone Audio v0.5.2` |
| 首次关闭弹窗视觉 | ✅ | 主题化卡片 + 图标 + 主/次按钮 + 背景遮罩，见 [docs/screenshots/close-prompt.png](docs/screenshots/close-prompt.png) |
| 小窗口下限 | ✅ | 最小 640×520；配置中的越界值会被 `normalize()` 夹紧 |
## 4. Windows 打包验证

```powershell
powershell -ExecutionPolicy Bypass -File scripts\fetch-windows-deps.ps1
powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
```

- 打包脚本会在 TEMP 里组装（避免 dist 里正在运行的 adb/scrcpy 锁文件），并：校验 `bin\scrcpy.exe` 可运行 → 若存在补丁版则校验 SHA-256 并叠加 → 复制 `bin/`、`licenses/`、`docs/`、`assets/`、`scrcpy-patched/` 补丁与修改说明 → 生成 zip 并打印 SHA-256。
- **实际结果（2026-10-24，v0.3.3 / eframe 0.37 / 新 UI）**：`dist\phone-audio-windows-x64.zip`（约 15 MB）。
  打包脚本会打印 SHA-256；最终产物哈希以生成时输出为准（文件内不再写死哈希，避免每次改文档都过期）。
  脚本内置校验通过：`scrcpy 5.0.1`、`输出设备选择：可用（补丁版 scrcpy）`。
  解压后包含 `phone-audio.exe`（v0.5.2，eframe 0.37/glow）、`bin/`（补丁版 scrcpy.exe + server + adb + DLL）、
  `licenses/`、`docs/`（含 4 张 UI 截图）、`scrcpy-patched/`（patch + CHANGES + build-windows.sh + LICENSE.scrcpy）。
- Windows PowerShell 5.1 会把无 BOM 的 UTF-8 `.ps1` 当 ANSI 解析，因此两个 Windows 脚本都保存为 **UTF-8 with BOM**（已用 5.1 的 Parser 验证语法）。
- 未做：代码签名、Windows 安装包（NSIS/MSI）。

### 4.1 分发包内容（应包含）

```
phone-audio/
├─ phone-audio.exe
├─ bin/ (scrcpy.exe[补丁版], scrcpy-server, adb.exe, DLLs, LICENSE.txt)
├─ licenses/ (Apache-2.0.txt, LGPL-2.1.txt)
├─ docs/ (scrcpy-output-device.md 等)
├─ scrcpy-patched/ (patch, CHANGES-phone-audio.md, build-windows.sh, LICENSE.scrcpy)
├─ README.md / TESTING.md / LICENSE / NOTICE / THIRD_PARTY_LICENSES.md
```

## 5. 未验证的部分（不要当成已测试）

1. **真机链路**：已用 vivo 测试机（Android 17，无线调试）验证「开始串流 + 默认输出出声」（见 §3.1）。仍未验证：
   - USB 有线授权、拔线重连（`Reconnecting` / exit code 2 实际触发）
   - 串流中断线后的自动重连（启动时端口变化的恢复已在 §3.3 验证）
   - 应用内无线配对 + mDNS 自动连接
   - 输出设备切换的端到端听感验证（补丁 scrcpy 的设备打开路径已在 SDL 层单独验证）
   - Android 11 锁屏、Android 10 拒绝、DRM App、通话/铃声
2. **Linux**：未在 Linux 真机构建/运行/打包；只做了 `cargo check --target x86_64-unknown-linux-gnu`。`PULSE_SINK` 输出设备选择路径**未验证**。
3. **CI**：`.github/workflows/build.yml` 未实际在 GitHub Actions 跑过（仓库尚未推送）。
4. **补丁版 scrcpy 的端到端音频**：补丁只验证了 `--version/--help/--list-audio-devices` 和设备名解析；未连接手机验证「指定设备后声音确实从该设备输出」。
5. **系统托盘 / 单实例 / 开机自启**：未实现。
6. **多设备同时串流**：未实现。

## 6. 如何复跑

```powershell
# Windows 11 + Rust + MSYS2（GNU 工具链）：
scripts\fetch-windows-deps.ps1
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
.\target\release\phone-audio.exe
scripts\package-windows.ps1
```

真机测试建议按以下矩阵进行（每项记录手机型号/Android 版本/系统版本/结果/日志）：

| 场景 | 期望 |
|---|---|
| USB 正常串流 | 手机静音，所选电脑输出设备出声，日志无 ERROR |
| 停止 / 关闭应用 | 无 scrcpy 残留进程 |
| 拔线再插回 | 状态进入 Reconnecting，插回后自动恢复 |
| 关闭自动重连 + 拔线 | 显示「设备已断开（自动重连已关闭）」 |
| Android 11 锁屏启动 | 应用显示音频采集错误（`--require-audio`），不是假绿色 |
| Android 10 | 预检提示需要 Android 11+ |
| 无线配对 | `adb pair` 成功，mDNS 自动连接或手动连接成功 |
| 选择输出设备 | 声音只从所选设备输出；选择不存在的设备名会报错 |

## 7. 已知环境问题

- **GNU 工具链交叉 check**：`cargo check --target x86_64-unknown-linux-gnu` 需要
  `RUSTFLAGS="-Clink-self-contained=yes"`，否则 host build script 链接 gcc 时找不到 `crt2.o`。
  这是本机 MSYS2 UCRT64 缺少 MSVCRT 版 MinGW CRT 导致的，不影响 Windows 本机构建。
- **CI 首次运行**：Linux job 的 apt 包名、`cargo fmt --check` 等未在本机验证；如失败按日志调整。
- **Linux 静态包许可证**：官方 Linux 静态构建把 FFmpeg/SDL 静态链入；若公开分发 Linux 包，需复核 LGPL-2.1 义务（见 `THIRD_PARTY_LICENSES.md`）。
