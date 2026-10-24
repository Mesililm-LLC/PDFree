# PDFree architecture and engine decision

## Goal and current scope

A personal, offline Windows 11 x64 PDF viewer, the first milestone of an editor. Startup, rendering latency, scrolling, zoom, and bounded raster residency come before editing features. The supplied research remains intact at the repository root.

## Decision

Rust 1.97.1 + Slint 1.18.1 + mupdf 0.8.0. Versions refer to published crates pinned in Cargo.lock, not upstream development branches. Use the Winit backend with FemtoVG, falling back to software; optional `pdfree/wgpu` adds a measurement configuration. CPU raster buffers are the baseline; direct GPU sharing is deferred until profiling demonstrates a need.

MuPDF is provisional: native Windows build, real clipped rendering, fidelity tests, and performance measurements govern acceptance. PDFium (`pdfium-render`) is the fallback/control candidate, not an integrated engine. Its serialized API would require a different adapter concurrency policy. No claim of Acrobat editing parity or measured advantage follows from engine selection.

## Boundaries and data flow

- `viewer-core`: PDF-point to logical-pixel geometry, DPI-aware 512-pixel tile keys, byte-budgeted LRU cache, bounded priority queue, and stale-result policy. No native or UI dependencies.
- `pdf-engine`: owns all native handles. A dedicated coordinator serializes document/page access; up to four workers rasterize completed immutable display lists. Published mupdf's thread-local contexts clone its shared locked base context. Draw devices and pixmaps remain worker-local.
- `pdfree`: owns Slint models/images, user actions, dialog coordination, traces, and display state. Plain RGBA buffers and owned text/search geometry cross into the UI. Images are constructed on the UI thread.

Open commands and viewport requests use replaceable latest-value slots, with a bounded wake channel. The coordinator's render queue holds at most 128 requests, and at most four jobs are in flight. The event queue is bounded at 64. Visible tiles outrank two neighboring pages. Each request carries document identity, generation, page, quantized physical scale, rotation, and clip bounds.

Opening reads page count and first-page geometry, then publishes readiness. Later page bounds are discovered one at a time between commands; distant pages begin with first-page estimates. Full-resolution page rasters are never eagerly allocated. Display lists are retained for visible pages plus two neighbors; outstanding workers may temporarily retain older lists.

Viewport/zoom generations invalidate queued results. Started native calls finish; this prototype does not claim hard interruption of parsing or rasterization. Shutdown joins owned threads after the event loop, and bounded channels disconnect to release blocked workers. PDFs remain untrusted native inputs; process isolation and fuzzing are future hardening work.

## Memory policy

Reader tools share the coordinator-owned PDF document. Authentication uses the published password API; COPY permission governs text snapshots and clipboard controls. Native search publishes owned page-local quadrilaterals as rectangles, with an independent search identity for cancellation. Text extraction retains at most 16 nearby snapshots of 100,000 glyphs each. Search caps at 10,000 grouped hits and 512 query bytes. Native page calls finish before cancellation takes effect.

The UI owns single-page drag selection, multiline copying, and scaled highlights. Clipboard copying uses Slint's read-only LineEdit copy API on the UI thread. Thumbnail rows are virtualized (196 logical pixels, at most 32 nearby rows), rasterized at up to 140×160 on existing workers; their document identity survives viewport generation changes. Temporary thumbnail display lists never enter the viewport residency map. Passwords and extracted/search/copied text are excluded from traces.

Raster cache: hard 128 MiB budget. UI image references are limited to the current viewport, with one previous visible set retained for up to 750 ms during zoom. Slint image copies, active worker buffers, page metadata, display lists, native store, fonts, and GPU allocations are additional memory and must be measured separately.

Deviation: mupdf 0.8.0 uses a fixed 256 MiB shared native-store threshold and has no public setter. The requested 64 MiB native target is **not achieved**. The threshold is not a process-memory cap and live native references can exceed it. Avoid layout-dependent access to private native handles merely to change this number.

## Annotation and file transactions

All native edits now run on the coordinator. Each command serializes the current PDF, authenticates an owned clone, applies a validated annotation/page operation, validates the result, and adds its serialized snapshot to a bounded revision journal before installing it. Undo/redo restores authenticated snapshots. The journal retains at most 200 commands / 128 MiB including current state; a before/after pair exceeding the budget is rejected without changing the document or history. Native clone/write buffers are additional memory. Existing legacy draft APIs remain for compatibility; the app uses native commands exclusively.

Content changes increment a separate content epoch, clear document/render/reader caches, and stamp jobs/results. Workers return results only to the coordinator, which emits UI events in order; stale content cannot publish after Changed. The UI clears raster/image, selection/search, annotation, and thumbnail models and requests fresh data. Annotations include revision and ordinal identity, so stale selections cannot target a different native annotation.

File commands share one noncoalescing bounded slot. Ctrl+S stages and verifies the current PDF, drains workers and releases file handles, and uses Windows ReplaceFileW with a recovery backup. Rename uses the same transaction and also checkpoints changes. Save As writes a new file with no-replace publication, switches the current path to the copy, and checkpoints history. Undo remains available after saving. Failed operations preserve edits, attempt rollback, and report recovery locations. Passwords remain session-only; encryption is retained.

Text notes and highlights support editing/deletion subject to ANNOTATE and annotation lock flags. Annotation previews retain at most 16 nearby pages, 256 items / 256 KiB text per page, 1,000 quads per highlight; truncated individual note previews are read-only. New notes accept 16,384 UTF-8 bytes. Page mutation requires ASSEMBLE, export requires COPY and ASSEMBLE; import copying requires COPY. Imports preflight all sources and reject annotation subtypes that the published graft API cannot preserve; supported Text/Highlight objects are reconstructed. Exports use cloned documents with selected pages retained, stage both split outputs before publication, and never overwrite existing files. Page deletion/extraction is not secure redaction; other references or metadata may retain excluded content. Bookmarks and destinations need broader qualification.

## Windows build findings

The installed Visual Studio 18 toolset is v145. mupdf-sys 0.8.0 defaults incorrectly to v142 on this installation; its documented environment override selects v145. MSVC's C headers omit `max_align_t`, though its C++ `cstddef` defines it as `double`. `scripts/msvc-bindgen.h` supplies the same definition and an allowlisted alias for binding generation only. Dependency sources and alignment assertions remain intact.

MuPDF default features are disabled; Base14 fonts are enabled. JavaScript, OCR, and other document format features are excluded at the Rust feature boundary. Upstream's Windows solution may still compile unused third-party projects; feature selection is not a claim that every native object or library is absent.

## Evidence and measurement

See `tests/corpus/manifest.json`, `scripts/check.ps1`, and `benchmarks/README.md`. Traces use monotonic time from entry to Rust main. `window_show_requested` is a request, `frame_rendered` is a renderer callback, and `first_page_pixels_published` is Slint model publication. None proves physical display presentation. A fresh-process warm trial is not a cold filesystem/driver trial.

Official references: [MuPDF threading](https://github.com/artifexsoftware/mupdf/blob/master/docs/reference/c/overview.md), [published Rust binding](https://docs.rs/mupdf/0.8.0/mupdf/), [Slint images](https://docs.slint.dev/latest/docs/rust/slint/struct.Image.html), [Slint renderer choices](https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backend_winit/).

## Forms and signing

The owned form model and isolated native adapter extend the shared journal and safe-save pipeline. Offline recognition uses a separate cancellable Tesseract process. Visual marks are ordinary annotation appearances. Certificate signing has a dedicated incremental path and never uses the full-rewrite save path. See [forms](forms.md) for contracts, limits, and qualification commands.
