import type {RgbDevice} from './api';
export function eligibleMouse(d:RgbDevice){return d.kind===6&&d.direct&&d.leds>0;}
export function reconcileTargets(devices:RgbDevice[],current:{id:number;key:string}[],auto:boolean,excluded:Set<string>){return devices.filter(d=>d.direct&&d.leds>0&&(current.some(t=>t.key===d.key)||(auto&&eligibleMouse(d)&&!excluded.has(d.key)))).map(d=>({id:d.id,key:d.key}));}
export function rgbBrand(d:Pick<RgbDevice,'vendor'|'name'>){if(d.vendor.trim())return d.vendor;const name=d.name;return ['Logitech','Razer','Corsair','SteelSeries','HyperX','Glorious','Roccat','ASUS','Cooler Master','Redragon','AOC','EVGA','NZXT','Lenovo','Patriot','Trust','Bloody'].find(v=>name.toLowerCase().includes(v.toLowerCase()))??'品牌未报告';}
