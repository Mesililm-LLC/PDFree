# PDFree viewer prototype implementation plan

Authority: the user-approved PDFree Windows PDF Viewer Prototype plan in this conversation.
Execution: inline implementation, followed by a fresh whole-project review.

1. Establish pinned Rust workspace and reproducible local native build prerequisites.
2. Test and implement engine-neutral geometry, tile cache, bounded priority scheduling, and generation checks in viewer-core.
3. Test and implement MuPDF coordinator, clipped parallel display-list rendering, progressive geometry, bounded residency, and shutdown in pdf-engine.
4. Integrate Slint viewport, native Open, keyboard navigation, fit/zoom, state handling, and instrumentation in pdfree.
5. Generate a provenance-tracked corpus, add independent samples, and run release checks and reproducible benchmark trials.
6. Review behavior and final diff, fix findings, package portable release, and document measured results and unverified acceptance cases.

Constraints: Windows 11 x64; Rust 1.97.1; Slint/slint-build 1.18.1; mupdf 0.8.0; no JavaScript, OCR, cloud, telemetry, original-file writes, remote repository, or file associations.
Review focus: stale document completions, high-DPI coordinate mapping, huge page allocations, parser failures, and native cleanup during rapid document changes.
Tests: mixed-size/DPI viewport geometry; clipping; LRU byte accounting; queue overflow and priority; stale generations; real render pixels; encrypted/malformed files; repeated open/close; thousand-page lazy loading.
