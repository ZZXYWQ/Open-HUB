import type {DpiIndicator,MultiPlan,MultiView} from './multi';
import {paletteColor,putPaletteColor} from './compx-palette';
export function indicatorValue(plan:MultiPlan,view:MultiView):DpiIndicator|undefined{
 const setting=plan.settings.find(s=>'DpiIndicator'in s);if(setting&&'DpiIndicator'in setting)return setting.DpiIndicator;
 const original=view.state.dpi_indicator;if(!original)return undefined;
 const legacy=plan.settings.find(s=>'Lighting'in s&&s.Lighting.zone===0);
 if(legacy&&'Lighting'in legacy){const effect=legacy.Lighting.state.effect;if(effect==='Off')return {...original,enabled:false};if(typeof effect==='object'&&'Static'in effect)return {...original,enabled:true,mode:'Static'};}
 return original;
}
export function putIndicator(plan:MultiPlan,view:MultiView,value:DpiIndicator):MultiPlan{
 // Migrating a legacy active-color plan must retain its explicitly selected RGB.
 const dpi=plan.settings.find(s=>'Dpi'in s),stage=dpi&&'Dpi'in dpi?dpi.Dpi.active:undefined;
 const color=stage===undefined?undefined:paletteColor(plan,view,stage);
 const selected=stage===undefined||!color?plan:putPaletteColor(plan,stage,color);
 return {...selected,settings:[...selected.settings.filter(s=>!('DpiIndicator'in s)&&!('Lighting'in s&&s.Lighting.zone===0)),{DpiIndicator:value}]};
}
