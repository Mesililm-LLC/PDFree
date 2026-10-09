# Reader tools validation — 2026-10-09

Release executable SHA256: `C8A2444D626595F89224A71ACA5E69F995A0706BB1758DCA00DC216E9C94DF5D`.

Formatting, strict workspace Clippy, and all **27 tests pass**. Native tests cover authentication/retry, COPY restrictions, Unicode selection contracts, case-insensitive native search/cancellation, crop-origin normalization, blank pages, thumbnail dimensions, viewport-independent thumbnail completion, bounded caches/requests, and cached replay after coalesced page return. Existing rendering, replacement, malformed recovery, embedded-font, clipping, and 1,000-page checks pass.

The full app's software-platform test dispatches real Slint pointer/key events: it enters a search, selects real PDF text at 125% zoom and 150% DPI, copies with Ctrl+C into a mocked clipboard, and clicks a thumbnail to navigate. Password cancellation/retry and error recovery are exercised. Application-only renderer captures were inspected for masked password entry, search highlights, selection alignment, and thumbnails. Windows clipboard contents were not changed by automated tests.

The optimized executable also completed first-page/navigation/zoom smoke schedules on all 13 locally added private PDFs using the software renderer. Their hashes remained unchanged. Names, reports, and captures stay in ignored local run storage and are excluded from source and portable archives. This smoke check does not qualify search or reading order on every private document.

## Warm launch measurements

The same 20-page `office.pdf` and existing navigation/zoom schedule ran 20 fresh-process **warm** trials with the default Winit/FemtoVG configuration. Raw data and machine details: [reader-tools-warm-office.json](reader-tools-warm-office.json).

| Measurement | Result |
|---|---:|
| Open to first pixel publication, median | 249.3 ms |
| Open to first pixel publication, nearest-rank p95 | 390.5 ms |
| Open to first pixel publication, min–max | 184.4–505.0 ms |
| Rust main to window-show request, median | 447.3 ms |
| Largest sampled working set | 152.6 MiB |

Publication and window-show requests do not establish physical presentation timing. The 250 ms small-PDF goal is met only at the median in this warm sample; slower trials miss it. These warm runs cannot qualify the 300 ms cold-window goal. No cold trials, sustained-scroll frame-time qualification, or independent-reader comparisons were added. Earlier prototype results belong to a different executable; differences are observations, not a proven performance advantage.

Selection remains single-page. Cross-page dragging/autoscroll, OCR, editing, and saving are deferred. Human Windows pointer/clipboard/password interaction and complex reading order remain qualification work; see [manual validation](../../docs/manual-validation.md).
