"""Verify pinned OEM model, UI choices and native layouts as static text."""
from pathlib import Path
import hashlib,json,re
p=Path(__file__).resolve().parents[2]/'research-atk/official-atk-3.2.27.js';raw=p.read_bytes();assert hashlib.sha256(raw).hexdigest()=='71a1ba1f86721609328c02ab3e69bfe8acf9e1aed84f328a94eeb1b2955b64b5';s=raw.decode('utf-8')
record=s[s.index('vendorId:14139,productId:4414,'):];record=record[:record.index('}}')+2];assert 'name:"ATK X1 Lite"' in record and 'sensor:mt.PAW3395' in record and 'mouseCidMid:"2,89"' in record;assert 'noZeroKeyDebounce' not in record and 'noFarDistance:!0' in record
assert 'sensorEnable=181' in s and 'stabilizationTime=169' in s and 'silentHeight=10' in s
bu=s[s.index('class Bu extends'):s.index('class Uu extends')]
for fact in ['e.dataValidLen=10','e.stabilizationTime=t.keyDebounceTime','e.motionSync=t.moveSync','e.closeLedTime=t.sleepTime','e.linearCorrection=t.lineCorrection','e.rippleControl=t.rippleCorrection','this.baseOffset+4)*10','85-t','const e=t/10']:assert fact in bu,fact
uu=s[s.index('class Uu extends'):s.index('class _1 extends')]
for fact in ['e.dataValidLen=6','e.sensorSleepEnabled=t.sensorSleepEnabled','e.sensorSleepTime=t.sensorSleepTime','this.baseOffset+2)*10','const e=t/10','85-e']:assert fact in uu,fact
a=s.index('const yZ=');b=s.index(',Edt=',a);assert [int(v) for v in re.findall(r'value:(\d+)',s[a:b])]==[0,1,2,4,8,15,20]
a=s.index('Pdt=[');b=s.index('];',a);assert [int(v) for v in re.findall(r'value:(\d+)',s[a:b])]==[30,60,120,180,300,1200,1500,1800]
a=s.index('__name:"SilenceHeight"');t=s[a:a+2100];assert 'value:1' in t and 'value:2' in t and '[mt.PAW3950,mt.PAW3950DM].includes(p.value.sensor)' in t
out={'date':'2026-10-28','source_url':'https://bpcdn.atkgear.com/hub-v3/production/3.2.27/static/index-DBE__Npj.js','sha256':hashlib.sha256(raw).hexdigest(),'static_only':True,'model':'ATK X1 Lite','cid_mid':[2,89],'sensor':'PAW3395','rows':{'LOD':[10,2],'performance':[169,10],'sensor':[181,6]},'checksum':'each scalar pair sums modulo256 to0x55','time_unit_seconds':10,'debounce_ms':[0,1,2,4,8,15,20],'idle_and_sleep_seconds':[30,60,120,180,300,1200,1500,1800],'LOD_microns':[1000,2000],'not_enabled':['far distance','0.7mm LOD','sensor model selection','other unverified advanced fields']}
(Path(__file__).resolve().parents[1]/'docs/development/2026-10-28-x1lite-performance-source.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');print('Pinned X1 Lite performance layout and exact OEM UI choices verified statically')
