import {useEffect,useState} from 'react';
import {MousePointer2,Save} from 'lucide-react';
import {Button} from '@/components/ui/button';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from '@/components/ui/dialog';
import {DeviceArt} from '@/components/device-art';
import {KEY_NAMES} from '@/lib/sino';
import type {DeviceSummary,OnboardProfile,OnboardInfo,OnboardBindingAction,OnboardBindingChange,OnboardBindingSettings} from '@/lib/api';
const style='rounded border bg-background p-2 text-sm';
const specials={WheelLeft:'滚轮左倾',WheelRight:'滚轮右倾',DpiUp:'提高 DPI',DpiDown:'降低 DPI',DpiCycle:'循环 DPI',DpiDefault:'默认 DPI',DpiShift:'按住 DPI Shift',ProfileUp:'下一个板载槽',ProfileDown:'上一个板载槽',ProfileCycle:'循环板载槽',GShift:'按住 G-Shift'};
const media:[[number,string],...Array<[number,string]>]=[[0xcd,'播放 / 暂停'],[0xb5,'下一曲'],[0xb6,'上一曲'],[0xb7,'停止播放'],[0xe2,'静音'],[0xe9,'增大音量'],[0xea,'减小音量'],[0x192,'计算器']];
function currentBinding(button:OnboardProfile['buttons'][number]){
 if(button.kind==='macro')return '原厂板载宏（保留内容）';
 if(button.kind==='unknown'||button.description==='invalid')return '未识别的原厂动作（保留）';
 if(button.kind==='disabled')return '禁用';if(button.kind==='none')return '无动作';
 const raw=button.raw;
 if(button.kind==='keyboard'){const mods=['左 Ctrl','左 Shift','左 Alt','左 Win','右 Ctrl','右 Shift','右 Alt','右 Win'].filter((_,i)=>raw[2]&(1<<i));const key=KEY_NAMES.find(([code])=>code===raw[3])?.[1];return [...mods,...(key?[key]:[])].join(' + ')||'原厂键盘动作（保留）';}
 if(button.kind==='consumer')return media.find(([usage])=>usage===(raw[2]<<8|raw[3]))?.[1]??'其他媒体控制（保留）';
 if(button.kind==='mouse'){const mask=raw[2]<<8|raw[3],bit=Array.from({length:16},(_,i)=>i).find(i=>mask===(1<<i));return bit!=null?['左键','右键','中键','后退','前进'][bit]??`鼠标按键 ${bit+1}`:'原厂鼠标组合（保留）';}
 if(button.kind==='special')return Object.values(specials)[raw[1]-1]??'其他原厂功能（保留）';return '原厂动作（保留）';
}
export function OnboardBindingsEditor({device,profile,info,onClose,onSave}:{device:DeviceSummary;profile:OnboardProfile|null;info:OnboardInfo|null;onClose:()=>void;onSave:(s:OnboardBindingSettings)=>Promise<boolean>}){
 const [changes,setChanges]=useState<OnboardBindingChange[]>([]),[layer,setLayer]=useState<'Normal'|'GShift'>('Normal'),[busy,setBusy]=useState(false);
 useEffect(()=>{setChanges([]);setLayer('Normal');},[profile]);
 if(!profile||!info)return null;
 const shifted=(info.mechanical_layout&3)===2,dpiShift=((info.mechanical_layout>>2)&3)===2;
 const buttons=layer==='Normal'?profile.buttons:profile.gshift_buttons;
 const put=(index:number,action:OnboardBindingAction|null)=>setChanges(old=>[...old.filter(c=>c.layer!==layer||c.button!==index+1),...(action?[{button:index+1,layer,expected:[...buttons[index].raw],action}]:[])]);
 return <Dialog open onOpenChange={open=>{if(!open&&!busy)onClose();}}><DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-4xl"><DialogHeader><DialogTitle>罗技板载按键 · 槽 {profile.slot}</DialogTitle><DialogDescription>逐只鼠标、逐板载槽配置。只写明确选择的按键；原厂宏、灯效、DPI 与未修改记录保持原样。</DialogDescription></DialogHeader>
  <div className="flex items-center gap-6 rounded-xl border bg-card/40 p-4"><DeviceArt kind="mouse" vendorId={device.vendor_id} productId={device.product_id} name={device.name} className="h-36 w-24 shrink-0"/><div><b>{device.name}</b><p className="mt-2 text-sm text-muted-foreground">{info.button_count} 个设备报告的物理按键位置。编号以当前鼠标板载表为准；当前动作来自刚读取的记录。</p><p className="mt-2 text-xs text-muted-foreground">实验性协议实现 · 尚未实机验证。新增板载宏仍在适配；已有原厂宏绑定默认保留。</p></div></div>
  <fieldset disabled={busy} className="space-y-4"><div className="flex gap-2" aria-label="罗技按键模式层">{(['Normal',...(shifted?['GShift']:[])] as const).map(value=><Button key={value} variant={layer===value?'default':'outline'} onClick={()=>setLayer(value as 'Normal'|'GShift')}>{value==='Normal'?'普通层':'G-Shift 层'}</Button>)}</div>
   <section aria-label="罗技板载按键列表" className="space-y-3">{buttons.map((button,index)=>{const selected=changes.find(c=>c.layer===layer&&c.button===index+1),action=selected?.action;const kind=!action?'Keep':typeof action==='string'?action:'Mouse'in action?'Mouse':'Key'in action?'Key':'Consumer'in action?'Consumer':'Special';return <div key={index} className="rounded-lg border bg-background/50 p-3 space-y-2"><div className="flex flex-wrap items-center gap-3"><span className="flex w-24 items-center gap-2 font-medium text-sm"><MousePointer2 size={14}/>按键 {index+1}</span><select className={style} aria-label={`罗技${layer}按键${index+1}动作`} value={kind} onChange={e=>put(index,e.target.value==='Keep'?null:e.target.value==='Mouse'?{Mouse:1}:e.target.value==='Key'?{Key:{usage:4,modifiers:0}}:e.target.value==='Consumer'?{Consumer:0xcd}:e.target.value==='Special'?{Special:'DpiCycle'}:'Disabled')}><option value="Keep">保留当前动作</option><option value="Mouse">鼠标按键</option><option value="Key">键盘与快捷键</option><option value="Consumer">媒体控制</option><option value="Special">鼠标功能</option><option value="Disabled">禁用</option></select>
    {typeof action==='object'&&'Mouse'in action&&<select className={style} aria-label={`罗技${layer}按键${index+1}鼠标键`} value={action.Mouse} onChange={e=>put(index,{Mouse:Number(e.target.value)})}>{['左键','右键','中键','后退','前进'].map((name,i)=><option key={i} value={i+1}>{name}</option>)}</select>}
    {typeof action==='object'&&'Key'in action&&<><select className={style} aria-label={`罗技${layer}按键${index+1}键码`} value={action.Key.usage} onChange={e=>put(index,{Key:{...action.Key,usage:Number(e.target.value)}})}><option value={0}>仅修饰键</option>{KEY_NAMES.filter(([k])=>k>=4&&k<=0xa4).map(([k,name])=><option key={k} value={k}>{name}</option>)}</select>{['左 Ctrl','左 Shift','左 Alt','左 Win','右 Ctrl','右 Shift','右 Alt','右 Win'].map((name,i)=><label key={name} className="text-xs"><input type="checkbox" checked={!!(action.Key.modifiers&(1<<i))} onChange={e=>put(index,{Key:{...action.Key,modifiers:e.target.checked?action.Key.modifiers|(1<<i):action.Key.modifiers&~(1<<i)}})}/>{name}</label>)}</>}
    {typeof action==='object'&&'Consumer'in action&&<select className={style} aria-label={`罗技${layer}按键${index+1}媒体键`} value={action.Consumer} onChange={e=>put(index,{Consumer:Number(e.target.value)})}>{media.map(([v,name])=><option key={v} value={v}>{name}</option>)}</select>}
    {typeof action==='object'&&'Special'in action&&<select className={style} aria-label={`罗技${layer}按键${index+1}鼠标功能`} value={action.Special} onChange={e=>put(index,{Special:e.target.value as keyof typeof specials})}>{Object.entries(specials).filter(([value])=>value!=='GShift'||shifted).filter(([value])=>value!=='DpiShift'||dpiShift).filter(([value])=>!value.startsWith('Profile')||info.profile_count>1).map(([v,name])=><option key={v} value={v}>{name}</option>)}</select>}
   </div><p className="pl-0 text-xs text-muted-foreground sm:pl-28">当前：{currentBinding(button)}</p></div>})}</section>
   <p className="text-xs text-muted-foreground">至少保留一个普通层实体按键作为左键。保存前检查绑定是否已被外部修改，先备份可读目录与配置扇区，再写入和读回校验；原厂宏内容不包含在此备份中；不会强制切换到另一个板载槽。</p><div className="flex justify-end gap-3"><Button variant="ghost" onClick={onClose}>取消</Button><Button disabled={!changes.length} onClick={async()=>{setBusy(true);try{if(await onSave({slot:profile.slot,sector:profile.sector,changes}))onClose();}finally{setBusy(false);}}}><Save size={14}/>{busy?'正在保存…':`保存 ${changes.length} 项按键修改`}</Button></div>
  </fieldset></DialogContent></Dialog>;
}
