param(
    [string]$Tag = "models-v2.0.0"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $PSScriptRoot "model-manifest.template.json"
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$downloadDir = Join-Path ([System.IO.Path]::GetTempPath()) "latexsnipper-model-ci"

if (Test-Path -LiteralPath $downloadDir) {
    Remove-Item -LiteralPath $downloadDir -Recurse -Force
}
New-Item -ItemType Directory -Path $downloadDir | Out-Null

$variants = @(
    @{ Category = "formula-det"; Id = "yolov8-mfd" },
    @{ Category = "formula-rec"; Id = "trocr-deit" },
    @{ Category = "text-det"; Id = "v6-small" },
    @{ Category = "text-rec"; Id = "v6-small" },
    @{ Category = "table-det"; Id = "tatr-detection" },
    @{ Category = "table-struct"; Id = "tatr-structure" }
)

foreach ($item in $variants) {
    $category = $manifest.categories.($item.Category)
    $variant = $category.variants | Where-Object { $_.id -eq $item.Id } | Select-Object -First 1
    if (-not $variant) {
        throw "Model manifest entry missing: $($item.Category)/$($item.Id)"
    }

    $zipName = $variant.zipFile
    $expected = $manifest.checksums.$zipName
    if (-not $expected) {
        throw "Checksum missing for required real-model asset: $zipName"
    }

    $archive = Join-Path $downloadDir $zipName
    $url = "https://github.com/strangelion/latexsnipper-core/releases/download/$Tag/$zipName"
    Write-Host "Downloading $url"
    Invoke-WebRequest -Uri $url -OutFile $archive -UseBasicParsing
    $actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $expected.ToLowerInvariant()) {
        throw "Checksum mismatch for $zipName`: expected $expected, got $actual"
    }

    $destination = Join-Path (Join-Path $root "models") $item.Category
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    Expand-Archive -LiteralPath $archive -DestinationPath $destination -Force
}

$orientationUrl = "https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-LCNet_x1_0_doc_ori_infer.tar"
$orientationSha256 = "282337df5c41f7cdf8dacd5acf71fddfdc10218399f4b318463c17f4eae96c97"
$orientationArchive = Join-Path $downloadDir "PP-LCNet_x1_0_doc_ori_infer.tar"
Invoke-WebRequest -Uri $orientationUrl -OutFile $orientationArchive -UseBasicParsing
$actualOrientation = (Get-FileHash -LiteralPath $orientationArchive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualOrientation -ne $orientationSha256) {
    throw "Checksum mismatch for orientation model"
}
$testModels = Join-Path $root "test-models"
New-Item -ItemType Directory -Path $testModels -Force | Out-Null
tar -xf $orientationArchive -C $testModels
if ($LASTEXITCODE -ne 0) {
    throw "Failed to extract orientation model"
}

Write-Host "Verified real-model test assets are ready."
