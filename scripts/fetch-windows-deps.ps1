# 下载 scrcpy 官方 win64 包与 adb 到 bin\，然后（如果存在）叠加本项目的
# 输出设备补丁版 scrcpy.exe。
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File scripts\fetch-windows-deps.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\fetch-windows-deps.ps1 -OfficialScrcpy
#
# 参数：
#   -OfficialScrcpy   不叠加补丁版 scrcpy（用于对比/排错）
#   -ScrcpyVersion    固定 scrcpy 版本，默认 5.0.1（改版本需同时改 SHA256）
#
# 说明：
# - scrcpy 版本固定，避免“latest”更新导致 DLL/补丁不匹配。
# - 下载后校验 SHA-256；可用环境变量 PHONE_AUDIO_SCRCPY_WIN64_SHA256 覆盖。
# - 补丁版 exe 来自 vendor\scrcpy-patched\win64\scrcpy.exe（见 docs/scrcpy-output-device.md）。
param(
    [switch]$OfficialScrcpy,
    [string]$ScrcpyVersion = "5.0.1"
)

$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$ProgressPreference = "SilentlyContinue"

$root = Split-Path -Parent $PSScriptRoot
$bin = Join-Path $root "bin"
New-Item -ItemType Directory -Force -Path $bin | Out-Null

$expectedSha = $env:PHONE_AUDIO_SCRCPY_WIN64_SHA256
if (-not $expectedSha) {
    # scrcpy-win64-v5.0.1.zip，取自官方 SHA256SUMS.txt（也记录在 vendor/downloads）。
    if ($ScrcpyVersion -ne "5.0.1") {
        throw "只内置了 $($ScrcpyVersion) 的校验和：请通过 PHONE_AUDIO_SCRCPY_WIN64_SHA256 提供"
    }
    $expectedSha = "B12A2C4EE8BE317422451FC7DCF8EE20A71B5EA7EF9AD73DDD825A25316227A5"
}

$assetName = "scrcpy-win64-v$ScrcpyVersion.zip"
$downloadUrl = "https://github.com/Genymobile/scrcpy/releases/download/v$ScrcpyVersion/$assetName"

$tmp = Join-Path $env:TEMP ("phone-audio-scrcpy-" + [guid]::NewGuid().ToString("N"))
$extract = Join-Path $tmp "extract"
New-Item -ItemType Directory -Force -Path $extract | Out-Null

try {
    Write-Host "下载 scrcpy $ScrcpyVersion ($assetName)..."
    $zip = Join-Path $tmp $assetName
    Invoke-WebRequest -Uri $downloadUrl -OutFile $zip -UseBasicParsing

    $actual = (Get-FileHash -Path $zip -Algorithm SHA256).Hash
    if ($actual -ne $expectedSha) {
        throw "scrcpy 压缩包 SHA-256 不匹配：期望 $expectedSha，实际 $actual。请检查下载源或设置 PHONE_AUDIO_SCRCPY_WIN64_SHA256。"
    }
    Write-Host "SHA-256 校验通过"

    Expand-Archive -Path $zip -DestinationPath $extract -Force
    $inner = Get-ChildItem $extract -Directory | Select-Object -First 1
    $source = if ($inner) { $inner.FullName } else { $extract }

    Write-Host "复制到 $bin ..."
    Copy-Item (Join-Path $source "*") $bin -Recurse -Force

    if (-not (Test-Path (Join-Path $bin "adb.exe"))) {
        Write-Host "scrcpy 包内没有 adb，下载 platform-tools..."
        $ptZip = Join-Path $tmp "platform-tools.zip"
        Invoke-WebRequest `
            -Uri "https://dl.google.com/android/repository/platform-tools-latest-windows.zip" `
            -OutFile $ptZip -UseBasicParsing
        Expand-Archive -Path $ptZip -DestinationPath (Join-Path $tmp "pt") -Force
        $pt = Join-Path $tmp "pt\platform-tools"
        Copy-Item (Join-Path $pt "adb.exe") $bin -Force
        Copy-Item (Join-Path $pt "AdbWinApi.dll") $bin -Force -ErrorAction SilentlyContinue
        Copy-Item (Join-Path $pt "AdbWinUsbApi.dll") $bin -Force -ErrorAction SilentlyContinue
    }

    foreach ($required in @("scrcpy.exe", "adb.exe", "scrcpy-server")) {
        if (-not (Test-Path (Join-Path $bin $required))) {
            throw "解压后缺少 $required，请检查 release 结构"
        }
    }

    # 叠加本项目补丁版 scrcpy（支持 --audio-output-device / --list-audio-devices）。
    $patched = Join-Path $root "vendor\scrcpy-patched\win64\scrcpy.exe"
    if ($OfficialScrcpy) {
        Write-Host "已指定 -OfficialScrcpy：保留官方版 scrcpy.exe（不支持选择输出设备）"
    } elseif (Test-Path $patched) {
        $expectedPatched = "20CE66F347D77A6784005918798517574EEFA729728991380A3FCF547F057F99"
        $actualPatched = (Get-FileHash -Path $patched -Algorithm SHA256).Hash
        if ($actualPatched -ne $expectedPatched) {
            throw "补丁版 scrcpy.exe 的 SHA-256 不匹配：期望 $expectedPatched，实际 $actualPatched。请重新构建（见 docs/scrcpy-output-device.md）。"
        }
        Copy-Item $patched (Join-Path $bin "scrcpy.exe") -Force
        Write-Host "已应用补丁版 scrcpy（支持选择电脑输出设备）"
    } else {
        Write-Host "警告：没有找到 vendor\scrcpy-patched\win64\scrcpy.exe，使用官方版（只能跟随系统默认输出）。"
    }

    $versionOut = & (Join-Path $bin "scrcpy.exe") --version 2>&1 | Out-String
    $helpOut = & (Join-Path $bin "scrcpy.exe") --help 2>&1 | Out-String
    $versionLine = ($versionOut -split "`n")[0].Trim()
    if ($versionOut -notmatch 'scrcpy\s+(\d+)\.(\d+)') {
        throw "无法解析 scrcpy --version 输出：$versionLine"
    }
    $major = [int]$Matches[1]
    $minor = [int]$Matches[2]
    if ($major -lt 2 -or ($major -eq 2 -and $minor -lt 1)) {
        throw "scrcpy 版本过旧（$versionLine），需要 >= 2.1"
    }

    $hasDeviceOption = $helpOut -match '--audio-output-device'
    Write-Host "完成：$versionLine"
    if ($hasDeviceOption) {
        Write-Host "输出设备选择：可用（补丁版 scrcpy）"
    } else {
        Write-Host "输出设备选择：不可用（官方 scrcpy，只能跟随系统默认输出）"
    }
}
finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
