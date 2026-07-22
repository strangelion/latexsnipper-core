param(
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
$manifestPath = Join-Path $PSScriptRoot "real-model-test-assets.json"
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
    Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $DiagnosticDir "real-model-test-assets.json") -Force

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
        ReleaseTag = $manifest.releaseTag
        CacheDir = $CacheDir
        RunnerOs = $env:RUNNER_OS
        OsDescription = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription
        PowerShellVersion = $PSVersionTable.PSVersion.ToString()
        Attempts = @($attempts)
        ExistingFiles = @($existingFiles)
    }
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $DiagnosticDir "setup-report.json") -Encoding utf8
}

try {
    New-Item -ItemType Directory -Path $CacheDir -Force | Out-Null

    foreach ($item in $manifest.variants) {
        # Find the zipFile by reading the model catalog template.
        # The test asset file carries the variant identity; the zipFile
        # is looked up from the locally-shipped catalog template.
        $catalogPath = Join-Path $PSScriptRoot "model-manifest.template.json"
        $catalog = Get-Content -LiteralPath $catalogPath -Raw | ConvertFrom-Json
        $categoryEntry = $catalog.categories.($item.category)
        if (-not $categoryEntry) {
            throw "Category '$($item.category)' not found in model catalog"
        }
        $variantEntry = $categoryEntry.variants | Where-Object { $_.id -eq $item.id } | Select-Object -First 1
        if (-not $variantEntry) {
            throw "Variant '$($item.category)/$($item.id)' not found in model catalog"
        }
        $assetName = $variantEntry.zipFile

        # Safe checksum access (avoids StrictMode PropertyNotFoundException on empty objects)
        $checksumProperty = $manifest.checksums.PSObject.Properties[$assetName]
        if ($null -eq $checksumProperty) {
            throw "Checksum missing for required real-model asset: $assetName"
        }
        $expected = [string]$checksumProperty.Value
        if ([string]::IsNullOrWhiteSpace($expected)) {
            throw "Checksum is empty for required real-model asset: $assetName"
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

        $destination = Join-Path (Join-Path $root "models") $item.category
        New-Item -ItemType Directory -Path $destination -Force | Out-Null
        Expand-Archive -LiteralPath $archive -DestinationPath $destination -Force
    }

    $orientation = $manifest.testAssets.orientation
    if (
        -not $orientation -or
        -not $orientation.fileName -or
        -not $orientation.sha256 -or
        -not $orientation.destination
    ) {
        throw "Orientation test asset is missing from the test assets manifest"
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

    $testModels = [System.IO.Path]::GetFullPath((Join-Path $root "test-models"))
    $orientationDestination = [System.IO.Path]::GetFullPath(
        (Join-Path $testModels $orientation.destination)
    )
    $testModelsPrefix = $testModels.TrimEnd(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar
    ) + [System.IO.Path]::DirectorySeparatorChar
    if (-not $orientationDestination.StartsWith(
            $testModelsPrefix,
            [System.StringComparison]::OrdinalIgnoreCase
        )) {
        throw "Orientation model destination escapes the test-models directory"
    }

    New-Item -ItemType Directory -Path (Split-Path -Parent $orientationDestination) -Force | Out-Null
    Copy-Item -LiteralPath $orientationArchive -Destination $orientationDestination -Force
    $installedSha256 = Get-FileSha256 -Path $orientationDestination
    if ($installedSha256 -ne $orientation.sha256.ToLowerInvariant()) {
        throw "Installed orientation model checksum mismatch"
    }

    Write-Host "Verified real-model test assets are ready."
}
catch {
    Write-SetupDiagnostics -Failure $_.Exception.Message
    throw
}
