param([string]$Source, [string]$Destination, [string]$Target)
$ErrorActionPreference='Stop'
if (!$Source -or !$Destination -or !$Target) {throw 'Source, Destination and Target are required'}
$env:CARGO_TARGET_DIR=$Target
$env:LIBCLANG_PATH='C:\Program Files\LLVM\bin'
$stamp=Get-Date -Format 'yyyyMMdd-HHmmss-fff'
$stage=Join-Path $Destination "builds\$stamp"
New-Item -ItemType Directory -Force $stage | Out-Null
Push-Location $Source
try {
    & cargo build --locked --manifest-path tools/lemon-wave-standalone/Cargo.toml --bin lemon-wave-standalone --features live-design *> (Join-Path $stage 'build.log')
    if ($LASTEXITCODE -ne 0) {throw "Development build failed. See $stage\build.log"}
    Copy-Item -LiteralPath (Join-Path $Target 'debug\lemon-wave-standalone.exe') -Destination (Join-Path $stage 'Lemon Wave Development.exe')
    $settings=Join-Path $Destination 'Settings.json'
    if (Test-Path -LiteralPath $settings) {Copy-Item -LiteralPath $settings -Destination $stage}
    else { '{"midi_input":"LPK25","output_device":null,"sample_rate":48000,"period_size":256,"dpi_scale":1.0}' | Set-Content -Encoding utf8NoBOM (Join-Path $stage 'Settings.json') }
    $state=@{ executable=(Join-Path $stage 'Lemon Wave Development.exe'); design=(Join-Path $Source 'plugins\lemon-wave\design\live.json'); source=$Source; built=$stamp }
    $temp=Join-Path $Destination 'development-next.json'
    $state | ConvertTo-Json | Set-Content -Encoding utf8NoBOM $temp
    Move-Item -LiteralPath $temp -Destination (Join-Path $Destination 'development.json') -Force
    Write-Output "Development build ready: $stage"
} finally {Pop-Location}
