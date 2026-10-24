# Validation boundaries

The optimized portable executable was launched successfully. Its own renderer snapshots were inspected at 125% Windows scaling before and after page navigation/zoom. These show correctly placed PDF text, readable light-theme controls, and clipped zoomed output. Automated schedules execute real navigation/zoom callbacks; native integration tests cover replacement, malformed-file recovery, encryption detection, clipping, transparency, embedded fonts, and 1,000-page documents.

The following human interaction checks remain **unverified**, and must be completed before calling this a qualified viewer release. Automated callbacks are not evidence of physical pointer/keyboard behavior. This environment has no enabled native desktop interaction surface.

| Check | Procedure | Status |
|---|---|---|
| Native picker | Ctrl+O, choose PDF; cancel and reopen picker | Pending manual check |
| Continuous scrolling | Wheel/drag scrollbar rapidly through mixed.pdf and thousand.pdf | Pending manual check |
| Page entry | Type a distant page, Enter; edit the number without it being overwritten | Pending manual check |
| Keyboard | PgUp/PgDn, arrows, Home/End, Ctrl +/−, Ctrl+0 | Pending manual check |
| Zoom and fit | 25–400%, fit width/page while scrolled within a page | Automated callbacks/captures; physical interaction pending |
| Resize/DPI | Resize continuously, move between 100%, 125%, and 150% displays | Geometry tests and 125% capture; mixed-monitor interaction pending |
| Reopen/recovery | Open malformed, encrypted, then valid PDF through picker | Engine tested; full picker path pending |
| Rapid switching | Replace an in-flight vector/scan document repeatedly | Engine tested; full picker path pending |
| Fidelity | Compare independent PDFs with Edge/Sumatra at identical scale | PDF rendering tested; broad visual comparison pending |

The original viewer validation did not modify PDFs. The annotation milestone additionally writes copies and renames files only through explicit user actions. No file associations, telemetry, cloud services, or remote repository were configured.

## Reader tools milestone

The reader-tools suite passes 27 automated tests, including real native password retry and COPY restrictions, case-insensitive search/cancellation, cropped-page text coordinates, blank-page extraction, bounded/coalesced requests, and cached replay after rapid return. The full Slint app runs against a software test platform: simulated pointer events select PDF text at 125% zoom and 150% DPI, Ctrl+C writes to a mocked clipboard, a thumbnail click navigates, password cancellation/retry works, and malformed-file recovery succeeds. Renderer snapshots show selection and lazy thumbnails aligned with the PDF. Automated clipboard tests do not overwrite the Windows clipboard.

Physical mouse dragging and Windows clipboard pasting, complex/multicolumn/RTL reading order, password entry through a real keyboard, and scrolling the thumbnail sidebar still need human qualification. Single-page selection is implemented; cross-page dragging/autoscroll and OCR are deferred. Prior viewer checks above retain their stated evidence boundaries.

## Annotation and rename checks

Use disposable copies when testing actual filename changes. Double-click the header filename, confirm with Enter, cancel with Escape, and try a name that already exists. Add a highlight and a Unicode/multiline note; edit the note, undo and redo, then Save As to a new path. Reopen the saved copy in PDFree and an independent reader and inspect geometry and note contents. Repeat with an encrypted fixture. The original bytes should remain unchanged by Save As.

Inline rename now saves the current annotations too. Add notes, confirm a new filename, and verify the Unsaved badge clears and the renamed PDF contains them after reopening. Confirm the same filename to save without moving it. Undo after rename should mark drafts unsaved again; redo restores the checkpoint. A conflicting name or failed file replacement must retain the annotations and report the failure.

Try opening another PDF and closing while drafts are unsaved; verify Save As, Discard and Cancel, including cancelling the native save picker and a save conflict. Resize and zoom with draft overlays. Test denied filesystem access on a disposable folder. Native file-picker interaction, physical double-click cadence, cross-reader appearance, access-denied recovery and mixed-monitor behavior require human qualification.

