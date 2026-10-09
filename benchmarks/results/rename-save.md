# Inline rename now saves annotations

The user requested that confirming the document filename also saves the current notes and highlights. Enter now commits the current annotation journal and clears Unsaved after success; confirming the unchanged name saves without moving the file. Undo/redo remains available. Existing unrelated destination files are never overwritten. Save As continues to write a separate copy.

Formatting, strict workspace Clippy, all **35 workspace tests**, and the optimized Windows release build pass. The UI regression initially failed with “renaming must save current annotations”, then passed after wiring rename-and-save and checkpointing the matching revision. Native tests reopen an encrypted renamed document, repeat same-name commits without duplicates, save after rename from the retained baseline, undo to zero annotations, reject conflicts without changing either file, and force a locked-source replacement failure while preserving original bytes.

`scripts/verify-annotations.py` independently inspects both the renamed PDF and Save As output with pypdf 6.1.1, checking native highlight and Unicode note contents. The Save As source is compared with its post-rename checkpoint, because rename now intentionally updates its contents. App software-renderer captures and a release launch smoke are retained under ignored `benchmarks/runs/` paths.

The Windows transaction stages and verifies output before moving the original, then uses [ReplaceFileW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew) with a reserved recovery backup. An authenticated, unannotated baseline stays in memory so later saves do not duplicate the retained UI draft journal. Rare partial replacement failures retain recovery data and report its path; those exceptional OS failure codes were reviewed against Microsoft documentation rather than directly injected.

This is a behavior correction; the earlier 20 warm-launch measurements describe the preceding executable. The new release launch smoke is not a new performance comparison. Physical Windows keyboard/picker checks and cross-reader visual qualification remain manual.
