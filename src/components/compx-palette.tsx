import {paletteColor,putPaletteColor} from '@/lib/compx-palette';
import type {MultiPlan,MultiView} from '@/lib/multi';
import {Button} from '@/components/ui/button';
export function CompxPalette({plan,view,onChange}:{plan:MultiPlan;view:MultiView;onChange:(p:MultiPlan)=>void}) {
 const count=view.capabilities.dpi_color_stages??0;if(!count)return null;
 const dpi=plan.settings.find(s=>'Dpi'in s),active=dpi&&'Dpi'in dpi?dpi.Dpi.active:-1,enabled=dpi&&'Dpi'in dpi?dpi.Dpi.stages.length:0;
 return <section aria-label="DPI 档位指示颜色" className="space-y-3 rounded-xl border p-5"><h3 className="font-bold">DPI 档位指示颜色</h3><p className="text-sm text-muted-foreground">同一指示灯随 DPI 档位切换颜色。未启用的档位也可保存颜色；修改色表会保留指示灯当前开关和模式。</p><div className="grid gap-3 sm:grid-cols-2">{Array.from({length:count},(_,stage)=>{
  const known=view.state.dpi_colors?.[String(stage)],color=paletteColor(plan,view,stage),selected=plan.settings.some(s=>'DpiColor'in s&&s.DpiColor.stage===stage);
  const hex=color?'#'+[color.r,color.g,color.b].map(n=>n.toString(16).padStart(2,'0')).join(''):'#000000';
  return <div key={stage} className="flex flex-wrap items-center gap-3 rounded-lg border p-3"><span className="mr-auto">档位 {stage+1}{stage===active?' · 当前':stage>=enabled?' · 未启用':''}</span><input aria-label={`DPI 档位${stage+1}指示颜色`} type="color" value={hex} disabled={!known} onChange={e=>{const h=e.target.value;onChange(putPaletteColor(plan,stage,{r:parseInt(h.slice(1,3),16),g:parseInt(h.slice(3,5),16),b:parseInt(h.slice(5,7),16)}));}}/>{!known&&<p className="w-full text-xs text-muted-foreground">此档原始颜色未通过校验，暂不可修改。</p>}{!known&&selected&&<Button variant="outline" size="sm" onClick={()=>onChange({...plan,settings:plan.settings.filter(s=>!('DpiColor'in s&&s.DpiColor.stage===stage))})}>暂不应用此档颜色</Button>}</div>;
 })}</div></section>;
}
