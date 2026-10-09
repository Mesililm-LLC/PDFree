# Feature coverage

Statuses describe application behavior, not everything MuPDF could support.

| Capability | Status | Evidence / limit |
|---|---|---|
| Native Open dialog / command-line PDF | Implemented | Background dialog, coordinator-owned document |
| Continuous view and page navigation | Implemented | Virtual page/tile models; page entry and shortcuts |
| Zoom 25–400%, fit width/page | Implemented | DPI-aware tiles; temporary scaled previews |
| Background rendering / bounded queue | Implemented | Coordinator plus up to four workers |
| Raster cache 128 MiB | Implemented | Byte accounting and LRU tests |
| Native store 64 MiB target | Blocked | Published binding fixes threshold at 256 MiB |
| Large-document geometry | Partial | Progressive estimates; distant bounds may adjust scroll layout |
| Cancellation | Partial | Coalesced requests and stale discard; active native calls finish |
| Encrypted PDF detection | Implemented | Password-required state and regression test |
| Password entry | Implemented | Masked input, retry/cancel; no password persistence |
| Malformed-file error / next-open recovery | Implemented | Native integration test |
| Rendering fidelity | Experimental | Corpus and clipping reference test; broad visual qualification remains |
| Performance advantage | Experimental | Must be established by comparable measurements |
| PDFium fallback | Unsupported in prototype | Documented alternative; adapter not built |
| Thumbnails | Implemented | Lazy sidebar, at most 32 nearby rows; existing render workers |
| Text search | Implemented | Case-insensitive native search, highlights and next/previous; 10,000-match limit |
| Mouse selection / Ctrl+C | Implemented | Single page, multiline Unicode text; respects COPY permission; no OCR |
| Single/facing views, outlines, links | Unsupported in prototype | Viewer foundation backlog |
| Highlights / sticky notes | Implemented, experimental | Session drafts, editable new notes, existing notes read-only; native annotations in saved copies |
| Undo / redo | Implemented | Annotation history only, 200 commands / 32 MiB; filename changes are separate |
| Save As | Implemented, experimental | New file only; original remains open and unchanged; preserves encryption |
| Inline filename rename | Implemented | Double-click header, Enter/Escape; real same-folder rename, no overwrite |
| Page editing / forms / existing content editing | Unsupported in prototype | Separate editing milestone |
| Redaction, OCR, compression, conversion | Unsupported in prototype | Separate acceptance gates |
| Existing content editing / signatures | Unsupported in prototype | Separate fidelity and security research |
| AI, cloud, telemetry | Unsupported in prototype | No startup services or network runtime |
