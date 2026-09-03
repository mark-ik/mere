# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [Alias("DeviceHostBinary")]
    [string]$DjinnBinary,
    [Parameter(Mandatory = $true)]
    [string]$NativeHostBinary,
    [Parameter(Mandatory = $true)]
    [string]$ExpectedFingerprint,
    [string]$InstallRoot = (Join-Path $env:LOCALAPPDATA "Djinn\bin"),
    [string]$TaskName = "djinn-resident",
    [string]$LegacyTaskName = "graphshell-device-host",
    [string]$PreviousTaskName = "personae-agent",
    [string]$DataRoot,
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

$deviceSource = Resolve-Binary $DjinnBinary "Djinn resident"
$nativeSource = Resolve-Binary $NativeHostBinary "native relay"
New-Item -ItemType Directory -Path $InstallRoot -Force | Out-Null
$installedDevice = Join-Path $InstallRoot "djinn.exe"
$installedNative = Join-Path $InstallRoot "graphshell-native-host.exe"
$launcher = Join-Path $InstallRoot "djinn.vbs"
$logFile = Join-Path (Split-Path -Parent $InstallRoot) "djinn.log"

$previousTask = Get-ScheduledTask -TaskName $PreviousTaskName -ErrorAction SilentlyContinue
$previousWasRunning = $previousTask -and $previousTask.State -eq "Running"
$previousWasEnabled = $previousTask -and $previousTask.State -ne "Disabled"
$legacyTask = if ($LegacyTaskName -ne $TaskName) {
    Get-ScheduledTask -TaskName $LegacyTaskName -ErrorAction SilentlyContinue
}
$legacyWasRunning = $legacyTask -and $legacyTask.State -eq "Running"
$legacyWasEnabled = $legacyTask -and $legacyTask.State -ne "Disabled"
$newTask = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
$newTaskWasRunning = $newTask -and $newTask.State -eq "Running"
if ($newTask) {
    Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
}
if (Test-Path -LiteralPath $installedDevice) {
    Stop-InstalledProcess $installedDevice
}

Copy-Item -LiteralPath $deviceSource -Destination $installedDevice -Force
Copy-Item -LiteralPath $nativeSource -Destination $installedNative -Force

$deviceArguments = "--log-file ""$logFile"""
$resolvedDataRoot = $null
if ($DataRoot) {
    $resolvedDataRoot = [System.IO.Path]::GetFullPath($DataRoot)
    $deviceArguments += " --data-root ""$resolvedDataRoot"""
}
$deviceCommand = """$installedDevice"" $deviceArguments"
$escapedDeviceCommand = $deviceCommand.Replace("""", """""")
$vbs = @"
' Djinn resident recovery loop. Task Scheduler owns this launcher; Djinn
' restarts after a crash.
'
' Personal sync is not configured here. Which graph, which lanes and which
' paired devices are owner settings, stored per Personae profile under
' %LOCALAPPDATA%\Graphshell\settings. Djinn retains this location while it
' migrates existing profiles. Editing this launcher is not how you
' change what the device synchronises.
'
' Arguments still override the settings file for a one-off run. Peer tickets
' stay arguments only: a ticket goes stale as soon as that peer rebinds, so
' pairing records a node id instead.
Set shell = CreateObject("WScript.Shell")
Do
  shell.Run "$escapedDeviceCommand", 0, True
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
    -Description "Djinn resident Personae authority and OpenSSH agent" `
    -Force | Out-Null

try {
    if ($previousTask) {
        Disable-ScheduledTask -TaskName $PreviousTaskName | Out-Null
        Stop-ScheduledTask -TaskName $PreviousTaskName -ErrorAction SilentlyContinue
    }
    if ($legacyTask) {
        Disable-ScheduledTask -TaskName $LegacyTaskName | Out-Null
        Stop-ScheduledTask -TaskName $LegacyTaskName -ErrorAction SilentlyContinue
    }
    $personaeExecutable = Join-Path $env:LOCALAPPDATA "personae\bin\personae-agent.exe"
    if (Test-Path -LiteralPath $personaeExecutable) {
        Stop-InstalledProcess $personaeExecutable
    }

    Start-ScheduledTask -TaskName $TaskName
    $before = Wait-Agent $ExpectedFingerprint
    $first = Find-DeviceHost $installedDevice
    if (-not $first) {
        throw "the scheduled task did not launch Djinn"
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
        throw "the recovery launcher did not replace Djinn"
    }

    if ($SshProbeTarget) {
        & ssh `
            -o BatchMode=yes `
            -o PasswordAuthentication=no `
            -o KbdInteractiveAuthentication=no `
            -o PreferredAuthentications=publickey `
            -o ConnectTimeout=5 `
            $SshProbeTarget `
            "printf djinn-resident"
        if ($LASTEXITCODE -ne 0) {
            throw "the real SSH login probe failed"
        }
    }

    if ($RetirePreviousTask -and $previousTask) {
        Unregister-ScheduledTask -TaskName $PreviousTaskName -Confirm:$false
    }

    [pscustomobject]@{
        schema = "mere.djinn.windows-cutover/v1"
        task = $TaskName
        first_pid = $first.ProcessId
        replacement_pid = $replacement.ProcessId
        fingerprint = $ExpectedFingerprint
        listed_before_restart = $before
        listed_after_restart = $after
        ssh_probe = if ($SshProbeTarget) { $SshProbeTarget } else { $null }
        data_root = $resolvedDataRoot
        previous_task_disabled = [bool]($previousTask -and -not $RetirePreviousTask)
        previous_task_retired = [bool]$RetirePreviousTask
        legacy_task_disabled = [bool]$legacyTask
        native_relay = $installedNative
    }
} catch {
    Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    Stop-InstalledProcess $installedDevice
    if ($newTask) {
        Disable-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue | Out-Null
    } else {
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction SilentlyContinue
    }
    if ($previousTask -and ($previousWasEnabled -or $newTaskWasRunning)) {
        Enable-ScheduledTask -TaskName $PreviousTaskName | Out-Null
        if ($previousWasRunning -or $newTaskWasRunning) {
            Start-ScheduledTask -TaskName $PreviousTaskName
        }
    }
    if ($legacyTask -and $legacyWasEnabled) {
        Enable-ScheduledTask -TaskName $LegacyTaskName | Out-Null
        if ($legacyWasRunning) {
            Start-ScheduledTask -TaskName $LegacyTaskName
        }
    }
    throw
}
