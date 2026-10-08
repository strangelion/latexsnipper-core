param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)

# Exercise the default transfer closure without network access or model bytes.
$outputIndex = [Array]::IndexOf($Arguments, '--output')
if ($outputIndex -lt 0 -or $outputIndex + 1 -ge $Arguments.Length) {
    throw 'Missing curl output argument.'
}
[IO.File]::WriteAllText($Arguments[$outputIndex + 1], 'authored TrOCR download test, not a model', [Text.UTF8Encoding]::new($false))
$global:LASTEXITCODE = 0
