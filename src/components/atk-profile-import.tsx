import {compxModel,compxModels,compxBrand,type CompxBrand} from '@/lib/compx';
import {useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import type {DeviceSummary,Profile} from '@/lib/api';
import {type MultiPlan,type MultiView,multiProfile} from '@/lib/multi';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from '@/components/ui/dialog';
import {Button} from '@/components/ui/button';

export function AtkProfileImport({devices,onClose,onSave,brand='atk'}:{devices:DeviceSummary[];onClose:()=>void;onSave:(p:Profile)=>Promise<boolean>;brand?:CompxBrand}){
 const label=compxBrand(brand), supported=compxModels.filter(m=>m.brand===brand);
 const peers=devices.filter(d=>compxModel(d.vendor_id,parseInt(d.product_id,16))?.brand===brand&&d.id.startsWith('multi:'+brand+':'));
 const [selected,setSelected]=useState(''),[preview,setPreview]=useState<{plan:MultiPlan;name:string;unknown:number}|null>(null),[error,setError]=useState(''),[busy,setBusy]=useState(false);
 const generation=useRef(0);
 const read=async()=>{const current=++generation.current;setBusy(true);setError('');setPreview(null);try{
  const view=await invoke<MultiView>('multi_read',{id:selected});
  if(current!==generation.current)return;
  const expected=peers.find(d=>d.id===selected);
  if(!expected||'multi:'+view.descriptor.id!==expected.id||view.descriptor.product_id!==parseInt(expected.product_id,16))throw Error(`读取结果与所选 ${label} 型号或设备不一致`);
  if(compxModel(view.descriptor.vendor_id,view.descriptor.product_id)?.brand!==brand||view.descriptor.vendor_id!==expected.vendor_id||view.descriptor.simulated||!view.state.dpi)throw Error(`读取结果不是受支持的 ${label} 实机板载配置`);
  // Read the hardware fields, never substitute a saved host-macro session plan.
  const plan:MultiPlan={vendor:view.descriptor.vendor_id,product:view.descriptor.product_id,macros:view.native_macros??[],settings:[{Dpi:view.state.dpi},...(view.state.report_rate&&view.capabilities.report_rates.includes(view.state.report_rate)?[{ReportRate:view.state.report_rate}]:[]),...view.capabilities.buttons.filter(b=>b.remappable&&view.state.buttons[String(b.id)]!=null).map(b=>({Button:{id:b.id,action:view.state.buttons[String(b.id)]}})),...Object.entries(view.state.lighting).filter(([,state])=>state.effect==='Off'||(typeof state.effect==='object'&&'Static'in state.effect)).map(([zone,state])=>({Lighting:{zone:Number(zone),state}}))]};
  setPreview({plan,name:view.descriptor.name+' · 板载导入',unknown:view.capabilities.buttons.filter(b=>b.id!==1&&!b.remappable).length});
 }catch(e){if(current===generation.current)setError(String(e));}finally{if(current===generation.current)setBusy(false);}};
 return <Dialog open onOpenChange={v=>{if(!v&&!busy){generation.current++;onClose();}}}><DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl"><DialogHeader><DialogTitle>{label} 板载配置导入</DialogTitle><DialogDescription>{supported.map(m=>m.name).join('；')}。先读取并预览，再保存到配置中心；此过程不写鼠标。不解析官方软件文件；仅导入能完整校验的板载宏。</DialogDescription></DialogHeader>
 <label>鼠标<select aria-label={`${label} 导入鼠标`} className="ml-3 max-w-full rounded border bg-background p-2" disabled={busy} value={selected} onChange={e=>{generation.current++;setSelected(e.target.value);setPreview(null);setError('');}}><option value="">请选择已连接的 {label} 鼠标</option>{peers.map(d=><option key={d.id} value={d.id}>{d.name}</option>)}</select></label>
 {!peers.length&&<p role="status" className="text-sm text-muted-foreground">未检测到这些已适配型号的 USB 配置接口。请连接鼠标，并使用主界面顶部的刷新按钮。</p>}
 <Button variant="outline" disabled={!selected||busy} onClick={()=>void read()}>{busy?'正在读取…':'读取板载配置'}</Button>
 {preview&&<div aria-label={`${label} 导入预览`} className="rounded-xl border p-4 space-y-2"><b>{preview.name}</b><p>{preview.plan.settings.filter(s=>'Dpi'in s).map(s=>'Dpi'in s?s.Dpi.stages.map(d=>`${d.x} / ${d.y}`).join('、'):'')} DPI</p><div className="grid gap-2 text-sm">{preview.plan.settings.filter(s=>'ReportRate'in s).map((s,i)=><p key={'rate'+i}>{'ReportRate'in s?`回报率：${s.ReportRate} Hz`:''}</p>)}{preview.plan.macros?.map(m=><p key={m.id}>板载宏：{m.name} · {m.mode==='Once'?'单次':m.mode==='WhileHeld'?'按住循环':'再次按下停止'} · {m.steps.length} 步</p>)}{preview.plan.settings.filter(s=>'Lighting'in s).map((s,i)=><p key={'light'+i}>DPI 指示灯：{'Lighting'in s&&s.Lighting.state.effect==='Off'?'关闭':'常亮'}</p>)}</div><p>将导入 {preview.plan.settings.length} 项已解析设置。支持的板载宏随配置导入；无法解析的原厂动作保持不变。</p>{preview.unknown>0&&<p>保留 {preview.unknown} 个无法完整解析的原厂按键。</p>}</div>}
 {error&&<p role="alert" className="lighting-error">{error}</p>}
 <div className="flex justify-end gap-3"><Button variant="ghost" disabled={busy} onClick={onClose}>取消</Button><Button disabled={!preview||busy} onClick={async()=>{if(!preview)return;setBusy(true);setError('');try{if(await onSave(multiProfile(preview.plan,preview.name)))onClose();}catch(e){setError(String(e));}finally{setBusy(false);}}}>导入到配置中心</Button></div>
 </DialogContent></Dialog>;
}
