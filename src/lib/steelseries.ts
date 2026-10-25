import {KEY_NAMES} from './sino';
// Mirrors the exact native bridge scope, checked by production DTO UI tests.
export const steelseriesProducts=[0x1710,0x1720,0x1722,0x1832];
export function isSteelseries(v:number,p:number){return v===0x1038&&steelseriesProducts.includes(p);}
export function steelseriesFixedStages(p:number){return [0x1710,0x1720,0x1722].includes(p)?2:null;}
export function steelseriesBreathPeriods(p:number){return p===0x1710?[3000,5000,7000]:null;}

const navigation:Record<number,string>={0x46:'Print Screen',0x47:'Scroll Lock',0x48:'Pause',0x49:'Insert',0x4a:'Home',0x4b:'Page Up',0x4c:'Delete',0x4d:'End',0x4e:'Page Down',0x53:'Num Lock',0x54:'Keypad /',0x55:'Keypad *',0x56:'Keypad −',0x57:'Keypad +',0x58:'Keypad Enter',0x62:'Keypad 0',0x63:'Keypad .',0x65:'Menu'};
const known=new Map<number,string>(KEY_NAMES);
// Native button-table encoder accepts these exact HID keyboard usages.
export const STEELSERIES_KEYS:Array<[number,string]>=[...Array.from({length:0x65-4+1},(_,i)=>i+4),...Array.from({length:12},(_,i)=>0x68+i),0x85,0x86,...Array.from({length:8},(_,i)=>0xe0+i)].map(code=>[code,known.get(code)??navigation[code]??(code>=0x59&&code<=0x61?`Keypad ${code-0x58}`:code>=0x68&&code<=0x73?`F${code-0x68+13}`:`HID 0x${code.toString(16).toUpperCase()}`)]);
