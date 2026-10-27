export interface Sc620Source {
 settings:{profile_id:number;fields:Record<string,string>;unconverted:Record<string,string>;patch:{dpi:number[];active:number;report_rate_hz:number;debounce_ms:number;light:string;colors:number[][];lod_mm:number|null;scroll_reverse:boolean|null;esports:boolean|null;sleep_minutes:number|null;ripple_control:boolean|null;angle_snap:boolean|null;motion_sync:boolean|null}};
 keys:Array<{id:number;profile:number;layer:number;key_code:number;layout_value:number;layout_description:string;kind:number;value:number;value2:number;value3:number;description:string}>;
 macros:Array<{id:number;name:string;playback:string|{Repeat:number};play_type:number;play_times:number;delay_type:number;delay_time:number;events:Array<{record_id:number;kind:number;index:number;description:string;value:number;delay_raw:number}>}>;
}
export interface Sc620Preview {
 profile_ids:number[];source:Sc620Source;
 keys:Array<{source_control:number;layer:number;record:number|null;action:unknown;macro_slot:number|null;problem:string|null}>;
 macros:Array<{id:number;problem:string|null}>;issues:string[];static_subset_convertible:boolean;hardware_apply_available:false;
}
const actions:Record<string,string>={DoubleClick:'双击',WheelUp:'滚轮向上',WheelDown:'滚轮向下',DpiCycle:'DPI 循环（旧编码）',DpiIncrease:'DPI 增加',DpiDecrease:'DPI 减少',DpiLoop:'DPI 循环',ReportRateIncrease:'回报率增加',ReportRateDecrease:'回报率减少',ReportRateLoop:'回报率循环',Fire:'火力键'};
const mouse:Record<string,string>={Left:'左键',Right:'右键',Middle:'中键',Forward:'前进',Back:'后退'};
const media:Record<string,string>={Player:'媒体播放器',PlayPause:'播放／暂停',Stop:'停止播放',Previous:'上一曲',Next:'下一曲',VolumeUp:'音量增加',VolumeDown:'音量减少',Mute:'静音',ScreenBrightnessUp:'屏幕亮度增加',ScreenBrightnessDown:'屏幕亮度减少',Calculator:'计算器',BrowserHome:'浏览器主页',Mail:'邮件'};
export function sc620ActionLabel(action:unknown):string{
 if(typeof action==='string')return actions[action]??action;
 if(!action||typeof action!=='object')return '未转换';
 const a=action as Record<string,unknown>;
 if('Mouse'in a)return mouse[String(a.Mouse)]??'鼠标动作';
 if('Media'in a)return media[String(a.Media)]??'媒体动作';
 if('Disabled'in a)return '禁用';
 if(a.Key&&typeof a.Key==='object'){
  const k=a.Key as {usage:number;modifiers:number},mods=['左 Ctrl','左 Shift','左 Alt','左 Win','右 Ctrl','右 Shift','右 Alt','右 Win'].filter((_,i)=>(k.modifiers&(1<<i))!==0);
  if(k.usage!==0)mods.push(k.usage>=4&&k.usage<=29?String.fromCharCode(61+k.usage):`HID 0x${k.usage.toString(16).toUpperCase()}`);
  return mods.join(' + ');
 }
 return '未转换';
}
export function sc620Playback(value:Sc620Source['macros'][number]['playback']){return typeof value==='object'?`重复 ${value.Repeat} 次`:({Once:'播放一次',WhileHeld:'按住播放',UntilAnyClick:'直到任意键点击停止'}[value]??value);}
export const sc620Lights:Record<string,string>={Wave:'波浪',Neon:'霓虹',CyclingFlash:'循环闪烁',YoyoBall:'悠悠球',SingleFlash:'单色闪烁',BreathingLoop:'循环呼吸',Off:'关闭'};
export interface Sc620EditRequest{settings:Partial<Sc620Source['settings']['patch']>;buttons:Array<{control:number;choice:unknown}>;macros:Array<{id:number;playback:Sc620Source['macros'][number]['playback']}>}
export const sc620BasicChoices=[...Object.entries(actions).map(([action,label])=>({action,label})),...Object.entries(mouse).map(([key,label])=>({action:{Mouse:key},label})),...Object.entries(media).map(([key,label])=>({action:{Media:key},label}))];