## Native editing and page tools milestone

This milestone adds direct Ctrl+S saving and native page/annotation changes. Use disposable PDF copies for qualification. Check saved note edit/delete and highlight colors, undo/redo before and after Save, Save As switching to the new copy, and rename saving all current changes. Close a dirty document from Manage pages and verify the Save/Discard/Cancel prompt stays visible.

Try page ranges, rotation, moving, deletion, blank insertion, PDF insertion/merge, extraction and two-part split. Verify unsupported imports and output conflicts report errors without changing the current PDF or overwriting files. Subset exports remove bookmarks and are not secure redaction. Imports containing links, forms, popups or unsupported annotations are intentionally rejected.

Automated software-platform tests simulate UI events; they do not certify physical Windows picker interaction, file-permission failure recovery, mixed-monitor behavior, or cross-reader appearance. These remain manual qualification tasks.


## Navigation and reading modes

- Open navigation.pdf and a PDF with a large outline. Toggle Bookmarks, follow nested entries, and return through page entry. PDFs without outlines should show an empty message.
- Click internal and web links; web links should open the default browser only on an explicit click. Drag across linked text to select it without opening the link. Unsupported actions should show a message.
- Switch Continuous, Single page and Facing pages; verify page jumps, keyboard next/previous, odd final pages, fit width/page and mixed page sizes.
- Drag across adjacent pages in both directions and use Ctrl+C. Drag near the viewport edge to scroll; releasing must stop scrolling. More than 16 pages should produce an explicit limit message and no partial copy.
- Resize to 680 × 480 with Bookmarks open, then move between monitors with different scaling. Open and view controls must remain reachable; Ctrl+C works while the Copy selection button is hidden on narrow windows.
- Merge interactive-merge.pdf into an owned blank copy, save, reopen in Edge or another independent viewer, and inspect links, note/popup, square appearance and form value. PDFree preserves imported form fields but does not fill them. Try an unsupported source and verify no partial insertion or history loss.

## Forms and signatures

Use generated/disposable fixtures. In Fill Form, type Unicode, tab across linked widgets and page boundaries, choose options, and toggle checkbox/radio values. Confirm one undo entry per completed text edit. Save and reopen in PDFree and an independent reader; compare values and visible appearances. Repeat with an encrypted form that permits filling but denies annotation/editing.

In Prepare Form, draw every supported type. Move and resize at 25%, 125%, and 400% zoom, with rotated/cropped pages and different device scaling. Apply labels, choice options, required/read-only settings, Date/Number formats, duplicate/delete, undo/redo, and save checkpoints. Try converting an incompatible nonempty value; it must remain available until explicitly resolved or cleared. Imported complex/action-driven fields must stay fill-only/read-only.

Run Detect Fields on native, scanned, and mixed pages. Check progress/cancel, page scopes, one-step undo, uncertain review navigation across pages, type/geometry correction, confirmation/deletion, and rerun preservation. Remove OCR dependencies from a disposable portable copy and confirm manual drawing still works after the explicit OCR error. Use separate labeled English fixtures to assess detection quality beyond the generated set; do not use private PDFs in automated artifacts.

Create typed, drawn, and imported Visual signatures. Preview, place, resize, replace, clear, undo, save, reopen, rotate, and print. Inspect output in an independent reader. Distinguish these marks from certificate integrity. Confirm a fresh app session retains no reusable image/stroke library.

With a disposable local PKCS#12 file, save edits, choose a Digital Signature field, and sign to a new output. Cancel password/pickers/signing, enter a wrong password, choose an existing target, and alter the source externally after review. Failures must preserve the source and publish no output. Independently verify untampered output and rejection after modification. Integrity status must not claim certificate trust. Signed/XFA/scripted inputs must not become editable through another tool or Save.

Physical picker/keyboard cadence, mixed-monitor behavior, printing, independent-reader interactivity, and diverse real-world form layouts remain human qualification tasks. Automated callback and independent-parser/render checks retain their exact documented limits.
