"""SC620 tuning labels, imports and original instruction/data checks; no execution."""
import hashlib,json,re,struct,sys
from pathlib import Path
root=Path(sys.argv[1]);p=(root/'AULA SC620.exe').read_bytes();d=Path(sys.argv[2]).read_bytes()
assert hashlib.sha256(p).hexdigest()=='cd947c856d287042bfa75e2617ee0488f9d29c9be532588a6cedfd8e3faf7514'
assert hashlib.sha256(d).hexdigest()=='d23dcf6f35566d990945fac89482cb06c541be1c845d85643a76c676ca305a54'
sections=json.loads(Path('../research-next-brands/aula-SC620-pe-metadata.json').read_text())['sections']
def off(va):
 for _,rva,_,o,n in sections:
  if rva<=va-0x400000<rva+n:return o+va-0x400000-rva
 raise ValueError(hex(va))
def text(va):
 raw=p[off(va):];end=next(i for i in range(0,len(raw),2)if raw[i:i+2]==b'\0\0');return raw[:end].decode('utf-16le')
langs={}
for lang in ['1033','2052']:
 b=(root/'language'/f'{lang}.lan').read_bytes();s=b.decode('utf-16')if b[:2]==b'\xff\xfe'else b.decode('utf-8-sig');langs[lang]=dict(l.split('=',1)for l in s.splitlines()if '='in l)
assert [langs['1033'][str(k)]for k in [585,586,587,601,602,606,607,611,613]]==['Ripple','Angle Snap','Motion Sync','Forward','Reverse','On','Off','Min','Hour']
lines={}
for line in d.decode().splitlines():
 m=re.match(r'\s*([0-9a-f]+):\s*(.*?)\t(.*)',line)
 if m:lines[m[1]]={'bytes':m[2].strip(),'instruction':m[3].strip()}
checks={'413516':'push\t0x249','41355d':'push\t0x24a','4135a4':'push\t0x24b','413679':'push\t0x259','4136c0':'push\t0x25a','41374e':'push\t0x25e','413795':'push\t0x25f','41a5c7':'mov\tecx, 0x4','41a5d5':'or\tesi, 0x8','41a5dc':'or\tesi, 0x10','42af72':'test\tal, 0x4','42af77':'mov\tedx, 0x10','42af86':'or\tcl, 0x1','42af90':'or\tcl, 0x20','41a2b3':'push\t0x5494bc','41a2ba':'push\t0x54950c','42af2e':'mov\tbyte ptr [ebp - 0x2f], al','41a3b9':'push\t0x549460','41a3c0':'push\t0x54950c','42aeec':'mov\tbyte ptr [ebp - 0x30], al','41a4c9':'push\t0x5494bc','41a4d0':'push\t0x54950c','42b05a':'mov\tbyte ptr [ebp - 0x2b], al','41a6e3':'call\tdword ptr [0x5376c4]','42afd5':'mov\tbyte ptr [ebp - 0x2d], al','42b017':'mov\tbyte ptr [ebp - 0x2c], al','42e88f':'push\t0x1','42e8fb':'push\t0x3','42e961':'push\t0x5','42e9c7':'push\t0xa','42ea2d':'push\t0x14','42ea93':'push\t0x1e','42eaf9':'push\t0x3c','42e844':'push\t0x263','42eaab':'push\t0x265','403112':'push\t0x549580','4294c3':'movzx\teax, byte ptr [ebp - 0x85]','42aa93':'mov\tbyte ptr [ebp - 0x55], 0x6'}
for a,op in checks.items():
 row=lines[a];assert row['instruction']==op,(a,row);raw=bytes.fromhex(row['bytes']);at=off(int(a,16));assert p[at:at+len(raw)]==raw,a
assert text(0x549460)=='0'and text(0x54950c)=='1'and text(0x5494bc)=='2'and text(0x549580)=='5'
assert [text(va)for va in [0x54b94c,0x54b958,0x54b964,0x54b970,0x54b97c,0x54b988]]==['1 %s','3 %s','5 %s','10 %s','20 %s','30 %s']
pe=struct.unpack_from('<I',p,0x3c)[0];opt=pe+24;directory=struct.unpack_from('<I',p,opt+96+8)[0];pos=off(0x400000+directory);imports={}
while any(p[pos:pos+20]):
 original,_,_,_,iat=struct.unpack_from('<IIIII',p,pos);index=0
 while True:
  rva=struct.unpack_from('<I',p,off(0x400000+original)+index*4)[0]
  if not rva:break
  address=0x400000+iat+index*4
  if address in [0x5376c4,0x5375f0] and not rva&0x80000000:
   at=off(0x400000+rva)+2;imports[hex(address)]=p[at:p.index(b'\0',at)].decode()
  index+=1
 pos+=20
assert imports['0x5376c4'].startswith('?GetCurData@MComboBox@')
assert imports['0x5375f0'].startswith('?AddItem@MComboBox@')
ids=[560,561,585,586,587,600,601,602,603,605,606,607,608,610,611,613]
r={'source_program_sha256':hashlib.sha256(p).hexdigest(),'disassembly_sha256':hashlib.sha256(d).hexdigest(),'method':'static original instruction/data/import/label checks, no OEM execution','labels':{str(i):{lang:langs[lang][str(i)]for lang in langs}for i in ids},'reviewed_instructions':{a:lines[a]for a in checks},'imports':imports,'native_mapping':{'lod':{'offset':41,'low_mm_code':1,'high_mm_code':2},'scroll':{'offset':40,'forward':0,'reverse':1},'esports':{'offset':45,'off':1,'on':2},'sensor_flags':{'offset':42,'ripple':{'source':4,'native':16},'angle_snap':{'source':8,'native':1},'motion_sync':{'source':16,'native':32}},'sleep':{'offset':44,'minutes':[1,3,5,10,20,30,60],'source':'MComboBox.GetCurData, not selected index'}},'dpi_flag_boundary':{'initial_default_string':5,'readback_stores_payload_stage_field_directly':True,'native_writer_constant':6,'converter_accepts_only':6,'other_values':'kept unconverted; no unsupported stage count silently turned into6'},'unknown_native_bits':'read-preserving mask, never clear unselected native bits','source_sensor_low_bits':'preserved source metadata, not used by verified SC620 native writer','physical_IO':False,'OEM_execution':False}
Path(sys.argv[3]).write_text(json.dumps(r,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');print('SC620 tuning labels, raw remapping, minute values and DPI-flag boundary verified statically.')
