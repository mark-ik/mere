# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

param(
    [string]$Matrix = (Join-Path $PSScriptRoot 'model-matrix.json'),
    [string]$Decoder = (Join-Path $PSScriptRoot 'decoder-model.json')
)

$ErrorActionPreference = 'Stop'
$probeRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$mereRoot = (Resolve-Path (Join-Path $probeRoot '..\..\..')).Path
$modelsRoot = Join-Path $mereRoot 'models'
$configuration = Get-Content -Raw $Matrix | ConvertFrom-Json
$decoderConfiguration = Get-Content -Raw $Decoder | ConvertFrom-Json
$models = @($configuration.models) + @($decoderConfiguration.model)

foreach ($model in $models) {
    $relativeDirectory = $model.model_base_url -replace '^/models/', ''
    if (-not $relativeDirectory -or $relativeDirectory -match '[\\/:*?"<>|]') {
        throw "Unsafe model directory from '$($model.model_base_url)'"
    }
    $directory = Join-Path $modelsRoot $relativeDirectory
    New-Item -ItemType Directory -Force -Path $directory | Out-Null

    foreach ($name in @('config', 'tokenizer', 'weights')) {
        $artifact = $model.artifacts.$name
        $fileName = switch ($name) {
            'config' { 'config.json' }
            'tokenizer' { 'tokenizer.json' }
            'weights' { 'model.safetensors' }
        }
        $target = Join-Path $directory $fileName
        $expectedHash = $artifact.sha256
        $valid = Test-Path -LiteralPath $target
        if ($valid) {
            $item = Get-Item -LiteralPath $target
            $valid = $item.Length -eq [long]$artifact.bytes
            if ($valid -and $expectedHash) {
                $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $target).Hash.ToLowerInvariant()
                $valid = $actualHash -eq $expectedHash
            }
        }
        if ($valid) {
            Write-Host "verified $($model.model_id) $fileName"
            continue
        }

        $part = "$target.part"
        $url = "https://huggingface.co/$($model.model_id)/resolve/$($model.revision)/${fileName}?download=true"
        Write-Host "fetching $($model.model_id) $fileName"
        & curl.exe --location --fail --retry 3 --continue-at - --output $part $url
        if ($LASTEXITCODE -ne 0) {
            throw "curl failed for $url"
        }
        $item = Get-Item -LiteralPath $part
        if ($item.Length -ne [long]$artifact.bytes) {
            throw "$fileName byte count $($item.Length) did not match $($artifact.bytes)"
        }
        if ($expectedHash) {
            $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $part).Hash.ToLowerInvariant()
            if ($actualHash -ne $expectedHash) {
                throw "$fileName SHA256 $actualHash did not match $expectedHash"
            }
        }
        Move-Item -Force -LiteralPath $part -Destination $target
    }
}
