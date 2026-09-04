# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

# Cross-repo smoke for the path-dep lattice (mere <- genet <- netrender, + turnstone).
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
#
# Repaired 2026-08-01. It had rotted past running: it aborted at step 2 on the
# deleted `netfetcher` repo, and three more targets (`orrery`, `meerkat`,
# `xilem-serval`) no longer exist either. Anything asserted by a script that
# cannot start is not actually asserted, so treat a failure to RUN as a failure.

param(
    [switch]$Tests,
    [switch]$KeepGoing
)

$ErrorActionPreference = "Stop"
$repos = Split-Path $PSScriptRoot | Split-Path  # repos/mere/scripts -> mere -> repos
$logDir = Join-Path $PSScriptRoot "..\target\smoke"
New-Item -ItemType Directory -Force $logDir | Out-Null
$failures = @()

function Step {
    param([string]$Name, [string]$Dir, [string[]]$CargoArgs)
    if (-not (Test-Path $Dir)) {
        Write-Host ("== {0}  SKIP (no {1})" -f $Name, $Dir) -ForegroundColor Yellow
        $script:failures += "$Name (missing repo)"
        return
    }
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

# A [patch] table whose URL does not match the dependency's URL is not an error:
# Cargo ignores it and resolves from git instead, so a local edit silently does
# not take. That is exactly what `ports/graphshell/web` did until 2026-08-01,
# building its wasm artifact against the remotes for an unknown stretch.
#
# Checking for the `was not used in the crate graph` warning is too blunt (a
# copied patch table legitimately carries entries the crate does not depend on).
# Assert the specific thing that matters instead: the sibling we edit locally
# must resolve to a local path, not a git source.
function Assert-Local {
    param([string]$Name, [string]$Dir, [string]$Package, [string[]]$CargoArgs = @())
    if (-not (Test-Path $Dir)) { return }
    Push-Location $Dir
    try {
        $tree = & cargo tree @CargoArgs -i $Package 2>$null | Select-Object -First 1
        $treeOk = $LASTEXITCODE -eq 0
    } finally {
        Pop-Location
    }
    # Distinguish "cargo could not resolve at all" from "package absent". The
    # first version of this check reported both as a skip, which hid a turnstone
    # whose p2panda patch paths pointed at a directory that did not exist.
    if (-not $treeOk) {
        Write-Host ("== patch {0}: cargo could not resolve the graph" -f $Name) -ForegroundColor Red
        $script:failures += "patch $Name (unresolvable)"
        return
    }
    if (-not $tree) {
        Write-Host ("== patch {0}: {1} not in graph (skipped)" -f $Name, $Package) -ForegroundColor Yellow
        return
    }
    if ($tree -match '\(https://') {
        Write-Host ("== patch {0}: {1} resolves from GIT, not the local checkout" -f $Name, $Package) -ForegroundColor Red
        Write-Host ("   {0}" -f $tree.Trim())
        Write-Host "   the [patch] table's URL probably does not match the dependency's URL"
        $script:failures += "patch $Name/$Package"
    } else {
        Write-Host ("== patch {0}: {1} local" -f $Name, $Package) -ForegroundColor Green
    }
}

$genet      = Join-Path $repos "genet"
$netrender  = Join-Path $repos "netrender"
$mere       = Join-Path $repos "mere"
$turnstone  = Join-Path $repos "turnstone"
$web        = Join-Path $mere "ports\graphshell\web"

# Innermost first: a failure here is the root cause for everything after it.
Step "netrender (netrender + text + lowering)" $netrender @(
    "check", "-p", "netrender", "-p", "netrender_text", "-p", "paint_list_render")
Step "genet components (layout/render/render-host/winit-host/scripted-dom)" $genet @(
    "check", "-p", "genet-layout", "-p", "genet-render", "-p", "genet-render-host",
    "-p", "genet-winit-host", "-p", "genet-scripted-dom")
Step "genet pelt (default member)" $genet @("check", "-p", "pelt")
Step "mere graphshell (resident host, full sync cone)" $mere @(
    "check", "-p", "graphshell", "--features", "personal-sync")
# No Knot step: Knot left this repository on 2026-09-04 (knot-editor
# design_docs/2026-09-01_knot_editor_repository_extraction_plan.md, E2) and is
# consumed from one immutable revision. The workspace gate below covers Djinn,
# which embeds it; Knot own gates run in its own repository.
# The whole-workspace gate (green since 2026-08-12). `--all-targets` on purpose:
# the lib-only form is what hid a broken bin until that commit found it.
#
# It does NOT cover everything. A target cargo never builds cannot fail here:
# crates cfg'd out for this host (graphshell-web) and bins behind an off-by-
# default `required-features` (mere-canvas's `native-present`) are both
# invisible to it. Those need their own steps.
Step "mere workspace (the green gate)" $mere @("check", "--workspace", "--all-targets")
Step "mere-canvas bin (required-features)" $mere @(
    "check", "-p", "mere-canvas", "--features", "native-present", "--all-targets")
# The wasm presenter IS a member of mere's workspace, but nothing above reaches
# it: the steps above are targeted `-p` checks that never name it, and none of
# them target wasm32. Its lib root is `#![cfg(target_arch = "wasm32")]`, so a
# native `--workspace` check compiles it to nothing. This step is the ONLY gate
# that sees its code at all.
Step "graphshell-web (wasm32)" $web @("check", "--target", "wasm32-unknown-unknown")
Step "turnstone (the app; consumes genet + mere)" $turnstone @("check")

Assert-Local "graphshell-web" $web "genet-render-host" @("--target", "wasm32-unknown-unknown")
Assert-Local "graphshell-web" $web "netrender" @("--target", "wasm32-unknown-unknown")
Assert-Local "turnstone" $turnstone "genet-winit-host"

if ($Tests) {
    # Sequential on purpose (one cargo test invocation at a time).
    # graphshell's lib tests also assert the committed receipts in
    # ports/graphshell/docs/receipts/ still match live output (view.rs,
    # sessions.rs, policy_projection.rs), so receipt drift fails here.
    Step "tests: graphshell (incl. receipt drift)" $mere @(
        "test", "-p", "graphshell", "--features", "web", "--lib")
    Step "tests: genet-render" $genet @("test", "-p", "genet-render")
}

if ($failures.Count -gt 0) {
    Write-Host ("smoke FAILED: {0}" -f ($failures -join ", ")) -ForegroundColor Red
    exit 1
}
Write-Host "smoke green across the lattice" -ForegroundColor Green
