# PDFree

An offline Windows 11 x64 PDF viewer prototype built with Rust, Slint, and MuPDF. It opens real PDFs with a virtualized continuous viewport, asynchronous clipped rendering, page navigation, and zoom/fit controls. This milestone does not edit or save documents.

## Build and run

Requirements: Rust 1.97.1 MSVC, Python 3, Visual Studio C++ Build Tools with the v145 toolset and a Windows SDK. Native compilation can take several minutes. The build uses workspace-local libclang 18.1.1 and CMake 3.31.6; it does not edit dependency sources or system toolchains.

```powershell
./scripts/bootstrap.ps1
./scripts/build.ps1 -Release
./target/release/pdfree.exe
./target/release/pdfree.exe ./tests/corpus/office.pdf
```

The first build downloads public Rust dependencies. The application itself has no network services, telemetry, OCR models, JavaScript, or cloud requirements. Use the portable folder produced by `scripts/package.ps1` for local deployment; source and the lockfile remain in this repository.

## Controls

Open PDF or Ctrl+O opens the native picker. Use the scrollbars or wheel to browse, enter a page number to jump, and use toolbar controls to zoom or fit the page. Ctrl +/− changes zoom, Ctrl+0 fits width, PgUp/PgDn changes page, arrows scroll, and Home/End navigates to the beginning/end. Zoom is limited to 25–400%.

Encrypted PDFs show a masked password form with retry and cancel. Ctrl+F or Find opens case-insensitive search; Enter/Next and Previous move between highlighted matches. Pages toggles lazy thumbnails; click one to jump. Drag across text within a page, then press Ctrl+C or Copy selection. Copy restrictions are respected. Image-only pages have no selectable text; OCR and selection across page boundaries are deferred. Passwords, queries, and copied text are not written to traces.

## Validation

```powershell
./scripts/check.ps1
./scripts/benchmark.ps1 -Trials 20
```

Fixtures are checked in with provenance/hashes in `tests/corpus/manifest.json`. To regenerate original PDFs run `python scripts/generate-corpus.py`; to add the encrypted fixture and refresh the complete manifest install the pinned test helper and run:

```powershell
python -m pip install --target .tools/python pypdf==6.1.1
python scripts/complete-corpus.py
python scripts/reader-corpus.py
```

Independent PDFs retain upstream copyrights and their recorded source URLs. Encrypted fixture password: `pdfree-test`.

Read [architecture](docs/architecture.md), [feature coverage](docs/feature-matrix.md), [benchmark results](benchmarks/results/REPORT.md), [manual validation boundaries](docs/manual-validation.md), and [backlog](docs/backlog.md). Performance targets are goals, not claims. Physical presentation timing, post-reboot cold trials, and broad independent-reader comparison require separate evidence.

Reader-tool test evidence and 20 updated warm trials: [reader tools validation](benchmarks/results/reader-tools.md).

## Known limits

The application raster cache is bounded at 128 MiB. MuPDF 0.8.0 fixes its native-store threshold at 256 MiB; the planned 64 MiB target remains unmet. Native allocations, Slint images, GPU resources, and live document/display-list objects are additional memory. Cancellation drops queued/stale work; active native calls finish. Malicious native inputs are not isolated in a separate process.

Search is capped at 10,000 matches and 512 UTF-8 bytes per query. Selectable text is capped at 100,000 characters per page, retained for at most 16 nearby pages. Thumbnails retain at most 32 nearby rows. Annotations, editing, forms, saving, OCR, redaction, signatures, and AI are not implemented. See the feature matrix for precise status.

## License

PDFree source: AGPL-3.0-only. MuPDF and mupdf-rs are AGPL licensed; Slint and other dependencies retain their own licenses. See LICENSE and THIRD_PARTY_NOTICES.md. No remote repository is configured by this prototype.
