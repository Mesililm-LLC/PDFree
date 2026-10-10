# Reader navigation and compatible imports

This milestone adds outline navigation, internal and HTTP/HTTPS links, continuous/single/facing views and mouse selection across pages. Continuous remains the default. Facing spreads pair pages 1–2, 3–4; mixed sizes remain top-aligned. Mode changes retain the current page. Existing annotations and save operations remain coordinator-owned.

## Implementation contracts

Native navigation objects stay in the adapter. Owned destinations, page-local rectangles and revision-stamped events cross the engine boundary. Requests coalesce and cap residency at 16 pages; links cap at 1,024 per page and outlines at 4,096 entries/depth 32. Unsupported PDF actions do not execute. Explicit web-link clicks open the Windows default browser.

Text dragging uses document coordinates so a captured pointer can cross page boundaries and scroll near viewport edges. Selection retains at most 16 pages and publishes clipboard text only after all required snapshots arrive. Page boundaries are separated by blank lines, in document order. Highlighting remains a single-page operation.

Imports preserve annotation object sharing, appearances, popup relationships, page destinations and supported form field trees. Imported field names are namespaced. Unsupported form/action structures must fail before installation. Undo and safe saving retain existing transactional guarantees. Importing forms preserves them; interactive form filling is a future feature.

## Execution and checks

1. Add engine-neutral navigation extraction and fixture tests.
2. Add indexed layouts and mixed-size/DPI/tile checks.
3. Integrate bookmark sidebar, web/internal link hit testing, mode picker and cross-page selection/autoscroll.
4. Preserve supported annotation/form graphs during merge and independently inspect exported results.
5. Run workspace tests, strict lint, formatting, release build and software UI captures; package the portable app. Native desktop input/browser launch and third-party appearance checks remain explicitly manual.
