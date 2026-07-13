Set-StrictMode -Version Latest

function Get-FileSha256 {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Test-VerifiedFile {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [string]$ExpectedSha256
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $false
    }

    return (Get-FileSha256 -Path $Path) -eq $ExpectedSha256.ToLowerInvariant()
}

function Invoke-VerifiedDownload {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Urls,

        [Parameter(Mandatory = $true)]
        [string]$OutputPath,

        [Parameter(Mandatory = $true)]
        [string]$ExpectedSha256,

        [string]$AssetName = (Split-Path -Leaf $OutputPath),

        [ValidateRange(1, 20)]
        [int]$MaxAttempts = 4,

        [ValidateRange(1, 7200)]
        [int]$TimeoutSeconds = 600,

        [System.Collections.IList]$AttemptLog,

        [scriptblock]$Transfer
    )

    if ($Urls.Count -eq 0) {
        throw "No download source configured for $AssetName"
    }

    $expected = $ExpectedSha256.ToLowerInvariant()
    $parent = Split-Path -Parent $OutputPath
    if ($parent) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }

    if (Test-Path -LiteralPath $OutputPath -PathType Leaf) {
        $actual = Get-FileSha256 -Path $OutputPath
        if ($actual -eq $expected) {
            Write-Host "Using verified cached asset $AssetName from $OutputPath"
            return [pscustomobject]@{
                AssetName = $AssetName
                Path = $OutputPath
                Source = "cache"
                Sha256 = $actual
                SizeBytes = (Get-Item -LiteralPath $OutputPath).Length
            }
        }

        Write-Warning "Removing invalid cached asset $AssetName (expected $expected, got $actual)"
        Remove-Item -LiteralPath $OutputPath -Force
    }

    $partialPath = "$OutputPath.partial"
    $errors = [System.Collections.Generic.List[string]]::new()
    if (-not $Transfer) {
        $Transfer = {
            param($Url, $Destination, $Timeout)
            Invoke-WebRequest `
                -Uri $Url `
                -OutFile $Destination `
                -UseBasicParsing `
                -TimeoutSec $Timeout
        }
    }

    foreach ($url in $Urls) {
        for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
            Remove-Item -LiteralPath $partialPath -Force -ErrorAction SilentlyContinue
            $startedAt = (Get-Date).ToUniversalTime().ToString("o")
            Write-Host "Downloading $AssetName from $url (attempt $attempt/$MaxAttempts)"

            try {
                & $Transfer $url $partialPath $TimeoutSeconds
                if (-not (Test-Path -LiteralPath $partialPath -PathType Leaf)) {
                    throw "Transfer completed without creating $partialPath"
                }

                $actual = Get-FileSha256 -Path $partialPath
                $size = (Get-Item -LiteralPath $partialPath).Length
                if ($actual -ne $expected) {
                    throw "Checksum mismatch: expected $expected, got $actual"
                }

                Move-Item -LiteralPath $partialPath -Destination $OutputPath -Force
                if ($null -ne $AttemptLog) {
                    [void]$AttemptLog.Add([pscustomobject]@{
                        AssetName = $AssetName
                        Url = $url
                        Attempt = $attempt
                        StartedAt = $startedAt
                        Success = $true
                        Error = $null
                        SizeBytes = $size
                        ActualSha256 = $actual
                    })
                }

                Write-Host "Verified $AssetName ($size bytes, sha256 $actual)"
                return [pscustomobject]@{
                    AssetName = $AssetName
                    Path = $OutputPath
                    Source = $url
                    Sha256 = $actual
                    SizeBytes = $size
                }
            }
            catch {
                $message = $_.Exception.Message
                $actual = $null
                $size = $null
                if (Test-Path -LiteralPath $partialPath -PathType Leaf) {
                    $size = (Get-Item -LiteralPath $partialPath).Length
                    try {
                        $actual = Get-FileSha256 -Path $partialPath
                    }
                    catch {
                        $actual = $null
                    }
                }

                if ($null -ne $AttemptLog) {
                    [void]$AttemptLog.Add([pscustomobject]@{
                        AssetName = $AssetName
                        Url = $url
                        Attempt = $attempt
                        StartedAt = $startedAt
                        Success = $false
                        Error = $message
                        SizeBytes = $size
                        ActualSha256 = $actual
                    })
                }

                $errors.Add("$url attempt $attempt`: $message")
                Write-Warning "Download failed for $AssetName from $url`: $message"
                Remove-Item -LiteralPath $partialPath -Force -ErrorAction SilentlyContinue

                if ($attempt -lt $MaxAttempts) {
                    $delay = [math]::Min(30, [math]::Pow(2, $attempt))
                    Start-Sleep -Seconds $delay
                }
            }
        }
    }

    Remove-Item -LiteralPath $partialPath -Force -ErrorAction SilentlyContinue
    throw "All download sources failed for $AssetName. $($errors -join '; ')"
}

Export-ModuleMember -Function Get-FileSha256, Test-VerifiedFile, Invoke-VerifiedDownload
