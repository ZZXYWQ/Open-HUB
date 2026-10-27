import models from './compx-models.json';
export type CompxBrand='atk'|'vxe'|'vgn'|'pulsar';
export const compxModels=models;
export function compxModel(vendor:number,product:number){return models.find(m=>m.vendor===vendor&&m.product===product);}
export function nativeCompx(vendor:number,product:number){return !!compxModel(vendor,product)?.native_macros;}
export function compxBrand(brand:string){return brand==='pulsar'?'Pulsar':brand==='vxe'?'蜻蜓 / VXE':brand==='vgn'?'VGN': 'ATK';}

export function pulsarShortcuts(v:number,p:number){return v===0x3554&&[0xf507,0xf508].includes(p);}

export function compxHorizontalScroll(v:number,p:number){const m=compxModel(v,p);return !!m?.native_macros&&m.native_buttons&&['atk','vxe','vgn'].includes(m.brand);}

export function compxVerticalScroll(v:number,p:number){return compxHorizontalScroll(v,p);}
