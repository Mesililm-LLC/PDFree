param([switch]$Release, [switch]$Wgpu, [string[]]$CargoArgs = @())
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
$env:LIBCLANG_PATH = Join-Path $root '.tools/python/clang/native'
$env:PATH = (Join-Path $root '.tools/python/cmake/data/bin') + ';' + $env:PATH
$env:MUPDF_MSVC_PLATFORM_TOOLSET = 'v145'
$env:CARGO_BUILD_JOBS = '4'
$env:BINDGEN_EXTRA_CLANG_ARGS = '-include "' + (Join-Path $root 'scripts/msvc-bindgen.h') + '"'
if (!(Test-Path -LiteralPath "$env:LIBCLANG_PATH/libclang.dll")) { throw 'Run scripts/bootstrap.ps1 first.' }
$arguments = @('build', '--locked')
if ($Release) { $arguments += '--release' }
if ($Wgpu) { $arguments += @('--features', 'pdfree/wgpu') }
$arguments += $CargoArgs
& cargo @arguments
exit $LASTEXITCODE
