# Annotation editing and inline rename

The user approved text highlights, sticky notes, undo/redo, and Save As, and requested double-click inline PDF filename renaming. Their AGENTS instructions authorize reversible implementation and routine decisions without additional approval. Execute this architectural extension inline with scoped parallel ownership, tests, and a fresh final review; do not introduce another approval handoff.

Keep the pinned Rust/Slint/MuPDF stack, coordinator ownership, bounded queues/caches, offline operation, and private corpus boundaries. Original supplied research remains unchanged.

## User behavior

- Highlight applies yellow markup to the current single-page text selection. Note lets the user click a page position and enter multiline text. Notes added in the session can be opened and edited; saved/native sticky notes can be opened read-only.
- A core annotation journal supports undo/redo and a saved checkpoint. Dirty state is visible. Opening another document prompts Save a copy / Discard / Cancel; cancelled dialogs and failed saves retain drafts. No autosaving or modification of source PDFs.
- Save As writes an annotated copy, with an explicit notice that the original remains open. This preserves undo/redo after saving and prevents duplicate annotations on repeated saves. Existing destination files, source aliases, and overwrite attempts are rejected. Temporary same-directory output is synchronized, reopened for verification, and published atomically without replacement. Preserve encryption and annotation permissions.
- Double-clicking the document name opens an inline editor, Enter commits, Esc cancels. Rename changes the actual filename in the same directory, preserving unsaved annotations and viewer position. Append .pdf when absent; reject invalid/reserved Windows filenames, separators, trailing dots/spaces, and existing destinations. Renaming/saving errors remain visible and leave files/drafts intact. No rename on blur or incidental focus changes.

## Architecture

Core owns Annotation, bounded EditHistory and Windows filename validation. UI owns draft highlights/note overlays and history. Native saved annotations remain part of baseline rendering; native note contents are exposed as owned, bounded nearby-page snapshots.

The coordinator serializes file operations through one bounded non-coalescing command slot. Operations carry document and operation identities; stale commands are rejected. Save clones the authenticated native baseline via published buffer APIs, applies only current drafts, writes/validates/publishes the copy, and leaves the baseline unchanged. The coordinator retains the session password only in memory for encrypted cloning/reopening; it never enters traces/settings.

Windows native file handles may prevent renaming: close the coordinator-owned source before atomic no-replace rename and reopen it afterward; recover the original when moving fails. Render workers retain immutable display lists only. Explicit failure handling is required if reopening fails after a successful move. No filesystem changes to user corpus files during development tests.

## Bounds and validation

Limit new annotations to 2,000/session, note text to 16,384 UTF-8 bytes, highlight rectangles to 1,000/annotation, and history to 200 commands. Require finite page-local geometry and valid page indices. Test undo/redo branching/checkpoints, filename rules, native saved annotation contents/geometry with an independent parser, encryption/restrictions, source/destination preservation, rename rollback/conflicts, UI double click/Enter/Escape, notes, pending-open protection, and stale completion handling. Run formatting, strict lint, full tests, optimized build, app-only capture inspection, and extracted portable launch. Report manual interaction limits separately.
