# Linux 依赖脚本与 GitHub Actions 审校报告

> 审校对象：`scripts/fetch-linux-deps.sh`、`scripts/package-linux.sh`、`.github/workflows/build.yml`，并新增 `.gitattributes`。
> 执行环境：Windows 11 + MSYS2 bash 5.2.37；Rust 工具链未安装、无 Linux 真机、无 GitHub Actions 运行记录。
> **本文所有结论都只在本机做了静态分析 + 合成包逻辑测试；没有在 Linux 真机运行过修改后的脚本，也没有真正跑过 `cargo build/test/clippy/fmt` 与 CI。**

---

## 0. 结论摘要

1. 已核对官方 **scrcpy v5.0.1 Linux 静态包** 的真实结构与校验和（下载 tar.gz、`tar -tvzf`、`sha256sum`、`readelf`），确认：
   - 顶层目录为 `scrcpy-linux-x86_64-v5.0.1/`；
   - `scrcpy-server` 就在包根，与 `scrcpy` 同级，**不在 `share/` 下**；
   - **v5.0.1 包内已经自带 `adb`**，且与 `platform-tools-latest-linux.zip` 里的 `adb` 逐字节相同 —— 原脚本每次额外下载 9 MB platform-tools 是完全多余并且会覆盖内置 adb 的；
   - 包内没有动态库 `.so`，官方所谓 “static” 指把 FFmpeg/SDL 静态链进 `scrcpy`，但它仍动态依赖系统 `libm.so.6`、`libudev.so.1`、`libc.so.6`；
   - 包内 `adb` 把 libc++ / libusb 静态链进去了（`readelf -d` 无 `libc++.so` / `libusb` 依赖），所以**只拷 `adb` 一个文件就能用**，不需要 platform-tools 的 `lib64/libc++.so`。
2. 修复了原脚本的实质性缺陷：**无平台守卫（Windows/MSYS2 上会把 Linux ELF 写进共享 `bin/`）**、**无完整性校验**、**版本未强制 ≥2.1**、**失败无回滚**、**整目录 `cp` 污染 `bin/`/把 Windows 文件打进 Linux 包**、**未检查 `scrcpy-server` 这个运行时必需文件**、**包内自带 adb 却被忽略**。
3. CI 保守增强：两个 job 都补 `cargo fmt --check` / `cargo clippy` / `cargo test`；补 rustfmt+clippy 组件；补齐 Linux 构建依赖（`libxkbcommon-x11-dev`、xcb 系列、`libudev1` 等）；artifact 加 `if-no-files-found: error`。原有 job/步骤一个未删。
4. `.gitattributes` 已添加，保证 `*.sh` 以 LF 检出（这是 CI 上 `bash: bad interpreter: /usr/bin/env: No such file or directory` 的根因防护）。

---

## 1. 事实核对：scrcpy v5.0.1 Linux 静态包

### 1.1 Release 资产

`https://api.github.com/repos/Genymobile/scrcpy/releases/latest` → `v5.0.1`，资产包括：

```
scrcpy-linux-x86_64-v5.0.1.tar.gz
scrcpy-macos-aarch64-v5.0.1.tar.gz
scrcpy-macos-x86_64-v5.0.1.tar.gz
scrcpy-server-v5.0.1
scrcpy-win32-v5.0.1.zip
scrcpy-win64-v5.0.1.zip
scrcpy-winarm64-v5.0.1.zip
SHA256SUMS.txt (+ .asc)
```

**没有 Linux aarch64 静态包**，所以脚本必须限定 x86_64。

### 1.2 下载与校验

在本机 TEMP 目录下载（未写入项目）：

- URL：`https://github.com/Genymobile/scrcpy/releases/download/v5.0.1/scrcpy-linux-x86_64-v5.0.1.tar.gz`
- 大小：`17,883,742` 字节
- SHA-256：`9f969d30cc574816077edecda65719c1c70b0aee1df1ba5fa00779ac07b8fd6e`
- 与 release 的 `SHA256SUMS.txt` 中该文件一行**完全一致**（该值已内置进 `fetch-linux-deps.sh`）。

