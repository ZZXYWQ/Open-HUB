"""Audit retained OEM protocol facts without executing or redistributing its JS."""
import hashlib,json,re
from pathlib import Path
source=Path(__file__).resolve().parents[2]/'research-atk'/'official-atk-3.2.27.js'
raw=source.read_bytes()
assert hashlib.sha256(raw).hexdigest()=='71a1ba1f86721609328c02ab3e69bfe8acf9e1aed84f328a94eeb1b2955b64b5'
s=raw.decode('utf-8')
a=s.index('class fb extends go');b=s.index('class hb extends go',a);definition=s[a:b]
assert 'a[a.GetMouseCIDMID=16]' in s
assert 'static get(){return new fb(fb.getData(),Za.GetMouseCIDMID)}' in definition
assert 'get cid(){return this.raw.getUint8(this.baseOffset)}' in definition
assert 'get mid(){return this.raw.getUint8(this.baseOffset+1)}' in definition
getter=definition[definition.index('get uniqueId()'):definition.index('static get()')]
assert [int(x) for x in re.findall(r'this.baseOffset\+(\d+)',getter)]==[6,7,8,9]
assert '.join(".")' in getter
result={'source_url':'https://bpcdn.atkgear.com/hub-v3/production/3.2.27/static/index-DBE__Npj.js','source_sha256':hashlib.sha256(raw).hexdigest(),'source_bytes':len(raw),'analysis':'static text only, no OEM execution or redistribution','command':16,'CID_offset':0,'MID_offset':1,'opaque_unit_token_offsets':[6,7,8,9],'oem_getter':'fb.uniqueId','getter_offset_utf16':len(s[:a+definition.index('get uniqueId()')].encode('utf-16-le'))//2,'not_proven':['global uniqueness','permanent mouse serial across all firmware and transports','meaning of all-zero or all-FF values','legacy two-byte response carries any physical identity','safe persisted routing or host macro authorization']}
output=Path(__file__).resolve().parents[1]/'docs'/'development'/'2026-10-27-compx-unit-identity-source.json'
output.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print('Pinned OEM CID/MID and four opaque uniqueId byte offsets verified; no OEM code executed')
