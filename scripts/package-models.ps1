# Package models for release
# Usage: .\scripts\package-models.ps1

$ErrorActionPreference = "Stop"
$modelsDir = "models"
$outputDir = "release_models"

# Create output directory
if (Test-Path $outputDir) {
    Remove-Item -Recurse -Force $outputDir
}
New-Item -ItemType Directory -Path $outputDir | Out-Null

Write-Host "Packaging models for release..." -ForegroundColor Green

# Define model categories and their files
$categories = @{
    "formula-det" = @{
        variant = "yolov8-mfd"
        files = @("mathcraft-mfd.onnx", "config.json")
        zipName = "latexsnipper-formula-det.zip"
    }
    "formula-rec" = @{
        variant = "trocr-deit"
        files = @("encoder_model.onnx", "decoder_model.onnx", "tokenizer.json", "config.json")
        zipName = "latexsnipper-formula-rec.zip"
    }
    "text-det" = @{
        variant = "v6-small"
        files = @("inference.onnx", "inference.yml", "config.json")
        zipName = "latexsnipper-text-det.zip"
    }
    "text-rec" = @{
        variant = "v6-small"
        files = @("inference.onnx", "inference.yml", "config.json")
        zipName = "latexsnipper-text-rec.zip"
    }
    "table-det" = @{
        variant = "tatr-detection"
        files = @("model.onnx", "model.onnx.data", "config.json")
        zipName = "latexsnipper-table-det.zip"
    }
    "table-struct" = @{
        variant = "tatr-structure"
        files = @("model.onnx", "model.onnx.data", "config.json")
        zipName = "latexsnipper-table-struct.zip"
    }
}

$checksums = @{}

foreach ($cat in $categories.Keys) {
    $info = $categories[$cat]
    $variantDir = Join-Path (Join-Path $modelsDir $cat) $info.variant
    $zipPath = Join-Path $outputDir $info.zipName

    if (-not (Test-Path $variantDir)) {
        Write-Host "Warning: $variantDir not found, skipping" -ForegroundColor Yellow
        continue
    }

    Write-Host "Packaging $cat ($($info.variant))..." -ForegroundColor Cyan

    # Create temp directory with proper structure
    $tempDir = Join-Path $outputDir "temp_$cat"
    if (Test-Path $tempDir) {
        Remove-Item -Recurse -Force $tempDir
    }
    $tempVariantDir = Join-Path (Join-Path $tempDir $cat) $info.variant
    New-Item -ItemType Directory -Path $tempVariantDir -Force | Out-Null

    # Copy files
    foreach ($file in $info.files) {
        $src = Join-Path $variantDir $file
        if (Test-Path $src) {
            Copy-Item $src $tempVariantDir
        } else {
            Write-Host "  Warning: $file not found in $variantDir" -ForegroundColor Yellow
        }
    }

    # Create zip
    Compress-Archive -Path "$tempDir\*" -DestinationPath $zipPath -Force

    # Calculate checksum
    $hash = Get-FileHash -Path $zipPath -Algorithm SHA256
    $checksums[$info.zipName] = $hash.Hash.ToLower()

    # Clean up temp
    Remove-Item -Recurse -Force $tempDir

    $size = (Get-Item $zipPath).Length / 1MB
    Write-Host "  Created: $zipPath ($([math]::Round($size, 1)) MB)" -ForegroundColor Green
}

# Update manifest with checksums
$manifestPath = Join-Path $modelsDir "model-manifest.json"
$manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json

# Update checksums
$manifest.checksums = $checksums

# Update file lists to match actual files
foreach ($cat in $categories.Keys) {
    $info = $categories[$cat]
    if ($manifest.categories.$cat.variants) {
        foreach ($variant in $manifest.categories.$cat.variants) {
            if ($variant.id -eq $info.variant) {
                $variant.files = $info.files
            }
        }
    }
}

# Save updated manifest
$manifest | ConvertTo-Json -Depth 10 | Set-Content $manifestPath

Write-Host "`nManifest updated with checksums" -ForegroundColor Green
Write-Host "`nFiles created in $outputDir/:" -ForegroundColor Green
Get-ChildItem $outputDir -Filter "*.zip" | ForEach-Object {
    Write-Host "  $($_.Name) ($([math]::Round($_.Length / 1MB, 1)) MB)" -ForegroundColor Cyan
}

Write-Host "`nTo upload to GitHub releases:" -ForegroundColor Yellow
Write-Host "  1. Run: gh auth login" -ForegroundColor White
Write-Host "  2. Run: gh release create models-v1.3.0 --title 'Models v1.3.0' --notes 'Model packages for LaTeXSnipper Core'" -ForegroundColor White
Write-Host "  3. Upload the zip files from $outputDir/" -ForegroundColor White
Write-Host "  4. Or use: gh release upload models-v1.3.0 $outputDir/*.zip" -ForegroundColor White