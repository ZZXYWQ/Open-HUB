import type {MultiColor} from './multi';
export const RIVAL3_SCHEMA='steelseries-rival3-lighting/1';
export const isRival3=(v:number,p:number)=>v===0x1038&&[0x1824,0x184c].includes(p);
export interface Rival3Lights{enabled:boolean;mode:number;zones:Array<{color:MultiColor;brightness:number}>}
export const rival3Modes=['彩虹流动','快速呼吸','中速呼吸','慢速呼吸','常亮','彩虹呼吸','迪斯科'];
export const defaultRival3Lights=():Rival3Lights=>({enabled:true,mode:4,zones:[{r:255,g:0,b:0},{r:0,g:255,b:0},{r:0,g:0,b:255},{r:128,g:0,b:128}].map(color=>({color,brightness:100}))});

export const RIVAL3_LOOP_SCHEMA='openhub-rival3-loop/1';
export interface Rival3Sequence {repeat:number;data:Array<[number,number,number]>}
export interface Rival3Loop {step_ms:number;colors:Array<[number,number,number]>;zones:Array<Rival3Sequence|null>}
export const defaultRival3Loop=(native?:Rival3Lights|null):Rival3Loop=>({step_ms:100,colors:native?.zones.map(z=>[z.color.r,z.color.g,z.color.b] as [number,number,number])??[[0,0,0],[0,0,0],[0,0,0],[0,0,0]],zones:[{repeat:255,data:[[0,180,255],[170,80,255],[255,70,150]]},null,null,null]});
