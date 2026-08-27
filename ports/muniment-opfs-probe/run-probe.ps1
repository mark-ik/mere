# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

param(
    [int]$Port = 8733,
    [int]$SinkPort = 8734,
    [string]$TargetDir = 'C:\t\muniment-opfs-probe',
    # A directory OUTSIDE the Code workspace. Cargo discovers `.cargo/config.toml`
    # from the working directory, and `Code/.cargo/config.toml` injects a large
    # machine-local `[patch]` table whose unused entries churn the lockfile. Run
    # from here with --manifest-path and `--locked` is satisfiable, so the build
    # is reproducible and the real lockfile can be hashed.
    [string]$NeutralDir = 'C:\t',
    [string]$WasmBindgen = 'wasm-bindgen',
    [switch]$SkipFixture,
    [switch]$NoServe
)

$ErrorActionPreference = 'Stop'
$probeRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$mereRoot = (Resolve-Path (Join-Path $probeRoot '..\..')).Path
$manifest = Join-Path $probeRoot 'Cargo.toml'
$munimentRoot = Join-Path $mereRoot 'crates\eidetic\muniment'
$env:CARGO_TARGET_DIR = $TargetDir
$env:MUNIMENT_OPFS_PROBE_COMMIT = (git -C $mereRoot rev-parse HEAD).Trim()
$ownedStatus = git -C $mereRoot status --porcelain -- ports/muniment-opfs-probe crates/eidetic/muniment
$env:MUNIMENT_OPFS_PROBE_DIRTY = if ($ownedStatus) { 'true' } else { 'false' }

New-Item -ItemType Directory -Force -Path $TargetDir, $NeutralDir | Out-Null

# ── provenance ───────────────────────────────────────────────────────────────
# A receipt has to name the whole build, and the hash has to be taken over
# inputs that are FROZEN by the time it is taken. Two ordering bugs in the
# first version, both fixed here:
#
#   1. The fixture manifest was hashed BEFORE the fixture was regenerated
#      later in the same run, so the recorded hash described a file the build
#      then replaced.
#   2. Prose files (README.md, .gitignore) were included, so editing docs
#      invalidated the hash of an unchanged executable build.
#
# So: generate fixtures first, hash only behavioural inputs, then build.
function Get-TreeHash {
    param([string[]]$Files, [string]$Root)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $buffer = New-Object System.IO.MemoryStream
    foreach ($file in ($Files | Sort-Object)) {
        $relative = $file.Substring($Root.Length).TrimStart('\', '/').Replace('\', '/')
        $bytes = [System.IO.File]::ReadAllBytes($file)
        # Normalize CRLF so a checkout's line endings do not change the hash.
        $text = [System.Text.Encoding]::UTF8.GetString($bytes).Replace("`r`n", "`n")
        $normalized = [System.Text.Encoding]::UTF8.GetBytes($text)
        $name = [System.Text.Encoding]::UTF8.GetBytes("$relative`n$($normalized.Length)`n")
        $buffer.Write($name, 0, $name.Length)
        $buffer.Write($normalized, 0, $normalized.Length)
    }
    $buffer.Position = 0
    ($sha.ComputeHash($buffer) | ForEach-Object { $_.ToString('x2') }) -join ''
}

# Fixtures FIRST, so the manifest that gets hashed is the one that will be
# served. Built with the pre-change binary, which is fine: the fixture is data
# the browser reads, and the hash below covers it once it is final.
Push-Location $NeutralDir
try {
    if (-not $SkipFixture) {
        cargo run --locked --manifest-path $manifest --release --bin fixture -- write (Join-Path $probeRoot 'fixtures')
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
} finally {
    Pop-Location
}

# Behavioural inputs only. Prose (README, .gitignore) and outputs (receipts,
# generated wasm, .redb fixtures) are excluded: editing docs must not
# invalidate the hash of an unchanged executable build, and a `.redb` is
# reproducible from its manifest.
$excluded = '\\web\\pkg\\|\\receipts\\|\\target\\|\\node_modules\\'
$probeFiles = Get-ChildItem -Path $probeRoot -Recurse -File |
    Where-Object {
        $_.FullName -notmatch $excluded -and
        $_.Extension -notin @('.redb', '.md') -and
        $_.Name -ne '.gitignore'
    } | ForEach-Object { $_.FullName }
$env:MUNIMENT_OPFS_PROBE_SOURCE_SHA256 = Get-TreeHash -Files $probeFiles -Root $probeRoot

# The compiled-in path dependency. Its version in the lockfile is just "0.1.1";
# only its source identifies what was actually built.
$munimentFiles = Get-ChildItem -Path (Join-Path $munimentRoot 'src') -Recurse -File |
    ForEach-Object { $_.FullName }
$munimentFiles += (Join-Path $munimentRoot 'Cargo.toml')
$env:MUNIMENT_OPFS_PROBE_MUNIMENT_SHA256 = Get-TreeHash -Files $munimentFiles -Root $munimentRoot

# The real lockfile, hashed whole: sources and checksums included, not just
# names and versions. `--locked` below proves it is the one that was used.
$lockPath = Join-Path $probeRoot 'Cargo.lock'
$env:MUNIMENT_OPFS_PROBE_LOCK_SHA256 = if (Test-Path $lockPath) {
    Get-TreeHash -Files @($lockPath) -Root $probeRoot
} else { 'absent' }
$env:MUNIMENT_OPFS_PROBE_BUILT_AT = (Get-Date).ToUniversalTime().ToString('o')

Write-Host "probe source  sha256: $env:MUNIMENT_OPFS_PROBE_SOURCE_SHA256"
Write-Host "muniment src  sha256: $env:MUNIMENT_OPFS_PROBE_MUNIMENT_SHA256"
Write-Host "Cargo.lock    sha256: $env:MUNIMENT_OPFS_PROBE_LOCK_SHA256"

$bindgenVersion = (& $WasmBindgen --version).Trim()
if ($bindgenVersion -ne 'wasm-bindgen 0.2.126') {
    throw "The probe pins wasm-bindgen 0.2.126; got '$bindgenVersion'. Pass -WasmBindgen with the matching executable."
}

# Build from the neutral directory so `--locked` holds.
Push-Location $NeutralDir
try {
    cargo fmt --manifest-path $manifest --check
    if ($LASTEXITCODE -ne 0) { throw 'cargo fmt --check failed; run cargo fmt.' }
    cargo build --locked --manifest-path $manifest --lib --release --target wasm32-unknown-unknown
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    cargo test --locked --manifest-path $manifest --release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

$wasm = Join-Path $TargetDir 'wasm32-unknown-unknown\release\muniment_opfs_probe.wasm'
& $WasmBindgen --target web --out-dir (Join-Path $probeRoot 'web\pkg') $wasm
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$url = "http://localhost:$Port/ports/muniment-opfs-probe/web/"
Write-Host "muniment OPFS probe: $url"
if ($NoServe) { exit 0 }

$sink = Start-Process -PassThru -FilePath 'python' `
    -ArgumentList @((Join-Path $probeRoot 'receipt-sink.py'), $SinkPort)
try {
    Write-Host "receipt sink on $SinkPort (pid $($sink.Id)). Press Ctrl+C to stop both."
    python -m http.server $Port --directory $mereRoot
} finally {
    if (-not $sink.HasExited) { Stop-Process -Id $sink.Id -Force }
}
