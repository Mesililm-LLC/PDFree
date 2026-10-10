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
| Mouse selection / Ctrl+C | Implemented | Cross-page multiline Unicode text, up to 16 pages; edge autoscroll; respects COPY; no OCR |
| Single/facing views, outlines, links | Implemented, experimental | Indexed mixed-size layouts; bounded outlines/links; internal jumps and explicit HTTP/HTTPS clicks; unsupported actions reported |
| Highlights / sticky notes | Implemented, experimental | Native Text/Highlight edits and deletion; locked or truncated previews read-only |
| Undo / redo | Implemented | Shared annotation/page history, 200 commands / 128 MiB; filename changes and exports are separate |
| Save As | Implemented, experimental | New file only; opens the copy, leaves previous source unchanged; preserves encryption |
| Inline filename rename and save | Implemented | Enter commits current annotations and filename; another file is never replaced; undo history retained |
| Direct Save / Ctrl+S | Implemented, experimental | Verified sibling staging and Windows replacement with recovery backup |
| Rotate / move / delete / blank page | Implemented, experimental | Shared undo; must retain one page; ASSEMBLE permission |
| Insert / merge | Partial | Up to 16 unpassworded PDFs; supported links/destinations, popup/annotation graphs and AcroForm fields preserved; incompatible forms/actions rejected |
| Extract / split | Implemented, experimental | New files only; COPY and ASSEMBLE; bookmarks removed; not secure redaction |
| Forms / existing content editing | Unsupported in prototype | Separate editing milestone |
| Redaction, OCR, compression, conversion | Unsupported in prototype | Separate acceptance gates |
| Existing content editing / signatures | Unsupported in prototype | Separate fidelity and security research |
| AI, cloud, telemetry | Unsupported in prototype | No startup services or network runtime |
