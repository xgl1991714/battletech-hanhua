param(
    [switch]$SkipBuild,
    [switch]$NoZip
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $projectRoot

if (-not $SkipBuild) {
    & "$PSScriptRoot\build.ps1"
    if ($LASTEXITCODE -ne 0) {
        throw "Rust release 构建失败"
    }
}

$releaseExe = Join-Path $projectRoot "src-tauri\target\release\battletech-hanhua.exe"
if (-not (Test-Path $releaseExe)) {
    throw "未找到 release exe: $releaseExe，请先执行 scripts\build.ps1"
}

$packageJson = Get-Content (Join-Path $projectRoot "package.json") -Raw | ConvertFrom-Json
$version = $packageJson.version
$outRoot = Join-Path $projectRoot "release-portable"
$outDir = Join-Path $outRoot "BATTLETECH-Hanhua-portable_$version"

if (Test-Path $outDir) {
    Remove-Item $outDir -Recurse -Force
}
New-Item -ItemType Directory -Force $outDir | Out-Null

# 使用 ASCII 文件名，避免 cmd 脚本在中文系统上的编码问题。
$portableExe = Join-Path $outDir "BATTLETECH-Hanhua.exe"
Copy-Item $releaseExe $portableExe -Force

# BTH_HOME 让配置、汉化数据、备份和 WebView2 数据都留在绿色包目录内。
$launcher = @"
@echo off
set "BTH_HOME=%~dp0data"
start "" "%~dp0BATTLETECH-Hanhua.exe"
"@
Set-Content -Path (Join-Path $outDir "start.bat") -Value $launcher -Encoding ASCII

$readme = @"
BATTLETECH 汉化工具 绿色版 v$version

运行 start.bat 启动。
程序数据保存在本目录 data\ 下（配置、汉化内容、备份、WebView2 缓存）。

系统要求：
- Windows 10/11 x64
- WebView2 Runtime（Windows 11 通常自带；缺失时请安装 Microsoft Edge WebView2 Runtime）

注意：
- 汉化/恢复前请关闭 BATTLETECH 与 RogueLauncher。
- 若游戏安装在 Program Files 等受保护目录，请以管理员身份运行。
"@
Set-Content -Path (Join-Path $outDir "README.txt") -Value $readme -Encoding UTF8

if (-not $NoZip) {
    $zipPath = Join-Path $outRoot "BATTLETECH-Hanhua-portable_$version.zip"
    if (Test-Path $zipPath) {
        Remove-Item $zipPath -Force
    }
    Compress-Archive -Path (Join-Path $outDir "*") -DestinationPath $zipPath -Force
    Write-Host "绿色包: $zipPath" -ForegroundColor Green
}

Write-Host "目录: $outDir" -ForegroundColor Green
