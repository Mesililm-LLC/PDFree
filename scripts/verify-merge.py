"""Read native import test output with independent pypdf; no document modification."""
from pathlib import Path
import hashlib,json,sys
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root/'.tools/python'))
from pypdf import PdfReader
outputs=list((root/'benchmarks/runs/page-tools-native').glob('pdfree-pages-*/interactive-merged.pdf'))
assert outputs, 'Run page_tools tests with PDFREE_PAGE_TEST_OUTPUT_DIR first'
path=max(outputs,key=lambda p:p.stat().st_mtime_ns)
r=PdfReader(path,strict=True)
assert len(r.pages)==3
annots=[obj.get_object() for obj in r.pages[1]['/Annots']]
assert len(annots)==7
assert annots[0]['/A']['/URI']=='https://example.com/pdfree'
assert annots[1]['/Dest'][0]==r.pages[2].indirect_reference
assert annots[6]['/A']['/D'][0]==r.pages[2].indirect_reference
assert annots[2].raw_get('/Popup')==r.pages[1]['/Annots'][3]
assert annots[3].raw_get('/Parent')==r.pages[1]['/Annots'][2]
assert annots[4]['/AP']['/N'].get_data().startswith(b'q 1 0 0 RG')
assert annots[4]['/AP'].raw_get('/N')==annots[4]['/AP'].raw_get('/R')
assert annots[4]['/AP']['/N']['/Resources']['/Font']['/A']['/BaseFont']=='/Helvetica'
widget=annots[5]
assert widget['/P'].indirect_reference==r.pages[1].indirect_reference
assert widget['/V']=='Preserved café'
form=r.trailer['/Root']['/AcroForm']
parent=form['/Fields'][0].get_object()
assert parent['/T'].startswith('PDFree_import_')
assert parent['/Kids'][0]==r.pages[1]['/Annots'][5]
assert widget.raw_get('/Parent')==form['/Fields'][0]
assert form['/DR']['/Font']['/Helv']['/BaseFont']=='/Helvetica'
fields=r.get_fields()
assert any(value.get('/V')=='Preserved café' for value in fields.values())
print(json.dumps({'file':str(path.relative_to(root)),'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'pages':3,'annotations':len(annots),'field_names':list(fields),'result':'passed'},indent=2))
