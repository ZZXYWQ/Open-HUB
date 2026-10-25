import {legacyModels,legacyModel,legacyNames,type LegacyBrand} from './legacy';
import {compxModel,compxModels,compxBrand,nativeCompx,type CompxBrand} from './compx';
export type BoardImportBrand=CompxBrand|'hyperx'|LegacyBrand;
export const hyperxModels=[{vendor:0x03f0,product:0x048e,brand:'hyperx',name:'HyperX Pulsefire Haste'},{vendor:0x03f0,product:0x028e,brand:'hyperx',name:'HyperX Pulsefire Haste Wireless'}] as const;
export function isHyperX(v:number,p:number){return hyperxModels.some(m=>m.vendor===v&&m.product===p);}
export function boardModel(v:number,p:number){return legacyModel(v,p)??compxModel(v,p)??hyperxModels.find(m=>m.vendor===v&&m.product===p);}
export function boardModels(brand:BoardImportBrand){return [...compxModels,...hyperxModels,...legacyModels].filter(m=>m.brand===brand);}
export function boardBrand(brand:BoardImportBrand){return brand in legacyNames?legacyNames[brand as LegacyBrand]:brand==='hyperx'?'HyperX':compxBrand(brand);}
export function nativeBoardMacros(v:number,p:number){return legacyModel(v,p)?.macros||nativeCompx(v,p)||isHyperX(v,p);}

export function nativeMediaKeys(v:number,p:number){const m=compxModel(v,p);return !!m?.native_macros&&m.native_buttons;}
export const NATIVE_MEDIA_KEYS=[[0xb5,'下一曲'],[0xb6,'上一曲'],[0xb7,'停止播放'],[0xcd,'播放 / 暂停'],[0xe2,'静音'],[0xe9,'音量 +'],[0xea,'音量 −']] as const;
