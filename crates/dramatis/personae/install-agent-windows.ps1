# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

# Build and (re)install the personae SSH agent + vault CLI on Windows.
#
# Interim scaffolding per the 2026-07-22 identity-vault-ssh-agent plan: the
# agent's durable home is the mere/Graphshell resident host, which will serve
# `personae::agent` in-process. Until then this installs the standalone bins
# and a logon scheduled task. Source is the folded crate here in mere (the
# standalone repos/personae was absorbed 2026-07-23), so rebuilds come from
# the mere workspace.
#
# Re-run any time after changing personae; it stops the task + process, copies
# fresh release bins, and restarts. Safe to run repeatedly.

$ErrorActionPreference = "Stop"
$bin = "$env:LOCALAPPDATA\personae\bin"
$mere = (Resolve-Path "$PSScriptRoot\..\..\..").Path

Write-Host "building personae bins (release, agent feature) from $mere"
Push-Location $mere
try {
    cargo build -p personae --features agent --release
} finally {
    Pop-Location
}

$agentExe = Join-Path $mere "target\release\personae-agent.exe"
$vaultExe = Join-Path $mere "target\release\personae-vault.exe"

New-Item -ItemType Directory -Force $bin | Out-Null

# The running exe holds a file lock; stop it before copying.
try { Stop-ScheduledTask -TaskName "personae-agent" } catch {}
Stop-Process -Name personae-agent -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

Copy-Item $agentExe (Join-Path $bin "personae-agent.exe") -Force
Copy-Item $vaultExe (Join-Path $bin "personae-vault.exe") -Force
Write-Host "installed to $bin"

# The windowless launcher loops the agent so a crash relaunches it (Task
# Scheduler's own restart policy only covers failure to launch, not a crashed
# child — proven 2026-07-22).
$vbs = @"
Set shell = CreateObject("WScript.Shell")
Do
  shell.Run """$bin\personae-agent.exe"" --log-file ""$env:LOCALAPPDATA\personae\agent.log""", 0, True
  WScript.Sleep 5000
Loop
"@
Set-Content (Join-Path $bin "personae-agent.vbs") -Value $vbs -Encoding ASCII

# Register the logon task if it is not already present.
if (-not (Get-ScheduledTask -TaskName "personae-agent" -ErrorAction SilentlyContinue)) {
    Write-Host "registering logon scheduled task"
    $action = New-ScheduledTaskAction -Execute "wscript.exe" -Argument "`"$bin\personae-agent.vbs`""
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Seconds 0) `
        -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited
    Register-ScheduledTask -TaskName "personae-agent" -Action $action -Trigger $trigger `
        -Settings $settings -Principal $principal `
        -Description "personae vault SSH agent on the OpenSSH pipe (interim; the mere/Graphshell host will serve it in-process)" | Out-Null
}

Start-ScheduledTask -TaskName "personae-agent"
Start-Sleep -Seconds 3
Write-Host "agent restarted; `ssh-add -l` should list vault keys"
