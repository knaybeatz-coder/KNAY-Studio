param([string]$Source, [string]$Runtime, [string]$Target)
$ErrorActionPreference='Stop'
if (!$Source -or !$Runtime -or !$Target) {throw 'Source, Runtime and Target are required'}
$env:CARGO_TARGET_DIR=$Target
$env:LIBCLANG_PATH='C:\Program Files\LLVM\bin'
$stamp=Get-Date -Format 'yyyyMMdd-HHmmss-fff'
$stage=Join-Path $Runtime "renderers\$stamp"
New-Item -ItemType Directory -Force $stage | Out-Null
Push-Location $Source
try {
    & cargo build --locked --release --manifest-path tools/lemon-wave-standalone/Cargo.toml --bin wave-preview *> (Join-Path $stage 'build.log')
    if ($LASTEXITCODE -ne 0) {throw "Preview build failed. See $stage\build.log"}
    $renderer=Join-Path $stage 'wave-preview.exe'
    Copy-Item -LiteralPath (Join-Path $Target 'release\wave-preview.exe') -Destination $renderer
    $temp=Join-Path $Runtime 'renderer-path-next.txt'
    $renderer | Set-Content -Encoding utf8NoBOM $temp
    Move-Item -LiteralPath $temp -Destination (Join-Path $Runtime 'renderer-path.txt') -Force
    Write-Output "Preview renderer ready: $renderer"
} finally {Pop-Location}
