# Forms and signatures validation

Windows x64, Rust 1.97.1, Slint 1.18.1, MuPDF 0.8.0. Implementation covers existing AcroForms, Prepare Form authoring and type changes, offline English detection and review, visual signatures, and incremental PKCS#12 digital signing. Features remain experimental pending the manual release boundary.

## Automated checks

Formatting and strict workspace/all-target Clippy pass. All 85 workspace tests pass from the recovered D: checkout. Actual registered UI callbacks and software pointer events cover drawing, filling, type changes, undo, resize, draft retention, navigation, review exit, signature capture/error recovery, and radio widget identity. Native tests cover linked widgets, Unicode appearances, export/display choices, list indices, restrictions, malformed trees, scripted/XFA read-only handling, failure recovery, and save checkpoints.

Digital signing tests verify SHA-256 CMS integrity with Windows CryptoAPI and independent OpenSSL, encrypted incremental outputs, source-prefix preservation, wrong passwords, cancellation/conflicts, same-length external changes with restored timestamps, tamper rejection, and read-only reopen. Certificate trust is not evaluated.

## Detection acceptance

The final reserved set contains 24 generated English forms (144 labeled fields), indices 120–143, spanning native text/vector, scanned, positive/negative skew, low contrast, table distractors, and mixed pages. Detection matched 116 fields from 121 predictions: precision **95.87%**, recall **80.56%**, high-confidence precision **99.14%**, and matched-field semantic-type accuracy **100%**. All numerical gates pass. See forms-detection-quality.json for every detection and rectangle.

Earlier partitions were consumed during development and are not claimed as fresh acceptance evidence. They exposed OCR letters mistaken for round controls, table fragments accepted as underlines, and label gaps exceeding lookup bounds. These fixtures share a synthetic template and translated layouts; the scores do not establish broad real-world accuracy. The existing private corpus was preserved.

## Independent PDF inspection

pypdf 6.1.1 strictly parses seven generated outputs. PDFium renders both screen and print appearances at 2x with its form environment enabled. Checks cover linked Unicode widgets, encrypted values, standard list values/indices, dropdown export/display pairs, rotated/cropped typed/drawn/image visual marks, and plain/encrypted digital signatures. See forms-independent-verification.json for output hashes and rendering counts. Generated PDFium captures were inspected for Unicode and rotated signatures. Rendering in print mode does not establish physical printing.

## Portable and manual boundary

The optimized Windows release build and portable package pass. SHA-256: `75b9f813b7faa7e866353f36c2f4c8cdcaa712a9e0332e58b9019bacf03e00f9`. A copied native detection integration executable inside the package passes scanned detection, cancellation, and repeat-detection reconciliation with `PDFREE_OCR_DIR` unset, exercising executable-relative bundled OCR discovery. The packaged release app opens the linked-widget fixture, renders Unicode, navigates/zooms, captures its native renderer, and exits 0. The captured render was inspected. This is a warm functional smoke, not a startup performance qualification. See forms-portable-smoke.json. Physical native pickers, physical keyboard cadence, mixed-monitor transitions, accessibility, physical printing, and broad Acrobat/Edge/Sumatra editing remain manual checks. Signing does not evaluate trust, revocation, or timestamps. Do not publish or declare general release qualification from automated checks alone.
