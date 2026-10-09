# Annotation Editing and Rename Implementation Plan

> Execute with superpowers:executing-plans, scoped parallel workers, TDD, and one fresh final whole-branch review.

**Goal:** Add useful PDF annotations and safe copy saving, plus inline physical filename renaming.
**Architecture:** UI journal/overlays; coordinator-owned authenticated native save/rename; no overwrites.
**Tech Stack:** Rust 1.97.1, Slint/slint-build 1.18.1, MuPDF 0.8.0; dependency lock preserved.
**Spec:** docs/superpowers/specs/2026-10-09-annotations-rename-design.md

## Global constraints

Private PDFs remain untracked and unchanged. No remote actions, document content logging, plaintext password persistence, or direct UI native handles. Save As keeps the original open and unchanged, explicitly reporting an annotated copy. Actual rename is same-directory and must never replace another file. Bounds: 2,000 annotations, 16,384 note bytes, 1,000 rectangles, 200 history commands.

## Review focus

Reject existing targets and source aliases without a race. Preserve drafts on failed/cancelled saves, picker cancellation, rename, and document switches. Reject stale identities. Preserve encrypted outputs and permissions. Saved notes reopen with correct Unicode content and crop/DPI positions; repeated saves do not duplicate drafts.

### Task 1: Core editing contracts

Own viewer-core/src/editing.rs, exports, tests/editing.rs. Write RED tests for Annotation, EditHistory push/undo/redo/checkpoint/branch/bounds and validate_pdf_name, then implement and run focused tests. Export Annotation::{Highlight{page,rects},Note{page,x,y,text}}, page(), EditHistory and Windows filename validation.

### Task 2: Native copy saving and rename

Own pdf-engine source/tests and minimal Windows filesystem dependency. Add can_annotate, bounded save_as/rename commands, FileDone/FileError identities, lazy Notes snapshots. Test RED before production. Implement published APIs for clone, annotations, encrypted copy serialization/verification, atomic no-replace publication, rename/reopen/recovery. Run native tests; parent independently verifies saved outputs with pypdf.

### Task 3: Full app flows

Own pdfree editing.rs, app/main integration, Slint controls and UI tests. Add yellow highlights, placed editable note drafts, read-only native note popups, undo/redo, visible dirty/busy/error states, Save As picker and pending-open protection. Inline rename uses double click, focused filename editor, Enter/Escape and conflict feedback. Test real software-platform pointer/key callbacks and native file outputs; inspect app-only captures.

### Task 4: Review and deliver

Run fmt, strict Clippy, all tests, release build, fresh final review and fixes. Record limits/validation; commit only owned source/public fixtures. Rebuild source archive and portable ZIP, exclude private corpus, extract and launch-test. Retain manual Windows interaction and cold/presentation evidence boundaries.
