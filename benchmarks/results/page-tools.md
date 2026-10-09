# Native editing and page tools validation

## Scope

Direct Ctrl+S saving, saved Text note and Highlight editing/deletion, unified undo/redo, rotation/moving/deleting pages, blank/PDF insertion and merging, extraction and two-part splitting. Save As creates and opens the copy. Rename saves the current native document. Dependency versions are unchanged.

## Automated evidence

- 44 workspace tests pass, including coordinator transactions, stale annotation revisions, unsupported import rejection without partial mutation, split conflicts, encrypted Save As/rename/undo/direct Save, subset-export garbage collection, and blank pages inheriting the preceding page dimensions in mixed-size documents.
- The software Slint test exercises actual simulated key/pointer events for rename and Ctrl+S, saved note edit/delete, highlight recoloring/undo, dirty-close from Manage pages, page mutations/undo, typed export dialog responses, and reopening extracted/split files.
- Independent pypdf 6.1.1 parsing verifies page order/rotation, saved notes, encryption after undo and Save, subset page text/counts, unchanged sources, and removal of an unreferenced excluded-page annotation marker. This is structural verification, not a secure-redaction claim or broad visual certification.
- Renderer captures of renamed output and the page-tools modal at 680x480 logical pixels / 150% DPI were inspected. The modal controls and Close button remain visible. Captures are software-renderer evidence, not physical desktop interaction.
- Source review found and fixed a modal overlap that hid the unsaved-close prompt behind page tools. The regression is covered.

Machine logs and generated PDFs are under ignored `benchmarks/runs/page-tools-*` and `benchmarks/runs/annotations-tests`. Reproduce with `scripts/check.ps1`, `python scripts/verify-annotations.py`, and `python scripts/verify-page-tools.py`. Retained native artifacts require `PDFREE_PAGE_TEST_OUTPUT_DIR` set to the workspace's `benchmarks/runs/page-tools-native` directory. UI captures use `PDFREE_CAPTURE_DIR`.

The first UI attempts exposed two test assumptions: lexical versus canonical Windows paths, and pure yellow versus the actual amber-yellow original highlight. The assertions now compare canonical paths and restored original color. Native compilation also caught a Save As borrowing error, fixed before tests passed.

## Limits

The 128 MiB history budget counts serialized states, not total process memory. Import may duplicate shared font/image resources through the published binding and hit that budget. Unsupported imported annotations, encrypted import sources, imported bookmarks, and broader bookmark/internal-destination reconciliation remain deferred. Subset exports drop bookmarks. Existing Text/Highlight editing respects lock flags and permissions; truncated note previews are read-only.

Physical picker interaction, access-denied filesystem recovery, mixed-monitor behavior, broad independent-reader visual comparison, physical presentation timings and post-reboot cold trials remain unverified. No performance advantage is claimed. The native store target remains 256 MiB rather than the requested 64 MiB.

## Release evidence

Formatting checks, strict all-target workspace Clippy, and all 44 tests passed on the final source. The Windows x64 optimized build succeeded. Executable SHA256: `1B04732F10D1E7076F9BB37B768E2A35C72390BF425BB99767F0A1D75D1C183B`. Upstream native linker/compiler warnings remain in the build log; no dependency sources were modified.

A final review corrected blank insertion to inherit the preceding/current page dimensions instead of the following page; a mixed-size native regression covers it.

## Current warm measurements

Twenty warm fresh-process runs on the public 20-page office fixture used the final packaged executable. First pixel publication from document-open request: median **473.3 ms**, p95 **993.3 ms**, range 236.0–1080.8 ms. **17/20** runs exceeded the 250 ms small-PDF goal; the goal is not consistently met. Window-show request median was 824.6 ms from Rust main. These are image publication/window requests, not actual presentation.

Tile render latency median was 0.98 ms and p95 1.92 ms across published tiles. Sampled process peak working set ranged 135.1–151.2 MiB. This is total process working set, not separate native-store accounting. Raw samples, cache counts, environment and hashes: [page-tools-warm-office.json](page-tools-warm-office.json).

Startup varied substantially; the measurements do not isolate its cause. No matching Edge/Sumatra or WGPU comparison was run for this build. Zero new post-reboot cold trials were run; the 300 ms cold-window goal is unverified. Frame gaps include idle periods and are not sustained scrolling frame-time percentiles. Build processes had finished before timing; unrelated machine activity was not controlled.
