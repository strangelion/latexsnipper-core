# Package models for release
# Usage: .\scripts\package-models.ps1 [-ManifestPath scripts/model-manifest.template.json] [-ModelsDir models] [-OutputDir release_models]
#
# Reads the model manifest template, discovers all category:variant entries,
# and packages each variant as a separate ZIP in the output directory.
#
# RuntimeVariant-aware: reads both top-level "files" and nested
# "runtimeVariants[*].artifacts" to collect all files for a variant.

param(
    [string]$ManifestPath = "scripts/model-manifest.template.json",
    [string]$ModelsDir = "models",
    [string]$OutputDir = "release_models"
)

$ErrorActionPreference = "Stop"

# Resolve manifest path
$manifestFile = Join-Path (Get-Location) $ManifestPath
if (-not (Test-Path $manifestFile)) {
    Write-Host "ERROR: Manifest not found at $manifestFile" -ForegroundColor Red
    exit 1
}

# Create output directory
if (Test-Path $OutputDir) {
    Remove-Item -Recurse -Force $OutputDir
}
New-Item -ItemType Directory -Path $OutputDir | Out-Null

Write-Host "Packaging models for release..." -ForegroundColor Green
Write-Host "Manifest: $manifestFile" -ForegroundColor Cyan

# Helper: collect all files for a variant (top-level + runtimeVariants.artifacts)
function Get-PackageFiles {
    param($Variant)

    $files = [System.Collections.Generic.HashSet[string]]::new(
        [System.StringComparer]::OrdinalIgnoreCase
    )

    # Top-level files
    foreach ($file in @($Variant.files)) {
        if ($file) {
            [void]$files.Add([string]$file)
        }
    }

    # RuntimeVariant artifacts
    foreach ($runtimeVariant in @($Variant.runtimeVariants)) {
        if ($null -eq $runtimeVariant.artifacts) {
            continue
        }
        foreach ($artifact in $runtimeVariant.artifacts.PSObject.Properties) {
            if ($artifact.Value) {
                [void]$files.Add([string]$artifact.Value)
            }
        }
    }

    return @($files | Sort-Object)
}

# Parse manifest to discover all variants
$manifest = Get-Content $manifestFile -Raw | ConvertFrom-Json
$variants = @()
$manifest.categories.PSObject.Properties | ForEach-Object {
    $cat = $_.Name
    $_.Value.variants | ForEach-Object {
        $variants += @{
            category       = $cat
            id             = $_.id
            zipFile        = $_.zipFile
            files          = $_.files
            adapter        = $_.adapter
            modelType      = $_.modelType
            runtimeVariants = $_.runtimeVariants
        }
    }
}

Write-Host "Discovered $($variants.Count) variants:" -ForegroundColor Cyan
$variants | ForEach-Object { Write-Host "  $($_.category)/$($_.id) -> $($_.zipFile)" }

$checksums = @{}
$packagedVariants = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase
)

