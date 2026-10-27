"""Static original-byte checks for SC620 SQL-event conversion, no OEM execution."""
import hashlib,json,re,sys
from pathlib import Path
program=Path(sys.argv[1]).read_bytes();blob=Path(sys.argv[2]).read_bytes()
assert hashlib.sha256(program).hexdigest()=='cd947c856d287042bfa75e2617ee0488f9d29c9be532588a6cedfd8e3faf7514'
assert hashlib.sha256(blob).hexdigest()=='d23dcf6f35566d990945fac89482cb06c541be1c845d85643a76c676ca305a54'
lines={}
for line in blob.decode().splitlines():
 m=re.match(r'\s*([0-9a-f]+):\s*(.*?)\t(.*)',line)
 if m:lines[m[1]]={'bytes':m[2].strip(),'instruction':m[3].strip()}
reviewed={}
def check(a,expected=None):
 row=lines[a]
 if expected is not None:assert row['instruction']==expected,(a,row)
 raw=bytes.fromhex(row['bytes']);offset=0x400+int(a,16)-0x401000
 assert program[offset:offset+len(raw)]==raw,a
 reviewed[a]=row
keys=[{'vk':v,'usage':v-61}for v in range(65,91)]+[{'vk':v,'usage':v-19}for v in range(49,58)]
ops=[(a,r['instruction'])for a,r in lines.items()if 0x43cd80<=int(a,16)<=0x43d385]
for i,(a,op)in enumerate(ops[:-2]):
 v=re.fullmatch(r'cmp\teax, (0x[0-9a-f]+)',op);u=re.fullmatch(r'mov\tecx, (0x[0-9a-f]+)',ops[i+2][1])
 if v and u and ops[i+1][1].startswith('jne\t'):
  vk,usage=int(v[1],16),int(u[1],16)
  if 4<=usage<=0xa4 and not 0xe0<=vk<=0xe7:
   keys.append({'vk':vk,'usage':usage})
   for address,_ in ops[i:i+3]:check(address)
for a,i in {'43cd8a':'cmp\teax, 0x41','43cd8f':'cmp\teax, 0x5a','43cd94':'lea\tecx, [eax - 0x3d]','43cd9d':'cmp\teax, 0x31','43cda2':'cmp\teax, 0x39','43cda7':'lea\tecx, [eax - 0x13]','408f57':'push\t0x3','408f5c':'mov\tdword ptr [edi + 0x8], eax','408fa7':'push\t0x5','408fb6':'mov\tdword ptr [edi + 0x10], eax','408fbe':'mov\tdword ptr [edi + 0x14], eax','429d7f':'mov\tecx, 0x1','429dcb':'add\tecx, dword ptr [esi + 0x10]','429e15':'cmp\teax, 0x40','429e21':'cmovle\tecx, edx','429e31':'mov\tedx, dword ptr [ebp - 0x10f4]','429e37':'or\tbyte ptr [ecx], dl','429f5e':'mov\tdword ptr [ebp - 0x10f4], 0x0','429f07':'push\t0x54be18','429fd6':'push\t0x54be4c','42a023':'push\t0x54be18','42a041':'push\t0x54be4c','42a06d':'mov\tword ptr [ebp + eax - 0x10d2], 0x144','42a081':'mov\tword ptr [ebp + eax - 0x10d2], 0xff44'}.items():check(a,i)
assert len(keys)==109 and len({k['vk']for k in keys})==109
fixture=json.loads(Path('src-tauri/tests/fixtures/yjx-sc620/oem-event-keys.json').read_text());assert fixture['keys']==keys
# Read original data by the independently retained PE section map.
meta=json.loads(Path('../research-next-brands/aula-SC620-pe-metadata.json').read_text())
strings={}
for va in [0x54be18,0x54be4c]:
 for _,rva,_,offset,size in meta['sections']:
  if rva<=va-0x400000<rva+size:
   start=offset+va-0x400000-rva;tail=program[start:];end=next(i for i in range(0,len(tail),2)if tail[i:i+2]==b'\0\0');strings[hex(va)]=tail[:end].decode('utf-16le')
assert strings=={'0x54be18':'Scroll Up','0x54be4c':'Scroll Down'}
result={'program_sha256':hashlib.sha256(program).hexdigest(),'disassembly_sha256':hashlib.sha256(blob).hexdigest(),'method':'static source and original instruction/data bytes, no OEM execution','supported_key_cases':109,'key_fixture':'src-tauri/tests/fixtures/yjx-sc620/oem-event-keys.json','reviewed_instructions':reviewed,'scroll_substrings':strings,'delays':'SQL value column5 ->object+10,not delay_time column6; accumulated into previous record; zero before next action defaults10; trailing delay kept raw; no physical unit proof','global_cursor_boundary':'each macro accumulator starts1; later nonempty first action ORs1 into previous macro terminal delay; even empty sentinel can gain delay1','strict_subset':['leading delay rejected because OEM would discard/overwrite header','overflow rejected rather than truncating','modifier/motion/unknown conversion unresolved','unconverted source records remain in preview and block preparation'],'physical_IO':False,'OEM_execution':False}
Path(sys.argv[3]).write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print('109 OEM key cases, SQL value field, scroll overrides and global delay boundary verified.')
