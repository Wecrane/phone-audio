# 生成 dist\phone-audio-windows-x64.zip。
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
#
# 前置条件：
#   - 已运行 scripts\fetch-windows-deps.ps1（bin\ 中有 scrcpy/adb/server）
#   - 已安装 Rust（MSVC 或 GNU 工具链均可）
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

foreach ($required in @("bin\scrcpy.exe", "bin\adb.exe", "bin\scrcpy-server")) {
    if (-not (Test-Path $required)) {
        throw "$required 不存在，请先运行 scripts\fetch-windows-deps.ps1"
    }
}

Write-Host "构建 Rust release..."
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build 失败" }
if (-not (Test-Path "target\release\phone-audio.exe")) {
    throw "构建产物 target\release\phone-audio.exe 不存在"
}

# 输出设备补丁版 scrcpy（若存在）：覆盖 bin\scrcpy.exe，其余 DLL/server 沿用官方包。
$patchedDir = "vendor\scrcpy-patched"
$patchedExe = Join-Path $patchedDir "win64\scrcpy.exe"
$usesPatched = $false
if (Test-Path $patchedExe) {
    $actual = (Get-FileHash -Path $patchedExe -Algorithm SHA256).Hash
    $expected = "20CE66F347D77A6784005918798517574EEFA729728991380A3FCF547F057F99"
    if ($actual -ne $expected) {
        throw "补丁版 scrcpy.exe 的 SHA-256 不匹配：期望 $expected，实际 $actual"
    }
    Copy-Item $patchedExe "bin\scrcpy.exe" -Force
    $usesPatched = $true
}

# 在 TEMP 里组装，避免 dist\phone-audio 里正在运行的 adb/scrcpy 锁住文件
# （从 dist 目录启动应用时，adb server 会一直占用 bin\adb.exe）。
$stage = Join-Path $env:TEMP ("phone-audio-package-" + [guid]::NewGuid().ToString("N"))
$pkg = Join-Path $stage "phone-audio"
New-Item -ItemType Directory -Force -Path $pkg | Out-Null

try {
    Copy-Item "target\release\phone-audio.exe" $pkg
    Copy-Item "bin" $pkg -Recurse
    Copy-Item "licenses" $pkg -Recurse
    if (Test-Path "assets") {
        Copy-Item "assets" $pkg -Recurse
    }
    if (Test-Path "docs") {
        Copy-Item "docs" $pkg -Recurse
    }
    Copy-Item "README.md", "LICENSE", "NOTICE", "THIRD_PARTY_LICENSES.md", "TESTING.md", "CHANGELOG.md", "CONTRIBUTING.md", "SECURITY.md" $pkg

    # Apache-2.0 §4：随包提供补丁 diff、修改说明和上游 LICENSE。
    if ($usesPatched) {
        $patchedOut = Join-Path $pkg "scrcpy-patched"
        New-Item -ItemType Directory -Force -Path $patchedOut | Out-Null
        Copy-Item (Join-Path $patchedDir "scrcpy-5.0.1-phone-audio.patch") $patchedOut
        Copy-Item (Join-Path $patchedDir "CHANGES-phone-audio.md") $patchedOut
        Copy-Item (Join-Path $patchedDir "build-windows.sh") $patchedOut
        Copy-Item (Join-Path $patchedDir "LICENSE") (Join-Path $patchedOut "LICENSE.scrcpy")
    }

    # 验证分发包中的 scrcpy 可以启动，并记录是否带输出设备补丁。
    $versionOut = & (Join-Path $pkg "bin\scrcpy.exe") --version 2>&1 | Out-String
    $helpOut = & (Join-Path $pkg "bin\scrcpy.exe") --help 2>&1 | Out-String
    $versionLine = ($versionOut -split "`n")[0].Trim()
    if ($LASTEXITCODE -ne 0 -or $versionOut -notmatch 'scrcpy\s+\d+\.\d+') {
        throw "分发包中的 scrcpy 无法运行：$versionLine"
    }
    $hasDeviceOption = $helpOut -match '--audio-output-device'
    if ($usesPatched -and -not $hasDeviceOption) {
        throw "补丁版 scrcpy 没有 --audio-output-device，拒绝打包"
    }

    $dist_dir = Join-Path $root "dist"
    New-Item -ItemType Directory -Force -Path $dist_dir | Out-Null
    $zip = Join-Path $dist_dir "phone-audio-windows-x64.zip"
    Remove-Item $zip -Force -ErrorAction SilentlyContinue
    Compress-Archive -Path $pkg -DestinationPath $zip -Force

    # 便利副本；若被正在运行的进程锁住就跳过（zip 已生成）。
    $dist = Join-Path $root "dist\phone-audio"
    try {
        if (Test-Path $dist) { Remove-Item $dist -Recurse -Force -ErrorAction Stop }
        Copy-Item $pkg $dist -Recurse
    } catch {
        Write-Host "提示：dist\phone-audio 被占用（可能有进程从该目录运行），已跳过更新；zip 产物不受影响。"
    }
}
finally {
    Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
}

$size = (Get-Item $zip).Length
$hash = (Get-FileHash -Path $zip -Algorithm SHA256).Hash
Write-Host ""
Write-Host "已生成 $zip"
Write-Host ("大小：{0:N1} MB" -f ($size / 1MB))
Write-Host "SHA-256：$hash"
Write-Host "scrcpy：$versionLine"
if ($hasDeviceOption) {
    Write-Host "输出设备选择：可用（补丁版 scrcpy）"
} else {
    Write-Host "输出设备选择：不可用（官方 scrcpy）"
}
