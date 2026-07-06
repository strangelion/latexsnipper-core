# Package models for release
# Usage: .\scripts\package-models.ps1 [-ManifestPath scripts/model-manifest.template.json] [-ModelsDir models] [-OutputDir release_models]
#
# Reads the model manifest template, discovers all category:variant entries,
# and packages each variant as a separate ZIP in the output directory.

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

# Parse manifest to discover all variants
$manifest = Get-Content $manifestFile -Raw | ConvertFrom-Json
$variants = @()
$manifest.categories.PSObject.Properties | ForEach-Object {
    $cat = $_.Name
    $_.Value.variants | ForEach-Object {
        $variants += @{
            category = $cat
            id = $_.id
            zipFile = $_.zipFile
            files = $_.files
            adapter = $_.adapter
            modelType = $_.modelType
        }
    }
}

Write-Host "Discovered $($variants.Count) variants:" -ForegroundColor Cyan
$variants | ForEach-Object { Write-Host "  $($_.category)/$($_.id) -> $($_.zipFile)" }

$checksums = @{}

foreach ($v in $variants) {
    $variantDir = Join-Path (Join-Path $ModelsDir $v.category) $v.id
    $zipPath = Join-Path $OutputDir $v.zipFile

    if (-not (Test-Path $variantDir)) {
        Write-Host "Warning: $variantDir not found, skipping" -ForegroundColor Yellow
        continue
    }

    Write-Host "Packaging $($v.category)/$($v.id)..." -ForegroundColor Cyan

    # Create temp directory with proper structure
    $tempDir = Join-Path $OutputDir "temp_$($v.category)_$($v.id)"
    if (Test-Path $tempDir) {
        Remove-Item -Recurse -Force $tempDir
    }
    $tempVariantDir = Join-Path $tempDir $v.id
    New-Item -ItemType Directory -Path $tempVariantDir -Force | Out-Null

    # Copy files (from manifest file list, or all if none specified)
    if ($v.files -and $v.files.Count -gt 0) {
        foreach ($file in $v.files) {
            $src = Join-Path $variantDir $file
            if (Test-Path $src) {
                Copy-Item $src $tempVariantDir
            } else {
                Write-Host "  Warning: $file not found in $variantDir" -ForegroundColor Yellow
            }
        }
    } else {
        # Copy all files from variant directory
        Get-ChildItem -Path $variantDir -File | ForEach-Object {
            Copy-Item $_.FullName $tempVariantDir
        }
    }

    # Create zip
    Compress-Archive -Path "$tempDir\*" -DestinationPath $zipPath -Force

    # Calculate checksum
    $hash = Get-FileHash -Path $zipPath -Algorithm SHA256
    $checksums[$v.zipFile] = $hash.Hash.ToLower()

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

# Update file lists to match actual packaged files
$manifest.categories.PSObject.Properties | ForEach-Object {
    $cat = $_.Name
    $_.Value.variants | ForEach-Object {
        $variant = $_
        $v = $variants | Where-Object { $_.category -eq $cat -and $_.id -eq $variant.id }
        if ($v -and $v.files) {
            $variant.files = $v.files
        }
    }
}

# Save updated manifest
$manifestPath = Join-Path $OutputDir "model-manifest.json"
$manifest | ConvertTo-Json -Depth 10 | Set-Content $manifestPath
Write-Host "`nManifest saved: $manifestPath" -ForegroundColor Green

# Summary
Write-Host "`nFiles created in $OutputDir/:" -ForegroundColor Green
Get-ChildItem $OutputDir -Filter "*.zip" | ForEach-Object {
    Write-Host "  $($_.Name) ($([math]::Round($_.Length / 1MB, 1)) MB)" -ForegroundColor Cyan
}
Write-Host "  SHA256SUMS" -ForegroundColor Cyan
Write-Host "  model-manifest.json" -ForegroundColor Cyan
