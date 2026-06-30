param(
    [ValidateSet("check", "build", "run", "test-roster", "drive")]
    [string]$Command = "run",
    [switch]$Release,
    [string]$TargetDir = "C:\t\meerkat-target"
)

$ErrorActionPreference = "Stop"
$repo = Split-Path $PSScriptRoot
$profile = if ($Release) { "release" } else { "debug" }
$exe = Join-Path $TargetDir "$profile\meerkat.exe"

New-Item -ItemType Directory -Force $TargetDir | Out-Null
$env:CARGO_TARGET_DIR = $TargetDir

function Invoke-Cargo {
    param([string[]]$CargoArgs)

    Push-Location $repo
    try {
        & cargo @CargoArgs
        if ($LASTEXITCODE -ne 0) {
            throw "cargo failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

function Build-Meerkat {
    $args = @("build", "-p", "meerkat", "--bin", "meerkat")
    if ($Release) {
        $args += "--release"
    }
    Invoke-Cargo $args
    Write-Host "meerkat exe: $exe"
}

switch ($Command) {
    "check" {
        Invoke-Cargo @("check", "-p", "meerkat", "--bin", "meerkat", "--message-format=short")
    }
    "build" {
        Build-Meerkat
    }
    "run" {
        Build-Meerkat
        & $exe
        if ($LASTEXITCODE -ne 0) {
            throw "meerkat exited with code $LASTEXITCODE"
        }
    }
    "drive" {
        Build-Meerkat
        $process = Start-Process -FilePath $exe -WorkingDirectory $repo -PassThru
        Write-Host "meerkat pid: $($process.Id)"
    }
    "test-roster" {
        $filters = @(
            "graphlet_card",
            "roster_marks_the_active_tab",
            "connections_spec_fans_multiple_relation_cells",
            "hidden_relation_records_round_trip_endpoint_visibility"
        )
        foreach ($filter in $filters) {
            Invoke-Cargo @("test", "-p", "meerkat", "--bin", "meerkat", $filter, "--", "--nocapture")
        }
    }
}
