# Reader navigation and merge compatibility

2026-10-09; Windows x64; Rust 1.97.1, Slint 1.18.1, mupdf 0.8.0. Dependency versions are unchanged.

## Coverage

Native tests exercise nested outlines, internal/web/unsupported link classification, crop-relative coordinates, mixed-size single/facing geometry, clipping at different DPI, raw annotation relationships, shared appearance streams, direct/named destinations, Unicode form values and namespaced field trees. Import tests also exercise undo, resource-collision rollback, malformed/cyclic destinations, and indirect destinations following insertion/moves. An independent pypdf 6.1.1 inspector checks the saved merged file's structure and appearance stream bytes; this is not visual validation in a separate viewer.

Software UI tests use native text, captured pointer events, a mock clipboard and window scaling. They cover facing-page drag/copy in both directions, selection preservation, pointer release, bookmarks/internal navigation and the narrow layout, alongside prior password/search/annotation/save/page regressions.

## Validation results

- Formatting and strict all-target Clippy passed.
- Optimized Windows x64 release build passed (`benchmarks/runs/navigation-delivery-release.log`).
- 56 automated tests passed, including captured cross-page dragging, edge autoscroll/release, invalid gutter starts, pointer cancellation, existing save/annotation/page operations and native import graph regressions.
- Independent inspection passed for the saved merged file, including named/direct link destinations, popup identity, shared appearance streams/font resources and the Unicode form value.
- Software captures inspected at 1,180 × 850 and 680 × 480 logical pixels / 150% scaling. The narrow header retains Open and the view controls; Ctrl+C remains available while the Copy selection button is hidden.

Logs: `benchmarks/runs/navigation-delivery-checks.log`, `navigation-independent-final.log`. Captures: `benchmarks/runs/navigation-captures/`.

## Limits

- Selection retains at most 16 pages and 100,000 text characters per page. Clipboard text is withheld while a selected page is pending. Highlighting remains single-page; complex reading order and OCR remain future work.
- Outlines cap output at 4,096 entries/depth 32; the published MuPDF binding loads the native outline tree eagerly, so this is not a native-allocation cap. Links cap at 1,024 per page and 16 resident pages.
- Imported source outlines are not copied. Form filling is not implemented. Imports reject incompatible form resource names, XFA, signatures, calculation order, additional actions and actions outside URI/GoTo.
- Legacy numeric destinations in an existing document are not normalized on rearrangement; deleted-page destinations may become orphaned. Imported destinations are remapped to page references.
- Default-browser opening, physical Windows pointer/keyboard use, mixed-monitor behavior and appearance in independent viewers still require the manual checklist.
- This is a feature milestone. No new performance advantage, post-reboot cold timing or actual physical presentation result is claimed. Prior performance targets remain unmet/unqualified as documented.
