param(
    [string]$Tag = "models-v2.0.0",
    [string]$CacheDir = $(
        if ($env:LATEXSNIPPER_MODEL_CACHE) {
            $env:LATEXSNIPPER_MODEL_CACHE
        }
        elseif ($env:RUNNER_TEMP) {
            Join-Path $env:RUNNER_TEMP "latexsnipper-model-cache"
        }
        else {
            Join-Path ([System.IO.Path]::GetTempPath()) "latexsnipper-model-cache"
        }
    ),
    [string]$DiagnosticDir = $(
        if ($env:LATEXSNIPPER_MODEL_DIAGNOSTICS) {
            $env:LATEXSNIPPER_MODEL_DIAGNOSTICS
        }
        elseif ($env:RUNNER_TEMP) {
            Join-Path $env:RUNNER_TEMP "latexsnipper-model-diagnostics"
        }
        else {
            Join-Path ([System.IO.Path]::GetTempPath()) "latexsnipper-model-diagnostics"
        }
    ),
    [string]$OverrideBaseUrl = $env:LATEXSNIPPER_MODEL_BASE_URL,
    [ValidateRange(1, 20)]
    [int]$MaxAttempts = 4,
    [ValidateRange(1, 7200)]
    [int]$TimeoutSeconds = 600
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$root = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $PSScriptRoot "model-manifest.template.json"
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$attempts = [System.Collections.ArrayList]::new()

Import-Module (Join-Path $PSScriptRoot "VerifiedDownload.psm1") -Force

function Get-CandidateUrls {
    param([Parameter(Mandatory = $true)][string]$AssetName)

    $urls = [System.Collections.Generic.List[string]]::new()
    $urls.Add($manifest.baseUrl.TrimEnd("/") + "/" + $AssetName)
    foreach ($mirror in $manifest.mirrors) {
        $urls.Add($mirror.TrimEnd("/") + "/" + $AssetName)
    }
    if ($OverrideBaseUrl) {
        $urls.Add($OverrideBaseUrl.TrimEnd("/") + "/" + $AssetName)
    }
    return $urls.ToArray()
}

function Write-SetupDiagnostics {
    param([Parameter(Mandatory = $true)][string]$Failure)

    New-Item -ItemType Directory -Path $DiagnosticDir -Force | Out-Null
    Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $DiagnosticDir "model-manifest.json") -Force

    $existingFiles = @()
    if (Test-Path -LiteralPath $CacheDir) {
        $existingFiles = Get-ChildItem -LiteralPath $CacheDir -File -Force | ForEach-Object {
            $sha = $null
            try {
                $sha = Get-FileSha256 -Path $_.FullName
            }
            catch {
                $sha = $null
            }
            [pscustomobject]@{
                Name = $_.Name
                SizeBytes = $_.Length
                Sha256 = $sha
            }
        }
    }

    $report = [ordered]@{
        TimestampUtc = (Get-Date).ToUniversalTime().ToString("o")
        Failure = $Failure
        Tag = $Tag
        CacheDir = $CacheDir
        RunnerOs = $env:RUNNER_OS
        OsDescription = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription
        PowerShellVersion = $PSVersionTable.PSVersion.ToString()
        Attempts = @($attempts)
        ExistingFiles = @($existingFiles)
    }
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $DiagnosticDir "setup-report.json") -Encoding utf8
}

$variants = @(
    @{ Category = "formula-det"; Id = "yolov8-mfd" },
    @{ Category = "formula-rec"; Id = "trocr-deit" },
    @{ Category = "text-det"; Id = "v6-small" },
    @{ Category = "text-rec"; Id = "v6-small" },
    @{ Category = "table-det"; Id = "tatr-detection" },
    @{ Category = "table-struct"; Id = "tatr-structure" }
)

try {
    New-Item -ItemType Directory -Path $CacheDir -Force | Out-Null

    foreach ($item in $variants) {
        $category = $manifest.categories.($item.Category)
        $variant = $category.variants | Where-Object { $_.id -eq $item.Id } | Select-Object -First 1
        if (-not $variant) {
            throw "Model manifest entry missing: $($item.Category)/$($item.Id)"
        }

        $assetName = $variant.zipFile
        $expected = $manifest.checksums.$assetName
        if (-not $expected) {
            throw "Checksum missing for required real-model asset: $assetName"
        }

        $archive = Join-Path $CacheDir $assetName
        Invoke-VerifiedDownload `
            -Urls (Get-CandidateUrls -AssetName $assetName) `
            -OutputPath $archive `
            -ExpectedSha256 $expected `
            -AssetName $assetName `
            -MaxAttempts $MaxAttempts `
            -TimeoutSeconds $TimeoutSeconds `
            -AttemptLog $attempts | Out-Null

        $destination = Join-Path (Join-Path $root "models") $item.Category
        New-Item -ItemType Directory -Path $destination -Force | Out-Null
        Expand-Archive -LiteralPath $archive -DestinationPath $destination -Force
    }

    $orientation = $manifest.testAssets.orientation
    if (-not $orientation -or -not $orientation.fileName -or -not $orientation.sha256) {
        throw "Orientation test asset is missing from the model manifest"
    }

    $orientationArchive = Join-Path $CacheDir $orientation.fileName
    Invoke-VerifiedDownload `
        -Urls @($orientation.sources) `
        -OutputPath $orientationArchive `
        -ExpectedSha256 $orientation.sha256 `
        -AssetName $orientation.fileName `
        -MaxAttempts $MaxAttempts `
        -TimeoutSeconds $TimeoutSeconds `
        -AttemptLog $attempts | Out-Null

    $testModels = Join-Path $root "test-models"
    New-Item -ItemType Directory -Path $testModels -Force | Out-Null
    tar -xf $orientationArchive -C $testModels
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to extract orientation model"
    }

    Write-Host "Verified real-model test assets are ready."
}
catch {
    Write-SetupDiagnostics -Failure $_.Exception.Message
    throw
}
