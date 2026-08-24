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

cargo run --release --manifest-path $fixture -- $resolvedModel
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
