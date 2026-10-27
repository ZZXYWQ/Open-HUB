"""Pinned original archive payload and static control/save/native mapping audit."""
import hashlib,json,re,sys
from pathlib import Path
root=Path(sys.argv[1]);dis=Path(sys.argv[2]).read_bytes()
assert hashlib.sha256(dis).hexdigest()=='d23dcf6f35566d990945fac89482cb06c541be1c845d85643a76c676ca305a54'
expected={'AULA SC620.exe':'cd947c856d287042bfa75e2617ee0488f9d29c9be532588a6cedfd8e3faf7514','config.xml':'db5f27dbbceed611e27c8613e2b19bda11e8b568d8ff3ca8ad34ade651922d90','language/1033.lan':'004ba916b47f3e4ecfbcf966c9cb4a19076b37c89dc3ed6e811f68b68f1a26df','language/2052.lan':'ad93fe8358b694ee6c1180564d805c4d1f9677d8dcbd2d4c906852291d493ee8'}
for rel,sha in expected.items():assert hashlib.sha256((root/rel).read_bytes()).hexdigest()==sha
program=(root/'AULA SC620.exe').read_bytes()
def language(rel):
 b=(root/rel).read_bytes();s=b.decode('utf-16') if b[:2]in[b'\xff\xfe',b'\xfe\xff']else b.decode('utf-8-sig');return dict(line.split('=',1)for line in s.splitlines()if '='in line)
langs={k:language('language/'+k+'.lan') for k in ['1033','2052']}
assert [langs['2052'][str(i)]for i in [365,366,367]]==['循环直到按键松开','循环直到任意键按下','循环次数']
lines={}
for line in dis.decode().splitlines():
 m=re.match(r'\s*([0-9a-f]+):\s*(.*?)\t(.*)',line)
 if m:lines[m[1]]={'bytes':m[2].strip(),'instruction':m[3].strip()}
checks={'413f75':'push\t0x16d','413f89':'mov\tecx, dword ptr [edi + 0x518]','413fbc':'push\t0x16e','413fd0':'mov\tecx, dword ptr [edi + 0x51c]','414003':'push\t0x16f','414017':'mov\tecx, dword ptr [edi + 0x520]','41f8db':'mov\tecx, dword ptr [edi + 0x518]','41f8e3':'mov\tecx, dword ptr [edi + 0x51c]','41f8ed':'mov\tecx, dword ptr [edi + 0x520]','41f903':'mov\tdword ptr [ebp - 0x24], 0x1','41f915':'mov\tdword ptr [ebp - 0x24], 0x2','41f968':'mov\tdword ptr [ebp - 0x24], 0x0','41f9c3':'mov\tdword ptr [edi + 0x8], eax','41f9c9':'mov\tdword ptr [edi + 0xc], eax','40891d':'push\t0x2','40892c':'mov\tdword ptr [edi + 0x8], eax','408927':'push\t0x3','408939':'mov\tdword ptr [edi + 0xc], eax','4112cc':'mov\tbyte ptr [esi + 0x3], 0x2','4112d7':'mov\tbyte ptr [esi + 0x3], 0x3','4112e6':'mov\tbyte ptr [esi + 0x2], al'}
for address,instruction in checks.items():
 assert lines[address]['instruction']==instruction,(address,lines.get(address))
 raw=bytes.fromhex(lines[address]['bytes']);offset=0x400+int(address,16)-0x401000
 assert program[offset:offset+len(raw)]==raw,(address,'program bytes differ')
schema={}
for at in [0x1484d0,0x1485d8]:
 text=program[at:program.index(b'\0',at)].decode();assert 'CREATE TABLE IF NOT EXISTS t_macro' in text
 schema[hex(at)]=re.findall(r'\b(macro_id|name|play_type|play_times|delay_type|delay_time|record_id|type|record_index|desc|value)\b',text)
assert 'device_type="101"' in (root/'config.xml').read_text(encoding='utf-8-sig')
record={'archive_source':'https://aulastar.com/mouse/','archive_sha256':'8c2537d687360a2922ec2b11e1710ba6b2612c8d8a3d4d60966fcad39cee9263','payload_sha256':expected,'method':'static text and instruction bytes verified against pinned original executable; no OEM execution','labels':{str(i):{'en':langs['1033'][str(i)],'zh':langs['2052'][str(i)]}for i in [365,366,367]},'reviewed_instructions':{a:lines[a]for a in checks},'mapping':[{'oem_type':0,'meaning':'fixed count,1=once','wire_mode':0,'repeat':'1..255 representable byte subset, not measured maximum'},{'oem_type':1,'meaning':'while held','wire_mode':2,'repeat':1},{'oem_type':2,'meaning':'until any click','wire_mode':3,'repeat':1}],'sql_schema_field_order':schema,'identity_limit':'config device_type101 is host-side descriptor, not a proved wire ID; shared USB VID/PID unchanged','macro_event_limit':'preview preserves SQL event fields; does not convert unknown events or assign native slots','hardware_tested':False,'OEM_executed':False}
Path(sys.argv[3]).write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');print('Pinned SC620 labels, save fields, schema and native mode bytes verified.')
