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

The UI owns engine-neutral highlight and note drafts and a bounded revision journal. Native baseline documents stay unchanged: Save As clones the baseline in memory, applies the current draft exactly once, retains encryption, writes and synchronizes a temporary file in the destination folder, reopens it, then publishes with a no-replace Windows move. The original document remains open; successful copies checkpoint the current revision. Repeated saves therefore do not duplicate drafts, and undo remains available after saving.

Save and rename share one noncoalescing pending slot and carry document and operation identities. The coordinator owns the password only for the current session to authenticate encrypted clones/reopens. Confirming inline rename also commits the current annotations: prepare and verify an annotated sibling, retain an authenticated unannotated baseline in memory, drain display-list workers, release file handles, move the original without replacing another file, and replace that moved original using Windows ReplaceFileW with a recovery backup. Confirming the unchanged filename commits edits without moving it. Failures preserve drafts and attempt rollback; uncertain recovery reports the actual path and retains the backup. Successful rename checkpoints the UI journal, preserving undo/redo; subsequent saves apply current drafts to the in-memory baseline exactly once. Filename changes themselves remain outside undo/redo. Unsaved drafts require Save As, Discard, or Cancel before opening another file or closing.

Existing Text annotations are loaded lazily for at most 16 nearby pages, 256 notes and 256 KiB of note text per page, with truncation disclosed. They are read-only in this milestone. New drafts are limited to 2,000 annotations, 1,000 rectangles per highlight and 16,384 UTF-8 bytes per note; history retains at most 200 commands and 32 MiB of snapshot accounting. This accounting excludes native clone/write buffers and UI models.

## Windows build findings

The installed Visual Studio 18 toolset is v145. mupdf-sys 0.8.0 defaults incorrectly to v142 on this installation; its documented environment override selects v145. MSVC's C headers omit `max_align_t`, though its C++ `cstddef` defines it as `double`. `scripts/msvc-bindgen.h` supplies the same definition and an allowlisted alias for binding generation only. Dependency sources and alignment assertions remain intact.

MuPDF default features are disabled; Base14 fonts are enabled. JavaScript, OCR, and other document format features are excluded at the Rust feature boundary. Upstream's Windows solution may still compile unused third-party projects; feature selection is not a claim that every native object or library is absent.

## Evidence and measurement

See `tests/corpus/manifest.json`, `scripts/check.ps1`, and `benchmarks/README.md`. Traces use monotonic time from entry to Rust main. `window_show_requested` is a request, `frame_rendered` is a renderer callback, and `first_page_pixels_published` is Slint model publication. None proves physical display presentation. A fresh-process warm trial is not a cold filesystem/driver trial.

Official references: [MuPDF threading](https://github.com/artifexsoftware/mupdf/blob/master/docs/reference/c/overview.md), [published Rust binding](https://docs.rs/mupdf/0.8.0/mupdf/), [Slint images](https://docs.slint.dev/latest/docs/rust/slint/struct.Image.html), [Slint renderer choices](https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backend_winit/).
