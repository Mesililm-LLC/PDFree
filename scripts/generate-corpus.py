"""Generate deterministic, original PDF fixtures without a PDF library."""
from pathlib import Path
import hashlib
import json
import zlib

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "tests" / "corpus"
OUT.mkdir(parents=True, exist_ok=True)

def pdf(name, count, kind="text"):
    objects = []
    def add(data):
        objects.append(data.encode("ascii") if isinstance(data, str) else data)
        return len(objects)
    add("<< /Type /Catalog /Pages 2 0 R >>")
    add("")
    font = add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    image = None
    if kind == "scan":
        # Rasterized stripes and glyph-like blocks exercise image decoding.
        data = bytes(25 if (y % 40 < 8 and 30 < x < 560) else 245 for y in range(800) for x in range(600))
        packed = zlib.compress(data)
        image = add(f"<< /Type /XObject /Subtype /Image /Width 600 /Height 800 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {len(packed)} >>\nstream\n".encode() + packed + b"\nendstream")
    pages = []
    for i in range(count):
        w, h = ((792, 612) if i % 2 and kind == "mixed" else (612, 792))
        content = f"BT /F1 24 Tf 50 {h - 70} Td (PDFree fixture - page {i+1}) Tj ET\n"
        if kind == "vector":
            content += "0.1 0.4 0.8 RG\n" + "".join(f"{j % 580} {j % 720} m {(j*7)%600} {(j*11)%750} l S\n" for j in range(12000))
        if kind == "transparency":
            content += "q /GS1 gs 0.2 0.5 0.9 rg 70 300 300 300 re f 0.9 0.3 0.2 rg 200 200 300 300 re f Q\n"
        if image:
            content += "q 512 0 0 650 50 40 cm /Im1 Do Q\n"
        stream = add(f"<< /Length {len(content)} >>\nstream\n{content}endstream")
        resources = f"/Font << /F1 {font} 0 R >>"
        if image: resources += f" /XObject << /Im1 {image} 0 R >>"
        if kind == "transparency": resources += " /ExtGState << /GS1 << /Type /ExtGState /ca 0.45 /CA 0.45 >> >>"
        pages.append(add(f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << {resources} >> /Contents {stream} 0 R >>"))
    objects[1] = f"<< /Type /Pages /Count {count} /Kids [{' '.join(f'{p} 0 R' for p in pages)}] >>".encode()
    result = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
    offsets = [0]
    for i, obj in enumerate(objects, 1):
        offsets.append(len(result))
        result += f"{i} 0 obj\n".encode() + obj + b"\nendobj\n"
    xref = len(result)
    result += f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode()
    result += b"".join(f"{o:010} 00000 n \n".encode() for o in offsets[1:])
    result += f"trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    (OUT / name).write_bytes(result)

for name, count, kind in [("text.pdf", 1, "text"), ("office.pdf", 20, "text"), ("mixed.pdf", 100, "mixed"), ("thousand.pdf", 1000, "text"), ("scan.pdf", 200, "scan"), ("vector.pdf", 1, "vector"), ("transparency.pdf", 1, "transparency")]:
    pdf(name, count, kind)
(OUT / "malformed.pdf").write_bytes(b"This is deliberately not a PDF.\x00\xff")
manifest_path = OUT / "manifest.json"
existing = {entry["file"]: entry for entry in json.loads(manifest_path.read_text(encoding="utf-8"))} if manifest_path.exists() else {}
# Only fixtures owned by this generator; personal PDFs in this folder stay private.
for name in ("text.pdf", "office.pdf", "mixed.pdf", "thousand.pdf", "scan.pdf", "vector.pdf", "transparency.pdf", "malformed.pdf"):
    p = OUT / name
    existing[name] = {"file": name, "sha256": hashlib.sha256(p.read_bytes()).hexdigest(), "bytes": p.stat().st_size, "provenance": "Original deterministic PDFree fixture; scripts/generate-corpus.py", "license": "AGPL-3.0-only"}
manifest = list(existing.values())
(OUT / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
print(f"Generated {len(manifest)} fixtures")
