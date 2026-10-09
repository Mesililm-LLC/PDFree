"""Independently inspect saved annotation test artifacts; never modify PDFs."""
from pathlib import Path
import hashlib
import json
import sys

root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root / ".tools/python"))
from pypdf import PdfReader

directories = sorted((root / "benchmarks/runs/annotations-tests").glob("*"))
assert directories, "Run the workspace UI tests first"
folder = directories[-1]
checks = []
for path, password in [(folder / "annotated.pdf", None),
                       (folder / "Renamed document.pdf", None),
                       (root / "benchmarks/runs/editing-native/encrypted-copy.pdf", "pdfree-test"),
                       (root / "benchmarks/runs/editing-native/restricted-copy.pdf", "pdfree-test")]:
    reader = PdfReader(path, strict=True)
    encrypted = reader.is_encrypted
    if password:
        assert encrypted
        assert reader.decrypt(password)
    annotations = [ref.get_object() for page in reader.pages for ref in page.get("/Annots", [])]
    kinds = [str(a.get("/Subtype")) for a in annotations]
    if path.name != "restricted-copy.pdf":
        assert kinds.count("/Highlight") == 1, kinds
        assert kinds.count("/Text") == 1, kinds
        assert any("日本語" in str(a.get("/Contents", "")) for a in annotations), annotations
        assert all(len(a["/QuadPoints"]) % 8 == 0 for a in annotations if a.get("/Subtype") == "/Highlight")
    checks.append({"file": str(path.relative_to(root)), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                   "pages": len(reader.pages), "encrypted": encrypted, "annotation_types": kinds})
original = folder / "rename-checkpoint.pdf"
renamed = folder / "Renamed document.pdf"
assert original.read_bytes() == renamed.read_bytes(), "Save As modified original bytes"
report = {"reader": "pypdf", "version": __import__("pypdf").__version__, "checks": checks,
          "save_as_source_unchanged": True, "visual_fidelity_verified": False}
if (folder / "extracted.pdf").exists():
    source = PdfReader(folder / "pages.pdf", strict=True)
    for name, indices in [("extracted.pdf", [0, 1, 3]),
                          ("split/pages-part-1.pdf", [0]),
                          ("split/pages-part-2.pdf", list(range(1, 20)))]:
        path = folder / name
        exported = PdfReader(path, strict=True)
        assert len(exported.pages) == len(indices)
        assert [page.extract_text() for page in exported.pages] == [source.pages[i].extract_text() for i in indices]
        assert not exported.outline, "Subset exports must remove potentially dangling bookmarks"
        checks.append({"file": str(path.relative_to(root)),
                       "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                       "pages": len(exported.pages), "page_text_matches_source": True})
    assert (folder / "pages.pdf").read_bytes() == (root / "tests/corpus/office.pdf").read_bytes()
    report["page_export_source_unchanged"] = True
output = root / "benchmarks/runs/annotations-verification.json"
output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
print(json.dumps(report, indent=2))
