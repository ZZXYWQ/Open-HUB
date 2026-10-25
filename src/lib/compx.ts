import models from './compx-models.json';
export type CompxBrand='atk'|'vxe'|'vgn'|'pulsar';
export const compxModels=models;
export function compxModel(vendor:number,product:number){return models.find(m=>m.vendor===vendor&&m.product===product);}
export function nativeCompx(vendor:number,product:number){return !!compxModel(vendor,product)?.native_macros;}
export function compxBrand(brand:string){return brand==='pulsar'?'Pulsar':brand==='vxe'?'蜻蜓 / VXE':brand==='vgn'?'VGN': 'ATK';}