### 1.3 包内真实结构（`tar -tvzf`）

| 路径（相对 `scrcpy-linux-x86_64-v5.0.1/`） | 大小 | 类型 / 说明 |
| --- | ---: | --- |
| `scrcpy` | 34,305,128 | ELF 64-bit PIE；`readelf -d` NEEDED：`libm.so.6`、`libudev.so.1`、`libc.so.6` |
| `scrcpy-server` | 733,930 | Android package (APK/jar) |
| `adb` | 10,642,368 | ELF 64-bit PIE；SHA-256 `a902be8f45c6c62e76c9efaf6947a0fa747c9cabd89a2ac8e0d16ecb30b3ed01` |
| `scrcpy.1` | 25,670 | man page（应用不需要） |
| `scrcpy.png` | 6,530 | 图标（应用不需要） |
| `disconnected.png` | 4,703 | scrcpy 窗口图标（应用不需要） |
| `LICENSE` | 11,387 | Apache-2.0 文本（再分发需带） |

补充核对：

- `scrcpy-server` 的 SHA-256 为 `764eb6f79811d5211fe9df341120882ba9994c7a61b897d7bf3fb662e53bc536`，与 `SHA256SUMS.txt` 中的 `scrcpy-server-v5.0.1` 资产一致，也与 Windows 包 `bin/scrcpy-server` 一致（两个平台共用同一个 server jar）。
- 包内**没有** `.so` 文件；`scrcpy` 是“把 FFmpeg/SDL 等静态链接进去”的产物，但**不是全静态**，仍依赖系统 `libudev.so.1` 等。Ubuntu/Fedora 桌面默认都有 `libudev1`。
- 包内 `adb` 的 RUNPATH 为 `$ORIGIN/:$ORIGIN/lib64:...`，NEEDED 里没有 `libc++.so` / `libusb-1.0.so`（libc++、libusb 已静态链入），因此把 `adb` 单独放到 `bin/` 是安全的。

### 1.4 platform-tools 的真实结构（对照用）

`https://dl.google.com/android/repository/platform-tools-latest-linux.zip`（9,054,187 字节）共 12 个条目：

```
platform-tools/adb                    ← 与 scrcpy v5.0.1 包内 adb 的 sha256 完全相同
platform-tools/fastboot
platform-tools/etc1tool / hprof-conv / make_f2fs / mke2fs / sqlite3 ...
platform-tools/lib64/libc++.so
platform-tools/NOTICE.txt / source.properties / mke2fs.conf
```

`lib64/libc++.so` 是给 fastboot 等其它工具用的；**adb 本身不需要**（NEEDED 无 libc++）。原脚本只拷 `platform-tools/adb` 本来也能跑，但纯属重复下载。

### 1.5 对原脚本的关键判断

- **`scrcpy-server` 路径没有 bug**：原脚本 `cp -a "$src"/. bin/` 会把包根的所有文件（含 server）一起拷过去；后面“有些包把 server 放 share/ 下”的 fallback 是多余且误导的注释（当前包不是这种布局）。
- **真正的浪费/隐患**：原脚本无视包内自带 `adb`，又下载一份 platform-tools 并覆盖；一旦 platform-tools 下载失败，脚本会整体失败（哪怕包里已经有可用的 adb）。
- 原脚本把 `scrcpy.1`、`scrcpy.png`、`disconnected.png`、`LICENSE` 一并塞进 `bin/`。

---

## 2. 发现的问题与修改

### 2.1 `scripts/fetch-linux-deps.sh`

