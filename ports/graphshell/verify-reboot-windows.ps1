<#
.SYNOPSIS
Prove the resident Graphshell host recovers across a real reboot.

.DESCRIPTION
"Task Scheduler says Running" is weaker than a post-reboot receipt: a process
that has been up since before the task was installed proves nothing about what
happens at logon. This script captures a baseline before the reboot and
compares against it after, asserting that the host was started BY the reboot
rather than having survived it.

The node id check is the load-bearing one. Pairing records a peer's node id and
expects it to keep working, so if a reboot changed it, every paired device
would silently stop resolving this machine.

.EXAMPLE
  # before restarting
  .\verify-reboot-windows.ps1 -Capture

  # after logging back in
  .\verify-reboot-windows.ps1 -Verify
#>
[CmdletBinding(DefaultParameterSetName = "Verify")]
param(
    [Parameter(ParameterSetName = "Capture", Mandatory = $true)]
    [switch]$Capture,
    [Parameter(ParameterSetName = "Verify")]
    [switch]$Verify,
    [string]$BaselinePath = (Join-Path $env:LOCALAPPDATA "Graphshell\reboot-baseline.json"),
    [string]$LogPath = (Join-Path $env:LOCALAPPDATA "Graphshell\device-host.log"),
    [string]$InstallRoot = (Join-Path $env:LOCALAPPDATA "Graphshell\bin"),
    [string]$TaskName = "graphshell-device-host",
    [string]$SshProbeTarget
)

$ErrorActionPreference = "Stop"

function Get-LastBoot {
    (Get-CimInstance Win32_OperatingSystem).LastBootUpTime
}

function Get-HostProcess {
    $expected = [System.IO.Path]::GetFullPath((Join-Path $InstallRoot "graphshell-device-host.exe"))
    Get-CimInstance Win32_Process |
        Where-Object {
            $_.ExecutablePath -and
            [System.IO.Path]::GetFullPath($_.ExecutablePath).Equals(
                $expected, [System.StringComparison]::OrdinalIgnoreCase)
        } |
        Select-Object -First 1
}

# The most recent bring-up line, parsed. Returns $null when sync never started.
function Get-SyncFacts {
    if (-not (Test-Path -LiteralPath $LogPath)) { return $null }
    $line = Get-Content -LiteralPath $LogPath |
        Select-String -Pattern "personal graph sync listening" |
        Select-Object -Last 1
    if (-not $line) { return $null }
    # Read every capture out of $Matches immediately: the next -match in this
    # scope overwrites it, which is exactly how the first version of this
    # script recorded a baseline full of nulls.
    if (-not ($line.Line -match 'graph=(?<graph>[0-9a-f]{64}).*node_id=(?<node>[0-9a-f]{64})')) {
        return $null
    }
    $graph = $Matches.graph
    $node = $Matches.node
    $stamp = $null
    if ($line.Line -match '^(?<ts>\S+)\s') { $stamp = [datetimeoffset]::Parse($Matches.ts) }
    [pscustomobject]@{
        graph   = $graph
        node_id = $node
        at      = $stamp
    }
}

function Get-StoreFile([string]$Graph) {
    $path = Join-Path $env:LOCALAPPDATA "Graphshell\data\personal-sync\$Graph.redb"
    if (Test-Path -LiteralPath $path) { Get-Item -LiteralPath $path } else { $null }
}

function Get-AgentFingerprints {
    $listed = (& ssh-add -l 2>&1) -join "`n"
    if ($LASTEXITCODE -ne 0) { return @() }
    @($listed -split "`n" |
        ForEach-Object { if ($_ -match '(SHA256:\S+)') { $matches[1] } } |
        Where-Object { $_ })
}

if ($Capture) {
    $sync = Get-SyncFacts
    if (-not $sync) {
        throw "personal sync has not started; capture a baseline only from a healthy host"
    }
    $store = Get-StoreFile $sync.graph
    $baseline = [ordered]@{
        captured_at   = (Get-Date).ToString("o")
        boot_before   = (Get-LastBoot).ToString("o")
        graph         = $sync.graph
        node_id       = $sync.node_id
        fingerprints  = Get-AgentFingerprints
        store_path    = if ($store) { $store.FullName } else { $null }
        store_written = if ($store) { $store.LastWriteTime.ToString("o") } else { $null }
        host_pid      = (Get-HostProcess).ProcessId
    }
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $BaselinePath) | Out-Null
    $baseline | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $BaselinePath -Encoding utf8
    Write-Output "baseline written to $BaselinePath"
    $baseline | ConvertTo-Json -Depth 4
    return
}

