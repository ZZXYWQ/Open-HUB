import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {Palette} from 'lucide-react';
import {Button} from '@/components/ui/button';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from '@/components/ui/dialog';
import {DeviceArt} from '@/components/device-art';
import type {DeviceSummary} from '@/lib/api';
import {zoneName,currentEffect,colorHex,parseColor,type LogitechLightView,type LogitechLightEffect} from '@/lib/logitech-lighting';
export function LogitechLighting({device}:{device:DeviceSummary}){
 const [open,setOpen]=useState(false),[view,setView]=useState<LogitechLightView|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(''),[note,setNote]=useState('');
 const [edits,setEdits]=useState<Record<number,{kind:'Keep'|'Off'|'Fixed';color:string}>>({});
 const current=useRef(true),generation=useRef(0),loadedFor=useRef('');
 useEffect(()=>{current.current=true;return()=>{current.current=false;generation.current++;};},[]);
 const read=async()=>{const turn=++generation.current;setBusy(true);setError('');setNote('');try{const value=await invoke<LogitechLightView>('read_logitech_lighting',{id:device.id});if(!current.current||turn!==generation.current)return;setView(value);loadedFor.current=device.id;setEdits({});}catch(e){if(current.current&&turn===generation.current){setView(null);loadedFor.current='';setError(String(e));}}finally{if(current.current&&turn===generation.current)setBusy(false);}};
 const apply=async(zone:number)=>{if(!view||loadedFor.current!==device.id)return;const selected=edits[zone],row=view.zones.find(z=>z.index===zone);if(!selected||selected.kind==='Keep'||!row?.current)return;const effect:LogitechLightEffect=selected.kind==='Off'?'Off':{Fixed:{color:parseColor(selected.color)}};const turn=++generation.current;setBusy(true);setError('');setNote('');try{const result=await invoke<{view:LogitechLightView;unchanged:boolean}>('apply_logitech_lighting',{id:device.id,change:{zone,expected:row.current,effect}});if(!current.current||turn!==generation.current)return;setView(result.view);setEdits(e=>({...e,[zone]:{...e[zone],kind:'Keep'}}));setNote(result.unchanged?'灯区已是所选状态':'此灯区已修改并通过回读校验');}catch(e){if(current.current&&turn===generation.current)setError(String(e));}finally{if(current.current&&turn===generation.current)setBusy(false);}};
 return <>
  <Button variant="outline" onClick={()=>{setOpen(true);void read();}}><Palette size={16}/>打开鼠标灯光</Button>
  <Dialog open={open} onOpenChange={value=>{if(!busy)setOpen(value);}}><DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-3xl"><DialogHeader><DialogTitle>罗技原生灯光</DialogTitle><DialogDescription>{device.name} · 灯区与效果由这只鼠标报告</DialogDescription></DialogHeader>
   {error&&<p role="alert" className="lighting-error">{error}</p>}{note&&<p role="status">{note}</p>}
   <div className="flex flex-wrap items-start gap-6"><DeviceArt kind="mouse" vendorId={device.vendor_id} productId={device.product_id} name={device.name} className="h-56 w-36 shrink-0"/><div className="min-w-64 flex-1 space-y-4">
    <p className="text-sm text-muted-foreground">当前可调整已核对的关闭与常亮颜色。修改即时生效，断电后由鼠标原有板载灯效接管。高级原厂效果可读取名称，参数仍保留。</p>
    {view&&!view.writable&&<p role="status" className="text-sm text-amber-500">此连接或固件暂只读；写入需要直接 USB 和可校验的灯效状态。</p>}
    {busy&&!view&&<p role="status">正在读取鼠标灯区…</p>}
    {view&&view.zones.length===0&&<p role="status">鼠标没有报告可用灯区。</p>}
    {view?.zones.map(zone=>{const effect=currentEffect(zone),edit=edits[zone.index]??{kind:'Keep' as const,color:effect===1&&zone.current?colorHex(zone.current.slice(1,4)):'#6ce5c8'},supported=(id:number)=>zone.effects.filter(e=>e.effect_id===id).length===1,put=(patch:Partial<typeof edit>)=>setEdits(e=>({...e,[zone.index]:{...edit,...patch}}));const effectName=effect===0?'关闭':effect===1?'常亮':({2:'脉冲',3:'光谱循环',4:'波浪',5:'星光',6:'按压亮起',7:'音频效果',8:'启动效果',10:'呼吸',11:'涟漪',12:'自定义'} as Record<number,string>)[effect??-1]??'原厂效果';return <fieldset disabled={busy} key={zone.index} className="rounded-xl border bg-card/50 p-4 space-y-3" aria-label={`罗技灯区${zone.index+1}`}>
     <div className="flex items-center justify-between"><h3 className="font-semibold">{zoneName(zone)}</h3><span className="text-xs text-muted-foreground">{zone.current?`当前：${effectName}`:'当前状态不可读'}</span></div>
     <div className="flex flex-wrap items-center gap-3"><label>灯效 <select aria-label={`罗技灯区${zone.index+1}灯效`} className="rounded border bg-background px-3 py-2" value={edit.kind} onChange={e=>put({kind:e.target.value as typeof edit.kind})}><option value="Keep">保留当前</option>{supported(0)&&<option value="Off">关闭</option>}{supported(1)&&<option value="Fixed">常亮</option>}</select></label>{edit.kind==='Fixed'&&<label className="flex items-center gap-2">颜色 <input type="color" aria-label={`罗技灯区${zone.index+1}颜色`} value={edit.color} onChange={e=>put({color:e.target.value})}/></label>}<Button disabled={!view.writable||!zone.current||!zone.effects.some(e=>e.index===zone.current?.[0])||edit.kind==='Keep'} onClick={()=>void apply(zone.index)}>应用此灯区</Button></div>
    </fieldset>;})}
    <Button variant="ghost" disabled={busy} onClick={()=>void read()}>重新读取灯区</Button>
   </div></div>
  </DialogContent></Dialog>
 </>;
}
