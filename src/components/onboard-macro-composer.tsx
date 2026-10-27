import {open,save} from '@tauri-apps/plugin-dialog';
import {useEffect,useRef,useState} from 'react';
import {Button} from '@/components/ui/button';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from '@/components/ui/dialog';
import {api,type OnboardProfile,type OnboardInfo} from '@/lib/api';
import {KEY_NAMES} from '@/lib/sino';
import type {NativeMacroRequest} from '@/lib/logitech-macro-plan';
type Mouse='Left'|'Right'|'Middle'|'Back'|'Forward';
type Event={KeyTap:{usage:number;mods:number}}|{KeyDown:number}|{KeyUp:number}|{MouseDown:Mouse}|{MouseUp:Mouse}|{Media:number}|{Delay:number};
const defaults:Record<string,Event>={KeyTap:{KeyTap:{usage:6,mods:1}},KeyDown:{KeyDown:4},KeyUp:{KeyUp:4},MouseDown:{MouseDown:'Back'},MouseUp:{MouseUp:'Back'},Media:{Media:0xe2},Delay:{Delay:77}};
const kinds:Record<string,string>={KeyTap:'快捷键点按',KeyDown:'键盘按下',KeyUp:'键盘松开',MouseDown:'鼠标按下',MouseUp:'鼠标松开',Media:'媒体点按',Delay:'延迟'};
const media:Array<[number,string]>=[[0xe2,'静音'],[0xcd,'播放 / 暂停'],[0xb5,'下一曲'],[0xb6,'上一曲'],[0xb7,'停止'],[0xe9,'增大音量'],[0xea,'减小音量'],[0x192,'计算器'],[0x223,'浏览器主页']];
const modifiers=['左 Ctrl','左 Shift','左 Alt','左 Win','右 Ctrl','右 Shift','右 Alt','右 Win'];
const input='rounded border bg-background px-2 py-1.5 text-sm';
export function OnboardMacroComposer({profile,info,deviceKey,connectionKey,deviceId,deviceName,onClose,onSave,onRefresh}:{profile:OnboardProfile;info:OnboardInfo;deviceKey:string;connectionKey:string;deviceId:string;deviceName:string;onClose:()=>void;onSave:(r:NativeMacroRequest)=>Promise<boolean>;onRefresh:()=>Promise<void>}){
 const [id,setId]=useState(()=>`logitech-native:${crypto.randomUUID()}`),[name,setName]=useState('新的板载宏'),[steps,setSteps]=useState<Event[]>([{KeyTap:{usage:6,mods:1}},{Delay:77},{Media:0xe2}]),[button,setButton]=useState(Math.min(4,info.button_count)),[layer,setLayer]=useState<'Normal'|'GShift'>('Normal'),[busy,setBusy]=useState(false),[error,setError]=useState(''),[checked,setChecked]=useState('');const generation=useRef(0);
 const [mode,setMode]=useState<'Once'|'Repeat'>('Once'),[repeat,setRepeat]=useState(2),[checkedPages,setCheckedPages]=useState(0);
 const [fileNote,setFileNote]=useState('');
 const records=layer==='Normal'?profile.buttons:profile.gshift_buttons;
 const request:NativeMacroRequest={selection:{expected_device:deviceKey,expected_connection:connectionKey,slot:profile.slot,sector:profile.sector,layer,button,expected:records[button-1]?.raw??[]},definition:{id,name,mode:mode==='Once'?'Once':{Repeat:repeat},steps}};
 const signature=JSON.stringify(request);
 useEffect(()=>{generation.current++;setChecked('');setError('');setBusy(false);return()=>{generation.current++;};},[signature]);
 const refresh=async()=>{const turn=++generation.current;setBusy(true);setChecked('');setError('');try{await onRefresh();}catch(e){if(turn===generation.current)setError(String(e));}finally{if(turn===generation.current)setBusy(false);}};
 const macroFile=async(importing:boolean)=>{
  const turn=++generation.current;setBusy(true);setError('');setChecked('');setFileNote('');
  try{
   const path=importing?await open({multiple:false,filters:[{name:'Open HUB 罗技板载宏',extensions:['json']}]}):await save({defaultPath:'Open-HUB-罗技板载宏.json',filters:[{name:'Open HUB 罗技板载宏',extensions:['json']}]});
   if(typeof path!=='string'||turn!==generation.current)return;
   if(importing){const value=await api.readLogitechMacroFile(path);if(turn!==generation.current)return;setId(value.id);setName(value.name);setMode(typeof value.mode==='object'?'Repeat':'Once');if(typeof value.mode==='object')setRepeat(value.mode.Repeat);setSteps(value.steps as Event[]);setFileNote('宏文件已载入；当前鼠标、模式层和触发按键保持不变，请重新检查后保存。');}
   else{await api.writeLogitechMacroFile(path,request.definition);if(turn===generation.current)setFileNote('宏文件已保存；未写入鼠标。');}
  }catch(e){if(turn===generation.current)setError((importing?'宏文件未载入，原草稿保留：':'宏文件未保存：')+String(e));}
  finally{if(turn===generation.current)setBusy(false);}
 };
 const put=(index:number,event:Event)=>setSteps(old=>old.map((v,i)=>i===index?event:v));
 const preview=async()=>{const turn=++generation.current;setBusy(true);setError('');setChecked('');try{const result=await api.previewOnboardMacro(deviceId,request);if(turn===generation.current){setCheckedPages(result.page_count);setChecked(signature);}}catch(e){if(turn===generation.current)setError(String(e));}finally{if(turn===generation.current)setBusy(false);}};
 return <Dialog open onOpenChange={value=>{if(!value&&!busy)onClose();}}><DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-3xl"><DialogHeader><DialogTitle>新增罗技板载宏 · 槽 {profile.slot}</DialogTitle><DialogDescription>{deviceName} · 单次或固定次数播放，保存在这只鼠标中。先核对空闲板载空间，再保存；其他槽和原厂宏保持原样。</DialogDescription></DialogHeader>
 <div className="flex flex-wrap gap-2"><Button variant="outline" disabled={busy} onClick={()=>void macroFile(true)}>导入宏文件</Button><Button variant="outline" disabled={busy||!name.trim()} onClick={()=>void macroFile(false)}>导出宏文件</Button></div>{fileNote&&<p role="status" className="text-xs text-muted-foreground">{fileNote}</p>}
 {error&&<p role="alert" className="text-sm text-destructive">{error}</p>}{checked===signature&&<p role="status" className="text-sm text-primary">检查通过：此宏可以保存到当前鼠标。需要 {checkedPages} 页板载空间。</p>}
 <fieldset disabled={busy} className="space-y-4"><div className="flex flex-wrap items-center gap-3"><label className="space-x-2">名称<input className={input} aria-label="罗技板载宏名称" maxLength={64} value={name} onChange={e=>setName(e.target.value)}/></label><label className="space-x-2">模式层<select className={input} aria-label="板载宏模式层" value={layer} onChange={e=>setLayer(e.target.value as typeof layer)}><option value="Normal">普通层</option>{(info.mechanical_layout&3)===2&&<option value="GShift">G-Shift 层</option>}</select></label><label className="space-x-2">触发按键<select className={input} aria-label="板载宏触发按键" value={button} onChange={e=>setButton(Number(e.target.value))}>{records.slice(0,info.button_count).map((_,i)=><option key={i} value={i+1}>按键 {i+1}</option>)}</select></label></div>
 <div className="flex flex-wrap items-center gap-3"><label>播放方式 <select aria-label="罗技板载宏播放方式" className={input} value={mode} onChange={e=>setMode(e.target.value as 'Once'|'Repeat')}><option value="Once">单次播放</option><option value="Repeat">固定次数</option></select></label>{mode==='Repeat'&&<label>次数 <input aria-label="罗技板载宏重复次数" className={`${input} w-24`} type="number" min={2} max={255} value={repeat} onChange={e=>setRepeat(Number(e.target.value))}/></label>}</div>
 <ol className="space-y-2" aria-label="新增板载宏事件">{steps.map((step,index)=>{const kind=Object.keys(step)[0],label=`板载宏事件${index+1}`,keyUsage='KeyTap'in step?step.KeyTap.usage:'KeyDown'in step?step.KeyDown:'KeyUp'in step?step.KeyUp:null;return <li key={index} className="flex flex-wrap items-center gap-2 rounded-lg border p-3"><span className="w-5 text-xs tabular-nums">{index+1}</span><select className={input} aria-label={`${label}类型`} value={kind} onChange={e=>put(index,JSON.parse(JSON.stringify(defaults[e.target.value])))}>{Object.entries(kinds).map(([k,v])=><option key={k} value={k}>{v}</option>)}</select>
 {('KeyTap'in step||'KeyDown'in step||'KeyUp'in step)&&<select className={input} aria-label={`${label}键码`} value={keyUsage??4} onChange={e=>{const n=Number(e.target.value);put(index,'KeyTap'in step?{KeyTap:{...step.KeyTap,usage:n}}:'KeyDown'in step?{KeyDown:n}:{KeyUp:n});}}>{KEY_NAMES.filter(([k])=>k>=4&&k<=0xa4||!('KeyTap'in step)&&k>=0xe0&&k<=0xe7).map(([k,n])=><option key={k} value={k}>{n}</option>)}{keyUsage!=null&&!KEY_NAMES.some(([k])=>k===keyUsage)&&<option value={keyUsage}>键位 0x{keyUsage.toString(16).toUpperCase()}</option>}</select>}
 {'KeyTap'in step&&modifiers.map((mod,n)=><label key={n} className="flex items-center gap-1 text-xs"><input type="checkbox" aria-label={`${label}${mod}`} checked={!!(step.KeyTap.mods&(1<<n))} onChange={e=>put(index,{KeyTap:{...step.KeyTap,mods:e.target.checked?step.KeyTap.mods|(1<<n):step.KeyTap.mods&~(1<<n)}})}/>{mod}</label>)}
 {('MouseDown'in step||'MouseUp'in step)&&<select className={input} aria-label={`${label}鼠标键`} value={'MouseDown'in step?step.MouseDown:step.MouseUp} onChange={e=>put(index,'MouseDown'in step?{MouseDown:e.target.value as Mouse}:{MouseUp:e.target.value as Mouse})}>{(['Left','Right','Middle','Back','Forward'] as Mouse[]).map((b,i)=><option key={b} value={b}>{['左键','右键','中键','后退','前进'][i]}</option>)}</select>}
 {'Media'in step&&<select className={input} aria-label={`${label}媒体键`} value={step.Media} onChange={e=>put(index,{Media:Number(e.target.value)})}>{media.map(([v,n])=><option key={v} value={v}>{n}</option>)}</select>}
 {'Delay'in step&&<label className="flex items-center gap-2"><input className={`${input} w-24`} aria-label={`${label}延迟`} type="number" min={0} max={10000} value={step.Delay} onChange={e=>put(index,{Delay:Number(e.target.value)})}/>毫秒</label>}
 <Button variant="ghost" size="sm" aria-label={`删除${label}`} disabled={steps.length<=1} onClick={()=>setSteps(old=>old.filter((_,i)=>i!==index))}>删除</Button></li>;})}</ol>
 <Button variant="outline" disabled={steps.length>=512} onClick={()=>setSteps(old=>[...old,{KeyTap:{usage:4,mods:0}}])}>新增事件</Button>
 <p className="text-xs text-muted-foreground">实验性功能，尚未实机验证。支持单次与固定次数，所需空间随次数增加；较长内容自动衔接空闲空间。按下与松开必须成对，支持左右独立修饰键；点按组合不能包含当前已按住的同一修饰键。没有空闲空间、原厂宏无法完整核对、或设备状态已改变时会拒绝保存。</p>
 <div className="flex flex-wrap justify-end gap-2"><Button variant="ghost" onClick={onClose}>取消</Button><Button variant="outline" onClick={()=>void refresh()}>重新读取鼠标，保留草稿</Button><Button variant="outline" disabled={!name.trim()} onClick={()=>void preview()}>检查板载宏</Button><Button disabled={checked!==signature||!name.trim()} onClick={async()=>{setBusy(true);try{if(await onSave(request))onClose();}finally{setBusy(false);}}}>{busy?'正在处理…':'保存到此鼠标'}</Button></div>
 </fieldset></DialogContent></Dialog>;
}