| # | 原问题 | 后果 | 修复 |
| --- | --- | --- | --- |
| 1 | 没有平台判断 | 在 Windows/MSYS2/WSL 共享目录上运行会把 Linux ELF 写进共享 `bin/`，和 Windows 依赖混在一起（本次任务明令禁止） | 开头 `uname -s` 必须为 `Linux`，否则直接报错并提示用 `fetch-windows-deps.ps1` |
| 2 | 没有架构判断 | ARM Linux 上会下 x86_64 包，后续执行报难懂的错误 | `uname -m` 限定 `x86_64/amd64`，否则给出明确说明 |
| 3 | 用未认证 GitHub API `releases/latest` | 未认证 60 次/小时/IP，CI 共享出口 IP 可能 403；版本不可复现 | 默认固定 `SCRCPY_VERSION=5.0.1` 直接拼 release URL；支持 `SCRCPY_VERSION=latest`（走 `releases/latest` 的 HTTP 重定向，不调 API）；支持 `SCRCPY_TARBALL_URL` 覆盖 |
| 4 | 无完整性校验 | 下载被劫持/截断时静默安装 | 内置 v5.0.1 的 SHA-256（来自官方 `SHA256SUMS.txt`），不符即失败；其它版本可用 `SCRCPY_SHA256` 指定，未提供时**警告**并继续 |
| 5 | `find ... \| head -n1` + `set -o pipefail` | `find` 可能被 SIGPIPE，导致脚本因 `set -e` 意外退出 | 改用 `find ... -print -quit` |
| 6 | `cp -a "$src"/. bin/` | 往 `bin/` 拷入 man page、图标、LICENSE 等杂物，且会与已有文件混合 | 只安装 `scrcpy`、`scrcpy-server`、`adb`，以及 `LICENSE.scrcpy`；并显式 `chmod` |
| 7 | 失败无回滚 | 下载/解压/校验中途失败时 `bin/` 可能已被写了一半 | 先在 `mktemp -d` 中完成下载、校验、解压、结构检查，全部通过后才写 `bin/` |
| 8 | 只打印版本，不校验 ≥2.1 | 误用旧包时运行期才失败（旧版没有 `--audio-source`） | 解析 `scrcpy --version` 并用 awk 比较，`< 2.1` 直接失败 |
| 9 | 未校验 platform-tools 内部结构；无 `--retry`；未预检命令 | 解压异常时错误难懂；网络抖动直接失败 | 增加结构检查、`curl --retry 3`、`curl/tar/unzip/sha256sum` 预检 |
| 10 | 默认“包内没有 adb”，每次下载 9 MB platform-tools 覆盖内置 adb | 多一次网络依赖与失败点；实际上 v5.0.1 已自带**字节相同**的 adb | **包内有 `adb` 就优先用内置的**并跳过下载；没有时才回退 platform-tools（向前/向后兼容） |
| 11 | 没有携带 scrcpy 的 Apache-2.0 LICENSE | 再分发合规风险（`THIRD_PARTY_LICENSES.md` 明确要求带 scrcpy LICENSE） | 有 `LICENSE` 时复制为 `bin/LICENSE.scrcpy`，由打包脚本带进产物 |

保持：`set -euo pipefail`、`mktemp` + `trap` 清理、`bin/` 布局 `bin/{scrcpy,scrcpy-server,adb}`。

### 2.2 `scripts/package-linux.sh`

| # | 原问题 | 后果 | 修复 |
| --- | --- | --- | --- |
| 1 | 没有平台判断 | 在 Windows 上运行会先失败在 `bin/scrcpy --version`，错误信息晦涩 | 明确要求 `uname -s` 为 Linux，提示改用 `package-windows.ps1` |
| 2 | 版本解析 `awk 'NR==1{print $2}'` 脆弱；版本比较用 `test -lt`（变量可能非数字） | 输出格式变化时误判/报语法错误 | 用 awk 从 `scrcpy <ver>` 行提取，再用 awk 做 `>= 2.1` 比较 |
| 3 | **不检查 `bin/scrcpy-server`** | 应用运行时要通过 `SCRCPY_SERVER_PATH` 指向它；缺失时会打出“成功”的包但运行即失败 | 强制检查 `bin/scrcpy`、`bin/adb`、`bin/scrcpy-server` 三者 |
| 4 | `cp -r bin "$out/"` 整目录复制 | 会把 `bin/` 里残留的 Windows `.exe/.dll` 或其它杂物打进 Linux 包 | 只复制三个运行必需文件 + `LICENSE.scrcpy`，其余不带 |
| 5 | 不显式设置权限 | 依赖源文件 mode，`scrcpy`/`adb` 可能不可执行 | `chmod 0755` 可执行、`0644` 数据文件 |
| 6 | 不校验构建产物是否存在 | 失败信息不够直接 | `[ -f target/release/phone-audio ]` 检查 |
| 7 | artifact 名硬编码 x86_64，无架构检查 | ARM 上产出错误命名 | 校验 `uname -m` 并据此命名（x86_64 不变） |
| 8 | 无产物校验和 | 发布时无法快速核对 | 输出 `sha256sum dist/phone-audio-linux-x86_64.tar.gz` |
| 9 | 不检查 `licenses/` | 缺失时报 cp 的原始错误 | 显式检查并报错 |

