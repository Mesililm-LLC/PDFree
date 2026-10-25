# Forms implementation ledger

Approved specification: the five-phase forms plan supplied in this chat on 2026-10-23.

Order: existing AcroForms and transactional filling; manual authoring and semantic types; local scanned/vector detection and review; visual signature capture; incremental certificate signing.

Shared interfaces: owned field/widget identities and revision-stamped operations feed the existing coordinator, snapshot journal, appearance rendering, and verified saving. Detection creates the same field definitions as manual drawing. Visual slots are distinct from cryptographic signatures.

Existing checkout at D:/Dev/PDFree and its private/untracked corpus are preserved. Implementation is in D:/Dev/PDFree-forms-recovered on codex/forms-integration.

## Progress

- Preflight: existing widget enumeration/value APIs are available; authoring requires a public PdfObject schema adapter. FORM and EDIT permissions must be added. Existing full-rewrite saving cannot preserve digital signatures.
- Preflight: OCR must run separately and ship English data; detection must never replace source content or insert an OCR text layer.
- Phases 1–5: implemented as experimental workflows; final recovery validation and release qualification remain in progress.
- Detection acceptance is not yet met. Do not treat feature availability as release qualification.

## Evidence and decisions

Results will be appended as checks run. No phase is qualified solely by compilation or mocked callbacks.

- Native checks: field creation/filling and Unicode values survive save/reopen; catalog field membership, checkbox/choice/list/radio values, conversion and undo pass focused integration tests.
- UI callback check: drawing, filling, Date conversion and undo pass on the real engine/software UI platform.
- Detection checks: digital and scanned samples create fields atomically; repeat runs add no duplicate fields. Offline Tesseract 5.5.0 with tessdata_fast English 4.1.0 is staged with SHA-256 pins.
- Digital signing check: an incremental output preserves the original revision byte-for-byte, passes Windows integrity verification and independent OpenSSL CMS verification, detects tampering and opens read-only. Certificate trust is explicitly not evaluated.
- Ruling: use Windows CryptoAPI with PKCS12_NO_PERSIST_KEY and cached key-context access for signing — avoids persisting private keys or bundling a cryptographic provider — costs Windows-only certificate signing (matching the existing platform).
- Ruling: the binding deep-copies direct PdfObject dictionaries; the authoring adapter installs an indirect AcroForm dictionary before mutating Fields — avoids orphan widgets and broken radio groups — no product behavior change.
- Qualification still pending: held-out detection scores including skew/low contrast/tables/mixed pages; complete UI flows and captures; encryption/restriction/signing-failure cases; strict lint/full suite; portable release and independent PDF parsing/rendering.

- Whole-branch review identified and regression tests reproduced failures in populated read-only fields, imported radio-group extension, locked marks, unsorted multiselect indices, overwritten inherited appearance defaults, draft retention, widget reselection, review-mode reset, and failed-operation continuations. Fixes retain widget identities and operation-specific permissions.
- Ruling: read declared Encrypt/P bits through the object adapter because mupdf-rs 0.8.0 maps unknown reserved permission bits to all permissions. PDFree conservatively honors declared restrictions even when an owner password was supplied. This prevents reserved bits from bypassing fill/authoring gates and also protects existing editing operations.
- Ruling: bundle mupdf-fonts-droid 0.8.0 for portable CJK fallback. A Unicode linked-widget regression reproduced empty appearances without it; generated appearances now survive save/reopen.
- Ruling: bind signing to a cryptographic source digest captured at open/save, and parse those exact bytes for the incremental output. Same-length replacement with restored mtime is covered by a failing-then-passing regression.
- Native qualification: 23 engine unit tests plus 5 focused form/signing integration tests pass. Includes encrypted fill-only restrictions, linked widgets without P, Unicode appearances, malformed/scripted/XFA read-only behavior, rotated/cropped visual marks at two scales, sorted selections, radio trees, read-only conversion, unchanged source, wrong PKCS#12 password, cancellation, conflict, encrypted incremental signing, OpenSSL CMS verification, tamper rejection, and read-only reopen.

- Recovery: an external stale-worktree cleanup deleted the active C: checkout during validation. Replayed the session's source edits into D:/Dev/PDFree-forms-recovered and saved local Git checkpoint 03e623e on codex/forms-integration. Original D:/Dev/PDFree and private corpus remain untouched. Validation is rerun from the recovered source; earlier results are not treated as recovery validation.

- Recovery validation: 24 engine unit tests and both native/scanned detection integrations pass again. Adding raster round outlines regressed qualification precision (85.5%, high-confidence 93.2%) by accepting letters in short OCR words. Tightened reciprocal word occupancy and require explicit grouping for round candidates. Reserved indices 48–71 for the next untouched synthetic qualification; indices 24–47 are now regression/development evidence.

- Appearance regression: direct list-value dictionary edits did not notify native appearance synthesis. Public rectangle setters now notify only affected text/choice widgets without changing geometry. MuPDF's list appearance generator also has no selected-item highlighting; the adapter generates Unicode selection markers through a temporary text appearance, then restores the standard list schema and values. A before/after appearance regression covers this behavior.

- Recovery qualification on synthetic indices 72–95: precision 88.03%, recall 86.81%, high-confidence precision 99.14%, type accuracy 99.20%. Overall precision fails the 90% gate. Short open-line fragments in skewed tables dominate false detections; the next change filters those fragments. This partition is now consumed regression evidence, not a fresh holdout.

- Table-fragment filter regression on consumed indices 72–95 passes: precision 95.42%, recall 86.81%, high-confidence precision 99.14%, type accuracy 99.20%. Fresh indices 96–119 are reserved for final acceptance. Recovered native unit tests: 25 passed. The forms UI callback regression now passes, including correct radio widget reselection.

- Fresh indices 96–119 met the requested detection precision/recall/high-confidence gates (90.84%, 82.64%, 98.59%) but failed the additional semantic-type gate (75.63%). Label lookup was bounded to 180 points, shorter than these fixtures' label-to-field gaps. Extend same-row lookup and second-pass crop to 300 points, retaining nearest-label ordering and row constraints. This set is now consumed; reserve indices 120–143. Formatting and strict workspace Clippy passed before this adjustment.

- Final recovery checks: cargo fmt --all --check and strict workspace/all-target Clippy pass. All 85 workspace tests pass. Fresh synthetic indices 120–143 pass: precision 95.87%, recall 80.56%, high-confidence precision 99.14%, semantic-type accuracy 100%. Independent pypdf 6.1.1 parsing and PDFium screen/print rendering pass for seven exported public fixtures, including encrypted fills/signatures, linked Unicode widgets, list selections, display/export choices, and rotated visual signatures. Portable release build/smoke remains pending.

- Portable qualification: optimized Windows build and package pass. Bundled OCR detection passes with no directory override; packaged release opens linked Unicode widgets, navigates/zooms, captures a correct native render, and exits 0. Executable SHA-256: 75b9f813b7faa7e866353f36c2f4c8cdcaa712a9e0332e58b9019bacf03e00f9. Automated implementation/acceptance work is complete; manual printing, pickers, accessibility, mixed-monitor and broad real-world reader/corpus checks remain explicit release boundaries.
