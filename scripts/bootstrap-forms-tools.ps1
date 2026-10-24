param([string]$Destination = '')
$ErrorActionPreference = 'Stop'
$formsRoot = Split-Path -Parent $PSScriptRoot
if (!$Destination) { $Destination = Join-Path $formsRoot '.tools/forms' }
$Destination = [IO.Path]::GetFullPath($Destination)
$formsLock = Get-Content -LiteralPath (Join-Path $formsRoot 'assets/forms-tools.lock.json') -Raw | ConvertFrom-Json
$null = New-Item -ItemType Directory -Force -Path $Destination
function Get-PinnedFormResource($Resource, [string]$Path) {
    if (!(Test-Path -LiteralPath $Path) -or (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ne $Resource.sha256) {
        Invoke-WebRequest -Uri $Resource.url -OutFile $Path
    }
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ne $Resource.sha256) { throw "Resource checksum mismatch: $Path" }
}
$formsInstaller = Join-Path $Destination 'tesseract-5.5.0.exe'
Get-PinnedFormResource $formsLock.tesseract $formsInstaller
$formsExtractor = (Get-Command 7z -ErrorAction Stop).Source
$formsOcr = Join-Path $Destination 'ocr'
# Extract the pinned installer as data; no installer, registry or PATH changes.
& $formsExtractor x $formsInstaller ('-o' + $formsOcr) -y | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Tesseract extraction failed.' }
Get-PinnedFormResource $formsLock.english (Join-Path $formsOcr 'tessdata/eng.traineddata')
foreach ($resource in $formsLock.runtime_files.PSObject.Properties) {
    if ((Get-FileHash -LiteralPath (Join-Path $formsOcr $resource.Name) -Algorithm SHA256).Hash -ne $resource.Value) { throw "OCR runtime checksum mismatch: $($resource.Name)" }
}
& (Join-Path $formsOcr 'tesseract.exe') --version
if ($LASTEXITCODE -ne 0) { throw 'The extracted OCR worker could not start.' }
Write-Host "Offline form recognition resources: $formsOcr"
