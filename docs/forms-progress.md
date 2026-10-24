# Forms implementation ledger

Approved specification: the five-phase forms plan supplied in this chat on 2026-10-23.

Order: existing AcroForms and transactional filling; manual authoring and semantic types; local scanned/vector detection and review; visual signature capture; incremental certificate signing.

Shared interfaces: owned field/widget identities and revision-stamped operations feed the existing coordinator, snapshot journal, appearance rendering, and verified saving. Detection creates the same field definitions as manual drawing. Visual slots are distinct from cryptographic signatures.

Existing checkout at D:/Dev/PDFree and its private/untracked corpus are preserved. Implementation is in the attached managed worktree.

## Progress

- Preflight: existing widget enumeration/value APIs are available; authoring requires a public PdfObject schema adapter. FORM and EDIT permissions must be added. Existing full-rewrite saving cannot preserve digital signatures.
- Preflight: OCR must run separately and ship English data; detection must never replace source content or insert an OCR text layer.
- Phase 1: in progress.
- Phases 2–5: pending.

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
