$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
if (!$env:LIBCLANG_PATH) { $env:LIBCLANG_PATH = Join-Path $root '.tools/python/clang/native' }
$env:MUPDF_MSVC_PLATFORM_TOOLSET = 'v145'
if (!$env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS = '1' }
if (!$env:BINDGEN_EXTRA_CLANG_ARGS) { $env:BINDGEN_EXTRA_CLANG_ARGS = '-include "' + (Join-Path $root 'scripts/msvc-bindgen.h') + '"' }
if (!$env:PDFREE_OCR_DIR) { $env:PDFREE_OCR_DIR = Join-Path $root '.tools/forms/ocr' }
& cargo fmt --all --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& cargo clippy --workspace --all-targets --locked -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& cargo test --workspace --locked
exit $LASTEXITCODE
