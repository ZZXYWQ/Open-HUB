import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {Button} from '@/components/ui/button';
import type {MultiDpi,MultiSetting,MultiSource} from '@/lib/multi';
import {KEY_NAMES} from '@/lib/sino';

interface RecoveryItem {key:string;revision:string;record:{version?:number;serial:string|null;device_id:string;source:MultiSource;settings:MultiSetting[];software_input:boolean;shift:{original:MultiDpi;expected:MultiDpi}|null}}
interface RecoveryList {records:RecoveryItem[];errors:string[]}
function describe(setting:MultiSetting){
 if(!('Button'in setting))return JSON.stringify(setting);
 const {id,action}=setting.Button;
 if(typeof action==='string')return `按键 ${id}：${{Default:'默认动作',Disabled:'禁用',Hypershift:'按住 Hypershift',ProfileCycle:'循环板载配置'}[action]}`;
 if('ProfileSelect'in action)return `按键 ${id}：切到板载配置 ${action.ProfileSelect+1}`;
 if('Key'in action){const name=KEY_NAMES.find(([k])=>k===action.Key.usage)?.[1]??`HID ${action.Key.usage}`;return `按键 ${id}：${name}（修饰键掩码 ${action.Key.mods}）`;}
 if('Mouse'in action)return `按键 ${id}：鼠标 ${action.Mouse}`;
 return `按键 ${id}：${JSON.stringify(action)}`;
}
export function MultiRecovery({id,name,serial,source,disabled,onRestored}:{id:string;name:string;serial:string|null;source:MultiSource|null;disabled:boolean;onRestored:()=>void}){
 const [list,setList]=useState<RecoveryList|null>(null),[selected,setSelected]=useState(''),[prepared,setPrepared]=useState(''),[confirmed,setConfirmed]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState(''),[note,setNote]=useState('');
 const generation=useRef(0);
 useEffect(()=>{generation.current++;setList(null);setSelected('');setPrepared('');setConfirmed(false);setError('');setNote('');setBusy(false);},[id]);
 useEffect(()=>{generation.current++;setPrepared('');setConfirmed(false);setBusy(false);},[source?.path,source?.vendor,source?.product,serial]);
 const item=list?.records.find(r=>r.key===selected);
 const mismatch=!!item?.record.serial&&item.record.serial!==serial;
 const stamp=item&&source?JSON.stringify([id,item.key,item.revision,source]):'';
 const run=async(action:'list'|'prepare'|'restore')=>{
  const token=++generation.current;setBusy(true);setError('');setNote('');
  try{
   if(action==='list'){
    setList(null);setSelected('');setPrepared('');setConfirmed(false);
    const result=await invoke<RecoveryList>('multi_recovery_list',{id});
    if(generation.current!==token)return;
    setList(result);setSelected(result.records[0]?.key??'');if(!result.records.length)setNote('此型号没有待恢复记录。');
   }else if(action==='prepare'&&item&&source){
    setPrepared('');setConfirmed(false);await invoke('multi_recovery_observe',{id,source});
    if(generation.current!==token)return;
    setPrepared(stamp);setNote('已开始观察所选输入来源。请移动鼠标并松开所有按钮，再核对下方内容。');
   }else if(action==='restore'&&item&&source&&confirmed&&prepared===stamp){
    await invoke('multi_recovery_restore',{id,key:item.key,revision:item.revision,source,confirmed:true});
    if(generation.current!==token)return;
    setPrepared('');setConfirmed(false);setList(null);setSelected('');setNote('原始按键和记录中的临时 DPI 已恢复；恢复记录已清除。');onRestored();
   }
  }catch(e){if(generation.current===token){setError(String(e));if(action==='list'){setList(null);setPrepared('');setConfirmed(false);}}}
  finally{if(generation.current===token)setBusy(false);}
 };
 return <section aria-label="待恢复按键记录" className="rounded-xl border border-amber-400/30 bg-amber-400/5 p-5 space-y-4">
  <div className="flex flex-wrap items-center justify-between gap-3"><h3 className="font-bold">重启后的按键恢复</h3><Button variant="outline" disabled={disabled||busy} onClick={()=>void run('list')}>查看待恢复记录</Button></div>
  <p className="text-sm text-muted-foreground">恢复目标：{name}；当前序列号：{serial??'设备未提供'}。无序列号时需要你核对原鼠标；海盗船和赛睿的当前路由无法完整读回。手动恢复会把下列原始设置写到当前选择的鼠标，请先停止它的软件动作。</p>
  <p className="break-all text-xs text-muted-foreground">当前设备接口：{id}</p>
  {list?.errors.map((message,i)=><p key={i} role="alert" className="lighting-error">{message}</p>)}
  {!!list?.records.length&&<fieldset disabled={disabled||busy} className="space-y-3">
   <label className="block">待恢复记录<select aria-label="选择待恢复记录" className="ml-3 max-w-full rounded border bg-background p-2" value={selected} onChange={e=>{setSelected(e.target.value);setPrepared('');setConfirmed(false);setError('');setNote('');}}>{list.records.map(r=><option key={r.key} value={r.key}>{r.key} · {r.record.serial??'无序列号'}</option>)}</select></label>
   {item&&<><p className="text-sm">记录序列号：{item.record.serial??'未记录，无法自动核对个体'}；原设备：<span className="break-all">{item.record.device_id}</span></p>
    {item.record.version===2&&<p className="text-sm">此记录包含整张已知按键表。恢复时会一次发送以下所有按键，保留方案中其他自定义动作。</p>}
    <ul className="list-disc pl-5 text-sm space-y-1">{item.record.software_input&&<li>退出软件按键路由，恢复硬件模式</li>}{item.record.settings.map((s,i)=><li key={i}>{describe(s)}</li>)}{item.record.shift&&<li>恢复原始 DPI：{item.record.shift.original.stages.map(s=>`${s.x} × ${s.y}`).join(' / ')}；活动档位 {item.record.shift.original.active+1}</li>}</ul>
    <p className="text-sm text-muted-foreground">请先在上方“鼠标输入来源”选择此鼠标的接口。准备恢复只观察输入，不写入设置。</p>
    {mismatch&&<p role="alert" className="lighting-error">记录序列号与当前鼠标不一致，不能恢复到此设备。</p>}
    <Button variant="outline" disabled={!source||mismatch} onClick={()=>void run('prepare')}>准备恢复所选鼠标</Button>
    {prepared===stamp&&!!stamp&&<><label className="flex gap-2 items-start text-sm"><input aria-label="确认恢复原始设置" type="checkbox" checked={confirmed} onChange={e=>setConfirmed(e.target.checked)}/>我已确认这是原鼠标，已松开所有按钮，并同意用上面列出的原始动作和 DPI 覆盖它当前对应的设置。</label><Button disabled={!confirmed} onClick={()=>void run('restore')}>恢复以上原始设置</Button></>}
   </>}
  </fieldset>}
  {error&&<p role="alert" className="lighting-error">{error}</p>}{note&&<p role="status">{note}</p>}
 </section>;
}
