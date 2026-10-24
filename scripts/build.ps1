param([switch]$Release, [switch]$Wgpu, [string[]]$CargoArgs = @())
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
if (!$env:LIBCLANG_PATH) { $env:LIBCLANG_PATH = Join-Path $root '.tools/python/clang/native' }
$env:PATH = (Join-Path $root '.tools/python/cmake/data/bin') + ';' + $env:PATH
$env:MUPDF_MSVC_PLATFORM_TOOLSET = 'v145'
if (!$env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS = '1' }
if (!$env:BINDGEN_EXTRA_CLANG_ARGS) { $env:BINDGEN_EXTRA_CLANG_ARGS = '-include "' + (Join-Path $root 'scripts/msvc-bindgen.h') + '"' }
if (!(Test-Path -LiteralPath "$env:LIBCLANG_PATH/libclang.dll")) { throw 'Run scripts/bootstrap.ps1 first.' }
$arguments = @('build', '--locked')
if ($Release) { $arguments += '--release' }
if ($Wgpu) { $arguments += @('--features', 'pdfree/wgpu') }
$arguments += $CargoArgs
& cargo @arguments
exit $LASTEXITCODE
