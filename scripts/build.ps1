param(
    [switch]$Test,
    [switch]$Check,
    [switch]$DebugBuild
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $projectRoot

# 本机 Rust 安装在 D:\Rust，优先使用该工具链。
if (Test-Path "D:\Rust\.cargo\bin\cargo.exe") {
    $env:RUSTUP_HOME = "D:\Rust\.rustup"
    $env:CARGO_HOME = "D:\Rust\.cargo"
    $env:Path = "D:\Rust\.cargo\bin;$env:Path"
    Write-Host "使用 Rust 工具链: D:\Rust" -ForegroundColor Cyan
} else {
    Write-Host "未找到 D:\Rust，使用 PATH 中的 cargo。" -ForegroundColor Yellow
}

Write-Host "==> 构建前端" -ForegroundColor Cyan
& npm run build
if ($LASTEXITCODE -ne 0) { throw "前端构建失败" }

if ($Test) {
    Write-Host "==> cargo test" -ForegroundColor Cyan
    & cargo test --manifest-path src-tauri/Cargo.toml
} elseif ($Check) {
    Write-Host "==> cargo check" -ForegroundColor Cyan
    & cargo check --manifest-path src-tauri/Cargo.toml
} elseif ($DebugBuild) {
    Write-Host "==> cargo build (debug)" -ForegroundColor Cyan
    & cargo build --manifest-path src-tauri/Cargo.toml
} else {
    Write-Host "==> cargo build --release" -ForegroundColor Cyan
    & cargo build --release --manifest-path src-tauri/Cargo.toml
}

if ($LASTEXITCODE -ne 0) {
    throw "Rust 构建失败，退出码 $LASTEXITCODE"
}

Write-Host "完成。" -ForegroundColor Green
