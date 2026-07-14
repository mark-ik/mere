param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CargoArgs = @()
)

& "$PSScriptRoot\meerkat.ps1" check -CargoArgs $CargoArgs
