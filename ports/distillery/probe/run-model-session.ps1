param(
    [string]$ModelDir = (Join-Path $PSScriptRoot '..\..\..\models\smollm2-135m-instruct-contradiction-lora'),
    [string]$TargetDir = 'C:\t\distillery-model-session'
)

$ErrorActionPreference = 'Stop'
$probeRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$mereRoot = (Resolve-Path (Join-Path $probeRoot '..\..\..')).Path
$fixture = Join-Path $probeRoot 'session-fixture\Cargo.toml'
$resolvedModel = (Resolve-Path -LiteralPath $ModelDir).Path
$env:CARGO_TARGET_DIR = $TargetDir
$env:ESP_MODEL_SESSION_COMMIT = (git -C $mereRoot rev-parse HEAD).Trim()
$ownedStatus = git -C $mereRoot status --porcelain -- `
    crates/intel/esp `
    crates/eidetic/eidetic-core `
    ports/distillery/probe/session-fixture `
    ports/distillery/probe/run-model-session.ps1
$env:ESP_MODEL_SESSION_DIRTY = if ($ownedStatus) { 'true' } else { 'false' }

# Cargo discovers config from its current directory rather than the manifest
# path. Run outside the checkout so the gitignored local patch redirects cannot
# contaminate the committed standalone lockfile or its receipt.
Push-Location ([System.IO.Path]::GetTempPath())
try {
    cargo run --release --locked --manifest-path $fixture -- $resolvedModel
    $cargoExit = $LASTEXITCODE
} finally {
    Pop-Location
}
if ($cargoExit -ne 0) { exit $cargoExit }
