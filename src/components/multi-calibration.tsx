import {Target} from 'lucide-react';
import type {MultiPlan,MultiView,SensorCalibration} from '@/lib/multi';

export function MultiCalibration({view,plan,onChange}:{view:MultiView;plan:MultiPlan;onChange:(p:MultiPlan)=>void}){
 const exact=plan.vendor===0x1532&&[0x00b6,0x00b7].includes(plan.product)
  &&view.descriptor.vendor_id===plan.vendor&&view.descriptor.product_id===plan.product;
 if(!exact)return null;
 const caps=view.capabilities.calibration,item=plan.settings.find(s=>'SensorCalibration'in s);
 const current=item&&'SensorCalibration'in item?item.SensorCalibration:view.state.calibration;
 if(!caps){return current?<section aria-label="传感器校准" className="rounded-2xl border bg-card/40 p-5"><h3 className="font-bold">传感器校准</h3><p className="mt-2 text-sm text-muted-foreground">校准状态暂未完整读到，请重新读取鼠标后再应用此方案。</p></section>:null;}
 const valid=current&&('Standard'in current?caps.standard_levels.includes(current.Standard.distance):
  caps.lift_levels.includes(current.Asymmetric.lift)&&caps.landing_levels.includes(current.Asymmetric.landing)&&current.Asymmetric.landing<current.Asymmetric.lift);
 const mode=valid?('Standard'in current?'Standard':'Asymmetric'):'';
 const put=(value:SensorCalibration)=>onChange({...plan,settings:[...plan.settings.filter(s=>!('SensorCalibration'in s)),{SensorCalibration:value}]});
 const chooseMode=(value:string)=>{
  if(value==='Standard'){
   const distance=caps.standard_levels.includes(2)?2:caps.standard_levels[0];
   if(distance!==undefined)put({Standard:{distance}});
  }else if(value==='Asymmetric'){
   const lift=caps.lift_levels.includes(13)?13:caps.lift_levels.find(v=>caps.landing_levels.some(l=>l<v));
   if(lift===undefined)return;
   const landing=caps.landing_levels.includes(12)&&12<lift?12:caps.landing_levels.find(v=>v<lift);
   if(landing!==undefined)put({Asymmetric:{lift,landing}});
  }
 };
 const changeLevel=(field:'lift'|'landing',value:number)=>{
  if(!current||!('Asymmetric'in current))return;
  let {lift,landing}=current.Asymmetric;
  if(field==='lift'){
   if(!caps.lift_levels.includes(value))return;
   lift=value;
   if(landing>=lift){const next=caps.landing_levels.filter(v=>v<lift).at(-1);if(next===undefined)return;landing=next;}
  }else{
   if(!caps.landing_levels.includes(value))return;
   landing=value;
   if(lift<=landing){const next=caps.lift_levels.find(v=>v>landing);if(next===undefined)return;lift=next;}
  }
  put({Asymmetric:{lift,landing}});
 };
 return <section aria-label="传感器校准" className="space-y-4 rounded-2xl border bg-card/40 p-5">
  <div className="flex items-center gap-3"><span className="rounded-xl bg-primary/10 p-2 text-primary"><Target className="size-5"/></span><div><h3 className="font-bold">传感器校准</h3><p className="text-xs text-muted-foreground">按此型号的原生等级设置 · 写入后回读核对</p></div></div>
  {!valid&&<p className="text-sm text-amber-400">当前校准状态未完整识别，请重新读取或明确选择模式与等级。</p>}
  <label className="flex flex-wrap items-center justify-between gap-3 text-sm">校准模式<select aria-label="校准模式" className="min-w-32 rounded-lg border bg-background px-3 py-2" value={mode} onChange={e=>chooseMode(e.target.value)}><option value="" disabled>未读取</option>{caps.standard_levels.length>0&&<option value="Standard">普通</option>}{caps.lift_levels.some(lift=>caps.landing_levels.some(landing=>landing<lift))&&<option value="Asymmetric">非对称</option>}</select></label>
  {valid&&current&&'Standard'in current&&<label className="flex flex-wrap items-center justify-between gap-3 text-sm">追踪等级<select aria-label="追踪等级" className="min-w-32 rounded-lg border bg-background px-3 py-2" value={current.Standard.distance} onChange={e=>{const distance=Number(e.target.value);if(caps.standard_levels.includes(distance))put({Standard:{distance}});}}>{caps.standard_levels.map(v=><option key={v} value={v}>等级 {v}</option>)}</select></label>}
  {valid&&current&&'Asymmetric'in current&&<div className="grid gap-4 sm:grid-cols-2">{(['lift','landing']as const).map(field=><label key={field} className="space-y-2 rounded-xl border bg-background/30 p-4 text-sm"><span className="block">{field==='lift'?'抬起等级':'落下等级'}</span><select aria-label={field==='lift'?'抬起等级':'落下等级'} className="w-full rounded-lg border bg-background px-3 py-2" value={current.Asymmetric[field]} onChange={e=>changeLevel(field,Number(e.target.value))}>{(field==='lift'?caps.lift_levels:caps.landing_levels).map(v=><option key={v} value={v}>等级 {v}</option>)}</select></label>)}</div>}
  <p className="text-xs text-muted-foreground">等级表示传感器的相对阈值。非对称模式的落下等级始终低于抬起等级；调整越过另一端时会一并修正。</p>
 </section>;
}
