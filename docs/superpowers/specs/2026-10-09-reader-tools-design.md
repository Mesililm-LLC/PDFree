# Reader tools milestone

User scope: add password entry, text search/copy, and thumbnails. The user explicitly chose mouse selection with Ctrl+C. Keep one offline document, pinned published dependencies, native ownership in the adapter, and original PDFs unchanged. Existing personal PDFs remain untracked and outside public manifests/results.

Extend the current coordinator rather than introducing another document owner or eagerly extracting every page. Password attempts reopen/authenticate on that thread; the masked input clears immediately, and neither passwords nor extracted text enter traces. Search is case-insensitive MuPDF structured-text search, progressive across pages, cancellable by query/document identity, with previous/next navigation and visible highlights. Cap results at 10,000, query input at 512 UTF-8 bytes, and report skipped pages and limits.

Selection uses engine-neutral character geometry for visible pages. Drag selects reading-order text across lines within one page; Ctrl+C and a Copy selection action copy that text. Cross-page dragging and selection autoscroll are deferred explicitly. Respect the PDF copy permission. Image-only pages have no selectable text; OCR remains absent.

Thumbnails occupy a collapsible left sidebar. Only sidebar-visible rows plus two neighbors request small complete-page rasters (140 by 160 maximum pixels). Use existing raster workers, prioritize viewport work, and release offscreen thumbnail images. Thumbnail lists cannot expand the retained native display-list cache. Plain RGBA reaches Slint only on the UI thread.

Approaches considered: eager all-page indexing/previews (simple but defeats large-document bounds); independent native background owners (breaks coordinator ownership); extend latest-value coordinator requests (chosen, preserves current invariants). Keep text/feature helpers separate from the existing viewport implementation.

Acceptance: correct/rejected passwords and cancellation; Unicode/multiline forward/reverse selection; native search hits/case handling, empty/cancelled queries and document replacement; bounded lazy thumbnails and stale result rejection; UI highlights, masked password retry, selection/copy, and thumbnail navigation; original regression suite, strict lint, optimized build, app captures and portable package.

Ruling: the user's explicit bias toward implementation and asking only consequential product questions supersedes generic repeated approval handoffs. Copy behavior was clarified before implementation; other choices preserve existing architecture. Implement inline and request one fresh final review under executing-plans.
