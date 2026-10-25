import type {MultiColor,MultiColorStop,MultiGradient} from '@/lib/multi';
import {Button} from '@/components/ui/button';
const empty=():MultiColorStop=>({position:0,color:{r:0,g:0,b:0}});
export function defaultGradient(color:MultiColor):MultiGradient{return {period_ms:1000,count:3,stops:[{position:0,color},{position:50,color:{r:0,g:0,b:0}},{position:100,color},...Array.from({length:11},empty)]};}
function hex(c:MultiColor){return '#'+[c.r,c.g,c.b].map(n=>n.toString(16).padStart(2,'0')).join('');}
export function NativeGradient({zone,value,onChange}:{zone:number;value:MultiGradient;onChange:(v:MultiGradient)=>void}){
 const active=value.stops.slice(0,value.count);
 const put=(stops:MultiColorStop[])=>onChange({...value,count:stops.length,stops:[...stops,...Array.from({length:14-stops.length},empty)]});
 return <section aria-label={`原生灯区${zone}渐变色点`} className="space-y-3 rounded-lg border bg-background/50 p-3">
  <div aria-hidden className="h-5 rounded" style={{background:`linear-gradient(90deg,${active.map(p=>`${hex(p.color)} ${p.position}%`).join(',')})`}}/>
  <div className="flex flex-wrap items-center justify-between gap-2 text-sm"><b>{value.count} / 14 色点</b><label>循环周期 <input aria-label={`原生灯区${zone}渐变周期`} className="w-24 rounded border bg-background p-1" type="number" min={1} max={65535} value={value.period_ms} onChange={e=>onChange({...value,period_ms:Number(e.target.value)})}/> 毫秒</label></div>
  <div className="max-h-72 space-y-2 overflow-y-auto pr-1">{active.map((point,index)=><div key={index} className="flex flex-wrap items-center gap-3 text-sm"><span className="w-12">色点 {index+1}</span><input aria-label={`原生灯区${zone}色点${index+1}颜色`} type="color" value={hex(point.color)} onChange={e=>{const h=e.target.value;put(active.map((p,i)=>i===index?{...p,color:{r:parseInt(h.slice(1,3),16),g:parseInt(h.slice(3,5),16),b:parseInt(h.slice(5,7),16)}}:p));}}/><label>位置 <input aria-label={`原生灯区${zone}色点${index+1}位置`} className="w-16 rounded border bg-background p-1" type="number" step={0.5} min={0} max={100} disabled={index===0} value={point.position} onChange={e=>put(active.map((p,i)=>i===index?{...p,position:Number(e.target.value)}:p))}/>%</label><Button size="sm" variant="ghost" disabled={active.length<=1} onClick={()=>{const next=active.filter((_,i)=>i!==index);next[0]={...next[0],position:0};put(next);}}>删除色点 {index+1}</Button></div>)}</div>
  <Button size="sm" variant="outline" disabled={value.count>=14} onClick={()=>{let start=0,end=100;for(let i=0;i<active.length;i++){const next=active[i+1]?.position??100;if(i===0||next-active[i].position>end-start){start=active[i].position;end=next;}}const next=[...active,{position:Math.round(start+end)/2,color:active[0].color}].sort((a,b)=>a.position-b.position);put(next);}}>添加色点</Button>
 </section>;
}
