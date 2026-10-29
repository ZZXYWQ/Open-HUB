import type {MultiColor} from './multi';
export const RIVAL3_SCHEMA='steelseries-rival3-lighting/1';
export const isRival3=(v:number,p:number)=>v===0x1038&&[0x1824,0x184c].includes(p);
export interface Rival3Lights{enabled:boolean;mode:number;zones:Array<{color:MultiColor;brightness:number}>}
export const rival3Modes=['彩虹流动','快速呼吸','中速呼吸','慢速呼吸','常亮','彩虹呼吸','迪斯科'];
export const defaultRival3Lights=():Rival3Lights=>({enabled:true,mode:4,zones:[{r:255,g:0,b:0},{r:0,g:255,b:0},{r:0,g:0,b:255},{r:128,g:0,b:128}].map(color=>({color,brightness:100}))});
