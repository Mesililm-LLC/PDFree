# Native editing and page tools

Approved by the user replying yes to the next milestone: direct saving, existing annotation editing/deletion, and page management. Routine implementation proceeds under the user autonomy instructions.

All mutations use coordinator-owned authenticated clones with unified bounded snapshot undo/redo. Support Text notes, Highlight colors and deletion; page rotation, moving, deletion, blank insertion, PDF insertion/merge, extraction and two-part split. Direct Save and rename use verified replacement with recovery backups. Save As creates and opens a new file. Existing target files are never overwritten by exports or Save As.

Permissions, annotation locks, stale revisions, page bounds and history budgets are enforced inside the adapter. Imports that cannot preserve annotations are rejected before changing the working document. Unsupported features remain explicit. Tests use public fixture copies only; private PDFs remain untouched. Validate core contracts, native transactions, software UI interactions, independent output parsing, lint, release build and portable smoke. Physical desktop interactions and cold performance remain separate validation tasks.
