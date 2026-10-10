# Prioritized backlog

1. Finish live performance/fidelity qualification: post-reboot trials, actual presentation timing, sustained scroll frame times, comparative Edge/Sumatra runs, full manual pointer/keyboard/DPI testing.
2. Reach the native-store target through an upstream safe configurable-context API; measure UI/GPU allocation residency independently.
3. Improve interruptibility/isolation for hostile or extremely complex documents; retain the tested page-relative anchor during progressive geometry refinement.
4. Qualify reader navigation, cross-page selection/autoscroll, outlines, hyperlinks and single/facing views on complex layouts and languages. Add richer reading order, cross-page highlighting, and selection beyond 16 pages with bounded streaming.
5. Qualify direct saving and page operations in independent viewers across complex/encrypted documents. Broaden qualification of link/popup/form imports; support encrypted sources, source bookmarks and incompatible form resource reconciliation. Normalize legacy numeric destinations and repair links to deleted pages. Improve snapshot history costs for large PDFs.
6. Prototype existing-text/image editing, secure redaction, forms, signatures, and OCR individually, using semantic, structural, visual, and recovery checks.
7. Revisit direct GPU texture upload only after measurements identify upload/composition cost; compare the same viewport with PDFium if MuPDF remains a blocker.

8. Reuse one graft map per imported document to avoid duplicating shared fonts/images under the published insert_pdf implementation; measure import time and memory before relaxing the snapshot budget.
