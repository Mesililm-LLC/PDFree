# Annotation and inline rename validation

Windows 11 x64, Rust 1.97.1, Slint/slint-build 1.18.1, MuPDF 0.8.0. This milestone adds highlights, sticky notes, annotation undo/redo, Save As copies, and actual same-folder filename rename by double-clicking the header.

## Automated evidence

Formatting, strict workspace Clippy, all 34 workspace tests, and the optimized Windows release build pass. Native integration tests exercise original byte preservation, repeated copies without duplicate annotations, real Windows rename/conflicts/case-only names, encrypted rename/reopen and encrypted output, denied annotations, invalid geometry, saved Unicode notes and cached replay, and saved crop-origin note/highlight positions. Core tests exercise filename rules and bounded command history. The software Slint platform exercises double-click, Enter/Escape, clipped selected highlights, note validation, draft editing, undo/redo, pending-open/close protection and failed save preservation.

To reproduce native retained artifacts and independent inspection:

```powershell
$env:PDFREE_EDIT_TEST_OUTPUT_DIR = Join-Path (Get-Location) 'benchmarks/runs/editing-native'
./scripts/check.ps1
python scripts/verify-annotations.py
```

`scripts/verify-annotations.py` independently reads retained test output with pypdf 6.1.1, checks highlight and Text annotations, Unicode contents, encryption, and unchanged original bytes. Its machine-readable report lives in ignored `benchmarks/runs/annotations-verification.json`; native retained test artifacts live in `benchmarks/runs/editing-native/`, UI test files in `benchmarks/runs/annotations-tests/`. These files contain only generated public fixtures. All 13 private user PDFs still match the earlier local SHA-256 inventory; private names and documents are excluded from source and packages.

App-only software-renderer captures were inspected for inline filename editing, the renamed title and draft highlight, and the reopened Unicode note. Simulated input and a mocked clipboard do not establish physical Windows pointer/keyboard or picker behavior. Native filesystem transactions are real, against disposable fixture copies.

## Updated warm launch measurements

Twenty fresh-process warm runs of `office.pdf` with the default Winit/FemtoVG renderer are recorded in `annotations-warm-office.json`. Open-to-image-publication median: **232.9 ms**; p95 (nearest rank): **302.0 ms**; range: **171.3–307.3 ms**. Median window-show request from Rust main: **358.9 ms**. Largest sampled working set: **165.9 MiB**. The 250 ms small-PDF goal is missed at p95. The 300 ms window goal is also missed in this warm request measurement; no post-reboot cold-window conclusion follows from it. Image publication and a window-show request do not prove physical presentation. These runs do not measure sustained scrolling or annotation-save latency.

The optimized executable additionally reopened a saved annotated fixture and its actual native renderer capture showed the saved highlight and sticky-note icon. This verifies PDFree's rendering of the generated copy, not cross-reader visual fidelity.

## Limits

Save As writes a new copy and keeps the original open. It never overwrites an existing target. The original is not checkpointed on disk; subsequent drafts are saved in another complete copy. Existing notes are read-only. Filename changes are outside annotation undo/redo. Native save parsing/serialization is not hard-interruptible, and clone/write buffers are additional memory beyond cache/history accounting.

Physical double-click cadence, native Save As picker interaction, cross-reader visual appearance, filesystem access-denied recovery, and mixed-monitor behavior remain manual checks in `docs/manual-validation.md`. Cold trials and actual display-presentation timing remain unmeasured; this milestone makes no performance advantage claim.
