export interface LogitechLightSlot {index:number;effect_id:number;capabilities:number;period:number}
export interface LogitechLightZone {index:number;location:number;persistency:number;effects:LogitechLightSlot[];current:number[]|null}
export interface LogitechLightView {device_key:string;connection_key:string;feature:number;version:number;nv_caps:number;ext_caps:number;writable:boolean;zones:LogitechLightZone[]}
export type LogitechLightEffect='Off'|{Fixed:{color:[number,number,number]}}|{Breathing:{color:[number,number,number];brightness_percent:number}}|{Cycle:{brightness_percent:number}};
export interface LogitechLightChange {expected_device:string;expected_connection:string;zone:number;expected:number[];effect:LogitechLightEffect}
export function zoneName(zone:LogitechLightZone){return ({1:'主灯区',2:'徽标',3:'左侧',4:'右侧',5:'组合灯区'} as Record<number,string>)[zone.location]??`灯区 ${zone.index+1}`;}
export function currentEffect(zone:LogitechLightZone){const raw=zone.current;return raw?zone.effects.find(s=>s.index===raw[0])?.effect_id:null;}
export function colorHex(rgb:number[]){return '#'+rgb.map(v=>v.toString(16).padStart(2,'0')).join('');}
export function parseColor(hex:string):[number,number,number]{if(!/^#[a-f0-9]{6}$/i.test(hex))throw Error('请选择有效颜色');return [1,3,5].map(i=>parseInt(hex.slice(i,i+2),16)) as [number,number,number];}

export function nativeRhythm(zone:LogitechLightZone,id:number){
 if(zone.current&&currentEffect(zone)===id){const at=id===10?4:6;return zone.current[at]*256+zone.current[at+1];}
 const slots=zone.effects.filter(s=>s.effect_id===id);return slots.length===1&&slots[0].period>0?slots[0].period:null;
}
export function currentBrightness(zone:LogitechLightZone){const at=currentEffect(zone)===10?7:currentEffect(zone)===3?8:-1;const value=at<0?0:zone.current?.[at]??0;return value>=1&&value<=100?value:100;}
