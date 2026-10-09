"""Add encryption and record provenance for independent compatibility samples."""
from pathlib import Path
import hashlib
import json
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / ".tools" / "python"))
from pypdf import PdfReader, PdfWriter

out = ROOT / "tests" / "corpus"
writer = PdfWriter(clone_from=out / "text.pdf")
writer.encrypt("pdfree-test", algorithm="RC4-128")
with (out / "encrypted.pdf").open("wb") as file:
    writer.write(file)
manifest = json.loads((out / "manifest.json").read_text(encoding="utf-8"))
original = {entry["file"]: entry for entry in manifest}
independent = {
    "w3c-dummy.pdf": "https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf",
    "tracemonkey.pdf": "https://raw.githubusercontent.com/mozilla/pdf.js/master/test/pdfs/tracemonkey.pdf",
}
for name in sorted(set(original) | set(independent) | {"encrypted.pdf"}):
    path = out / name
    entry = original.get(path.name, {"file": path.name})
    entry.update(sha256=hashlib.sha256(path.read_bytes()).hexdigest(), bytes=path.stat().st_size)
    if path.name in independent:
        entry.update(provenance=independent[path.name], license="Retains upstream document copyright; compatibility fixture only")
    elif path.name == "encrypted.pdf":
        entry.update(provenance="PDFree text.pdf encrypted with pypdf 6.1.1; test password pdfree-test", license="AGPL-3.0-only")
    if path.name != "malformed.pdf":
        reader = PdfReader(path)
        entry["encrypted"] = reader.is_encrypted
        if reader.is_encrypted:
            reader.decrypt("pdfree-test")
        entry["pages"] = len(reader.pages)
        embedded = False
        for page in list(reader.pages)[:20]:
            fonts = page.get("/Resources", {}).get("/Font", {})
            if hasattr(fonts, "get_object"):
                fonts = fonts.get_object()
            for font in fonts.values():
                font = font.get_object()
                descriptor = font.get("/FontDescriptor")
                if descriptor:
                    descriptor = descriptor.get_object()
                    embedded |= any(key in descriptor for key in ("/FontFile", "/FontFile2", "/FontFile3"))
        entry["embedded_fonts"] = embedded
    original[path.name] = entry
(out / "manifest.json").write_text(json.dumps(list(original.values()), indent=2) + "\n", encoding="utf-8")
print(f"Recorded {len(original)} fixture hashes and provenance")
