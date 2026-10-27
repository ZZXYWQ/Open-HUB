import type {MultiColor,MultiPlan,MultiSetting,MultiView} from './multi';
const same=(a:MultiColor,b:MultiColor)=>a.r===b.r&&a.g===b.g&&a.b===b.b;
export function syncCompxPalette(before:MultiPlan,next:MultiPlan,view:MultiView):MultiPlan {
 if(!(view.capabilities.dpi_color_stages??0))return next;
 const active=(p:MultiPlan)=>p.settings.find(s=>'Dpi'in s);
 const oldDpi=active(before),dpi=active(next);if(!dpi||!('Dpi'in dpi))return next;
 const stage=dpi.Dpi.active;
 const alias=(p:MultiPlan)=>p.settings.find(s=>'Lighting'in s&&s.Lighting.zone===0);
 const old=alias(before),light=alias(next);if(!light||!('Lighting'in light)||typeof light.Lighting.state.effect!=='object'||!('Static'in light.Lighting.state.effect))return next;
 const color=light.Lighting.state.effect.Static.color;
 const entry=next.settings.find(s=>'DpiColor'in s&&s.DpiColor.stage===stage);if(!entry||!('DpiColor'in entry))return next;
 const previous=before.settings.find(s=>'DpiColor'in s&&s.DpiColor.stage===stage);
 const editedAlias=old&&'Lighting'in old&&typeof old.Lighting.state.effect==='object'&&'Static'in old.Lighting.state.effect&&oldDpi&&'Dpi'in oldDpi&&oldDpi.Dpi.active===stage&&!same(old.Lighting.state.effect.Static.color,color);
 const editedPalette=!previous||!('DpiColor'in previous)||!same(previous.DpiColor.color,entry.DpiColor.color);
 if(editedAlias&&!editedPalette)return {...next,settings:next.settings.map(s=>s===entry?{DpiColor:{stage,color}}:s)};
 return {...next,settings:next.settings.map(s=>s===light?{Lighting:{...light.Lighting,state:{...light.Lighting.state,effect:{Static:{color:entry.DpiColor.color}}}}}:s)};
}
export function paletteColor(plan:MultiPlan,view:MultiView,stage:number):MultiColor|undefined {
 const setting=plan.settings.find(s=>'DpiColor'in s&&s.DpiColor.stage===stage);if(setting&&'DpiColor'in setting)return setting.DpiColor.color;
 const dpi=plan.settings.find(s=>'Dpi'in s),light=plan.settings.find(s=>'Lighting'in s&&s.Lighting.zone===0);
 if(dpi&&'Dpi'in dpi&&dpi.Dpi.active===stage&&light&&'Lighting'in light&&typeof light.Lighting.state.effect==='object'&&'Static'in light.Lighting.state.effect)return light.Lighting.state.effect.Static.color;
 return view.state.dpi_colors?.[String(stage)];
}
export function putPaletteColor(plan:MultiPlan,stage:number,color:MultiColor):MultiPlan {
 const setting:MultiSetting={DpiColor:{stage,color}};
 return {...plan,settings:[...plan.settings.filter(s=>!('DpiColor'in s&&s.DpiColor.stage===stage)),setting]};
}
