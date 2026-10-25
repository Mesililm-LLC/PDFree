# Forms and signature implementation

## Architecture

`viewer-core` owns field specifications, field/widget identities, typed values, semantic types, confidence, and edit commands. `pdf-engine` owns all MuPDF objects on the coordinator. Requests carry document/revision identifiers; events carry owned records. The UI keeps at most 16 nearby pages of form records and pins a selected page while its draft is active. Keyboard navigation requests further pages as needed.

All filling, authoring, detected batches, and visual marks use the existing serialized snapshot journal, dirty state, undo/redo, and save checkpoints. A text draft commits on blur, Enter, Tab, selection change, save, close, or document replacement. Invalid values remain available for correction. Standard PDF form-filling permission is separate from annotation/modify permissions. Declared encryption restrictions are honored conservatively even with an owner password; reserved permission bits never grant access. Signed documents and XFA are read-only; scripted/calculated, file-select, and rich-text fields are not edited. JavaScript execution remains disabled.

The isolated object adapter maintains AcroForm Fields/Kids/Parent references, page Annots/P references, resources, and appearances. Field and widget identity remain separate, including linked widgets on different pages and widgets without P entries. Type conversion is limited to simple fields and preserves compatible values; incompatible values require explicit correction or clearing. PDFree-created radio groups support moving/resizing and deleting members; imported shared trees remain fill-only. Date and Number are ordinary text widgets with PDFree metadata for date format and decimal precision. Other readers do not enforce those local semantics.

Save uses the existing sibling staging/verification/publication pipeline. List appearances show selection markers while retaining standard choice fields and values. Reopening checks field count, properties, values, export options, password/multiselect flags, and decoded normal appearance streams. Encryption is retained. Bundled Droid CJK fallback permits portable Unicode appearance generation. Full-document saving is blocked for signed files.

## Offline recognition

Detect Fields accepts 1–100 pages per operation, with a maximum of 2,000 results and 512 fields per page. Each page snapshot contains owned native text/vector geometry, an 18-megapixel-bounded grayscale raster, and existing/suppressed rectangles. Pages run incrementally with progress. Tesseract runs in a separate hidden local process, limited to one OCR thread, a 30-second page deadline, and bounded output. Its scratch directory is removed after completion. No recognized text or signatures are written to product traces. This does not insert a searchable text layer.

Recognition combines blank rectangles, underlines, checkbox/round outlines, nearby labels, and native text. Scanned pages are deskewed within five degrees. Ambiguous results receive a second segmentation pass on a crop at the full raster resolution. Open raster underlines and short native lines remain uncertain because their extents are ambiguous. Candidate overlap, existing fields, previous origins, and deleted-detection exclusions are reconciled before installation. Radio groups require multiple round controls and explicit select-one/choose-one text. Ungrouped round outlines are left for manual authoring. Dropdown/list options are never guessed.

Detection installs one atomic history edit. Stale revisions and cancellation before installation leave the document unchanged. Low-confidence fields open in the review queue for navigation, correction, confirmation, or deletion. Deleting a detected field records its original area as an exclusion; moving/retyping a detected field keeps its origin so reruns respect corrections. OCR failure reports an error and keeps manual authoring available.

## Visual signatures

Visual slots are managed Stamp annotations, not cryptographic Sig fields. Typed marks embed the pinned Caveat font; drawn marks use bounded normalized strokes; imported PNG/JPEG images are checked before decoding (4 MiB, 8,192 pixels per dimension, 16 megapixels). Standard Form appearances contain fonts/images and adapt to page transforms. PDFree metadata permits reopening, moving, resizing, replacing, and clearing marks. Capture buffers remain session-local; saving embeds the selected mark in the PDF. There is no reusable-signature library.

## Digital signing

The Windows signing adapter imports a local PKCS#12 key with NO_PERSIST_KEY, silently acquires only cached key contexts, and wipes the password/PFX buffers on drop. Exactly one usable, currently valid private key is required. Passwords are never logged or saved. Certificate trust and revocation are not evaluated.

Signing requires saved, reviewed edits and an unsigned Digital Signature field. Seed-value/field-lock policies, pre-existing signatures, repaired PDFs, multiple signing keys, oversized PDFs/chains, and scripted fields are rejected. Limits are 256 MiB for the source, 4 MiB for PKCS#12, and a 32 KiB CMS reservation. The writer appends an incremental revision to a new sibling staged output, verifies that the original bytes are unchanged, patches ByteRange, creates detached SHA-256 CMS, verifies complete coverage and cryptographic integrity, and publishes without overwriting an existing file. SHA-256 source digests are captured at open/save and checked before signing and publication; the signer parses those exact reviewed bytes. Cancellation/failure removes staging and preserves the source. The new output reopens read-only. No timestamp or online trust service is contacted.

## Reproduce qualification

Run `scripts/bootstrap.ps1`, `scripts/bootstrap-forms-tools.ps1`, and `scripts/check.ps1`. Override `LIBCLANG_PATH`, `CARGO_TARGET_DIR`, and `PDFREE_OCR_DIR` when sharing installed build tools from another checkout. The build/check scripts honor those paths. Use `CARGO_BUILD_JOBS=1` on constrained machines. MSVC native compilation may also need `_CL_=/MP1` to constrain its own parallelism.

Set `PDFREE_FORM_EVIDENCE_DIR` to an owned output folder before native tests to retain generated PDFs and the detection JSON report. Set `PDFREE_CAPTURE_DIR` before the UI tests to retain software-renderer captures. `PDFREE_FORM_DEVELOPMENT_CASES=0,1,2,3,4,5` selects development layouts. Unset it for the 24 reserved layouts (indices 96–119). The held-out test asserts precision >=90%, recall >=80%, high-confidence precision >=95%, and type accuracy >=90%. The layouts are synthetic English fixtures across clean native text, scans, moderate positive/negative skew, low contrast, tables, and mixed pages. Passing them does not establish performance on an arbitrary real-world form corpus. The private corpus is not modified.

Use `scripts/package.ps1 -Executable <release exe> -OcrSource <pinned worker folder>` for a complete portable package. The worker/model/font hashes are recorded in `assets/forms-tools.lock.json`. The English model is verified again during packaging. Signing tests generate disposable certificates and use independent OpenSSL CMS verification; OpenSSL is not shipped.

## Remaining manual qualification boundary

Automated UI events and renderer captures cover callbacks and layout. Physical native pickers, accessibility, mixed-monitor transitions, physical printing, and broad Acrobat/Edge/Sumatra interaction still require manual checks. See manual-validation.md. External trust, timestamping, revocation, additional OCR languages, complex field-tree authoring, and multiple signers are outside this initial release.
