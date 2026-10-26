import {useRef,useState} from 'react';
import type {MchoseMacro,MchoseMacroEvent} from '@/lib/api';
import {MCHOSE_KEYS,mchoseKeyUsage} from '@/lib/mchose-keys';
import {MCHOSE_MACRO_MODES} from '@/lib/mchose-macros';
import {Button} from '@/components/ui/button';
import {Input} from '@/components/ui/input';
const pick='h-9 rounded-md border border-input bg-background px-2 text-sm';
type CompleteMacro=MchoseMacro&{raw:number[]};
const keyEvent=(code:number,down:boolean):MchoseMacroEvent=>({kind:'key',code,down,delay_ms:0});
/** Draft-only: no device calls, hooks, synthesized input or background recording. */
export function MchosePlanMacroEditor({value,onChange}:{value:CompleteMacro;onChange:(value:CompleteMacro)=>void}) {
 const [key,setKey]=useState(4),[mouse,setMouse]=useState(1),[recording,setRecording]=useState(false),[hint,setHint]=useState('');
 const held=useRef(new Set<number>()),buffer=useRef<MchoseMacroEvent[]>([]),last=useRef(0),live=useRef(false),capture=useRef<HTMLDivElement>(null);
 const edit=(index:number,patch:Partial<MchoseMacroEvent>)=>onChange({...value,events:value.events.map((event,i)=>i===index?{...event,...patch}:event)});
 const append=(events:MchoseMacroEvent[])=>onChange({...value,events:[...value.events,...events]});
 const finish=()=>{if(!live.current)return;live.current=false;const events=[...buffer.current,...[...held.current].map(code=>keyEvent(code,false))];held.current.clear();setRecording(false);onChange({...value,events});setHint('录制已停止；所有按住的键已补齐释放事件。');};
 const record=(e:React.KeyboardEvent,down:boolean)=>{
  if(!live.current)return;e.preventDefault();e.stopPropagation();if(e.code==='Escape'){finish();return;}
  const code=mchoseKeyUsage(e.code);if(code==null){setHint(`尚未核对按键 ${e.code}，未加入。`);return;}
  if(down?held.current.has(code):!held.current.has(code))return;
  if(buffer.current.length+held.current.size>=995){finish();return;}
  const now=performance.now(),delay=Math.min(65535,Math.round(now-last.current));last.current=now;
  if(buffer.current.length&&delay>0)buffer.current.push({kind:'delay',code:0,down:false,delay_ms:delay});
  if(down)held.current.add(code);else held.current.delete(code);buffer.current.push(keyEvent(code,down));setHint(`已录制 ${buffer.current.length} 条事件；按 Esc 或移开焦点停止。`);
 };
 return <div className="space-y-3">
  <label className="grid gap-1 text-sm">宏名称<Input aria-label="方案宏名称" value={value.name} onChange={e=>onChange({...value,name:e.target.value})}/></label>
  <label className="grid gap-1 text-sm">播放方式<select className={pick} aria-label="方案宏播放方式" value={value.mode} onChange={e=>onChange({...value,mode:Number(e.target.value)})}>{!MCHOSE_MACRO_MODES.some(m=>m.value===value.mode)&&<option value={value.mode}>保留当前模式 {value.mode}</option>}{MCHOSE_MACRO_MODES.map(m=><option key={m.value} value={m.value}>{m.label}</option>)}</select></label>
  <p className="text-xs text-muted-foreground">{MCHOSE_MACRO_MODES.find(m=>m.value===value.mode)?.description}。只修改配置草稿，保存前核对完整按下/释放和原厂宏内容。</p>
  <div className="flex flex-wrap items-center gap-2"><select className={pick} aria-label="新增宏键盘键" value={key} onChange={e=>setKey(Number(e.target.value))}>{MCHOSE_KEYS.map(k=><option key={k.usage} value={k.usage}>{k.label}</option>)}</select><Button variant="outline" onClick={()=>append([keyEvent(key,true),keyEvent(key,false)])}>添加键盘点按</Button><select className={pick} aria-label="新增宏鼠标键" value={mouse} onChange={e=>setMouse(Number(e.target.value))}>{[[1,'左键'],[2,'右键'],[4,'中键'],[8,'后退'],[16,'前进']].map(([code,label])=><option key={code} value={code}>{label}</option>)}</select><Button variant="outline" onClick={()=>append([{kind:'mouse',code:mouse,down:true,delay_ms:0},{kind:'mouse',code:mouse,down:false,delay_ms:0}])}>添加鼠标点按</Button><Button variant="outline" onClick={()=>append([{kind:'delay',code:0,down:false,delay_ms:50}])}>添加延时</Button><Button variant="outline" onClick={()=>append([{kind:'scroll',code:0,down:true,delay_ms:0}])}>添加滚动</Button></div>
  <div className="flex flex-wrap gap-2"><Button variant="outline" onClick={()=>{buffer.current=[];held.current.clear();last.current=performance.now();live.current=true;setRecording(true);setHint('仅在录制区有焦点时记录，Esc 停止。');capture.current?.focus();}}>重新录制键盘事件</Button>{recording&&<Button onClick={finish}>停止录制</Button>}</div>
  <div ref={capture} data-mchose-recording={recording?"true":undefined} tabIndex={0} role="group" aria-label="方案宏录制区域" className="rounded-md border border-dashed p-3 text-sm outline-none focus:ring-2 focus:ring-primary" onKeyDown={e=>record(e,true)} onKeyUp={e=>record(e,false)} onBlur={finish}>{recording?'正在录制；按 Esc 停止':'键盘录制区'}{hint&&<p role="status" className="mt-1 text-xs text-muted-foreground">{hint}</p>}</div>
  <div className="max-h-72 overflow-y-auto rounded-md border" aria-label="方案宏事件列表">{value.events.map((event,i)=><div key={i} className="flex flex-wrap items-center gap-2 border-b p-2 text-xs"><span className="w-6 tabular-nums">{i+1}</span><select aria-label={`宏事件 ${i+1} 类型`} className={pick} value={event.kind} onChange={e=>edit(i,{kind:e.target.value as MchoseMacroEvent['kind'],code:e.target.value==='delay'?0:e.target.value==='key'?4:e.target.value==='mouse'?1:0,down:e.target.value!=='delay',delay_ms:e.target.value==='delay'?50:0})}>{[['key','键盘'],['mouse','鼠标'],['scroll','滚动'],['delay','延时']].map(([v,label])=><option value={v} key={v}>{label}</option>)}</select>
  {event.kind==='delay'?<label>毫秒<Input className="inline-block w-24" aria-label={`宏事件 ${i+1} 延时`} type="number" min={0} max={65535} value={event.delay_ms} onChange={e=>edit(i,{delay_ms:Number(e.target.value)})}/></label>:<><select aria-label={`宏事件 ${i+1} 代码`} className={pick} value={event.code} onChange={e=>edit(i,{code:Number(e.target.value)})}>{event.kind==='key'?<>{!MCHOSE_KEYS.some(k=>k.usage===event.code)&&<option value={event.code}>保留代码 {event.code}</option>}{MCHOSE_KEYS.map(k=><option key={k.usage} value={k.usage}>{k.label}</option>)}</>:event.kind==='mouse'?[[1,'左键'],[2,'右键'],[4,'中键'],[8,'后退'],[16,'前进']].map(([v,label])=><option key={v} value={v}>{label}</option>):[[0,'向上'],[1,'向下']].map(([v,label])=><option key={v} value={v}>{label}</option>)}</select><select className={pick} aria-label={`宏事件 ${i+1} 状态`} value={String(event.down)} onChange={e=>edit(i,{down:e.target.value==='true'})}><option value="true">按下</option><option value="false">释放</option></select>{event.delay_ms!==0&&<span>保留原始附加字段 {event.delay_ms}</span>}</>}
  <Button size="sm" variant="ghost" disabled={i===0} aria-label={`上移宏事件 ${i+1}`} onClick={()=>{const events=[...value.events];[events[i-1],events[i]]=[events[i],events[i-1]];onChange({...value,events});}}>↑</Button><Button size="sm" variant="ghost" aria-label={`删除宏事件 ${i+1}`} onClick={()=>onChange({...value,events:value.events.filter((_,j)=>j!==i)})}>删除</Button></div>)}</div>
 </div>;
}
