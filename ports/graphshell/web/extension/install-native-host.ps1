[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$BinaryPath,
    [string]$InstallRoot = (Join-Path $env:LOCALAPPDATA "Graphshell\NativeMessagingHosts")
)

$ErrorActionPreference = "Stop"
$resolvedBinary = (Resolve-Path -LiteralPath $BinaryPath).Path
if (-not [System.IO.Path]::IsPathFullyQualified($resolvedBinary)) {
    throw "The native host binary path must be absolute."
}

New-Item -ItemType Directory -Path $InstallRoot -Force | Out-Null
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$escapedBinary = $resolvedBinary.Replace("\", "\\")
$chromeManifest = Join-Path $InstallRoot "org.mere.graphshell.chromium.json"
$firefoxManifest = Join-Path $InstallRoot "org.mere.graphshell.firefox.json"

(Get-Content -Raw -LiteralPath (Join-Path $scriptRoot "native-host.chromium.json.in")).
    Replace("__GRAPHSHELL_NATIVE_HOST__", $escapedBinary) |
    Set-Content -NoNewline -Encoding utf8 -LiteralPath $chromeManifest
(Get-Content -Raw -LiteralPath (Join-Path $scriptRoot "native-host.firefox.json.in")).
    Replace("__GRAPHSHELL_NATIVE_HOST__", $escapedBinary) |
    Set-Content -NoNewline -Encoding utf8 -LiteralPath $firefoxManifest

$chromeKey = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\org.mere.graphshell"
$firefoxKey = "HKCU:\Software\Mozilla\NativeMessagingHosts\org.mere.graphshell"
New-Item -Path $chromeKey -Force | Out-Null
Set-Item -Path $chromeKey -Value $chromeManifest
New-Item -Path $firefoxKey -Force | Out-Null
Set-Item -Path $firefoxKey -Value $firefoxManifest

Write-Output "Registered org.mere.graphshell for Chrome and Firefox."
Write-Output "Chromium extension id: oajkkocppbpbmfblepgbiidagliniofd"
Write-Output "Firefox extension id: graphshell@mere.systems"
