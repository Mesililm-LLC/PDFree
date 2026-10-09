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
