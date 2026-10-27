"""Verify retained, hash-pinned SC620 instructions; never execute OEM software."""
import hashlib,json,re,struct,sys
from pathlib import Path
blob=Path(sys.argv[1]).read_bytes()
sha=hashlib.sha256(blob).hexdigest()
assert sha=='d23dcf6f35566d990945fac89482cb06c541be1c845d85643a76c676ca305a54'
lines={}
for line in blob.decode().splitlines():
 m=re.match(r'\s*([0-9a-f]+):\s*(.*?)\t(.*)',line)
 if m:lines[int(m[1],16)]={'bytes':m[2].strip(),'instruction':m[3].strip()}
checks={
0x4073a3:'push\tdword ptr [ebp + 0x8]',0x4073b0:'push\t0x0',0x4073b2:'push\t0x2',
0x407414:'push\t0x3',0x407426:'mov\tdword ptr [edi + 0xc], eax',
0x407471:'push\t0x6',0x407480:'mov\tdword ptr [edi + 0x18], eax',
0x40747b:'push\t0x7',0x40748d:'mov\tdword ptr [edi + 0x1c], eax',
0x407488:'push\t0x8',0x40749a:'mov\tdword ptr [edi + 0x20], eax',
0x407495:'push\t0x9',0x4074a7:'mov\tdword ptr [edi + 0x24], eax',
0x42a598:'mov\teax, dword ptr [ecx + 0xc]',0x42a59b:'cmp\teax, 0x4',0x42a5a0:'jmp\t0x42a5af <.text+0x295af>',0x42a5a2:'cmp\teax, 0x5',0x42a5a7:'mov\teax, 0x3',0x42a5ae:'dec\teax',0x42a5b5:'lea\teax, [edx + 4*eax]',0x42a5ba:'call\t0x411110 <.text+0x10110>',
0x4111d4:'cmp\teax, 0x16',0x4111dc:'mov\tal, byte ptr [edi + 0x20]',0x4111df:'mov\tbyte ptr [esi + 0x1], al',0x4111e2:'push\tdword ptr [edi + 0x1c]',0x4111e5:'call\t0x43d390 <.text+0x3c390>',0x4111ea:'mov\tbyte ptr [esi + 0x2], al',
0x411223:'cmp\teax, 0x17',0x41128e:'cmp\teax, dword ptr [edi + 0x1c]',0x4112bb:'mov\tbyte ptr [esi + 0x1], cl',
0x411301:'mov\tword ptr [esi], 0x120',0x411310:'mov\tword ptr [esi], 0x420',0x41131f:'mov\tword ptr [esi], 0x220',0x41132e:'mov\tdword ptr [esi], 0x20120',0x41133e:'mov\tword ptr [esi], 0x1020',0x41134d:'mov\tword ptr [esi], 0x820',0x41135c:'mov\tword ptr [esi], 0x3821',0x411361:'mov\tbyte ptr [esi + 0x2], 0x1',0x411374:'mov\tbyte ptr [esi + 0x2], -0x1',0x411382:'mov\tword ptr [esi], 0x5521',
0x411440:'mov\tbyte ptr [esi], 0x30',0x41144b:'mov\tbyte ptr [esi + 0x1], -0x33',0x411459:'mov\tbyte ptr [esi + 0x1], -0x49',0x411467:'mov\tbyte ptr [esi + 0x1], -0x4a',0x411475:'mov\tbyte ptr [esi + 0x1], -0x4b',0x411483:'mov\tbyte ptr [esi + 0x1], -0x17',0x411491:'mov\tbyte ptr [esi + 0x1], -0x16',0x41149f:'mov\tbyte ptr [esi + 0x1], -0x1e',0x4114b1:'mov\tword ptr [esi + 0x1], 0x183',
}
for a,i in checks.items():assert lines[a]['instruction']==i,(hex(a),lines.get(a))
# Disassembly also contains original jump-table bytes, despite decoding them as instructions.
mem={}
for a,entry in lines.items():
 for n,b in enumerate(bytes.fromhex(entry['bytes'])):mem[a+n]=b
expected=[0x43d3b0,0x43d3b9,0x43d3c2,0x43d3cb,0x43d3d4,0x43d3dd,0x43d3e6,0x43d3ef]
assert list(struct.unpack('<8I',bytes(mem[a]for a in range(0x43d3f8,0x43d418))))==expected
for i,a in enumerate(expected):assert bytes(mem[a+n]for n in range(5))==bytes([0xb8])+struct.pack('<I',1<<i)
result={'method':'retained pinned static instructions and jump-table bytes; no OEM execution or device capture','source_program_sha256':'cd947c856d287042bfa75e2617ee0488f9d29c9be532588a6cedfd8e3faf7514','disassembly_sha256':sha,'checks':{hex(a):lines[a]for a in checks},'modifier_jump_targets':[hex(x)for x in expected],'source_control_to_record':{'1':0,'2':1,'3':2,'4':4,'5':3,'6':5,'7':6,'8':7,'9':8,'10':9,'11':10},'macro_reference':'source numeric ID resolved through ordered converted macro library, not used as a native slot','source_layer':'OEM query uses profile argument and fn_layer=0','limitations':['SQL schema inherited from previously audited original program','physical button placement not proven','no model admission, live dispatch, readback recovery or UI','strict whole-library conversion; no slot allocator or selective macro merging']}
Path(sys.argv[2]).write_text(json.dumps(result,indent=2)+'\n')
print(len(checks),'exact instructions and eight original modifier jump targets verified')
