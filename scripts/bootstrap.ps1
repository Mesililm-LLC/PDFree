$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
& python -m pip install --target .tools/python 'libclang==18.1.1' 'cmake==3.31.6'
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host 'Local build prerequisites ready. Visual Studio C++ Build Tools and Windows SDK are also required.'