foreach ($v in $variants) {
    $variantDir = Join-Path (Join-Path $ModelsDir $v.category) $v.id
    $zipPath = Join-Path $OutputDir $v.zipFile

    if (-not (Test-Path $variantDir)) {
        Write-Host "Warning: $variantDir not found, skipping $($v.category)/$($v.id)" -ForegroundColor Yellow
        continue
    }

    Write-Host "Packaging $($v.category)/$($v.id)..." -ForegroundColor Cyan

    # Resolve actual files to package (combine top-level + runtimeVariants artifacts)
    $packageFiles = Get-PackageFiles (Get-Content $manifestFile -Raw | ConvertFrom-Json |
        Select-Object -ExpandProperty categories |
        Select-Object -ExpandProperty $v.category |
        Select-Object -ExpandProperty variants |
        Where-Object { $_.id -eq $v.id })

    # Create temp directory with proper structure
    $tempDir = Join-Path $OutputDir "temp_$($v.category)_$($v.id)"
    if (Test-Path $tempDir) {
        Remove-Item -Recurse -Force $tempDir
    }
    $tempVariantDir = Join-Path $tempDir $v.id
    New-Item -ItemType Directory -Path $tempVariantDir -Force | Out-Null

    # Copy files from the resolved package file list
    $missingFiles = @()
    foreach ($file in $packageFiles) {
        $src = Join-Path $variantDir $file
        if (Test-Path $src) {
            Copy-Item $src $tempVariantDir
        } else {
            $missingFiles += $file
            Write-Host "  Warning: $file not found in $variantDir" -ForegroundColor Yellow
        }
    }

    if ($missingFiles.Count -gt 0) {
        Write-Host "  Warning: $($missingFiles.Count) file(s) missing, skipping this variant" -ForegroundColor Yellow
        Remove-Item -Recurse -Force $tempDir
        continue
    }

    # Inject runtimeVariants into the packaged config.json so the model carries
    # its own runtime metadata even without the remote catalog.
    $configPath = Join-Path $tempVariantDir "config.json"
    if (
        (Test-Path $configPath) -and
        $v.runtimeVariants -and
        $v.runtimeVariants.Count -gt 0
    ) {
        $config = Get-Content $configPath -Raw | ConvertFrom-Json

        $config | Add-Member `
            -MemberType NoteProperty `
            -Name runtimeVariants `
            -Value $v.runtimeVariants `
            -Force

        $config |
            ConvertTo-Json -Depth 32 |
            Set-Content $configPath -Encoding utf8

        Write-Host "  Injected runtimeVariants into config.json" -ForegroundColor DarkGray
    }

    # Create zip
    Compress-Archive -Path "$tempDir\*" -DestinationPath $zipPath -Force

    # Calculate checksum
    $hash = Get-FileHash -Path $zipPath -Algorithm SHA256
    $checksums[$v.zipFile] = $hash.Hash.ToLower()

    # Mark as successfully packaged
    $key = "$($v.category)/$($v.id)"
    [void]$packagedVariants.Add($key)

    # Clean up temp
    Remove-Item -Recurse -Force $tempDir

    $size = (Get-Item $zipPath).Length / 1MB
    Write-Host "  Created: $zipPath ($([math]::Round($size, 1)) MB)" -ForegroundColor Green
}

# Generate SHA256SUMS
$shaPath = Join-Path $OutputDir "SHA256SUMS"
Get-ChildItem $OutputDir -Filter "*.zip" | ForEach-Object {
    $hash = Get-FileHash $_.FullName -Algorithm SHA256
    "$($hash.Hash.ToLower())  $($_.Name)" | Add-Content $shaPath
}
Write-Host "`nSHA256SUMS generated:" -ForegroundColor Green
Get-Content $shaPath

# Update manifest with real checksums
$manifest.checksums = $checksums | ForEach-Object {
    $h = @{}
    $_.GetEnumerator() | ForEach-Object { $h[$_.Key] = $_.Value }
    $h
}

# Prune categories/variants that were not packaged (missing model directories/files)
$categoryNames = @(
    $manifest.categories.PSObject.Properties |
    ForEach-Object { $_.Name }
)

foreach ($category in $categoryNames) {
    $categoryInfo = $manifest.categories.$category

    $kept = @(
        $categoryInfo.variants |
        Where-Object {
            $packagedVariants.Contains(
                "$category/$($_.id)"
            )
        }
    )

    if ($kept.Count -eq 0) {
        $manifest.categories.PSObject.Properties.Remove($category)
        Write-Host "Removed empty category: $category" -ForegroundColor Yellow
        continue
    }

    $categoryInfo.variants = $kept

    # If default variant was pruned, pick the first remaining
    $defaultExists = @(
        $kept |
        Where-Object {
            $_.id -eq $categoryInfo.default
        }
    ).Count -gt 0

    if (-not $defaultExists) {
        $newDefault = $kept[0].id
        Write-Host "Changed default for $category: $($categoryInfo.default) -> $newDefault" -ForegroundColor Yellow
        $categoryInfo.default = $newDefault
    }
}

# Update variant file lists and runtimeVariants to match actual packaged files
# (Read back from template, only keep successfully packaged entries)
$manifest.categories.PSObject.Properties | ForEach-Object {
    $cat = $_.Name
    $_.Value.variants | ForEach-Object {
        $variant = $_
        $packagedFileSet = Get-PackageFiles $variant
        $variant.files = $packagedFileSet
    }
}

# Save updated manifest
$manifestPath = Join-Path $OutputDir "model-manifest.json"
$manifest | ConvertTo-Json -Depth 32 | Set-Content $manifestPath
Write-Host "`nManifest saved: $manifestPath" -ForegroundColor Green

# Summary
Write-Host "`nFiles created in $OutputDir/:" -ForegroundColor Green
Get-ChildItem $OutputDir -Filter "*.zip" | ForEach-Object {
    Write-Host "  $($_.Name) ($([math]::Round($_.Length / 1MB, 1)) MB)" -ForegroundColor Cyan
}
Write-Host "  SHA256SUMS" -ForegroundColor Cyan
Write-Host "  model-manifest.json" -ForegroundColor Cyan
