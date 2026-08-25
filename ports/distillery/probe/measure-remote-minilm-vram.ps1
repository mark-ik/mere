param(
    [string]$ModelDir = (Join-Path $PSScriptRoot '..\..\..\models\all-MiniLM-L6-v2'),
    [string]$Text = 'query: Mere keeps a model local.',
    [int]$CancellationBatch = 512,
    [string]$TargetDir = 'C:\t\distillery-remote-minilm-vram',
    [int]$StageHoldMs = 3000,
    [int]$ProcessTimeoutSeconds = 240,
    [int]$ActiveMinimumDeltaMiB = 64,
    [int]$ReclaimToleranceMiB = 32
)

$ErrorActionPreference = 'Stop'
$probeRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$mereRoot = (Resolve-Path (Join-Path $probeRoot '..\..\..')).Path
$fixture = Join-Path $probeRoot 'remote-fixture\Cargo.toml'
$resolvedModel = (Resolve-Path -LiteralPath $ModelDir).Path
$artifactDir = Join-Path $TargetDir 'driver-vram-artifacts'
$executable = Join-Path $TargetDir 'release\distillery-remote-minilm-fixture.exe'
$stdoutPath = Join-Path $artifactDir 'remote-plain.stdout.json'
$stderrPath = Join-Path $artifactDir 'remote-plain.stderr.txt'
$receiptPath = Join-Path $artifactDir 'remote-plain.driver-vram.json'
$sampleStages = @(
    'allocator-baseline',
    'first-remote-executed',
    'first-reclaim-clean',
    'recovery-remote-executed',
    'recovery-reclaim-clean',
    'shutdown-complete'
)

function Invoke-TypeperfSnapshot {
    param(
        [int]$ProcessId,
        [string]$Stage
    )

    $counterPath = "\GPU Process Memory(pid_${ProcessId}*)\Dedicated Usage"
    $raw = @(& typeperf.exe $counterPath -sc 1 2>&1 | ForEach-Object { "$_" })
    $csvLines = @($raw | Where-Object { $_.StartsWith('"') })
    if ($csvLines.Count -lt 2) {
        return [pscustomobject]@{
            stage = $Stage
            captured_utc = [DateTime]::UtcNow.ToString('O')
            counter_available = $false
            dedicated_bytes = $null
            counters = @()
            nvidia_gpu_0_visible = $false
            nvidia_pmon_row = $null
            nvidia_gpu_summary = $null
            typeperf_output = ($raw -join "`n")
        }
    }

    $row = @(($csvLines -join [Environment]::NewLine) | ConvertFrom-Csv)[-1]
    $counterValues = @()
    $dedicatedBytes = 0.0
    foreach ($property in @($row.PSObject.Properties | Select-Object -Skip 1)) {
        $bytes = [double]::Parse(
            "$($property.Value)",
            [Globalization.CultureInfo]::InvariantCulture
        )
        $dedicatedBytes += $bytes
        $counterValues += [pscustomobject]@{
            path = $property.Name
            bytes = [uint64]$bytes
        }
    }

    $pmon = @(& nvidia-smi.exe pmon -c 1 2>&1 | ForEach-Object { "$_" })
    $pmonRow = @($pmon | Where-Object { $_ -match "^\s*0\s+$ProcessId\s+" }) | Select-Object -First 1
    $gpuSummary = @(& nvidia-smi.exe `
        --query-gpu=index,name,driver_version,memory.total,memory.used,memory.free `
        --format=csv,noheader,nounits 2>&1 | ForEach-Object { "$_" })

    [pscustomobject]@{
        stage = $Stage
        captured_utc = [DateTime]::UtcNow.ToString('O')
        counter_available = $true
        dedicated_bytes = [uint64]$dedicatedBytes
        counters = $counterValues
        nvidia_gpu_0_visible = $null -ne $pmonRow
        nvidia_pmon_row = $pmonRow
        nvidia_gpu_summary = ($gpuSummary -join "`n")
        typeperf_output = $null
    }
}

