param([string]$Destination = 'dist/PDFree', [string]$Executable = 'target/release/pdfree.exe')
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
$destinationPath = [IO.Path]::GetFullPath((Join-Path $root $Destination))
if (!$destinationPath.StartsWith($root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Package destination must be inside this workspace.' }
$null = New-Item -ItemType Directory -Force -Path $destinationPath
Copy-Item -LiteralPath $Executable -Destination (Join-Path $destinationPath 'pdfree.exe')
foreach ($file in @('README.md','LICENSE','THIRD_PARTY_NOTICES.md','Cargo.lock')) { Copy-Item -LiteralPath $file -Destination $destinationPath }
Copy-Item -LiteralPath 'docs' -Destination $destinationPath -Recurse -Force
$benchmarkDestination = Join-Path $destinationPath 'benchmarks'
$null = New-Item -ItemType Directory -Force -Path $benchmarkDestination
Copy-Item -LiteralPath 'benchmarks/README.md' -Destination $benchmarkDestination -Force
Copy-Item -LiteralPath 'benchmarks/results' -Destination $benchmarkDestination -Recurse -Force
$metadata = & cargo metadata --locked --format-version 1
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$packages = ($metadata | ConvertFrom-Json).packages
$inventory = @($packages | Select-Object name,version,license,repository)
$inventory | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $destinationPath 'dependency-licenses.json') -Encoding utf8
$licenseRoot = Join-Path $destinationPath 'licenses'
$null = New-Item -ItemType Directory -Force -Path $licenseRoot
foreach ($package in $packages) {
    $source = Split-Path -Parent $package.manifest_path
    $licenseFiles = @(Get-ChildItem -LiteralPath $source -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE)' })
    if ($licenseFiles.Count -gt 0) {
        $folder = Join-Path $licenseRoot ($package.name + '-' + $package.version)
        $null = New-Item -ItemType Directory -Force -Path $folder
        foreach ($license in $licenseFiles) { Copy-Item -LiteralPath $license.FullName -Destination $folder }
    }
    if ($package.name -eq 'mupdf-sys') {
        $nativeSource = Join-Path $source 'mupdf'
        $nativeLicenses = Get-ChildItem -LiteralPath $nativeSource -Recurse -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE)' }
        foreach ($license in $nativeLicenses) {
            $relative = [IO.Path]::GetRelativePath($nativeSource, $license.FullName)
            $target = Join-Path $licenseRoot ('mupdf-native/' + $relative)
            $null = New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target)
            Copy-Item -LiteralPath $license.FullName -Destination $target
        }
    }
}
$vswhere = 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
$installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
$runtime = Get-ChildItem -LiteralPath (Join-Path $installation 'VC/Redist/MSVC') -Directory | Sort-Object Name -Descending | ForEach-Object { Get-ChildItem -LiteralPath (Join-Path $_.FullName 'x64') -Directory -Filter 'Microsoft.VC*.CRT' -ErrorAction SilentlyContinue } | Select-Object -First 1
if (!$runtime) { throw 'Installed Visual Studio x64 redistributable CRT directory not found.' }
Get-ChildItem -LiteralPath $runtime.FullName -File -Filter '*.dll' | Copy-Item -Destination $destinationPath
Get-FileHash -LiteralPath (Join-Path $destinationPath 'pdfree.exe') -Algorithm SHA256 | Format-List
Write-Host "Portable release: $destinationPath"
