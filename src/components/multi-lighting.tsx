import {NativeGradient,defaultGradient} from '@/components/native-gradient';
import {isSteelseries,steelseriesBreathPeriods} from '@/lib/steelseries';
import {legacyModel} from '@/lib/legacy';
import type {MultiColor,MultiEffect,MultiPlan,MultiView,MultiZoneState} from '@/lib/multi';

const names:Record<string,string>={Gradient:'自定义渐变',Off:'关闭',Static:'常亮',Breathing:'单色呼吸',ColorCycle:'光谱循环',Wave:'波浪',Reactive:'按键响应'};
const kind=(effect:MultiEffect)=>typeof effect==='string'?effect:Object.keys(effect)[0];
function colorOf(effect?:MultiEffect):MultiColor {
 if(effect&&typeof effect==='object'){
  if('Gradient'in effect)return effect.Gradient.stops[0]?.color??{r:110,g:231,b:208};
  if('Static'in effect)return effect.Static.color;
  if('Breathing'in effect)return effect.Breathing.color;
  if('Reactive'in effect)return effect.Reactive.color;
 }
 return {r:110,g:231,b:208};
}
function createEffect(name:string,color:MultiColor):MultiEffect {if(name==='Gradient')return {Gradient:defaultGradient(color)};
 switch(name){
  case 'Off':return 'Off';case 'Static':return {Static:{color}};
  case 'Breathing':return {Breathing:{color,period_ms:4000}};
  case 'ColorCycle':return {ColorCycle:{period_ms:10000}};
  case 'Wave':return {Wave:{period_ms:3000,direction:'LeftToRight'}};
  case 'Reactive':return {Reactive:{color,duration_ms:500}};
  default:throw Error('此光效尚未实现');
 }
}
export function MultiLighting({view,plan,onChange}:{view:MultiView;plan:MultiPlan;onChange:(p:MultiPlan)=>void}){
 const zones=view.capabilities.lighting,legacy=legacyModel(plan.vendor,plan.product);const make=(name:string,color:MultiColor)=>{const e=createEffect(name,color);if(isSteelseries(plan.vendor,plan.product)&&steelseriesBreathPeriods(plan.product)&&typeof e==='object'&&'Breathing'in e)e.Breathing.period_ms=3000;if(legacy?.brand==='asus'&&typeof e==='object'){if('Breathing'in e)e.Breathing.period_ms=0;if('ColorCycle'in e)e.ColorCycle.period_ms=0;}return e;};
 const put=(zone:number,state:MultiZoneState)=>onChange({...plan,settings:[...plan.settings.filter(s=>!('Lighting'in s&&s.Lighting.zone===zone)),{Lighting:{zone,state}}]});
 return <section aria-label="原生灯效" className="rounded-xl border bg-gradient-to-br from-emerald-400/5 via-background to-cyan-500/5 p-5 space-y-4"><div><h3 className="font-bold">原生灯效</h3><p className="mt-1 text-sm text-muted-foreground">按此型号实际灯区提供效果。编辑后点击“应用到此鼠标”；效果和颜色可能来自本次会话，不能完整从硬件读回。</p></div>
 {!zones.length?<p className="rounded-lg border border-dashed p-4 text-sm text-muted-foreground">此型号未提供可控制的 RGB 灯区。独立底座与电脑灯光可在“灯光同步”中选择。</p>:<div className="grid items-start gap-4 sm:grid-cols-2">{zones.map(zone=>{
  const setting=plan.settings.find(s=>'Lighting'in s&&s.Lighting.zone===zone.id);
  const state=setting&&'Lighting'in setting?setting.Lighting.state:view.state.lighting[String(zone.id)];
  const effect=state?.effect,name=effect?kind(effect):'',color=colorOf(effect),hex='#'+[color.r,color.g,color.b].map(n=>n.toString(16).padStart(2,'0')).join('');
  const allowed=zone.effects.filter(n=>n in names);
  return <div key={zone.id} className="rounded-xl border bg-background/70 p-4 space-y-4"><div className="flex items-center justify-between"><b>{zone.name}</b><span aria-hidden className="h-3 w-3 rounded-full" style={{background:effect==='Off'?'#475569':effect?hex:'#475569',boxShadow:effect&&effect!=='Off'?`0 0 14px ${hex}66`:undefined}}/></div>
   <label className="flex items-center justify-between gap-3 text-sm">效果<select aria-label={`原生灯区${zone.id}效果`} className="max-w-full rounded border bg-background p-2" value={name} onChange={e=>put(zone.id,{effect:make(e.target.value,color),brightness:state?.brightness??100})}><option disabled value="">尚未读取或设置</option>{allowed.map(n=><option key={n} value={n}>{names[n]}</option>)}</select></label>
   {effect&&['Static','Breathing','Reactive'].includes(name)&&<label className="flex items-center justify-between text-sm">颜色<input aria-label={`鼠标灯区${zone.id}颜色`} type="color" value={hex} onChange={e=>{const h=e.target.value,nextColor={r:parseInt(h.slice(1,3),16),g:parseInt(h.slice(3,5),16),b:parseInt(h.slice(5,7),16)};const next=typeof effect==='object'&&'Breathing'in effect?{Breathing:{...effect.Breathing,color:nextColor}}:typeof effect==='object'&&'Reactive'in effect?{Reactive:{...effect.Reactive,color:nextColor}}:{Static:{color:nextColor}};put(zone.id,{effect:next,brightness:state?.brightness??100});}}/></label>}
   {state&&typeof effect==='object'&&'Gradient'in effect&&<NativeGradient zone={zone.id} value={effect.Gradient} onChange={next=>put(zone.id,{...state,effect:{Gradient:next}})}/>}
   {state&&zone.brightness!==false&&<label className="grid gap-2 text-sm">亮度 {state.brightness}%<input aria-label={`原生灯区${zone.id}亮度`} type="range" min={0} max={100} value={state.brightness} onChange={e=>put(zone.id,{...state,brightness:Number(e.target.value)})}/></label>}
   {isSteelseries(plan.vendor,plan.product)&&state&&typeof effect==='object'&&('Breathing'in effect||'ColorCycle'in effect)&&<label className="flex items-center justify-between gap-3 text-sm">灯效周期{steelseriesBreathPeriods(plan.product)&&'Breathing'in effect?<select aria-label={`原生灯区${zone.id}周期`} className="rounded border bg-background p-2" value={effect.Breathing.period_ms} onChange={e=>put(zone.id,{...state,effect:{Breathing:{...effect.Breathing,period_ms:Number(e.target.value)}}})}>{steelseriesBreathPeriods(plan.product)!.map(ms=><option key={ms} value={ms}>{ms/1000} 秒</option>)}</select>:<input aria-label={`原生灯区${zone.id}周期`} className="w-28 rounded border bg-background p-2" type="number" min={1} max={65535} step={1} value={'Breathing'in effect?effect.Breathing.period_ms:effect.ColorCycle.period_ms} onChange={e=>put(zone.id,{...state,effect:'Breathing'in effect?{Breathing:{...effect.Breathing,period_ms:Number(e.target.value)}}:{ColorCycle:{period_ms:Number(e.target.value)}}})}/>}</label>}
   {legacy?.brand==='marsgaming'&&state&&typeof effect==='object'&&'Breathing'in effect&&<label>呼吸周期<select aria-label={`原生灯区${zone.id}呼吸周期`} value={effect.Breathing.period_ms} onChange={e=>put(zone.id,{...state,effect:{Breathing:{...effect.Breathing,period_ms:Number(e.target.value)}}})}>{[2000,4000,6000,8000,10000,12000,14000,16000,18000].map(v=><option key={v} value={v}>{v/1000} 秒</option>)}</select></label>}
   {state&&typeof effect==='object'&&'Reactive'in effect&&<label className="flex items-center justify-between gap-3 text-sm">响应渐隐<select aria-label={`原生灯区${zone.id}响应时长`} className="rounded border bg-background p-2" value={effect.Reactive.duration_ms} onChange={e=>put(zone.id,{...state,effect:{Reactive:{...effect.Reactive,duration_ms:Number(e.target.value)}}})}>{[500,1000,1500,2000].map(n=><option key={n} value={n}>{n/1000} 秒</option>)}</select></label>}
   {state&&typeof effect==='object'&&'Wave'in effect&&<label className="flex items-center justify-between gap-3 text-sm">方向<select aria-label={`原生灯区${zone.id}波浪方向`} className="rounded border bg-background p-2" value={effect.Wave.direction} onChange={e=>put(zone.id,{...state,effect:{Wave:{...effect.Wave,direction:e.target.value as 'LeftToRight'|'RightToLeft'}}})}><option value="LeftToRight">从左到右</option><option value="RightToLeft">从右到左</option></select></label>}
   {(view.descriptor.vendor_id===0x1532||legacy?.brand==='asus')&&['Breathing','ColorCycle'].includes(name)&&<p className="text-xs text-muted-foreground">此协议的呼吸与循环速度由固件决定。</p>}
  </div>;
 })}</div>}
 <p className="text-xs text-muted-foreground">屏幕取色、音乐律动及电脑光同步，在“灯光同步”选择具体鼠标或灯光设备。</p></section>;
}
