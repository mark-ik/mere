# Cross-repo smoke for the path-dep lattice (mere ← serval ← netrender, + netfetcher).
#
# The lattice has no CI; a `git pull` (or an agent edit) in any sibling can break
# the others silently. This script is the minimal net: targeted `cargo check`s of
# the load-bearing crates in dependency order, innermost first, so a breakage
# names the repo that introduced it. Run after pulling/landing cross-repo work.
#
#   pwsh scripts/cross-repo-smoke.ps1            # checks only (~minutes warm)
#   pwsh scripts/cross-repo-smoke.ps1 -Tests     # + the fast lib-test suites
#   pwsh scripts/cross-repo-smoke.ps1 -KeepGoing # don't stop at first failure
#
# Logs land in repos/mere/target/smoke/ (gitignored). See the external-deps
# topology brief (design_docs/2026-05-24_external_deps_topology_brief.md) for
# the lattice this protects.

param(
    [switch]$Tests,
    [switch]$KeepGoing
)

$ErrorActionPreference = "Stop"
$code = Split-Path (Split-Path $PSScriptRoot)   # repos/mere/scripts -> repos -> ...
$repos = Split-Path $PSScriptRoot | Split-Path  # repos/
$logDir = Join-Path $PSScriptRoot "..\target\smoke"
New-Item -ItemType Directory -Force $logDir | Out-Null
$failures = @()

function Step {
    param([string]$Name, [string]$Dir, [string[]]$CargoArgs)
    $log = Join-Path $logDir ("{0}.log" -f ($Name -replace '[^\w-]', '_'))
    Write-Host ("== {0}" -f $Name) -NoNewline
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    Push-Location $Dir
    try {
        & cargo @CargoArgs *> $log
        $ok = $LASTEXITCODE -eq 0
    } finally {
        Pop-Location
        $sw.Stop()
    }
    if ($ok) {
        Write-Host ("  ok  ({0:n0}s)" -f $sw.Elapsed.TotalSeconds) -ForegroundColor Green
    } else {
        Write-Host ("  FAIL ({0:n0}s)  -> {1}" -f $sw.Elapsed.TotalSeconds, $log) -ForegroundColor Red
        $script:failures += $Name
        if (-not $KeepGoing) {
            Write-Host "stopping at first failure (use -KeepGoing to continue)"
            exit 1
        }
    }
}

$serval     = Join-Path $repos "serval"
$netrender  = Join-Path $repos "netrender"
$netfetcher = Join-Path $repos "netfetcher"
$mere       = Join-Path $repos "mere"

# Innermost first: a failure here is the root cause for everything after it.
Step "netrender (netrender + text + lowering)" $netrender @(
    "check", "-p", "netrender", "-p", "netrender_text", "-p", "paint_list_render")
Step "netfetcher" $netfetcher @("check")
Step "serval components (layout/render/winit-host/xilem-serval/scripted-dom)" $serval @(
    "check", "-p", "serval-layout", "-p", "serval-render", "-p", "serval-winit-host",
    "-p", "xilem-serval", "-p", "serval-scripted-dom")
Step "serval pelt (default member)" $serval @("check", "-p", "pelt")
Step "mere orrery" $mere @("check", "-p", "orrery")
Step "mere meerkat (full cross-repo graph)" $mere @("check", "-p", "meerkat")

if ($Tests) {
    # Sequential on purpose (one cargo test invocation at a time).
    Step "tests: xilem-serval" $serval @("test", "-p", "xilem-serval", "--lib")
    Step "tests: serval-render" $serval @("test", "-p", "serval-render")
    Step "tests: meerkat" $mere @("test", "-p", "meerkat")
}

if ($failures.Count -gt 0) {
    Write-Host ("smoke FAILED: {0}" -f ($failures -join ", ")) -ForegroundColor Red
    exit 1
}
Write-Host "smoke green across the lattice" -ForegroundColor Green
