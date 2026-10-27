import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {Button} from '@/components/ui/button';
import {nativeCompx} from '@/lib/compx';
import type {MultiView} from '@/lib/multi';
export function CompxBattery({view}:{view:MultiView}){
 const [battery,setBattery]=useState(view.state.battery??null),[busy,setBusy]=useState(false);
 const lifetime=useRef({live:true,reading:false});
 useEffect(()=>{const owner={live:true,reading:false};lifetime.current=owner;setBattery(view.state.battery??null);setBusy(false);return()=>{owner.live=false;};},[view.descriptor.id,view.write_connection,view.state.battery]);
 if(!nativeCompx(view.descriptor.vendor_id,view.descriptor.product_id)||!view.capabilities.battery)return null;
 const refresh=async()=>{
  const owner=lifetime.current;if(owner.reading)return;owner.reading=true;setBusy(true);
  try{const value=await invoke<MultiView['state']['battery']>('multi_battery',{id:view.descriptor.id});if(owner.live)setBattery(value??null);}
  catch{if(owner.live)setBattery(null);}
  finally{owner.reading=false;if(owner.live)setBusy(false);}
 };
 return <section aria-label="鼠标电量" className="flex flex-wrap items-center justify-between gap-3 rounded-xl border px-5 py-3">
  <p role="status" aria-label="电量读数" className="text-sm">{battery?.percent!=null?`${battery.percent}% · ${battery.charging?'正在充电':'未充电'}`:'未能读取电量'}{view.descriptor.simulated?'（模拟数据）':''}</p>
  <Button size="sm" variant="outline" disabled={busy||view.descriptor.simulated} onClick={()=>void refresh()}>{busy?'正在读取…':'刷新电量'}</Button>
 </section>;
}
