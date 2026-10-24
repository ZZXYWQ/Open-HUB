import models from './compx-models.json';
export type CompxBrand='atk'|'vxe'|'vgn';
export const compxModels=models;
export function compxModel(vendor:number,product:number){return models.find(m=>m.vendor===vendor&&m.product===product);}
export function nativeCompx(vendor:number,product:number){return !!compxModel(vendor,product);}
export function compxBrand(brand:string){return brand==='vxe'?'蜻蜓 / VXE':brand==='vgn'?'VGN': 'ATK';}
