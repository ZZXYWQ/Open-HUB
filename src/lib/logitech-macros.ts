import {KEY_NAMES} from './sino';
export interface NativeMacroAddress{sector:number;offset:number}
export type NativeMacroAction='NoOp'|'WaitForRelease'|'RepeatWhilePressed'|'RepeatUntilCanceled'|'End'|{Delay:{milliseconds:number}}|{KeyDown:{modifiers:number;usage:number}}|{KeyUp:{modifiers:number;usage:number}}|{MouseDown:{buttons:number}}|{MouseUp:{buttons:number}}|{ConsumerDown:{usage:number}}|{ConsumerUp:{usage:number}}|{Jump:{target:NativeMacroAddress}};
export interface NativeMacroTrace{entry:NativeMacroAddress;complete:boolean;note:string|null;steps:Array<{address:NativeMacroAddress;bytes:number[];action:NativeMacroAction}>;pages:Array<{sector:number;crc_valid:boolean;hex:string}>}
export interface NativeMacroSelection{expected_device:string;expected_connection:string;slot:number;sector:number;layer:'Normal'|'GShift';button:number;expected:number[]}
const media:Record<number,string>={0xcd:'播放 / 暂停',0xb5:'下一曲',0xb6:'上一曲',0xb7:'停止播放',0xe2:'静音',0xe9:'增大音量',0xea:'减小音量',0x192:'计算器'};
export function nativeMacroActionName(action:NativeMacroAction):string{
 if(typeof action==='string')return {NoOp:'等待一个原生宏步长',WaitForRelease:'等待松开触发按键',RepeatWhilePressed:'按住时重复',RepeatUntilCanceled:'重复至取消',End:'结束标记'}[action];
 if('Delay'in action)return `延迟 ${action.Delay.milliseconds} 毫秒`;
 if('Jump'in action)return `跳转到第 ${action.Jump.target.sector} 页，偏移 ${action.Jump.target.offset}`;
 if('KeyDown'in action||'KeyUp'in action){const down='KeyDown'in action,key='KeyDown'in action?action.KeyDown:action.KeyUp;const mods=['左 Ctrl','左 Shift','左 Alt','左 Win','右 Ctrl','右 Shift','右 Alt','右 Win'].filter((_,i)=>key.modifiers&(1<<i));const name=KEY_NAMES.find(([k])=>k===key.usage)?.[1]??(key.usage===0?'':`HID ${key.usage}`);return `${down?'按下':'松开'} ${[...mods,...(name?[name]:[])].join(' + ')||'无键值'}`;}
 if('MouseDown'in action||'MouseUp'in action){const down='MouseDown'in action,mask='MouseDown'in action?action.MouseDown.buttons:action.MouseUp.buttons;const keys=Array.from({length:16},(_,i)=>i).filter(i=>mask&(1<<i)).map(i=>['左键','右键','中键','后退','前进'][i]??`鼠标键 ${i+1}`);return `${down?'按下':'松开'} ${keys.join(' + ')||'空鼠标位图'}`;}
 const down='ConsumerDown'in action,usage='ConsumerDown'in action?action.ConsumerDown.usage:action.ConsumerUp.usage;return `${down?'按下':'松开'} ${media[usage]??`媒体 HID ${usage}`}`;
}
