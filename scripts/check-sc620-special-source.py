"""Audit SC620 original special-button bytes and labels; never execute OEM code."""
import hashlib,json,re,sys
from pathlib import Path
root=Path(sys.argv[1]);program=(root/'AULA SC620.exe').read_bytes();disasm=Path(sys.argv[2]).read_bytes()
assert hashlib.sha256(program).hexdigest()=='cd947c856d287042bfa75e2617ee0488f9d29c9be532588a6cedfd8e3faf7514'
assert hashlib.sha256(disasm).hexdigest()=='d23dcf6f35566d990945fac89482cb06c541be1c845d85643a76c676ca305a54'
lines={}
for line in disasm.decode().splitlines():
 m=re.match(r'\s*([0-9a-f]+):\s*(.*?)\t(.*)',line)
 if m:lines[int(m[1],16)]={'bytes':m[2].strip(),'instruction':m[3].strip()}
checks={
0x41114a:'cmp\teax, 0x18',0x411153:'mov\tdword ptr [esi], 0x0',0x411159:'mov\teax, dword ptr [edi + 0xc]',0x411161:'mov\tbyte ptr [esi + 0x1], al',0x41116e:'mov\tbyte ptr [esi + 0x1], al',0x41117b:'mov\tbyte ptr [esi + 0x1], 0x4',0x411189:'mov\tbyte ptr [esi + 0x1], 0x8',0x411197:'mov\tbyte ptr [esi + 0x1], 0x10',0x4111a5:'mov\tword ptr [esi + 0x1], 0x138',0x4111b5:'mov\tword ptr [esi + 0x1], 0xff38',0x4111c9:'mov\tword ptr [esi + 0x1], 0xaa21',
0x4113a3:'cmp\teax, 0xb',0x4113a8:'mov\tword ptr [esi], 0x6121',0x4120a0:'push\t0xb',0x4120b2:'push\t0x9c',
0x4113c9:'cmp\teax, 0xf',0x4113ce:'mov\tword ptr [esi], 0x1f0',0x4113d3:'mov\tbyte ptr [esi + 0x2], 0x1',0x4113dc:'cmp\teax, 0x10',0x4113e1:'mov\tword ptr [esi], 0x1f0',0x4113e6:'mov\tbyte ptr [esi + 0x2], 0x2',0x4113ef:'cmp\teax, 0x11',0x4113f4:'mov\tword ptr [esi], 0x1f0',0x4113f9:'mov\tbyte ptr [esi + 0x2], 0x3',
0x411402:'cmp\teax, 0x13',0x411407:'mov\tword ptr [esi], 0x2f0',0x41140c:'mov\tbyte ptr [esi + 0x2], 0x1',0x411415:'cmp\teax, 0x14',0x41141a:'mov\tword ptr [esi], 0x2f0',0x41141f:'mov\tbyte ptr [esi + 0x2], 0x2',0x411428:'cmp\teax, 0x15',0x41142d:'mov\tword ptr [esi], 0x2f0',0x411432:'mov\tbyte ptr [esi + 0x2], 0x3',
0x4120c4:'push\t0xf',0x4120d6:'push\t0x96',0x4120e8:'push\t0x10',0x4120fa:'push\t0x97',0x41210c:'push\t0x11',0x41211e:'push\t0x98',0x412130:'push\t0x13',0x412142:'push\t0x99',0x412154:'push\t0x14',0x412166:'push\t0x9a',0x412178:'push\t0x15',0x41218a:'push\t0x9b',0x4121e4:'push\t0x18',0x4121f6:'push\t0xa2',
0x4114bc:'cmp\teax, 0x1a',0x4114d0:'mov\tword ptr [esi], 0x6f30',0x4114e3:'mov\tword ptr [esi], 0x7030',0x4114f6:'mov\tword ptr [esi], 0x9230',0x4114fb:'mov\tbyte ptr [esi + 0x2], 0x1',0x411509:'mov\tword ptr [esi], 0x810',0x41150e:'mov\tbyte ptr [esi + 0x2], 0x8',0x41151c:'mov\tword ptr [esi], 0x2330',0x411521:'mov\tbyte ptr [esi + 0x2], 0x2',0x41152f:'mov\tword ptr [esi], 0x8a30',0x411534:'mov\tbyte ptr [esi + 0x2], 0x1',0x41153f:'mov\tword ptr [esi], 0x10',0x411544:'mov\tbyte ptr [esi + 0x2], 0x3e',0x41154f:'mov\tword ptr [esi], 0x810',0x411554:'mov\tbyte ptr [esi + 0x2], 0x7',0x41155f:'mov\tword ptr [esi], 0x810',0x411564:'mov\tbyte ptr [esi + 0x2], 0x46',0x41156f:'mov\tword ptr [esi], 0x410',0x411574:'mov\tbyte ptr [esi + 0x2], 0x2b',0x41157f:'mov\tword ptr [esi + 0x1], 0x601',0x41158c:'mov\tword ptr [esi + 0x1], 0x1901',0x411599:'mov\tword ptr [esi + 0x1], 0x1b01',
}
for a,i in checks.items():
 assert lines[a]['instruction']==i,(hex(a),lines[a]);raw=bytes.fromhex(lines[a]['bytes']);off=0x400+a-0x401000;assert program[off:off+len(raw)]==raw
labels={};hashes={}
for lang,sha in [('1033','004ba916b47f3e4ecfbcf966c9cb4a19076b37c89dc3ed6e811f68b68f1a26df'),('2052','ad93fe8358b694ee6c1180564d805c4d1f9677d8dcbd2d4c906852291d493ee8')]:
 b=(root/'language'/f'{lang}.lan').read_bytes();assert hashlib.sha256(b).hexdigest()==sha;hashes[lang]=sha
 d=dict(l.split('=',1)for l in b.decode('utf-16').splitlines()if '='in l)
 labels[lang]={k:d[str(k)]for k in [31,32,36,150,151,152,153,154,155,156,162,185,186,187,188,189,190,191,192,193,194]}
assert [labels['1033'][k]for k in [150,151,152,153,154,155,156,162]]==['DPI +','DPI -','DPI Loop','Report Rate +','Report Rate -','Report Rate Loop','Fire Key','Disabled']
assert [labels['2052'][k]for k in [156,162]]==['火力键','禁用']
result={'source_program_sha256':hashlib.sha256(program).hexdigest(),'disassembly_sha256':hashlib.sha256(disasm).hexdigest(),'language_sha256':hashes,'method':'hash-pinned original bytes, exact instruction operands, source menu IDs and bilingual labels; no OEM execution','checks':{hex(a):lines[a]for a in checks},'labels':labels,'correction':'source24 is Disabled, not restore; its original-control hint bytes are retained before source4/5 record remapping','distinct_dpi_loops':{'source9':'21 55 00 00','source17':'f0 01 03 00'},'source26':'13 explicit source shortcut IDs to original HID records; unknown IDs refused','still_missing':['actual firmware execution and timing','custom AIM source27','unlabelled legacy source10/12 and unsupported actions','identity, physical mapping, recovery and UI'],'hardware_tested':False,'live_writer':False}
Path(sys.argv[3]).write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(len(checks),'exact original instruction byte checks; two language hashes and labels passed')
