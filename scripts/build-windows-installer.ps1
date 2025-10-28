param(
    [string]$Target = "x86_64-pc-windows-msvc"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Write-Host "Building report-builder release binary for target $Target..."
cargo build --release --target $Target

Write-Host "Building Windows installer with WiX..."
cargo wix --target $Target

Write-Host "Installer created in target\wix"
