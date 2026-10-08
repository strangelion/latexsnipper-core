$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$scriptPath = Join-Path (Split-Path -Parent $PSScriptRoot) 'download-trocr-ci.ps1'
$tempPrefix = [IO.Path]::GetFullPath((Join-Path ([IO.Path]::GetTempPath()) 'ls-trocr-ci-test-'))
$taskDirectory = "$tempPrefix$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $taskDirectory | Out-Null

function Require([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

try {
    $bytes = [Text.Encoding]::UTF8.GetBytes('authored TrOCR download test, not a model')
    $hash = [Security.Cryptography.SHA256]::Create()
    try { $checksum = ([BitConverter]::ToString($hash.ComputeHash($bytes)) -replace '-', '').ToLowerInvariant() }
    finally { $hash.Dispose() }
    $settings = @{ ExpectedSha256 = $checksum; PrimaryUrl = 'https://github.com/test/model.zip';
        ApiUrl = 'https://api.github.com/test/model'; MaxAttempts = 1 }
    $calls = [Collections.Generic.List[string]]::new()
    $transfer = {
        param($Url, $Destination, $Timeout)
        $calls.Add($Url)
        if ($Url -eq $settings.PrimaryUrl) { throw 'simulated HTTP 500' }
        [IO.File]::WriteAllBytes($Destination, $bytes)
    }
    $output = Join-Path $taskDirectory 'fallback.zip'
    $result = & $scriptPath @settings -OutputPath $output -Transfer $transfer
    Require ($result.Source -eq $settings.ApiUrl) 'Fallback did not resolve the same declared asset.'
    Require ($calls.Count -eq 2) 'Fallback should follow the primary source.'
    Require ((Get-FileHash -LiteralPath $output).Hash.ToLowerInvariant() -eq $checksum) 'Promoted asset is not verified.'
    Require (-not (Test-Path -LiteralPath "$output.partial")) 'Successful transfer leaked partial bytes.'
    $calls.Clear()
    $result = & $scriptPath @settings -OutputPath $output -Transfer $transfer
    Require ($result.Source -eq 'cache' -and $calls.Count -eq 0) 'Verified cache triggered a transfer.'

    $output = Join-Path $taskDirectory 'bad.zip'
    $wrong = { param($Url, $Destination, $Timeout); [IO.File]::WriteAllText($Destination, 'not the pinned model') }
    $failed = $false
    try { & $scriptPath @settings -OutputPath $output -Transfer $wrong | Out-Null }
    catch { $failed = $_.Exception.Message -match 'Checksum mismatch' }
    Require $failed 'Wrong bytes must fail across both sources.'
    Require (-not (Test-Path -LiteralPath $output)) 'Unverified bytes were promoted.'
    Require (-not (Test-Path -LiteralPath "$output.partial")) 'Checksum failure leaked partial bytes.'

    $incomplete = { param($Url, $Destination, $Timeout); [IO.File]::WriteAllText($Destination, 'partial'); throw 'transfer interrupted' }
    $failed = $false
    try { & $scriptPath @settings -OutputPath $output -Transfer $incomplete | Out-Null }
    catch { $failed = $_.Exception.Message -match 'transfer interrupted' }
    Require $failed 'Interrupted downloads must remain errors.'
    Require (-not (Test-Path -LiteralPath "$output.partial")) 'Interrupted transfer leaked partial bytes.'

    # Reproduce Linux discovery returning multiple application paths. Do not
    # inject Transfer: this must exercise the production executable resolver.
    $fakeCurlPath = Join-Path $PSScriptRoot 'fixtures/FakeTrOcrCurl.ps1'
    function Get-Command {
        param($Name, $CommandType, $ErrorAction)
        if ($Name -ne 'curl' -or $CommandType -ne 'Application') {
            throw 'Unexpected command discovery in curl regression test.'
        }
        [pscustomobject]@{ Source = $fakeCurlPath }
        [pscustomobject]@{ Source = 'nonexistent-second-curl-application' }
    }
    try {
        $output = Join-Path $taskDirectory 'multiple-curl-paths.zip'
        $result = & $scriptPath @settings -OutputPath $output
        Require ($result.Source -eq $settings.PrimaryUrl) 'Default curl transfer did not select one application.'
        Require ((Get-FileHash -LiteralPath $output).Hash.ToLowerInvariant() -eq $checksum) 'Default transfer bytes were not verified.'
    }
    finally { Remove-Item Function:Get-Command }
    Write-Host 'TrOCR CI download fallback and checksum tests passed.'
}
finally {
    $resolved = [IO.Path]::GetFullPath($taskDirectory)
    if (-not $resolved.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase) -or $resolved -eq $tempPrefix) {
        throw 'Refusing cleanup outside the owned test directory.'
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
