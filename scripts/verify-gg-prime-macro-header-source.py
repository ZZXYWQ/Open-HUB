"""Verify pinned Prime/libsteel macro-header evidence without loading vendor code."""
from pathlib import Path
import argparse, hashlib, json, struct
parser=argparse.ArgumentParser()
parser.add_argument('engine',type=Path)
parser.add_argument('--coverage',action='store_true',help='verify descriptor transfer-loop evidence')
parser.add_argument('--blocks',action='store_true',help='verify complete-block and binding evidence')
parser.add_argument('--events',action='store_true',help='verify extended canonical-event evidence')
args=parser.parse_args()
facts=json.loads((Path(__file__).resolve().parents[1]/('docs/development/gg122-prime-macro-coverage-source.json' if args.coverage else 'docs/development/gg122-prime-macro-block-source.json' if args.blocks else 'docs/development/gg122-prime-macro-events-source.json' if args.events else 'docs/development/gg122-prime-macro-header-source.json')).read_text(encoding='utf8'))
b=args.engine.read_bytes()
assert hashlib.sha256(b).hexdigest()==facts['engine_sha256'],'engine build changed'
u16=lambda at:struct.unpack_from('<H',b,at)[0]
u32=lambda at:struct.unpack_from('<I',b,at)[0]
u64=lambda at:struct.unpack_from('<Q',b,at)[0]
pe=u32(60);opt=pe+24;image=u64(opt+24);head=opt+u16(pe+20)
sections=[struct.unpack_from('<IIII',b,head+i*40+8)for i in range(u16(pe+6))]
def read(address,n):
    rva=address-image
    hits=[pos+rva-va for _,va,size,pos in sections if va<=rva and rva+n<=va+size]
    assert len(hits)==1,'ambiguous/unbacked native address'
    return b[hits[0]:hits[0]+n]
def ptr(at):return struct.unpack('<Q',read(at,8))[0]
def name(at):
    data=read(at,256);i=1;count=0;shift=0
    while True:
        byte=data[i];i+=1;count|=(byte&127)<<shift
        if byte<128:break
        shift+=7;assert shift<35
    return data[i:i+count].decode('utf8')
pcln=int(facts['pclntab_offset'],16)
assert b[pcln:pcln+8]==bytes.fromhex('f1ffffff00000108')
count=u64(pcln+8);text=u64(pcln+24);names=pcln+u64(pcln+32);table=pcln+u64(pcln+64)
matched={}
for i in range(count):
    entry,offset=struct.unpack_from('<II',b,table+i*8)
    at=names+u32(table+offset+4);end=b.index(0,at);label=b[at:end].decode('utf8')
    if any(f['name']==label for f in facts['functions']):matched[label]=(text+entry,text+u32(table+(i+1)*8))
for f in facts['functions']:
    bounds=(int(f['entry'],16),int(f['end'],16))
    assert matched[f['name']]==bounds,'function boundary changed'
    assert hashlib.sha256(read(bounds[0],bounds[1]-bounds[0])).hexdigest()==f['sha256']
typ=int(facts['metadata_type_address'],16)
assert ptr(typ)==8 and read(typ+23,1)[0]&31==25,'metadata type layout changed'
fields=ptr(typ+56);n=ptr(typ+64);assert n==len(facts['metadata_fields'])
for i,f in enumerate(facts['metadata_fields']):
    at=fields+i*24
    assert (name(ptr(at)),ptr(at+16),ptr(ptr(at+8)))==(f['name'],f['offset'],f['size'])
for extra in facts.get('extra_types',[]):
    typ=int(extra['address'],16)
    assert ptr(typ)==extra['size'] and read(typ+23,1)[0]&31==25
    fields=ptr(typ+56);assert ptr(typ+64)==len(extra['fields'])
    for i,f in enumerate(extra['fields']):
        at=fields+i*24
        assert (name(ptr(at)),ptr(at+16),ptr(ptr(at+8)))==(f['name'],f['offset'],f['size'])
for anchor in facts['anchors']:
    expected=bytes.fromhex(anchor['bytes'])
    assert read(int(anchor['address'],16),len(expected))==expected,anchor['meaning']
print(f"Verified pinned engine, {len(facts['functions'])} Go function ranges/digests, {len(facts['metadata_fields'])} typed header fields and {len(facts['anchors'])} instruction anchors. Vendor not executed.")
