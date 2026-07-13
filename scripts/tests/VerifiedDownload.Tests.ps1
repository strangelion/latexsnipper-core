$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

Import-Module (Join-Path $PSScriptRoot "..\VerifiedDownload.psm1") -Force

function Assert-True {
    param(
        [Parameter(Mandatory = $true)]
        [bool]$Condition,
        [Parameter(Mandatory = $true)]
        [string]$Message
    )

    if (-not $Condition) {
        throw "Assertion failed: $Message"
    }
}

function Get-BytesSha256 {
    param([byte[]]$Bytes)

    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash($Bytes)) -replace "-", "").ToLowerInvariant()
    }
    finally {
        $sha.Dispose()
    }
}

$root = Join-Path ([System.IO.Path]::GetTempPath()) ("latexsnipper-download-tests-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $root | Out-Null

try {
    $payload = [Text.Encoding]::UTF8.GetBytes("verified model fixture")
    $checksum = Get-BytesSha256 -Bytes $payload

    $primary = {
        param($Url, $Destination, $Timeout)
        [IO.File]::WriteAllBytes($Destination, $payload)
    }
    $output = Join-Path $root "primary.bin"
    $result = Invoke-VerifiedDownload -Urls @("primary") -OutputPath $output -ExpectedSha256 $checksum -Transfer $primary -MaxAttempts 1
    Assert-True ($result.Source -eq "primary") "primary source should succeed"
    Assert-True (Test-VerifiedFile -Path $output -ExpectedSha256 $checksum) "primary output should verify"

    $calls = [Collections.Generic.List[string]]::new()
    $mirror = {
        param($Url, $Destination, $Timeout)
        $calls.Add($Url)
        if ($Url -eq "primary") {
            throw "simulated primary failure"
        }
        [IO.File]::WriteAllBytes($Destination, $payload)
    }
    $output = Join-Path $root "mirror.bin"
    $result = Invoke-VerifiedDownload -Urls @("primary", "mirror") -OutputPath $output -ExpectedSha256 $checksum -Transfer $mirror -MaxAttempts 1
    Assert-True ($result.Source -eq "mirror") "mirror should be used after primary failure"
    Assert-True (($calls -join ",") -eq "primary,mirror") "sources should preserve order"

    $retryCount = 0
    $retry = {
        param($Url, $Destination, $Timeout)
        $script:retryCount++
        if ($script:retryCount -lt 2) {
            throw "simulated transient failure"
        }
        [IO.File]::WriteAllBytes($Destination, $payload)
    }
    $script:retryCount = 0
    $output = Join-Path $root "retry.bin"
    Invoke-VerifiedDownload -Urls @("retry") -OutputPath $output -ExpectedSha256 $checksum -Transfer $retry -MaxAttempts 2 | Out-Null
    Assert-True ($script:retryCount -eq 2) "transient failure should retry"

    $wrong = {
        param($Url, $Destination, $Timeout)
        [IO.File]::WriteAllText($Destination, "wrong")
    }
    $output = Join-Path $root "wrong.bin"
    $failed = $false
    try {
        Invoke-VerifiedDownload -Urls @("wrong") -OutputPath $output -ExpectedSha256 $checksum -Transfer $wrong -MaxAttempts 1 | Out-Null
    }
    catch {
        $failed = $_.Exception.Message -match "Checksum mismatch"
    }
    Assert-True $failed "checksum mismatch should fail"
    Assert-True (-not (Test-Path -LiteralPath "$output.partial")) "partial file should be cleaned"

    [IO.File]::WriteAllBytes($output, $payload)
    $networkCalled = $false
    $cached = {
        param($Url, $Destination, $Timeout)
        $script:networkCalled = $true
        throw "cache should avoid transfer"
    }
    $script:networkCalled = $false
    $result = Invoke-VerifiedDownload -Urls @("unused") -OutputPath $output -ExpectedSha256 $checksum -Transfer $cached -MaxAttempts 1
    Assert-True ($result.Source -eq "cache") "verified cache should be reused"
    Assert-True (-not $script:networkCalled) "verified cache should not call transfer"

    $attempts = [Collections.ArrayList]::new()
    $alwaysFail = {
        param($Url, $Destination, $Timeout)
        [IO.File]::WriteAllText($Destination, "partial")
        throw "simulated source failure"
    }
    $output = Join-Path $root "all-fail.bin"
    $failed = $false
    try {
        Invoke-VerifiedDownload -Urls @("one", "two") -OutputPath $output -ExpectedSha256 $checksum -Transfer $alwaysFail -MaxAttempts 1 -AttemptLog $attempts | Out-Null
    }
    catch {
        $failed = $_.Exception.Message -match "one" -and $_.Exception.Message -match "two"
    }
    Assert-True $failed "final error should include every source"
    Assert-True ($attempts.Count -eq 2) "diagnostic log should contain every attempt"
    Assert-True (-not (Test-Path -LiteralPath "$output.partial")) "failed transfer should clean partial file"

    Write-Host "VerifiedDownload tests passed."
}
finally {
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