保持：先 `cargo build --release`（CI 已构建，重复执行是无害的幂等操作，保留是为了让脚本可单独使用）；`dist/phone-audio-linux/` 布局不变（可执行文件在根、依赖在 `bin/`，与 `tools.rs` 的 `<exe dir>/bin` 查找顺序一致）。

### 2.3 `.github/workflows/build.yml`

| # | 原问题 | 修复 |
| --- | --- | --- |
| 1 | 没有 `cargo fmt --check` / `cargo test` / `clippy` | Windows 与 Linux 两个 job 都在依赖安装之后、构建之前加入这三步 |
| 2 | `dtolnay/rust-toolchain@stable` 未声明 components | 加 `components: rustfmt, clippy`（`rust-toolchain.toml` 也声明了，显式更稳） |
| 3 | Linux 构建依赖不全 | 补 `libxkbcommon-x11-dev`、`libxcb1-dev`/`libxcb-render0-dev`/`libxcb-shape0-dev`/`libxcb-xfixes0-dev`、`ca-certificates`；另加 `libudev1`，因为 fetch 脚本会执行 `bin/scrcpy --version`，而包内 scrcpy 动态依赖 `libudev.so.1` |
| 4 | `upload-artifact` 没有 `if-no-files-found` | 加 `if-no-files-found: error`，打包脚本没产出文件时 CI 直接红，而不是上传空 artifact |
| 5 | clippy 若直接 `-D warnings` 可能因现有告警阻塞 | 先 warning-only；`docs` 里给了清零后切 `-D warnings` 的建议 |
| 6 | 现有 job/步骤 | **全部保留**，仅新增步骤与少量系统包；`windows` job 的 fetch/package 调用方式未动 |

说明：`Swatinem/rust-cache@v2` 已存在，未再加 `bin/` 依赖缓存（每次 fetch 约 18 MB，可接受；如要缓存建议用 pinned 版本做 key，并在命中时跳过 fetch 步骤，属于另一项改动）。未加 `permissions`/`concurrency` 等，避免超出“保守修改”范围。

### 2.4 `.gitattributes`（新增）

```
* text=auto
*.sh text eol=lf      # 关键：CRLF 会让 Linux 上 bash 报 bad interpreter
*.bash text eol=lf
*.yml/*.yaml/*.toml text eol=lf
*.ps1/*.bat/*.cmd text eol=crlf
*.png/*.ico/*.jpg/*.zip/*.gz/*.jar binary
```

仓库目前还没有 `.git`，但文件已按要求写入；等 `git init` 后生效。注意 `*.ps1 eol=crlf` 意味着将来检出时 PowerShell 脚本会变 CRLF（当前工作区是 LF，PowerShell 两种都接受；若想保持 LF 可改成 `eol=lf`）。

---

## 3. 本机验证记录（已做 / 未做）

### 3.1 已完成的验证

1. **语法检查**（MSYS2 bash 5.2.37，LF 行尾）：
   - `bash -n scripts/fetch-linux-deps.sh` → OK
   - `bash -n scripts/package-linux.sh` → OK
2. **ShellCheck**：MSYS2 的 `pacman` 没有同步数据库（`pacman -Q shellcheck` 报“未找到”），因此改用官方 standalone Windows 版 `shellcheck 0.10.0`（下载到 TEMP，不落项目）：
   - `shellcheck -s bash -x scripts/fetch-linux-deps.sh` → 0 findings，exit 0
   - `shellcheck -s bash -x scripts/package-linux.sh` → 0 findings，exit 0
