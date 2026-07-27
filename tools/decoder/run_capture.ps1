param(
    [Parameter(Mandatory = $true)]
    [string]$ModelDir,
    [Parameter(Mandatory = $true)]
    [string]$FeedNpz,
    [Parameter(Mandatory = $true)]
    [string]$Output,
    [string]$Python = "python"
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$requirements = Join-Path $scriptDir "requirements-paddle.txt"
$capture = Join-Path $scriptDir "capture_paddle_while_state.py"

if (-not (Test-Path -LiteralPath $ModelDir -PathType Container)) {
    throw "ModelDir does not exist: $ModelDir"
}
if (-not (Test-Path -LiteralPath $FeedNpz -PathType Leaf)) {
    throw "FeedNpz does not exist: $FeedNpz"
}

$tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$venv = Join-Path $tempRoot ("latexsnipper-decoder-" + [System.Guid]::NewGuid().ToString("N"))
try {
    & $Python -m venv $venv
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to create the isolated Paddle capture environment."
    }
    $venvPython = Join-Path $venv "Scripts\python.exe"
    & $venvPython -m pip install --requirement $requirements
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to install the Paddle capture environment."
    }
    & $venvPython $capture --model-dir $ModelDir --feed-npz $FeedNpz --output $Output
    exit $LASTEXITCODE
}
finally {
    $resolved = [System.IO.Path]::GetFullPath($venv)
    if ($resolved.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase) -and
        (Test-Path -LiteralPath $resolved)) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
