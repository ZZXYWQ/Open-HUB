"""Static source-anchor verification; never load or execute the vendor DLL."""
from pathlib import Path
import argparse, hashlib, json, struct
parser=argparse.ArgumentParser();parser.add_argument("dll",type=Path);args=parser.parse_args()
facts=json.loads((Path(__file__).resolve().parents[1]/"docs/development/gg122-native-sequencer-facts.json").read_text(encoding="utf-8"));raw=args.dll.read_bytes()
assert hashlib.sha256(raw).hexdigest()==facts["native_dll"]["sha256"],"vendor DLL build changed"
u16=lambda p:struct.unpack_from("<H",raw,p)[0]
u32=lambda p:struct.unpack_from("<I",raw,p)[0]
assert raw[:2]==b"MZ";pe=u32(60);assert raw[pe:pe+4]==b"PE\0\0";opt=pe+24;assert u16(opt)==0x20b
image=struct.unpack_from("<Q",raw,opt+24)[0];headers=opt+u16(pe+20);sections=[struct.unpack_from("<IIII",raw,headers+n*40+8)for n in range(u16(pe+6))]
for anchor in facts["anchors"]:
 rva=int(anchor["address"],16)-image;want=bytes.fromhex(anchor["bytes"]);matches=[pos+rva-va for virtual_size,va,size,pos in sections if va<=rva and rva+len(want)<=va+size];assert len(matches)==1,"ambiguous or missing source mapping";at=matches[0];assert raw[at:at+len(want)]==want,anchor["meaning"]
print(f"Verified {len(facts['anchors'])} native instruction anchors against exact DLL SHA256; no vendor execution")
