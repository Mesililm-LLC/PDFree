"""Inspect generated public form artifacts with independent PDF libraries.

Run native tests with PDFREE_FORM_EVIDENCE_DIR first. This never edits PDFs.
"""
from pathlib import Path
import argparse
import hashlib
import json
import sys

parser = argparse.ArgumentParser()
parser.add_argument("--directory", type=Path, default=Path("benchmarks/runs/forms"))
parser.add_argument("--pypdf-dir", type=Path, default=Path(".tools/python"))
args = parser.parse_args()
sys.path.insert(0, str(args.pypdf_dir.resolve()))
import pypdf
import pypdfium2 as pdfium

folder = args.directory.resolve()
checks = []
for name, password in [
    ("linked-widgets.pdf", None),
    ("encrypted-filled.pdf", "reader"),
    ("visual-signatures-rotated.pdf", None),
    ("digitally-signed.pdf", None),
    ("digitally-signed-encrypted.pdf", "pdfree-document"),
]:
    path = folder / name
    assert path.is_file(), f"Missing generated artifact: {name}"
    reader = pypdf.PdfReader(path, strict=True)
    if password:
        assert reader.is_encrypted
        assert reader.decrypt(password)
    fields = reader.get_fields() or {}
    widgets = [ref.get_object() for page in reader.pages for ref in page.get("/Annots", []) if ref.get_object().get("/Subtype") == "/Widget"]
    if name == "linked-widgets.pdf":
        assert fields["linked"]["/V"] == "Zoë 東京"
        assert len(widgets) == 2
        assert widgets[0]["/Parent"].indirect_reference == widgets[1]["/Parent"].indirect_reference
        assert all(w["/AP"]["/N"].get_data() for w in widgets)
    elif name == "encrypted-filled.pdf":
        assert fields["restricted"]["/V"] == "Off"
        assert reader.is_encrypted
    elif name == "visual-signatures-rotated.pdf":
        marks = [ref.get_object() for ref in reader.pages[0]["/Annots"]]
        assert len(marks) == 3
        assert all(m["/Subtype"] == "/Stamp" and m["/PDFreeVisual"] == 1 for m in marks)
        assert all(m["/AP"]["/N"].get_data() for m in marks)
        assert reader.pages[0].rotation == 90
    else:
        signatures = [f for f in fields.values() if f.get("/FT") == "/Sig"]
        assert len(signatures) == 1
        sig = signatures[0]["/V"].get_object()
        ranges = list(sig["/ByteRange"])
        assert ranges[0] == 0 and ranges[2] + ranges[3] == path.stat().st_size
        assert sig["/SubFilter"] == "/adbe.pkcs7.detached"
    # PDFium is independent of both MuPDF and pypdf. Render screen and print
    # appearances, with its form-fill environment enabled for widget rendering.
    doc = pdfium.PdfDocument(path, password=password)
    doc.init_forms()
    rendered = []
    for index in range(len(doc)):
        page = doc[index]
        for mode in [None, "print"]:
            bitmap = page.render(scale=2, draw_annots=True, optimize_mode=mode)
            image = bitmap.to_pil().convert("RGB")
            ink = sum(1 for r, g, b in image.getdata() if min(r, g, b) < 180)
            if "digitally-signed" not in name:
                assert ink > 40, f"Independent rendering is blank: {name} page {index+1}"
            output = folder / f"{path.stem}-{index+1}-{mode or 'screen'}-pdfium.png"
            image.save(output)
            rendered.append({"page": index+1, "mode": mode or "screen", "ink_pixels": ink, "image": output.name})
            bitmap.close()
        page.close()
    doc.close()
    checks.append({"file": name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "pages": len(reader.pages), "encrypted": reader.is_encrypted, "fields": len(fields), "renders": rendered})
report = {"parser": "pypdf", "parser_version": pypdf.__version__, "renderer": "PDFium", "checks": checks, "physical_printing_verified": False, "certificate_trust_evaluated": False}
output = folder / "independent-verification.json"
output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
print(json.dumps(report, indent=2))
