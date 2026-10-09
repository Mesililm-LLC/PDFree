$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
$env:LIBCLANG_PATH = Join-Path $root '.tools/python/clang/native'
$env:MUPDF_MSVC_PLATFORM_TOOLSET = 'v145'
$env:CARGO_BUILD_JOBS = '4'
$env:BINDGEN_EXTRA_CLANG_ARGS = '-include "' + (Join-Path $root 'scripts/msvc-bindgen.h') + '"'
& cargo fmt --all --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& cargo clippy --workspace --all-targets --locked -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& cargo test --workspace --locked
exit $LASTEXITCODE