if (-not (Test-Path -LiteralPath $BaselinePath)) {
    throw "no baseline at $BaselinePath; run with -Capture before rebooting"
}
$baseline = Get-Content -LiteralPath $BaselinePath -Raw | ConvertFrom-Json
$failures = [System.Collections.Generic.List[string]]::new()
$boot = Get-LastBoot

if ($boot -le [datetimeoffset]::Parse($baseline.boot_before)) {
    throw "this machine has not rebooted since the baseline was captured; the proof requires a real restart"
}

# 1. The task brought the host up, and the process is younger than the boot.
$task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if (-not $task) { $failures.Add("scheduled task $TaskName is missing") }
elseif ($task.State -ne "Running") { $failures.Add("scheduled task state is $($task.State), expected Running") }

$process = Get-HostProcess
if (-not $process) {
    $failures.Add("no device-host process is running after the reboot")
} else {
    $started = [Management.ManagementDateTimeConverter]::ToDateTime($process.CreationDate)
    if ($started -lt $boot) {
        $failures.Add("the device host predates the boot; it did not start from the logon trigger")
    }
}

# 2. The SSH agent serves the same keys it did before.
$fingerprints = Get-AgentFingerprints
foreach ($expected in @($baseline.fingerprints)) {
    if ($fingerprints -notcontains $expected) {
        $failures.Add("the agent no longer discloses $expected")
    }
}

# 3. Personal sync came back, on the same graph, with the SAME node id.
$sync = Get-SyncFacts
if (-not $sync) {
    $failures.Add("personal sync did not start after the reboot")
} else {
    if ($sync.at -and $sync.at -lt $boot) {
        $failures.Add("the newest sync bring-up line predates the boot; sync did not restart")
    }
    if ($sync.graph -ne $baseline.graph) {
        $failures.Add("graph changed: $($baseline.graph) -> $($sync.graph)")
    }
    if ($sync.node_id -ne $baseline.node_id) {
        $failures.Add(
            "NODE ID CHANGED across the reboot: $($baseline.node_id) -> $($sync.node_id). " +
            "Every paired device stores this and would stop resolving this machine.")
    }
}

# 4. The store was reopened, not recreated.
if ($baseline.store_path) {
    if (-not (Test-Path -LiteralPath $baseline.store_path)) {
        $failures.Add("the personal-graph store is gone: $($baseline.store_path)")
    } else {
        $store = Get-Item -LiteralPath $baseline.store_path
        if ($store.CreationTime -gt $boot) {
            $failures.Add("the store was recreated after the boot rather than reopened")
        }
    }
}

# 5. Optional: a real SSH login through the restored agent.
$sshProbe = $null
if ($SshProbeTarget) {
    & ssh -o BatchMode=yes -o PasswordAuthentication=no -o StrictHostKeyChecking=accept-new `
        $SshProbeTarget "echo graphshell-reboot-proof" | Out-Null
    if ($LASTEXITCODE -ne 0) { $failures.Add("SSH login to $SshProbeTarget failed after the reboot") }
    else { $sshProbe = $SshProbeTarget }
}

[ordered]@{
    verified_at     = (Get-Date).ToString("o")
    boot_before     = $baseline.boot_before
    boot_after      = $boot.ToString("o")
    host_pid_before = $baseline.host_pid
    host_pid_after  = if ($process) { $process.ProcessId } else { $null }
    graph           = if ($sync) { $sync.graph } else { $null }
    node_id_stable  = [bool]($sync -and $sync.node_id -eq $baseline.node_id)
    fingerprints    = $fingerprints
    store_reopened  = [bool]($baseline.store_path -and (Test-Path -LiteralPath $baseline.store_path))
    ssh_probe       = $sshProbe
    failures        = @($failures)
    passed          = ($failures.Count -eq 0)
} | ConvertTo-Json -Depth 4

if ($failures.Count -gt 0) { exit 1 }
