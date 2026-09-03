# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

param(
    [string]$ModelDir = (Join-Path $PSScriptRoot '..\..\..\models\all-MiniLM-L6-v2'),
    [string]$Text = 'query: Mere keeps a model local.',
    [int]$CancellationBatch = 512,
    [string]$TargetDir = 'C:\t\distillery-remote-minilm',
    [switch]$Matrix,
    [int]$ProcessTimeoutSeconds = 120
)

$ErrorActionPreference = 'Stop'
$probeRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$mereRoot = (Resolve-Path (Join-Path $probeRoot '..\..\..')).Path
$fixture = Join-Path $probeRoot 'remote-fixture\Cargo.toml'
$resolvedModel = (Resolve-Path -LiteralPath $ModelDir).Path
$env:DISTILLERY_REMOTE_PROBE_COMMIT = (git -C $mereRoot rev-parse HEAD).Trim()
$ownedStatus = git -C $mereRoot status --porcelain -- `
    ports/distillery `
    crates/intel/esp `
    crates/mesh/mesh `
    crates/mesh/host `
    crates/murm/transport `
    support/patches/burn-remote
$env:DISTILLERY_REMOTE_PROBE_DIRTY = if ($ownedStatus) { 'true' } else { 'false' }

function Build-Fixture {
    param(
        [string]$Profile,
        [string]$Features
    )

    $profileTarget = if ($Profile -eq 'plain') { $TargetDir } else { Join-Path $TargetDir $Profile }
    $env:CARGO_TARGET_DIR = $profileTarget
    $featureArgs = if ($Features) { @('--features', $Features) } else { @() }
    cargo build --release --locked --manifest-path $fixture @featureArgs | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "fixture build failed for $Profile" }
    Join-Path $profileTarget 'release\distillery-remote-minilm-fixture.exe'
}

function Build-DiagnosticFixture {
    param(
        [string]$Profile,
        [string]$Features
    )

    # Reuse the already-built combined target for the two diagnostic feature sets. Separate Cargo
    # invocations do not unify features; preserve each resulting executable before the next build
    # overwrites it so the row still names and runs one exact feature graph.
    $profileTarget = Join-Path $TargetDir 'fusion-autotune'
    $env:CARGO_TARGET_DIR = $profileTarget
    cargo build --release --locked --manifest-path $fixture --features $Features | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "fixture build failed for $Profile" }

    $artifactDir = Join-Path $TargetDir 'matrix-artifacts'
    New-Item -ItemType Directory -Force -Path $artifactDir | Out-Null
    $preserved = Join-Path $artifactDir "distillery-remote-minilm-fixture-$Profile.exe"
    Copy-Item -LiteralPath (Join-Path $profileTarget 'release\distillery-remote-minilm-fixture.exe') `
        -Destination $preserved -Force
    $preserved
}

function Invoke-FixtureRow {
    param(
        [string]$Name,
        [string]$Mode,
        [string]$Features,
        [string]$Executable
    )

    $artifactDir = Join-Path $TargetDir 'matrix-artifacts'
    New-Item -ItemType Directory -Force -Path $artifactDir | Out-Null
    $stdoutPath = Join-Path $artifactDir "$Name.stdout.json"
    $stderrPath = Join-Path $artifactDir "$Name.stderr.txt"

    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $Executable
    $start.UseShellExecute = $false
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.CreateNoWindow = $true
    $start.ArgumentList.Add($Mode)
    $start.ArgumentList.Add($resolvedModel)
    $start.ArgumentList.Add($Text)
    if ($Mode -eq 'remote') { $start.ArgumentList.Add($CancellationBatch.ToString()) }

    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    if (-not $process.Start()) { throw "fixture process did not start for $Name" }
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $timedOut = -not $process.WaitForExit($ProcessTimeoutSeconds * 1000)
    if ($timedOut) {
        $process.Kill($true)
        $process.WaitForExit()
    }
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $stderr = $stderrTask.GetAwaiter().GetResult()
    [System.IO.File]::WriteAllText($stdoutPath, $stdout)
    [System.IO.File]::WriteAllText($stderrPath, $stderr)
    $exitCode = if ($timedOut) { $null } else { $process.ExitCode }
    $passes = -not $timedOut -and $exitCode -eq 0

    [pscustomobject]@{
        name = $Name
        mode = $Mode
        features = $Features
        timed_out = $timedOut
        exit_code = $exitCode
        stdout = $stdoutPath
        stderr = $stderrPath
        passes = $passes
    }
}

# Avoid the gitignored checkout-local Cargo redirects so the committed nested
# lockfile is the exact clean-checkout dependency graph used by the receipt.
Push-Location ([System.IO.Path]::GetTempPath())
try {
    if (-not $Matrix) {
        $env:CARGO_TARGET_DIR = $TargetDir
        cargo run --release --locked --manifest-path $fixture -- remote $resolvedModel $Text $CancellationBatch
        $cargoExit = $LASTEXITCODE
    } else {
        $plain = Build-Fixture -Profile 'plain' -Features ''
        $combined = Build-Fixture -Profile 'fusion-autotune' -Features 'fusion-autotune'
        $rows = @(
            Invoke-FixtureRow -Name 'local-plain' -Mode 'local' -Features 'plain' -Executable $plain
            Invoke-FixtureRow -Name 'local-fusion-autotune' -Mode 'local' -Features 'fusion-autotune' -Executable $combined
            Invoke-FixtureRow -Name 'remote-plain' -Mode 'remote' -Features 'plain' -Executable $plain
            Invoke-FixtureRow -Name 'remote-fusion-autotune' -Mode 'remote' -Features 'fusion-autotune' -Executable $combined
        )

        $remoteCombined = $rows | Where-Object name -eq 'remote-fusion-autotune'
        if (-not $remoteCombined.passes) {
            $autotune = Build-DiagnosticFixture -Profile 'autotune-only' -Features 'autotune'
            $fusion = Build-DiagnosticFixture -Profile 'fusion-only' -Features 'fusion'
            $rows += @(
                Invoke-FixtureRow -Name 'local-autotune-only' -Mode 'local' -Features 'autotune' -Executable $autotune
                Invoke-FixtureRow -Name 'remote-autotune-only' -Mode 'remote' -Features 'autotune' -Executable $autotune
                Invoke-FixtureRow -Name 'local-fusion-only' -Mode 'local' -Features 'fusion' -Executable $fusion
                Invoke-FixtureRow -Name 'remote-fusion-only' -Mode 'remote' -Features 'fusion' -Executable $fusion
            )
        }

        [pscustomobject]@{
            schema = 'distillery.remote-minilm-matrix/v1'
            process_timeout_seconds = $ProcessTimeoutSeconds
            rows = $rows
            passes = -not ($rows | Where-Object { -not $_.passes })
        } | ConvertTo-Json -Depth 5
        $cargoExit = if ($rows | Where-Object { -not $_.passes }) { 1 } else { 0 }
    }
} finally {
    Pop-Location
}
if ($cargoExit -ne 0) { exit $cargoExit }