function Find-StageSample {
    param(
        [object[]]$Samples,
        [string]$Stage
    )
    @($Samples | Where-Object stage -eq $Stage) | Select-Object -First 1
}

foreach ($command in @('cargo', 'typeperf.exe', 'nvidia-smi.exe')) {
    if (-not (Get-Command $command -ErrorAction SilentlyContinue)) {
        throw "required command is unavailable: $command"
    }
}
New-Item -ItemType Directory -Force -Path $artifactDir | Out-Null

$sourceCommit = (git -C $mereRoot rev-parse HEAD).Trim()
$ownedStatus = git -C $mereRoot status --porcelain -- `
    ports/distillery `
    crates/intel/esp `
    crates/mesh/mesh `
    crates/mesh/host `
    crates/murm/transport `
    support/patches/burn-remote
$ownedPathsDirty = [bool]$ownedStatus

$previousTargetDir = $env:CARGO_TARGET_DIR
Push-Location ([System.IO.Path]::GetTempPath())
try {
    $env:CARGO_TARGET_DIR = $TargetDir
    cargo build --release --locked --manifest-path $fixture | Out-Host
    if ($LASTEXITCODE -ne 0) { throw 'plain fixture build failed' }
} finally {
    Pop-Location
    $env:CARGO_TARGET_DIR = $previousTargetDir
}

$start = [System.Diagnostics.ProcessStartInfo]::new()
$start.FileName = $executable
$start.UseShellExecute = $false
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$start.CreateNoWindow = $true
$start.ArgumentList.Add('remote')
$start.ArgumentList.Add($resolvedModel)
$start.ArgumentList.Add($Text)
$start.ArgumentList.Add($CancellationBatch.ToString())
$start.Environment['DISTILLERY_REMOTE_PROBE_COMMIT'] = $sourceCommit
$start.Environment['DISTILLERY_REMOTE_PROBE_DIRTY'] = $ownedPathsDirty.ToString().ToLowerInvariant()
$start.Environment['DISTILLERY_REMOTE_STAGE_HOLD_MS'] = $StageHoldMs.ToString()

$process = [System.Diagnostics.Process]::new()
$process.StartInfo = $start
if (-not $process.Start()) { throw 'fixture process did not start' }
$fixtureProcessId = $process.Id
$stdoutTask = $process.StandardOutput.ReadToEndAsync()
$stderrTask = $process.StandardError.ReadLineAsync()
$stderrLines = [System.Collections.Generic.List[string]]::new()
$samples = [System.Collections.Generic.List[object]]::new()
$started = [DateTime]::UtcNow
$timedOut = $false

while (-not $process.HasExited -or $null -ne $stderrTask) {
    if ($null -ne $stderrTask -and $stderrTask.IsCompleted) {
        $line = $stderrTask.GetAwaiter().GetResult()
        if ($null -eq $line) {
            $stderrTask = $null
        } else {
            $stderrLines.Add($line)
            if ($line -match '^distillery-remote-minilm stage: (.+)$') {
                $stage = $Matches[1]
                if ($sampleStages -contains $stage) {
                    $samples.Add((Invoke-TypeperfSnapshot -ProcessId $fixtureProcessId -Stage $stage))
                }
            }
            $stderrTask = $process.StandardError.ReadLineAsync()
        }
    } else {
        Start-Sleep -Milliseconds 20
    }

    if (-not $process.HasExited -and
        ([DateTime]::UtcNow - $started).TotalSeconds -gt $ProcessTimeoutSeconds) {
        $timedOut = $true
        $process.Kill($true)
        $process.WaitForExit()
    }
}

$process.WaitForExit()
$stdout = $stdoutTask.GetAwaiter().GetResult()
$stderr = $stderrLines -join "`n"
[System.IO.File]::WriteAllText($stdoutPath, $stdout)
[System.IO.File]::WriteAllText($stderrPath, $stderr)
$exitCode = if ($timedOut) { $null } else { $process.ExitCode }
$fixtureReceipt = if (-not [string]::IsNullOrWhiteSpace($stdout)) {
    $stdout | ConvertFrom-Json
} else {
    $null
}

