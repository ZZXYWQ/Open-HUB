"""Verify pinned GG122 loop source hashes/ABI anchors without loading vendor code.

Optional decoded-dir verifies the separately MDC-validated local plaintext hashes
and declared step. This tool does not decrypt vendor files or execute Lisp/SQL.
"""
from pathlib import Path
import argparse
import hashlib
import json
import re
import struct

parser=argparse.ArgumentParser()
parser.add_argument('engine',type=Path)
parser.add_argument('specs',type=Path)
parser.add_argument('--decoded-dir',type=Path)
args=parser.parse_args()
facts=json.loads((Path(__file__).resolve().parents[1]/'docs/development/gg122-rival3-loop-source.json').read_text(encoding='utf8'))
raw=args.engine.read_bytes()
assert hashlib.sha256(raw).hexdigest()==facts['engine_sha256'],'GG engine build differs'
u16=lambda pos:struct.unpack_from('<H',raw,pos)[0]
u32=lambda pos:struct.unpack_from('<I',raw,pos)[0]
assert raw[:2]==b'MZ'
pe=u32(60);assert raw[pe:pe+4]==b'PE\0\0'
opt=pe+24;assert u16(opt)==0x20b
base=struct.unpack_from('<Q',raw,opt+24)[0]
heads=opt+u16(pe+20)
sections=[struct.unpack_from('<IIII',raw,heads+i*40+8) for i in range(u16(pe+6))]
def read(address,size):
    rva=address-base
    matches=[pos+rva-va for _,va,count,pos in sections if va<=rva and rva+size<=va+count]
    assert len(matches)==1,'uninitialized or ambiguous source mapping'
    at=matches[0]
    assert at+size<=len(raw),'truncated source bytes'
    return raw[at:at+size]
for anchor in facts['anchors']:
    expected=bytes.fromhex(anchor['bytes'])
    assert read(int(anchor['address'],16),len(expected))==expected,anchor['meaning']
# Verify the indirect Windows call really binds the named native export.
imports=base+u32(opt+120)
matched=[]
for index in range(100):
    original,stamp,chain,name,iat=struct.unpack('<5I',read(imports+index*20,20))
    if not any((original,stamp,chain,name,iat)):break
    for slot in range(2048):
        thunk=struct.unpack('<Q',read(base+original+slot*8,8))[0]
        if not thunk:break
        if thunk>>63:continue
        name=read(base+thunk+2,80).split(b'\0')[0]
        if name==facts['native_import']['name'].encode('ascii'):matched.append(base+iat+slot*8)
assert matched==[int(facts['native_import']['IAT_address'],16)],'native import binding differs'
# The DLL constructor confirms both final native fields, including stack delta.
native=(args.engine.parent/'SSEdevice.dll').read_bytes()
assert hashlib.sha256(native).hexdigest()==facts['native_dll']['sha256'],'native DLL build differs'
engine_bytes=raw
raw=native
pe=u32(60);opt=pe+24;base=struct.unpack_from('<Q',raw,opt+24)[0];heads=opt+u16(pe+20)
sections=[struct.unpack_from('<IIII',raw,heads+i*40+8) for i in range(u16(pe+6))]
for anchor in facts['native_dll']['anchors']:
    expected=bytes.fromhex(anchor['bytes'])
    assert read(int(anchor['address'],16),len(expected))==expected,anchor['meaning']
raw=engine_bytes
for descriptor in facts['descriptors']:
    source=args.specs/descriptor['file']
    assert hashlib.sha256(source.read_bytes()).hexdigest()==descriptor['encrypted_sha256'],source.name
    if args.decoded_dir:
        decoded=args.decoded_dir/(source.stem+'.lisp')
        assert hashlib.sha256(decoded.read_bytes()).hexdigest()==descriptor['decoded_sha256'],decoded.name
if args.decoded_dir:
    texts={p.stem:re.sub(r';[^\n]*','',p.read_text()) for p in args.decoded_dir.glob('*.lisp')}
    for owner,child in facts['includes']:
        assert re.search(r'\(include\s+"'+re.escape(child)+r'"\)',texts[owner]),'include chain differs'
    steps=[int(value) for text in texts.values() for value in re.findall(r'\(sequence-step-length-ms\s+(\d+)\)',text)]
    assert steps==[facts['step_ms']],'sequence step declaration differs'
print(f"Verified {len(facts['anchors'])} engine anchors,5 DLL constructor anchors, native import and {len(facts['descriptors'])} encrypted source hashes"+('; decoded include chain and33ms step verified' if args.decoded_dir else '')+'; no vendor execution')
