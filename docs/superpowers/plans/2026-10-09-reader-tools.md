# Reader Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans inline; one fresh whole-branch review at the end.

**Goal:** Unlock encrypted PDFs, find and copy text, and browse lazy thumbnails.
**Architecture:** Extend coalesced coordinator commands; keep all document/text handles on that thread and thumbnail rasterization on existing workers. UI owns selection, highlights, clipboard actions, and bounded thumbnail/text models.
**Tech Stack:** Rust 1.97.1, Slint/slint-build 1.18.1, mupdf 0.8.0.
**Spec:** docs/superpowers/specs/2026-10-09-reader-tools-design.md

## Global constraints

- One offline document; no editing, OCR, telemetry, or remote actions.
- Keep private PDFs untracked; never sweep them into the shared fixture manifest.
- Passwords and extracted text must not enter traces or saved settings.
- Preserve viewport queue/cache/residency limits and source dependency pins.

## Review focus

- Old queries/thumbnails must not appear after replacement or document switching.
- Wrong passwords must leave a usable retry/cancel state and clear entered text.
- PDF copy restrictions must prevent clipboard copying.
- Selection coordinates must remain correct after zoom, DPI changes, and page-origin normalization.
- Large documents must not produce eager thumbnails or all-page text residency.

### Task 1: Engine-neutral selection contracts

Files: viewer-core/src/text.rs, viewer-core/src/lib.rs, viewer-core/tests/text.rs.
Interfaces: TextRect, TextGlyph, PageText, TextSelection; PageText::select(start,end).
- [ ] Write forward/reverse Unicode multiline selection and geometry tests; run `cargo test -p viewer-core --test text --target-dir target/core-tests` and observe missing-contract failure.
- [ ] Implement nearest-character selection, line boundaries, and merged line highlights; rerun the focused tests and existing core tests.

### Task 2: Coordinator reader services

Files: pdf-engine/src/lib.rs, coordinator.rs, reader.rs; tests/reader.rs.
Interfaces: open_with_password, search(document,id,query), text_pages(document,pages), thumbnails(document,pages); owned text/search/thumbnail events.
- [ ] Write native tests for correct/wrong authentication, extraction, search cancellation/case, copy permission, thumbnail dimensions, and replacement. Observe missing-API failure before implementation.
- [ ] Implement authentication and copy permission using published Document::authenticate and PdfDocument::permissions.
- [ ] Implement one-page-at-a-time search/text service; use TextPage::search_cb and plain normalized character geometry; cap text snapshots and search hits.
- [ ] Extend raster jobs with thumbnail identity independent of viewport generations; keep thumbnail display lists temporary. Test bounds/stale cleanup.
- [ ] Run native tests and original regressions with the existing native build environment.

### Task 3: Slint reader flows and portable verification

Files: pdfree/src/features.rs, app.rs, main.rs, ui/app.slint; docs, README, feature matrix.
- [ ] Add a software test platform and tests for password retry, search/highlights, actual pointer selection, clipboard copy, and lazy thumbnail state. Observe new-flow failures.
- [ ] Implement masked password form, Ctrl+F/search navigation, visible highlights, page-local drag selection/Ctrl+C, and collapsible virtual thumbnail sidebar. Clear feature state on open.
- [ ] Run formatting, strict Clippy and all tests; build release, exercise corpus/private files locally, and inspect app-only captures. Keep manual checks distinct.
- [ ] Fresh reviewer checks ownership, bounded residency, stale feature events, secrets/clipboard behavior, and acceptance claims; fix material findings.
- [ ] Commit only owned source/public fixtures, export source, package and verify extracted portable launch; document remaining limits.

## Execution ledger

- Preflight: clean tracked prototype; 13 untracked user PDFs preserved. Current dependency APIs inspected and Context7 consulted. Inline execution follows explicit user autonomy instructions.
