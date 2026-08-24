param(
    [string]$ModelDir = (Join-Path $PSScriptRoot '..\..\..\models\all-MiniLM-L6-v2'),
    [string]$Text = 'query: Mere keeps a model local.',
    [int]$CancellationBatch = 512,
    [string]$TargetDir = 'C:\t\distillery-remote-minilm'
)

$ErrorActionPreference = 'Stop'
$probeRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$mereRoot = (Resolve-Path (Join-Path $probeRoot '..\..\..')).Path
$fixture = Join-Path $probeRoot 'remote-fixture\Cargo.toml'
$resolvedModel = (Resolve-Path -LiteralPath $ModelDir).Path
$env:CARGO_TARGET_DIR = $TargetDir
$env:DISTILLERY_REMOTE_PROBE_COMMIT = (git -C $mereRoot rev-parse HEAD).Trim()
$ownedStatus = git -C $mereRoot status --porcelain -- `
    ports/distillery `
    crates/intel/esp `
    crates/mesh/mesh `
    crates/mesh/host `
    crates/murm/transport `
    support/patches/burn-remote
$env:DISTILLERY_REMOTE_PROBE_DIRTY = if ($ownedStatus) { 'true' } else { 'false' }

cargo run --release --manifest-path $fixture -- $resolvedModel $Text $CancellationBatch
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
