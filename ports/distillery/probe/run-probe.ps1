param(
    [int]$Port = 8732,
    [string]$TargetDir = 'C:\t\distillery-model-probe',
    [string]$WasmBindgen = 'wasm-bindgen'
)

$ErrorActionPreference = 'Stop'
$probeRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$mereRoot = (Resolve-Path (Join-Path $probeRoot '..\..\..')).Path
$env:CARGO_TARGET_DIR = $TargetDir
$env:DISTILLERY_PROBE_COMMIT = (git -C $mereRoot rev-parse HEAD).Trim()
$ownedStatus = git -C $mereRoot status --porcelain -- `
    ports/distillery/probe `
    crates/eidetic/muniment/Cargo.toml `
    crates/eidetic/muniment/src/indexeddb_backend.rs `
    crates/intel/esp/src/embed/bert/provider.rs
$env:DISTILLERY_PROBE_DIRTY = if ($ownedStatus) { 'true' } else { 'false' }

New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null
$bindgenVersion = (& $WasmBindgen --version).Trim()
if ($bindgenVersion -ne 'wasm-bindgen 0.2.122') {
    throw "The probe requires wasm-bindgen CLI 0.2.122; got '$bindgenVersion'. Pass -WasmBindgen with the matching executable."
}
Push-Location $TargetDir
try {
    cargo build --locked --manifest-path (Join-Path $probeRoot 'Cargo.toml') --release --target wasm32-unknown-unknown
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

$wasm = Join-Path $TargetDir 'wasm32-unknown-unknown\release\distillery_model_probe.wasm'
$package = Join-Path $probeRoot 'web\pkg'
& $WasmBindgen --target web --out-dir $package $wasm
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$url = "http://localhost:$Port/ports/distillery/probe/web/"
Write-Host "Distillery model probe: $url"
Write-Host 'Press Ctrl+C to stop the server.'
python -m http.server $Port --directory $mereRoot
