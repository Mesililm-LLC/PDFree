# PDFree

An offline Windows 11 x64 PDF viewer and annotation prototype built with Rust, Slint, and MuPDF. It opens real PDFs with a virtualized continuous viewport, asynchronous clipped rendering, page navigation, and zoom/fit controls.

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

Encrypted PDFs show a masked password form with retry and cancel. Ctrl+F or Find opens case-insensitive search; Enter/Next and Previous move between highlighted matches. Pages toggles lazy thumbnails; click one to jump. Use Bookmarks to navigate document outlines. Click an internal link to jump to its destination, or a web link to open the default browser. The view picker offers Continuous, Single page, and Facing pages (1–2, 3–4). Drag across text and page boundaries, then press Ctrl+C or Copy selection. Drag near the viewport edge to scroll. Selection is limited to 16 pages, with page boundaries copied as blank lines; highlighting remains single-page. Copy restrictions are respected. Image-only pages have no selectable text; OCR is deferred. Passwords, queries, and copied text are not written to traces.

## Validation

Double-click the document filename in the header to rename the actual file in its current folder. Enter also saves the current highlights and notes and clears the Unsaved badge; undo/redo stays available. Confirming the current filename saves edits without moving the file. Escape cancels, and a missing `.pdf` extension is added. A conflict with another file or invalid Windows filename leaves the editor and your edits available.

Select text and choose Highlight, or choose Add note and click a page. Click a saved note to edit or delete it. Annotations lists notes and highlights on the current page; highlights offer yellow, green, blue, or pink and Delete. Locked annotations and truncated note previews are read-only.

Ctrl+S or Save commits the current PDF in place. Ctrl+Shift+S or Save As copy creates and opens a new file; existing targets are rejected. Opening another PDF or closing with unsaved changes offers Save, Discard, or Cancel. Filename changes also save current changes.

Manage pages accepts ranges such as `1-3,5`. Rotate or delete selected pages, move the current page to a new position, insert a blank page or PDF after the current page, or merge PDFs at the end. Extract writes selected pages to a new PDF. Split writes two PDFs into a chosen folder, dividing after the current page. At least one page must remain. Annotation and page changes share Ctrl+Z/Ctrl+Y undo/redo, including after saving; filenames and exported files are outside undo history.

Editing history retains up to 200 commands within 128 MiB of serialized snapshots. A change is rejected if its before/after pair cannot fit. This excludes native clones, serialization buffers, and renderer memory. Editing supports up to 100,000 pages; each import accepts at most 16 PDFs. Imported PDFs must allow copying and open without a password. Imports preserve supported links, named/internal destinations, annotation appearance streams, popup relationships, and standard AcroForm fields. Imported field names are namespaced; incompatible form resources, additional actions, XFA, signatures, calculation order, and unsupported actions are rejected transactionally. Form filling is not implemented. Bookmarks are not imported. Page assembly and annotation permissions are respected. Extract/split require copying and page assembly permission, remove bookmarks, and preserve encryption; original passwords remain necessary. Page extraction/deletion is not secure redaction: document metadata or other references can retain excluded content.

```powershell
./scripts/check.ps1
./scripts/benchmark.ps1 -Trials 20
```

Fixtures are checked in with provenance/hashes in `tests/corpus/manifest.json`. To regenerate original PDFs run `python scripts/generate-corpus.py`; to add the encrypted fixture and refresh the complete manifest install the pinned test helper and run:

```powershell
python -m pip install --target .tools/python pypdf==6.1.1
python scripts/complete-corpus.py
python scripts/reader-corpus.py
python scripts/navigation-corpus.py
python scripts/merge-corpus.py
```

Independent PDFs retain upstream copyrights and their recorded source URLs. Encrypted fixture password: `pdfree-test`.

Read [architecture](docs/architecture.md), [feature coverage](docs/feature-matrix.md), [benchmark results](benchmarks/results/REPORT.md), [manual validation boundaries](docs/manual-validation.md), and [backlog](docs/backlog.md). Performance targets are goals, not claims. Physical presentation timing, post-reboot cold trials, and broad independent-reader comparison require separate evidence.

Reader-tool test evidence and 20 updated warm trials: [reader tools validation](benchmarks/results/reader-tools.md).

Annotation, save-copy, and rename checks: [annotation validation](benchmarks/results/annotations.md). Run `python scripts/verify-annotations.py` after the UI tests and retained native test artifacts to inspect generated copies independently.

Native editing, direct save, and page-tool checks: [page tools validation](benchmarks/results/page-tools.md).

Bookmarks, links, reading modes, cross-page selection and interactive merging: [navigation validation](benchmarks/results/navigation.md).

## Known limits

The application raster cache is bounded at 128 MiB. MuPDF 0.8.0 fixes its native-store threshold at 256 MiB; the planned 64 MiB target remains unmet. Native allocations, Slint images, GPU resources, and live document/display-list objects are additional memory. Cancellation drops queued/stale work; active native calls finish. Malicious native inputs are not isolated in a separate process.

Search is capped at 10,000 matches and 512 UTF-8 bytes per query. Selectable text is capped at 100,000 characters per page, retained for at most 16 selected/nearby pages. Thumbnails retain at most 32 nearby rows. Existing text/image content editing, forms, OCR, redaction, signatures, and AI are not implemented. Annotation editing supports Text notes and Highlight annotations; other annotation types are preserved in the current document but have no editor. See the feature matrix for precise status.

## License

PDFree source: AGPL-3.0-only. MuPDF and mupdf-rs are AGPL licensed; Slint and other dependencies retain their own licenses. See LICENSE and THIRD_PARTY_NOTICES.md. No remote repository is configured by this prototype.
