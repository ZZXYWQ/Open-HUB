"""Audit original static bytes for SC620 known configuration fields."""
import hashlib,re,json,sys
from pathlib import Path
root=Path(sys.argv[1]);p=(root/'AULA SC620.exe').read_bytes();d=Path(sys.argv[2]).read_bytes()
assert hashlib.sha256(p).hexdigest()=='cd947c856d287042bfa75e2617ee0488f9d29c9be532588a6cedfd8e3faf7514'
assert hashlib.sha256(d).hexdigest()=='d23dcf6f35566d990945fac89482cb06c541be1c845d85643a76c676ca305a54'
meta=json.loads(Path('../research-next-brands/aula-SC620-pe-metadata.json').read_text())
def text(va):
 for _,rva,_,off,size in meta['sections']:
  if rva<=va-0x400000<rva+size:
   raw=p[off+va-0x400000-rva:];end=next(i for i in range(0,len(raw),2)if raw[i:i+2]==b'\0\0');return raw[:end].decode('utf-16le')
lines={}
for line in d.decode().splitlines():
 m=re.match(r'\s*([0-9a-f]+):\s*(.*?)\t(.*)',line)
 if m:lines[m[1]]={'bytes':m[2].strip(),'instruction':m[3].strip()}
checks={'42e100':'push\t0x72','42e138':'push\t0x54c924','42e15b':'mov\tdword ptr [eax + 0x41c], esi','42e165':'push\t0x73','42e193':'push\t0x54c954','42e1b6':'mov\tdword ptr [eax + 0x420], esi','42e1c0':'push\t0x74','42e1ee':'push\t0x54c948','42e204':'mov\tdword ptr [eax + 0x424], esi','42e21b':'push\t0x75','42e249':'push\t0x54c970','42e26c':'mov\tdword ptr [eax + 0x428], esi','419fc2':'cmp\teax, 0x72','419fc7':'push\t0x54950c','419fce':'cmp\teax, 0x73','419fd3':'push\t0x5494bc','419fda':'cmp\teax, 0x74','419fdf':'push\t0x54949c','419fe6':'cmp\teax, 0x75','419feb':'push\t0x5494b8','41a021':'push\t0x5494a0','42aa22':'push\t0x5494a0','42aa51':'mov\tbyte ptr [ebp - 0x56], al','41a0f4':'test\tesi, esi','41a0f8':'xor\tesi, esi','41a0fc':'mov\teax, 0x14','41a103':'cmovg\tesi, eax','41a15a':'push\t0x5494c0','42afa6':'push\t0x5494c0','42afd5':'mov\tbyte ptr [ebp - 0x2d], al'}
for a,op in checks.items():
 row=lines[a];assert row['instruction']==op,(a,row)
 raw=bytes.fromhex(row['bytes']);off=0x400+int(a,16)-0x401000;assert p[off:off+len(raw)]==raw,a
expected={0x54c924:'125Hz',0x54c954:'250Hz',0x54c948:'500Hz',0x54c970:'1000Hz',0x54950c:'1',0x5494bc:'2',0x54949c:'3',0x5494b8:'4',0x5494a0:'report_rate',0x5494c0:'button_respondtime'}
for va,s in expected.items():assert text(va)==s,(hex(va),text(va))
lang=(root/'language/2052.lan').read_bytes();assert hashlib.sha256(lang).hexdigest()=='ad93fe8358b694ee6c1180564d805c4d1f9677d8dcbd2d4c906852291d493ee8'
s=lang.decode('utf-16')if lang[:2]==b'\xff\xfe'else lang.decode('utf-8-sig');labels=dict(l.split('=',1)for l in s.splitlines()if '='in l);assert labels['111']=='按键延迟响应(毫秒)'
schema=p[0x148120:p.index(b'\0',0x148120)].decode();assert all(k in schema for k in ['config_id INTEGER PRIMARY KEY AUTOINCREMENT','profile INTEGER','key TEXT','value TEXT'])
key_reads={}
for a,row in lines.items():
 m=re.fullmatch(r'push\t(0x54[0-9a-f]+)',row['instruction'])
 if m and 0x42a960<=int(a,16)<=0x42b0cd:key_reads[a]={'address':m[1],'name':text(int(m[1],16))}
record={'source_archive':'https://aulastar.com/mouse/','program_sha256':hashlib.sha256(p).hexdigest(),'retained_disassembly_sha256':hashlib.sha256(d).hexdigest(),'method':'pinned original program instruction/data bytes and language resource, no OEM execution','reviewed_instructions':{a:lines[a]for a in checks},'report_rate_mapping':[{'code':i+1,'hz':rate,'control_id':hex(0x72+i)}for i,rate in enumerate([125,250,500,1000])],'report_rate_payload_offset':2,'debounce':{'key':'button_respondtime','payload_offset':43,'editor_min':0,'editor_max':20,'label':labels['111'],'physical_timing_measured':False},'config_table_columns':['config_id','profile','key','value'],'writer_source_keys':key_reads,'converted_keys':16,'unconverted_keys_policy':'preserve unknown fields and block whole-source preparation','identity':'no model-specific discriminator proved; shared VID/PID not registered','physical_IO':False,'OEM_execution':False}
Path(sys.argv[3]).write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');print('SC620 rate labels/codes, debounce bounds and config schema verified against original bytes.')
