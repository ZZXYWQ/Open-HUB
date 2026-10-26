import {legacyModel,legacyNames} from './legacy';
import {compxModel,compxBrand} from './compx';
export const BRANDS=[{vendor:0x046d,name:'罗技 / Logitech',source:'G HUB'},{vendor:0x3837,name:'迈从 / MCHOSE',source:'M HUB'}] as const;
export function profileVendor(p:{target_vendor?:number|null;mchose_board?:number|null;ghub_id?:string|null;mchose?:{vendor:number;model:number}|null}){return p.mchose!=null||p.mchose_board!=null?0x3837:p.target_vendor??(p.ghub_id?0x046d:null);}
export function brandName(v:number|null|undefined,p?:number|null){const legacy=v!=null?legacyModel(v,p??-1):undefined;if(legacy)return legacyNames[legacy.brand];const byVendor=Object.values(legacyNames).find((_,i)=>[0x0b05,0x1e7d,0x28da,0x1ea7,0x04d9][i]===v);if(byVendor)return byVendor;const model=v!=null&&p!=null?compxModel(v,p):undefined;if(model)return compxBrand(model.brand);if(v===0x373b)return 'ATK / VXE';if(v===0x3554)return 'VXE / VGN';if(v===0x1d57&&p!=null&&[0xfa55,0xfa60].includes(p))return 'Attack Shark';if(v===0x03f0)return 'HyperX';if(v===0x1532)return '雷蛇 / Razer';if(v===0x1b1c)return '海盗船 / Corsair';if(v===0x1038)return '赛睿 / SteelSeries';if(v===0x258a)return 'SinoWealth 多品牌';if(v===0x3794)return 'Glorious';return BRANDS.find(b=>b.vendor===v)?.name??(v==null?'通用配置':`品牌 ${v.toString(16)}`);}
export function matchesDevice(p:{target_vendor?:number|null;target_model?:number|null;mchose_board?:number|null;ghub_id?:string|null;mchose?:{vendor:number;model:number}|null;multi?:{vendor:number;product:number}|null;sino?:{model:{vendor:number;product:number;firmware:string}}|null},d:{vendor_id:number;model_ids:string[];product_id?:string;firmware_name?:string;id?:string;kind?:string;features?:{id:number}[]}){
 if(d.kind&&!["mouse","trackball"].includes(d.kind.toLowerCase())&&!d.features?.some(f=>f.id===0x2201||f.id===0x2202))return false;
 if(p.mchose&&(p.mchose.vendor!==0x3837||d.id?.startsWith('multi:')||d.id?.startsWith('sino:')))return false;
 const models=[...d.model_ids,...(d.product_id?[d.product_id]:[])].map(v=>parseInt(v,16));
 if(p.multi&&(!d.id?.startsWith('multi:')||p.multi.vendor!==d.vendor_id||!models.includes(p.multi.product)))return false;
 if(d.id?.startsWith('multi:')&&!p.multi)return false;
 if(p.sino&&(!d.id?.startsWith('sino:')||p.sino.model.vendor!==d.vendor_id||!models.includes(p.sino.model.product)||p.sino.model.firmware!==d.firmware_name))return false;
 if(d.id?.startsWith('sino:')&&!p.sino&&(p.ghub_id||p.mchose_board!=null))return false;
 const vendor=profileVendor(p),model=p.mchose?.model??(p.mchose_board!=null?0x4026:p.target_model);
 return(vendor==null||vendor===d.vendor_id)&&(model==null||models.includes(model));
}


/** Imported templates can be selected for any matching model; private copies
 * only appear in their owner's device picker. Cross-device copies are explicit. */
export function ownedProfileMatches(p:import('./api').Profile,d:import('./api').DeviceSummary,view:import('./api').ProfilesView){
 const owner=view.profile_owners?.[p.id];return matchesDevice(p,d)&&(!owner||owner===d.profile_key);
}
/** Native MCHOSE profiles, including generic DPI and board references, need
 * the transport generation captured by the user's selection. */
export function needsProfileConnection(d:{vendor_id:number;id:string}|null|undefined){
 return d?.vendor_id===0x3837&&!d.id.startsWith('multi:')&&!d.id.startsWith('sino:');
}
export function activeDeviceProfile(view:import('./api').ProfilesView|null,d:import('./api').DeviceSummary|null|undefined){
 if(!view||!d?.profile_key)return null;const id=view.device_profiles?.[d.profile_key]?.active_id;
 return view.profiles.find(p=>p.id===id&&ownedProfileMatches(p,d,view))??null;
}
