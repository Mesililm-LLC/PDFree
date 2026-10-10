"""Original public interactivity fixture, independently produced with pypdf."""
from pathlib import Path
import sys
root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root / '.tools/python'))
from pypdf import PdfWriter
from pypdf.generic import (DictionaryObject as D, NameObject as N, TextStringObject as T,
                          ArrayObject as A, NumberObject as I, DecodedStreamObject)
w = PdfWriter()
w.add_blank_page(width=300,height=400)
w.add_blank_page(width=500,height=600)
def dictionary(**kw): return D({N('/'+key):value for key,value in kw.items()})
def add(**kw): return w._add_object(dictionary(**kw))
def rect(*values): return A([I(v) for v in values])
page0=w.pages[0].indirect_reference; page1=w.pages[1].indirect_reference
uri=add(Type=N('/Annot'),Subtype=N('/Link'),Rect=rect(10,10,100,30),A=dictionary(S=N('/URI'),URI=T('https://example.com/pdfree')))
internal=add(Type=N('/Annot'),Subtype=N('/Link'),Rect=rect(10,40,100,60),Dest=A([page1,N('/Fit')]))
note=add(Type=N('/Annot'),Subtype=N('/Text'),Rect=rect(10,80,30,100),Contents=T('Preserved popup note'),P=page0)
popup=add(Type=N('/Annot'),Subtype=N('/Popup'),Rect=rect(30,80,180,180),Parent=note,P=page0)
note.get_object()[N('/Popup')]=popup
appearance=DecodedStreamObject();appearance.set_data(b'q 1 0 0 RG 2 w 0 0 80 30 re S Q')
appearance.update(dictionary(Type=N('/XObject'),Subtype=N('/Form'),BBox=rect(0,0,80,30)))
appearance[N('/Resources')]=dictionary(Font=dictionary(A=dictionary(Type=N('/Font'),Subtype=N('/Type1'),BaseFont=N('/Helvetica'))))
ap=w._add_object(appearance)
square=add(Type=N('/Annot'),Subtype=N('/Square'),Rect=rect(10,120,90,150),Contents=T('Square appearance'),AP=dictionary(N=ap,R=ap),P=page0)
font=add(Type=N('/Font'),Subtype=N('/Type1'),BaseFont=N('/Helvetica'))
widget=add(Type=N('/Annot'),Subtype=N('/Widget'),FT=N('/Tx'),T=T('shared_name'),V=T('Preserved café'),Rect=rect(10,200,180,225),P=page0,DA=T('/Helv 11 Tf 0 g'))
named=add(Type=N('/Annot'),Subtype=N('/Link'),Rect=rect(10,250,100,270),A=dictionary(S=N('/GoTo'),D=T('second_page')))
w._root_object[N('/Names')]=dictionary(Dests=dictionary(Names=A([T('second_page'),A([page1,N('/XYZ'),I(10),I(20),I(0)])])))
w.pages[0][N('/Annots')]=A([uri,internal,note,popup,square,widget,named])
w._root_object[N('/AcroForm')]=dictionary(Fields=A([widget]),DR=dictionary(Font=dictionary(Helv=font)),DA=T('/Helv 11 Tf 0 g'))
w.write(root/'tests/corpus/interactive-merge.pdf')
