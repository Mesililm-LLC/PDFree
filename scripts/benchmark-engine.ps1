$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
& ./scripts/build.ps1 -Release -CargoArgs @('-p','pdf-engine','--example','cancellation_probe')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$null = New-Item -ItemType Directory -Force -Path benchmarks/results
& ./target/release/examples/cancellation_probe.exe | Set-Content -LiteralPath benchmarks/results/cancellation.json -Encoding utf8
exit $LASTEXITCODE
