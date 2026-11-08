"""Generate MIT synthetic documents after the generic bundle has been built."""
from pathlib import Path
import argparse,hashlib,json,sys
HERE=Path(__file__).resolve().parent;ROOT=HERE.parent.parent.parent
sys.path.insert(0,str(ROOT/'experiments/pdf-provider/fixtures'));from generate import Pdf,literal

def make(pages,prefix):
    pdf=Pdf();catalog,tree=pdf.reserve(),pdf.reserve();font=pdf.add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>');ids=[];texts=[]
    for index in range(pages):
        title=f'{prefix} page {index+1}';lines=[title,'A runtime selected local document.','Dynamic selection stays on this source.'];texts.append(lines)
        content=('BT /F1 14 Tf 72 700 Td '+' Tj 0 -24 Td '.join(literal(line) for line in lines)+' Tj ET').encode('ascii')
        stream=pdf.stream(content);page=pdf.add(f'<< /Type /Page /Parent {tree} 0 R /MediaBox [0 0 612 792] /CropBox [18 24 594 768] /Resources << /Font << /F1 {font} 0 R >> >> /Contents {stream} 0 R >>');ids.append(page)
    pdf.set(tree,f'<< /Type /Pages /Kids [{" ".join(str(i)+" 0 R" for i in ids)}] /Count {pages} >>');pdf.set(catalog,f'<< /Type /Catalog /Pages {tree} 0 R >>');return pdf.encode(catalog),texts

def generate(directory):
    directory=Path(directory);directory.mkdir(parents=True,exist_ok=True);records={}
    for name,pages,prefix in [('runtime-three-pages.pdf',3,'Runtime novel'),('other-four-pages.pdf',4,'Alternate source'),('too-many-pages.pdf',17,'Unsupported long')]:
        data,texts=make(pages,prefix);(directory/name).write_bytes(data);records[name]={'sha256':hashlib.sha256(data).hexdigest(),'pages':pages,'construction_lines':texts}
    (directory/'broken.pdf').write_bytes(b'%PDF-1.7\nThis is deliberately malformed.\n');records['broken.pdf']={'purpose':'parser failure must preserve current document and draft'}
    (directory/'manifest.json').write_text(json.dumps({'license':'MIT synthetic fixture, original author QUSOR contributors','generated_after_build':True,'documents':records},indent=2)+'\n');return records
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('directory',type=Path);a=p.parse_args();print(json.dumps(generate(a.directory),indent=2))
