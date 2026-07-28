[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$DeviceHostBinary,
    [Parameter(Mandatory = $true)]
    [string]$NativeHostBinary,
    [Parameter(Mandatory = $true)]
    [string]$ExpectedFingerprint,
    [string]$InstallRoot = (Join-Path $env:LOCALAPPDATA "Graphshell\bin"),
    [string]$TaskName = "graphshell-device-host",
    [string]$PreviousTaskName = "personae-agent",
    [string]$SshProbeTarget,
    [switch]$RetirePreviousTask,
    [switch]$ConfirmLogonProof
)

$ErrorActionPreference = "Stop"

if ($RetirePreviousTask -and -not $ConfirmLogonProof) {
    throw "-RetirePreviousTask requires -ConfirmLogonProof after a real sign-out or reboot receipt."
}
if ($RetirePreviousTask -and -not $SshProbeTarget) {
    throw "-RetirePreviousTask requires -SshProbeTarget for the real login receipt."
}

function Resolve-Binary([string]$Path, [string]$Name) {
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    if (-not [System.IO.Path]::IsPathFullyQualified($resolved)) {
        throw "$Name path must be absolute."
    }
    return $resolved
}

function Stop-InstalledProcess([string]$ExecutablePath) {
    $expected = [System.IO.Path]::GetFullPath($ExecutablePath)
    Get-CimInstance Win32_Process |
        Where-Object {
            $_.ExecutablePath -and
            [System.IO.Path]::GetFullPath($_.ExecutablePath).Equals(
                $expected,
                [System.StringComparison]::OrdinalIgnoreCase
            )
        } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction Stop }
}

function Wait-Agent([string]$Fingerprint, [int]$Attempts = 40) {
    for ($attempt = 0; $attempt -lt $Attempts; $attempt++) {
        $listed = (& ssh-add -l 2>&1) -join "`n"
        if ($LASTEXITCODE -eq 0 -and $listed.Contains($Fingerprint)) {
            return $listed
        }
        Start-Sleep -Milliseconds 500
    }
    throw "the standard agent did not disclose $Fingerprint"
}

function Find-DeviceHost([string]$ExecutablePath, [int]$ExceptPid = 0) {
    $expected = [System.IO.Path]::GetFullPath($ExecutablePath)
    return Get-CimInstance Win32_Process |
        Where-Object {
            $_.ProcessId -ne $ExceptPid -and
            $_.ExecutablePath -and
            [System.IO.Path]::GetFullPath($_.ExecutablePath).Equals(
                $expected,
                [System.StringComparison]::OrdinalIgnoreCase
            )
        } |
        Select-Object -First 1
}

$deviceSource = Resolve-Binary $DeviceHostBinary "device host"
$nativeSource = Resolve-Binary $NativeHostBinary "native relay"
New-Item -ItemType Directory -Path $InstallRoot -Force | Out-Null
$installedDevice = Join-Path $InstallRoot "graphshell-device-host.exe"
$installedNative = Join-Path $InstallRoot "graphshell-native-host.exe"
$launcher = Join-Path $InstallRoot "graphshell-device-host.vbs"
$logFile = Join-Path (Split-Path -Parent $InstallRoot) "device-host.log"

$previousTask = Get-ScheduledTask -TaskName $PreviousTaskName -ErrorAction SilentlyContinue
$previousWasRunning = $previousTask -and $previousTask.State -eq "Running"
$newTask = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if ($newTask) {
    Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
}
if (Test-Path -LiteralPath $installedDevice) {
    Stop-InstalledProcess $installedDevice
}

Copy-Item -LiteralPath $deviceSource -Destination $installedDevice -Force
Copy-Item -LiteralPath $nativeSource -Destination $installedNative -Force

$vbs = @"
' Graphshell resident host recovery loop. Task Scheduler owns this launcher;
' the launcher restarts the device host after a crash.
Set shell = CreateObject("WScript.Shell")
Do
  shell.Run """$installedDevice"" --log-file ""$logFile""", 0, True
  WScript.Sleep 5000
Loop
"@
Set-Content -LiteralPath $launcher -Value $vbs -Encoding ASCII

$action = New-ScheduledTaskAction -Execute "wscript.exe" -Argument "`"$launcher`""
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$settings = New-ScheduledTaskSettingsSet `
    -ExecutionTimeLimit (New-TimeSpan -Seconds 0) `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries
$principal = New-ScheduledTaskPrincipal `
    -UserId $env:USERNAME `
    -LogonType Interactive `
    -RunLevel Limited
Register-ScheduledTask `
    -TaskName $TaskName `
    -Action $action `
    -Trigger $trigger `
    -Settings $settings `
    -Principal $principal `
    -Description "Graphshell resident Personae authority and OpenSSH agent" `
    -Force | Out-Null

try {
    if ($previousTask) {
        Stop-ScheduledTask -TaskName $PreviousTaskName -ErrorAction SilentlyContinue
    }
    $personaeExecutable = Join-Path $env:LOCALAPPDATA "personae\bin\personae-agent.exe"
    if (Test-Path -LiteralPath $personaeExecutable) {
        Stop-InstalledProcess $personaeExecutable
    }

    Start-ScheduledTask -TaskName $TaskName
    $before = Wait-Agent $ExpectedFingerprint
    $first = Find-DeviceHost $installedDevice
    if (-not $first) {
        throw "the scheduled task did not launch Graphshell's device host"
    }

    Stop-Process -Id $first.ProcessId -Force -ErrorAction Stop
    $after = Wait-Agent $ExpectedFingerprint
    $replacement = $null
    for ($attempt = 0; $attempt -lt 40 -and -not $replacement; $attempt++) {
        $replacement = Find-DeviceHost $installedDevice $first.ProcessId
        if (-not $replacement) {
            Start-Sleep -Milliseconds 500
        }
    }
    if (-not $replacement) {
        throw "the recovery launcher did not replace the killed device host"
    }

    if ($SshProbeTarget) {
        & ssh `
            -o BatchMode=yes `
            -o PasswordAuthentication=no `
            -o KbdInteractiveAuthentication=no `
            -o PreferredAuthentications=publickey `
            -o ConnectTimeout=5 `
            $SshProbeTarget `
            "printf graphshell-h4f"
        if ($LASTEXITCODE -ne 0) {
            throw "the real SSH login probe failed"
        }
    }

    if ($RetirePreviousTask -and $previousTask) {
        Unregister-ScheduledTask -TaskName $PreviousTaskName -Confirm:$false
    }

    [pscustomobject]@{
        schema = "graphshell.h4f.windows-cutover/v1"
        task = $TaskName
        first_pid = $first.ProcessId
        replacement_pid = $replacement.ProcessId
        fingerprint = $ExpectedFingerprint
        listed_before_restart = $before
        listed_after_restart = $after
        ssh_probe = if ($SshProbeTarget) { $SshProbeTarget } else { $null }
        previous_task_retired = [bool]$RetirePreviousTask
        native_relay = $installedNative
    }
} catch {
    Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    Stop-InstalledProcess $installedDevice
    if ($previousTask -and $previousWasRunning) {
        Start-ScheduledTask -TaskName $PreviousTaskName
    }
    throw
}
