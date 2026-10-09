"""Independently inspect retained native test artifacts without modifying PDFs."""
from pathlib import Path
import hashlib
import json
import sys

root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root / ".tools/python"))
from pypdf import PdfReader

folders = list((root / "benchmarks/runs/page-tools-native").glob("pdfree-pages-*"))
assert folders, "Run tests with PDFREE_PAGE_TEST_OUTPUT_DIR configured first"
checks = []

def latest(name):
    paths = [folder / name for folder in folders if (folder / name).exists()]
    assert paths, name
    return max(paths, key=lambda p: p.stat().st_mtime_ns)

def inspect(path, pages, password=None):
    reader = PdfReader(path, strict=True)
    if password:
        assert reader.is_encrypted
        assert reader.decrypt(password)
    assert len(reader.pages) == pages
    checks.append({"file": str(path.relative_to(root)), "pages": pages,
                   "encrypted": reader.is_encrypted,
                   "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    return reader

source = latest("source-part-1.pdf").parent
full = inspect(source / "source.pdf", 2)
assert full.pages[1].get("/Rotate", 0) == 90
notes = [a.get_object() for page in full.pages for a in page.get("/Annots", [])]
assert any(a.get("/Contents") == "Revised café" for a in notes)
subset = inspect(source / "extracted.pdf", 1)
assert subset.pages[0].extract_text() == full.pages[1].extract_text()
inspect(source / "source-part-1.pdf", 1)
inspect(source / "source-part-2.pdf", 1)
encrypted = latest("renamed.pdf")
reader = inspect(encrypted, 1, "pdfree-test")
assert all(not page.get("/Annots") for page in reader.pages)
assert (encrypted.parent / "encrypted.pdf").read_bytes() == (root / "tests/corpus/encrypted.pdf").read_bytes()
gc = latest("only-first.pdf")
reader = inspect(gc, 1)
assert not reader.pages[0].get("/Annots")
assert b"EXCLUDED_PAGE_PRIVATE_MARKER" not in gc.read_bytes()
report = {"reader": "pypdf", "version": __import__("pypdf").__version__, "checks": checks,
          "encrypted_source_unchanged": True, "visual_fidelity_verified": False}
output = root / "benchmarks/runs/page-tools-verification.json"
output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
print(json.dumps(report, indent=2))
