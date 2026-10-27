"""Pinned static protocol audit; never execute vendor JavaScript."""
from pathlib import Path
import hashlib,json
p=Path(__file__).resolve().parents[2]/'research-atk'/'official-atk-3.2.27.js'
raw=p.read_bytes();assert hashlib.sha256(raw).hexdigest()=='71a1ba1f86721609328c02ab3e69bfe8acf9e1aed84f328a94eeb1b2955b64b5'
s=raw.decode('utf-8');a=s.index('let zv=');b=s.index('get value1()',a);codec=s[a:b]
assert 'Ct.KeyType.Scroll&&(e=e|4|2|0)' in codec
assert 't===Ct.KeyState.Scroll?e=e&-193' in codec
ko=s[s.index('class ko extends'):];a=ko.index('async setMacro(e,n)');b=ko.index('async getProfile()',a);writer=ko[a:b]
for fact in ['I=M===ho.KeyWheelDown?255:1','M=Ct.KeyState.Scroll','keyType:x,value1:I,value2:0','keyType:x,value1:0,value2:0','c.push(zv.create','c.length','p.push(g)']:assert fact in writer,fact
out={'source_url':'https://bpcdn.atkgear.com/hub-v3/production/3.2.27/static/index-DBE__Npj.js','source_sha256':hashlib.sha256(raw).hexdigest(),'source_bytes':len(raw),'analysis':'static only; no OEM execution or distribution','state_bits':'neither Down 0x80 nor Up 0x40','event_class':6,'value_up':1,'value_down':255,'neutral_value':0,'paired_events':True,'value2':0,'delay':'big-endian16 at event offsets3-4','implemented_canonical_form':'one-tick event with delay1, neutral event with optional user delay','not_proven':['other scroll magnitudes or timings','cross-bank macro ownership','physical wheel playback timing','in-flight macro quiescence']}
(Path(__file__).resolve().parents[1]/'docs/development/2026-10-27-compx-scroll-macro-source.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');print('Pinned COMPX scroll class, direction values and neutral pair verified statically')
