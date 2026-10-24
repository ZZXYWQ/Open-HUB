import type {EffectConfig,RgbDevice} from './api';
const kinds=['static','rainbow','gradient','breathe','comet','twinkle','music','screen'];
const color=(v:unknown):v is number[]=>Array.isArray(v)&&v.length===3&&v.every(x=>Number.isInteger(x)&&x>=0&&x<=255);
const range=(v:unknown,a:number,b:number)=>typeof v==='number'&&Number.isFinite(v)&&v>=a&&v<=b;
export function importLightingPreset(doc:unknown,devices:RgbDevice[],current:EffectConfig['rgb']){
 if(!doc||typeof doc!=='object')throw Error('灯光方案格式无效');
 const d=doc as Record<string,unknown>;if(d.format&&d.format!=='open-hub-lighting/1')throw Error('不支持的灯光方案版本');
 const c=(d.format?d.config:doc) as EffectConfig;
 if(!c||!kinds.includes(c.kind)||!Number.isInteger(c.style)||!range(c.style,0,5)||!color(c.color)||!Array.isArray(c.palette)||c.palette.length<2||c.palette.length>8||!c.palette.every(color)||!range(c.brightness,0,1)||!range(c.speed,.1,5)||!range(c.gain,.5,8)||typeof c.dock!=='boolean')throw Error('灯光方案参数无效，原设置未改变');
 const keys=d.target_keys;if(keys!==undefined&&(!Array.isArray(keys)||keys.length>64||keys.some(k=>typeof k!=='string'||k.length>4096)))throw Error('方案设备标识无效');
 const rgb=devices.filter(v=>v.direct&&v.leds>0&&(Array.isArray(keys)?keys.includes(v.key):current.some(t=>t.key===v.key))).map(v=>({id:v.id,key:v.key}));
 return {config:{kind:c.kind,style:c.style,color:c.color,palette:c.palette,brightness:c.brightness,speed:c.speed,gain:c.gain,dock:c.dock,rgb},missing:Array.isArray(keys)?keys.filter(k=>!rgb.some(t=>t.key===k)).length:0};
}
