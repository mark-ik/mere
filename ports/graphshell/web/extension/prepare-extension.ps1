[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("chromium", "firefox")]
    [string]$Browser,
    [Parameter(Mandatory = $true)]
    [string]$Destination
)

$ErrorActionPreference = "Stop"
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$destinationPath = [System.IO.Path]::GetFullPath($Destination)
New-Item -ItemType Directory -Path $destinationPath -Force | Out-Null

Copy-Item -LiteralPath (Join-Path $scriptRoot "background.js") -Destination $destinationPath
Copy-Item -LiteralPath (Join-Path $scriptRoot "bridge.html") -Destination $destinationPath
Copy-Item -LiteralPath (Join-Path $scriptRoot "bridge.css") -Destination $destinationPath
Copy-Item -LiteralPath (Join-Path $scriptRoot "bridge.js") -Destination $destinationPath
Copy-Item `
    -LiteralPath (Join-Path $scriptRoot "manifest.$Browser.json") `
    -Destination (Join-Path $destinationPath "manifest.json")

Write-Output $destinationPath
