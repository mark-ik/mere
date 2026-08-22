param(
    [int]$Port = 8734,
    [string]$TargetDir = 'C:\t\burn-browser-embedding-repro',
    [string]$WasmBindgen = 'wasm-bindgen'
)

$ErrorActionPreference = 'Stop'
$reproRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$env:CARGO_TARGET_DIR = $TargetDir

New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null
$bindgenVersion = (& $WasmBindgen --version).Trim()
if ($bindgenVersion -ne 'wasm-bindgen 0.2.122') {
    throw "The repro requires wasm-bindgen CLI 0.2.122; got '$bindgenVersion'."
}
Push-Location $TargetDir
try {
    cargo build --locked --manifest-path (Join-Path $reproRoot 'Cargo.toml') --release --target wasm32-unknown-unknown
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

$wasm = Join-Path $TargetDir 'wasm32-unknown-unknown\release\burn_browser_embedding_repro.wasm'
$package = Join-Path $reproRoot 'web\pkg'
& $WasmBindgen --target web --out-dir $package $wasm
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$url = "http://localhost:$Port/"
Write-Host "Burn browser embedding repro: $url"
Write-Host 'Press Ctrl+C to stop the server.'
python -m http.server $Port --directory $reproRoot\web
