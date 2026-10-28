"""Static exact-model audit against the retained pinned OEM source."""
from pathlib import Path
import hashlib,json
p=Path(__file__).resolve().parents[2]/'research-atk/official-atk-3.2.27.js';raw=p.read_bytes();assert hashlib.sha256(raw).hexdigest()=='71a1ba1f86721609328c02ab3e69bfe8acf9e1aed84f328a94eeb1b2955b64b5';s=raw.decode('utf-8')
record='vendorId:14139,productId:4414,usagePage:65284,usage:2,custom:{name:"ATK X1 Lite",sensor:mt.PAW3395,skin:[...V1],mouseCidMid:"2,89",firmwareMark:"x1",noFarDistance:!0}'
assert s.count(record)==1
assert 'class ko extends JS' in s
assert 'JS=class extends Wi' in s
assert 'transferForResultAsync(e.toUint8Array(),8)' in s[s.index('JS=class extends Wi'):s.index('class ko extends JS')]
assert 'PAW3395:{max:3e4,min:100,xStep:50,yStep:50}' in s
assert 'productId:4119,usagePage:65284,usage:2,custom:{name:"ATK X1 PRO MAX",sensor:mt.PAW3950,skin:[...V1]' in s
out={'source_url':'https://bpcdn.atkgear.com/hub-v3/production/3.2.27/static/index-DBE__Npj.js','sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw),'static_only':True,'model':'ATK X1 Lite','vid':14139,'pid':4414,'usage_page':65284,'usage':2,'controller':'ko extends JS','report_id':8,'cid':2,'mid':89,'sensor':'PAW3395','sensor_limits':{'min':100,'max':30000,'step':50},'skin':'V1, shared with X1 Pro Max OEM entry','noFarDistance':True,'limits':['enabled polling subset 125/250/500/1000 is not an OEM maximum claim','performance layout not yet admitted for this model','no physical firmware or full OEM parity proof']}
(Path(__file__).resolve().parents[1]/'docs/development/2026-10-27-x1lite-source.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');print('Pinned X1 Lite exact USB collection, CID/MID, controller, sensor and shared OEM artwork family verified')