3. **合成包功能测试**（临时目录，脚本副本中删掉 Linux 守卫后运行；假 scrcpy/adb 都是 bash 脚本，**没有在 Windows 上执行任何 Linux 二进制**）：共 **44 项断言全部通过**，覆盖：
   - 正常安装、重复执行幂等；
   - 包内自带 adb 时**跳过** platform-tools 下载（故意给不存在的 URL 验证）；
   - 包内无 adb 时回退 platform-tools 且结构正确；
   - SHA-256 不匹配时失败、且不写入 `bin/scrcpy`；
   - scrcpy 版本 1.25 时失败并提示版本过旧；
   - 包内缺 `scrcpy-server` 时失败并提示；
   - 无校验和时警告并继续；
   - platform-tools zip 结构异常时失败；
   - `package-linux.sh` 正常产出 `dist/phone-audio-linux/{phone-audio,bin/scrcpy,bin/scrcpy-server,bin/adb,bin/LICENSE.scrcpy,licenses,assets,文档}` 和 `dist/phone-audio-linux-x86_64.tar.gz`，并输出 sha256；
   - 缺 `scrcpy-server` 或版本过旧时打包失败；
   - tar 包内路径正确。
4. **真实产物核对**（下载到 TEMP 的实际 release 文件）：
   - tar.gz SHA-256 与官方 `SHA256SUMS.txt` 一致（见 1.2）；
   - `tar -tvzf` 完整列表（见 1.3）；
   - `file` / `readelf -d` 分析 scrcpy、adb 的动态依赖；
   - tar 内 `adb` 与 platform-tools zip 内 `adb` 的 sha256 相同；
   - tar 内 `scrcpy-server` 与 Windows `bin/scrcpy-server` 的 sha256 相同；
   - platform-tools zip 完整条目列表（见 1.4）。
5. **YAML 校验**：PyYAML 6.0.3 解析 `.github/workflows/build.yml` 成功，并断言 `windows`/`linux` 两个 job、Linux 步骤顺序、artifact 路径与 `if-no-files-found: error`、新增 apt 包均在位。

### 3.2 明确**未**验证（重要）

- **未在 Linux 真机/容器运行修改后的两个脚本**，包括真实网络下载、`bin/scrcpy --version`、`bin/adb version`、`tar` 打包；
- **未运行任何 Rust 命令**：本机没有 `cargo`/`rustc`，所以 `cargo fmt --check`、`cargo clippy`、`cargo test`、`cargo build --release` 均未执行，代码当前是否 rustfmt-clean、clippy 是否零告警未知；
- **未在 GitHub Actions 上跑过 workflow**，YAML 只做了语法/结构校验，`libxkbcommon-x11-dev` 等 apt 包未实际 `apt-get install` 验证；
- `fetch-linux-deps.sh` 的**真实下载路径**没有端到端跑过（本机到 GitHub release 很慢，约 10–30 KB/s；tar.gz 与 platform-tools 是手工用 curl 下载后核对的），脚本内的校验和/结构判断逻辑用合成包验证；
- `.gitattributes` 无 git 仓库，未实际验证 checkout 行为。

---

## 4. 风险与后续建议

### 4.1 需要关注的风险

