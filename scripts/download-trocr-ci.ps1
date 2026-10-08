param(
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ExpectedSha256,
    [Parameter(Mandatory = $true)][string]$PrimaryUrl,
    [Parameter(Mandatory = $true)][string]$ApiUrl,
    [ValidateRange(1, 20)][int]$MaxAttempts = 4,
    [scriptblock]$Transfer
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module (Join-Path $PSScriptRoot 'VerifiedDownload.psm1') -Force

foreach ($url in @($PrimaryUrl, $ApiUrl)) {
    $uri = [Uri]$url
    if (-not $uri.IsAbsoluteUri -or $uri.Scheme -ne 'https' -or $uri.UserInfo.Length -ne 0) {
        throw 'Model sources must be absolute HTTPS URLs without embedded credentials.'
    }
}

if (-not $Transfer) {
    $curlApplication = (Get-Command curl -CommandType Application -ErrorAction Stop).Source
    $Transfer = {
        param($Url, $Destination, $Timeout)
        # Both routes resolve the same public release asset. No auth forwarding
        # or third-party mirrors; the outer helper verifies before promotion.
        & $curlApplication --fail --location --silent --show-error --max-redirs 5 `
            --connect-timeout 20 --max-time $Timeout --max-filesize 134217728 `
            --header 'Accept: application/octet-stream' --output $Destination $Url
        if ($LASTEXITCODE -ne 0) { throw "curl transfer failed with exit code $LASTEXITCODE" }
    }.GetNewClosure()
}

Invoke-VerifiedDownload -Urls @($PrimaryUrl, $ApiUrl) -OutputPath $OutputPath `
    -ExpectedSha256 $ExpectedSha256 -AssetName 'TrOCR CI archive' -MaxAttempts $MaxAttempts `
    -TimeoutSeconds 600 -Transfer $Transfer