$afterExit = $null
for ($attempt = 0; $attempt -lt 5; $attempt++) {
    $afterExit = Invoke-TypeperfSnapshot -ProcessId $fixtureProcessId -Stage 'after-process-exit'
    if (-not $afterExit.counter_available) { break }
    Start-Sleep -Milliseconds 250
}
$baseline = Find-StageSample -Samples $samples -Stage 'allocator-baseline'
$firstActive = Find-StageSample -Samples $samples -Stage 'first-remote-executed'
$firstReclaimed = Find-StageSample -Samples $samples -Stage 'first-reclaim-clean'
$recoveryActive = Find-StageSample -Samples $samples -Stage 'recovery-remote-executed'
$recoveryReclaimed = Find-StageSample -Samples $samples -Stage 'recovery-reclaim-clean'
$activeMinimumDelta = [uint64]$ActiveMinimumDeltaMiB * 1MB
$reclaimTolerance = [uint64]$ReclaimToleranceMiB * 1MB

$requiredSamples = @($baseline, $firstActive, $firstReclaimed, $recoveryActive, $recoveryReclaimed)
$samplesComplete = @($requiredSamples | Where-Object { $null -eq $_ -or -not $_.counter_available }).Count -eq 0
$firstActiveDelta = if ($samplesComplete) {
    [int64]$firstActive.dedicated_bytes - [int64]$baseline.dedicated_bytes
} else { $null }
$firstRetainedDelta = if ($samplesComplete) {
    [Math]::Max(0, [int64]$firstReclaimed.dedicated_bytes - [int64]$baseline.dedicated_bytes)
} else { $null }
$recoveryActiveDelta = if ($samplesComplete) {
    [int64]$recoveryActive.dedicated_bytes - [int64]$baseline.dedicated_bytes
} else { $null }
$recoveryRetainedDelta = if ($samplesComplete) {
    [Math]::Max(0, [int64]$recoveryReclaimed.dedicated_bytes - [int64]$baseline.dedicated_bytes)
} else { $null }

$passes = -not $timedOut -and $exitCode -eq 0 -and $samplesComplete -and `
    $firstActiveDelta -ge $activeMinimumDelta -and `
    $recoveryActiveDelta -ge $activeMinimumDelta -and `
    $firstRetainedDelta -le $reclaimTolerance -and `
    $recoveryRetainedDelta -le $reclaimTolerance -and `
    $firstActive.nvidia_gpu_0_visible -and $recoveryActive.nvidia_gpu_0_visible -and `
    -not $afterExit.counter_available

$receipt = [pscustomobject]@{
    schema = 'distillery.remote-minilm-driver-vram/v1'
    source = [pscustomobject]@{
        commit = $sourceCommit
        owned_paths_dirty = $ownedPathsDirty
    }
    fixture = [pscustomobject]@{
        process_id = $fixtureProcessId
        timed_out = $timedOut
        exit_code = $exitCode
        stdout = $stdoutPath
        stderr = $stderrPath
        receipt = $fixtureReceipt
    }
    method = [pscustomobject]@{
        counter = 'Windows GPU Process Memory Dedicated Usage, summed across the fixture PID instances'
        adapter_attribution = 'nvidia-smi pmon observes the same PID on NVIDIA GPU 0 at active stages'
        board_memory = 'nvidia-smi total memory is contextual only and is not a gate'
        stage_hold_ms = $StageHoldMs
        active_minimum_delta_mib = $ActiveMinimumDeltaMiB
        reclaim_tolerance_mib = $ReclaimToleranceMiB
    }
    samples = $samples
    after_process_exit = $afterExit
    deltas = [pscustomobject]@{
        first_active_bytes = $firstActiveDelta
        first_retained_after_reclaim_bytes = $firstRetainedDelta
        recovery_active_bytes = $recoveryActiveDelta
        recovery_retained_after_reclaim_bytes = $recoveryRetainedDelta
    }
    driver_vram_claimed = $passes
    passes = $passes
}

$json = $receipt | ConvertTo-Json -Depth 12
[System.IO.File]::WriteAllText($receiptPath, $json)
$json
if (-not $passes) { exit 1 }
