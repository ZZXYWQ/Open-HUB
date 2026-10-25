import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {api} from '@/lib/api';
import {multiProfile,type MultiPlan,type MultiSource,type MultiView,type MultiSetting} from '@/lib/multi';
import {Button} from '@/components/ui/button';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from '@/components/ui/dialog';
import {KEY_NAMES} from '@/lib/sino';
import {NATIVE_MEDIA_KEYS} from '@/lib/native-models';

interface Target{id:string;name:string;serial:string|null;vendor_id:number;product_id:number}
interface Item{key:string;revision:string;pending:{target:Target;plan:MultiPlan;requested:number;sent:boolean;error:string|null}}
interface List{records:Item[];errors:string[]}
interface Candidate{descriptor:Target;rate:number|null;revision:string;blocked:string|null}
interface Candidates{record:Item;devices:Candidate[];needs_source:boolean;warnings:string[]}
interface SourceInfo{source:MultiSource;kind:string;serial:string|null}
function settingLabel(s:MultiSetting,plan:MultiPlan){
 if('VendorProfile'in s)return 'X11 完整原生配置：DPI、性能、灯效、八个控制项和板载宏';
 if('Dpi'in s)return `DPI：${s.Dpi.stages.map(v=>`${v.x} × ${v.y}`).join(' / ')}，活动档位 ${s.Dpi.active+1}`;
 if('ReportRate'in s)return `目标回报率：${s.ReportRate} Hz`;
 if('LiftOff'in s)return `抬升高度：原生档位 ${s.LiftOff+1}`;
 if('AngleSnapping'in s)return `直线修正：${s.AngleSnapping?'开启':'关闭'}`;
 if('Tuning'in s){const labels:Record<string,string>={debounce_ms:'按键消抖',motion_sync:'Motion Sync',angle_snapping:'直线修正',ripple_control:'波纹修正',led_idle_seconds:'闲置熄灯时间',sensor_sleep_enabled:'自定义传感器休眠',sensor_sleep_seconds:'传感器休眠时间',lift_off_microns:'抬升高度'};return Object.entries(s.Tuning).map(([k,v])=>`${labels[k]??k}：${typeof v==='boolean'?(v?'开启':'关闭'):k==='lift_off_microns'?Number(v)/1000+' mm':v+(k==='debounce_ms'?' ms':' 秒')}`).join('；');}
 if('Button'in s){const a=s.Button.action;let action:string;
  if(typeof a==='string')action=a==='Default'?'默认动作':'禁用';
  else if('Macro'in a)action=`软件宏“${plan.macros?.find(m=>m.id===a.Macro)?.name??a.Macro}”`;
  else if('Key'in a){const modifiers=['左 Ctrl','左 Shift','左 Alt','左 Win','右 Ctrl','右 Shift','右 Alt','右 Win'].filter((_,i)=>a.Key.mods&(1<<i));action=`键盘 ${[...modifiers,KEY_NAMES.find(([k])=>k===a.Key.usage)?.[1]??String(a.Key.usage)].join(' + ')}`;}
  else if('Mouse'in a)action=`鼠标 ${{Left:'左键',Right:'右键',Middle:'中键',Back:'后退',Forward:'前进',WheelLeft:'滚轮左倾',WheelRight:'滚轮右倾'}[a.Mouse]}`;
  else if('Media'in a)action=`媒体 ${NATIVE_MEDIA_KEYS.find(([id])=>id===a.Media)?.[1]??'0x'+a.Media.toString(16)}`;
  else action=typeof a.Dpi==='string'?`DPI ${{Cycle:'循环档位',Up:'提高一档',Down:'降低一档'}[a.Dpi]}`:`按住狙击 DPI ${a.Dpi.Shift}`;
  return `按键 ${s.Button.id}：${action}`;
 }
 const effect=s.Lighting.state.effect;
 const rgb=(color:{r:number;g:number;b:number})=>`RGB(${color.r}, ${color.g}, ${color.b})`;
 let description:string;
 if(effect==='Off')description='关闭';
 else if('Static'in effect)description=`常亮 ${rgb(effect.Static.color)}`;
 else if('Breathing'in effect)description=`单色呼吸 ${rgb(effect.Breathing.color)}`;
 else if('ColorCycle'in effect)description='光谱循环';
 else if('Wave'in effect)description=`波浪 ${effect.Wave.direction==='LeftToRight'?'从左到右':'从右到左'}`;
 else description=`按键响应 ${rgb(effect.Reactive.color)}，渐隐 ${effect.Reactive.duration_ms} ms`;
 return `灯区 ${s.Lighting.zone}：${description}，亮度 ${s.Lighting.state.brightness}%`;
}
export function ReconnectPlans({refreshKey=0}:{refreshKey?:number}){
 const [list,setList]=useState<List|null>(null),[key,setKey]=useState(''),[candidates,setCandidates]=useState<Candidates|null>(null),[target,setTarget]=useState(''),[sources,setSources]=useState<SourceInfo[]>([]),[sourcePath,setSourcePath]=useState(''),[confirmed,setConfirmed]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState(''),[note,setNote]=useState(''),[discard,setDiscard]=useState(false);
 const generation=useRef(0),item=list?.records.find(v=>v.key===key),candidate=candidates?.devices.find(v=>v.descriptor.id===target),source=sources.find(v=>v.source.path===sourcePath)?.source??null;
 const resetSelection=()=>{setCandidates(null);setTarget('');setSources([]);setSourcePath('');setConfirmed(false);};
 useEffect(()=>{if(refreshKey>0)void run('list');},[refreshKey]);
 useEffect(()=>()=>{generation.current++;},[]);
 const run=async(action:'list'|'scan'|'resume'|'retry'|'save'|'discard')=>{
  const token=++generation.current;setBusy(true);setError('');setNote('');
  const current=()=>generation.current===token;
  try{
   if(action==='list'){
    resetSelection();setList(null);setKey('');const result=await invoke<List>('multi_reconnect_list');if(!current())return;
    const data=result??{records:[],errors:[]};setList(data);setKey(data.records[0]?.key??'');if(!data.records.length)setNote('没有待续接设备方案。');
   }else if(action==='scan'&&item){
    resetSelection();const data=await invoke<Candidates>('multi_reconnect_candidates',{key:item.key,revision:item.revision});
    let found:SourceInfo[]=[];try{found=await invoke<SourceInfo[]>('multi_input_sources')??[];}catch(e){if(data.needs_source)data.warnings.push(`输入来源读取失败：${String(e)}`);}
    if(!current())return;setCandidates(data);setSources(found.filter(v=>v.kind==='Vendor HID'&&v.source.vendor===item.pending.plan.vendor&&v.source.product===item.pending.plan.product));
    const matched=data.devices.filter(v=>!v.blocked&&!!item.pending.target.serial&&v.descriptor.serial===item.pending.target.serial);if(matched.length===1)setTarget(matched[0].descriptor.id);
    if(!data.devices.length)setNote('暂未发现匹配型号，请等待鼠标重新连接后再次查看。');
   }else if((action==='resume'||action==='retry')&&item&&candidate&&confirmed&&!candidate.blocked){
    if(action==='resume'){
     const view=await invoke<MultiView>('multi_reconnect_resume',{key:item.key,revision:item.revision,id:candidate.descriptor.id,candidateRevision:candidate.revision,source,confirmed:true});if(!current())return;
     setList(v=>v?{...v,records:v.records.filter(i=>i.key!==item.key)}:v);setKey('');resetSelection();setNote(`${view.descriptor.name} 的剩余 DPI、按键和宏方案已应用；待续接记录已清除。`);
    }else{
     const message=await invoke<string>('multi_reconnect_retry',{key:item.key,revision:item.revision,id:candidate.descriptor.id,candidateRevision:candidate.revision,confirmed:true});const data=await invoke<List>('multi_reconnect_list');if(!current())return;
     setList(data);setKey(data.records.some(v=>v.key===item.key)?item.key:data.records[0]?.key??'');resetSelection();setNote(message);
    }
   }else if(action==='save'&&item){
    await api.saveProfile(multiProfile(item.pending.plan,`${item.pending.target.name} · 待续接方案`));if(current())setNote('整套方案已保存到配置中心；尚未继续写入鼠标。');
   }else if(action==='discard'&&item){
    await invoke('multi_reconnect_discard',{key:item.key,revision:item.revision});if(!current())return;
    setList(v=>v?{...v,records:v.records.filter(i=>i.key!==item.key)}:v);setKey('');resetSelection();setDiscard(false);setNote('已放弃这份待续接记录；已生效的回报率没有自动回退。');
   }
  }catch(e){if(current()){setError(String(e));if(action==='list'){setList(null);setKey('');resetSelection();}else if(/已变化|序列号|标识不唯一|已更换/.test(String(e)))resetSelection();}}
  finally{if(current())setBusy(false);}
 };
 return <section aria-label="待续接设备方案" className="rounded-xl border p-5 space-y-4">
  <div className="flex flex-wrap items-center justify-between gap-3"><h3 className="font-bold">待续接设备方案</h3><Button variant="outline" disabled={busy} onClick={()=>void run('list')}>查看待续接方案</Button></div>
  {list?.errors.map((v,i)=><p role="alert" key={i} className="lighting-error">{v}</p>)}
  {!!list?.records.length&&<fieldset disabled={busy} className="space-y-4">
   <p className="text-sm text-muted-foreground">更改回报率可能让鼠标重新连接。方案已先保存；核对新的设备和输入来源后再继续。发送结果不确定时不会自动重发。</p>
   <label className="block">待续接方案<select aria-label="选择待续接方案" className="ml-3 max-w-full rounded border bg-background p-2" value={key} onChange={e=>{generation.current++;setKey(e.target.value);resetSelection();setError('');setNote('');}}><option value="">请选择</option>{list.records.map(v=><option key={v.key} value={v.key}>{v.pending.target.name} · {v.pending.requested} Hz · {v.pending.target.serial??'无序列号'}</option>)}</select></label>
   {item&&<>
    <div className="rounded-lg bg-muted/40 p-4 text-sm space-y-2"><p>原设备：{item.pending.target.name}；序列号：{item.pending.target.serial??'未提供，续接前须明确选择原鼠标'}。</p><p>{item.pending.sent?'回报率命令已发送，仍需核对新连接的回报率。':'发送结果不确定；请先查看新连接，避免重复发送。'}</p>{item.pending.error&&<p>原发送错误：{item.pending.error}</p>}<ul className="list-disc pl-5">{item.pending.plan.settings.map((v,i)=><li key={i}>{settingLabel(v,item.pending.plan)}</li>)}</ul><p>软件宏：{item.pending.plan.macros?.map(m=>`${m.name}（${m.steps.length} 步）`).join('、')||'无'}</p></div>
    <div className="flex flex-wrap gap-3"><Button variant="outline" onClick={()=>void run('scan')}>查看重连鼠标</Button>{item.pending.plan.settings.some(v=>'Dpi'in v)&&<Button variant="outline" onClick={()=>void run('save')}>保存待续接方案到配置中心</Button>}<Button variant="ghost" onClick={()=>setDiscard(true)}>放弃此待续接方案</Button></div>
    {candidates&&<div className="rounded-lg border p-4 space-y-4">{candidates.warnings.map((v,i)=><p role="alert" key={i} className="lighting-error">{v}</p>)}
     <label className="block">重连后的鼠标<select aria-label="重连后的目标鼠标" className="ml-3 max-w-full rounded border bg-background p-2" value={target} onChange={e=>{setTarget(e.target.value);setSourcePath('');setConfirmed(false);}}><option value="">请选择；没有序列号时不能自动判断是哪一只</option>{candidates.devices.map(v=><option key={v.descriptor.id} value={v.descriptor.id} disabled={!!v.blocked}>{v.descriptor.name} · {v.descriptor.serial??v.descriptor.id} · {v.rate??'?'} Hz{v.blocked?` · ${v.blocked}`:''}</option>)}</select></label>
     {candidate&&<><p className="text-sm break-all">当前接口：{candidate.descriptor.id}；新连接回报率：{candidate.rate??'未知'} Hz；目标：{item.pending.requested} Hz。</p>
      {candidates.needs_source&&<label className="block">重新选择鼠标输入来源<select aria-label="重连后的鼠标输入来源" className="ml-3 max-w-full rounded border bg-background p-2" value={sourcePath} onChange={e=>{setSourcePath(e.target.value);setConfirmed(false);}}><option value="">续接按键和宏前，请选择此鼠标当前的输入接口</option>{sources.filter(v=>!candidate.descriptor.serial||!v.serial||v.serial===candidate.descriptor.serial).map(v=><option key={v.source.path} value={v.source.path}>{v.kind} · {v.serial??v.source.path}</option>)}</select></label>}
      <label className="flex gap-2 items-start text-sm"><input aria-label="确认重连目标" type="checkbox" checked={confirmed} onChange={e=>setConfirmed(e.target.checked)}/>我已核对这是原鼠标，并同意向此设备应用上面列出的待续接设置。</label>
      <div className="flex flex-wrap gap-3"><Button disabled={!confirmed||!!candidate.blocked||candidate.rate!==item.pending.requested||(candidates.needs_source&&!source)} onClick={()=>void run('resume')}>继续应用整套方案</Button>{candidate.rate!==item.pending.requested&&<Button variant="outline" disabled={!confirmed||!!candidate.blocked} onClick={()=>void run('retry')}>明确重试一次回报率</Button>}</div>
     </>}
    </div>}
   </>}
  </fieldset>}
  {error&&<p role="alert" className="lighting-error">{error}</p>}{note&&<p role="status">{note}</p>}
  <Dialog open={discard} onOpenChange={v=>{if(!busy)setDiscard(v);}}><DialogContent><DialogHeader><DialogTitle>放弃待续接方案？</DialogTitle><DialogDescription>只删除这份待续接记录，已发送的回报率不会自动回退。未另存到配置中心的修改会随此记录删除。</DialogDescription></DialogHeader><div className="flex gap-3"><Button variant="outline" disabled={busy} onClick={()=>setDiscard(false)}>保留方案</Button><Button disabled={busy} onClick={()=>void run('discard')}>确认放弃待续接记录</Button></div></DialogContent></Dialog>
 </section>;
}