1. **首次 CI 可能因 `cargo fmt --check` 失败**：本机无 cargo，无法预知当前代码是否 rustfmt-clean。若失败，本地执行 `cargo fmt --all` 后提交即可（不涉及逻辑）。
2. **Clippy 用 warning-only**：不会阻塞，但也无法保证零告警；建议告警清零后改为 `cargo clippy --all-targets -- -D warnings`。
3. **apt 包名未实测**：`libxkbcommon-x11-dev`、`libxcb-*-dev`、`libudev1` 都是 Ubuntu 24.04 常规包，但未在本环境 `apt-get install` 验证；若 CI 报 “Unable to locate package”，按报错删改对应 `lib*-dev` 即可。
4. **scrcpy 版本被固定为 5.0.1**：这是有意的可复现选择。升级步骤：改 `SCRCPY_VERSION` 默认值，并把新 release `SHA256SUMS.txt` 里 `scrcpy-linux-x86_64-vX.Y.Z.tar.gz` 的 SHA-256 填进脚本默认值（或必须显式传 `SCRCPY_SHA256`）。
5. **Linux 静态包的许可证细节**：包内 FFmpeg/SDL 是**静态**链进 scrcpy 的（`readelf` 无 `avcodec.so` 等），与 `THIRD_PARTY_LICENSES.md` 里“FFmpeg 动态链接”的描述在 Linux 上不完全一致。bin 里现在会带 scrcpy 的 Apache-2.0 LICENSE；若发布 Linux 包，建议请项目负责人复核 LGPL-2.1 静态链接的合规路径（提供目标文件/重链接方式，或改用动态 FFmpeg 构建）。
6. **运行时依赖 `libudev.so.1`**：最终用户机器上运行 `bin/scrcpy` 需要该库（Ubuntu/Fedora 桌面默认满足；精简容器可能没有）。这是上游静态包的依赖，不是本脚本能消除的，可写进 README 的 Linux 运行要求（README 不在本次写入范围）。
7. **`fetch-windows-deps.ps1` 仍有同类问题**（不在本任务写入范围，未改动）：使用未认证 GitHub API `releases/latest`（限流风险）、无 SHA-256 校验、`Copy-Item *` 整目录复制。建议后续与 Linux 脚本采用同样的固定版本 + 校验和策略。
8. **CI 没有 shellcheck/actionlint、没有依赖缓存**：都是可选项。审校过程中工作区已出现 `Cargo.lock`（由 `cargo build` 生成）；CI 目前**不加** `--locked`，避免 `Cargo.toml` 改动与 lock 不同步时 CI 直接失败；等依赖稳定后建议加 `--locked` 提升可复现性。`vendor/scrcpy-*` 是另一成员的工作目录，当前仓库没有 `.cargo/config.toml`，不会影响 CI 从 crates.io 解析依赖。

### 4.2 建议在 Linux 上做一次人工复核

```bash
# 1) 静态检查
bash -n scripts/fetch-linux-deps.sh scripts/package-linux.sh
shellcheck scripts/fetch-linux-deps.sh scripts/package-linux.sh

# 2) 真实下载 + 校验 + 版本
bash scripts/fetch-linux-deps.sh
ls -l bin/                       # 期望 scrcpy、scrcpy-server、adb、LICENSE.scrcpy
bin/scrcpy --version | head -n1  # 期望 scrcpy 5.0.1
bin/adb version | head -n1

# 3) Rust 侧（需要网络拉 crates.io）
cargo fmt --all --check
cargo clippy --all-targets
cargo test --all-targets
cargo build --release

# 4) 打包
bash scripts/package-linux.sh
tar -tzf dist/phone-audio-linux-x86_64.tar.gz | sort
./dist/phone-audio-linux/phone-audio   # 需要桌面环境/显示器

# 5) 可选：在 headless 机器上验证 scrcpy 依赖
ldd bin/scrcpy bin/adb
```

---

## 5. 变更文件清单

| 文件 | 变更 |
| --- | --- |
| `scripts/fetch-linux-deps.sh` | 平台/架构守卫、固定版本、SHA-256 校验、内置 adb 优先、`find -print -quit`、失败回滚、版本 ≥2.1、LICENSE 携带、重试与依赖预检、显式安装权限 |
| `scripts/package-linux.sh` | Linux 守卫、架构校验、版本解析/比较重写、强制检查 scrcpy-server、显式复制 bin 内容（含 LICENSE.scrcpy）、权限、构建产物校验、sha256 输出 |
| `.github/workflows/build.yml` | 两 job 补 fmt/clippy/test；toolchain 组件；补 Linux apt 依赖（xkbcommon-x11/xcb/libudev1 等）；artifact `if-no-files-found: error`；原有 job/步骤全部保留 |
| `.gitattributes` | 新增；`*.sh` 强制 LF，二进制文件标记 |
| `docs/linux-ci.md` | 本报告 |

未改动：`src/`、`Cargo.toml`、`bin/`、`README.md`、`scripts/fetch-windows-deps.ps1`、`scripts/package-windows.ps1`。
