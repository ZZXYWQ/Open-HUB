import {legacyModels,legacyModel,legacyNames,type LegacyBrand} from './legacy';
import {compxModel,compxModels,compxBrand,nativeCompx,type CompxBrand} from './compx';
export type BoardImportBrand=CompxBrand|'hyperx'|LegacyBrand;
export const hyperxModels=[{vendor:0x03f0,product:0x048e,brand:'hyperx',name:'HyperX Pulsefire Haste'},{vendor:0x03f0,product:0x028e,brand:'hyperx',name:'HyperX Pulsefire Haste Wireless'}] as const;
export function isHyperX(v:number,p:number){return hyperxModels.some(m=>m.vendor===v&&m.product===p);}
export function boardModel(v:number,p:number){return legacyModel(v,p)??compxModel(v,p)??hyperxModels.find(m=>m.vendor===v&&m.product===p);}
export function boardModels(brand:BoardImportBrand){return [...compxModels,...hyperxModels,...legacyModels].filter(m=>m.brand===brand);}
export function boardBrand(brand:BoardImportBrand){return brand in legacyNames?legacyNames[brand as LegacyBrand]:brand==='hyperx'?'HyperX':compxBrand(brand);}
export function nativeBoardMacros(v:number,p:number){return legacyModel(v,p)?.macros||nativeCompx(v,p)||isHyperX(v,p);}
