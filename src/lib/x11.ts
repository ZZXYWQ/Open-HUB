export const X11_SCHEMA='attack-shark-x11/1';
export const isX11=(v:number,p:number)=>v===0x1d57&&[0xfa55,0xfa60].includes(p);
export type X11Light='Off'|'Static'|'Breathing'|'Neon'|'ColorBreathing'|'StaticDpi'|'BreathingDpi';
export const x11Lights:Record<X11Light,string>={Off:'关闭',Static:'常亮',Breathing:'单色呼吸',Neon:'霓虹循环',ColorBreathing:'彩色呼吸',StaticDpi:'DPI 常亮',BreathingDpi:'DPI 呼吸'};
export const x11Basic:Record<string,string>={Disabled:'禁用',Left:'左键',Right:'右键',Middle:'中键',Back:'后退',Forward:'前进',DoubleClick:'双击',WheelUp:'向上滚动',WheelDown:'向下滚动',DpiCycle:'循环 DPI',DpiUp:'提高 DPI',DpiDown:'降低 DPI'};
export const x11Media:Record<string,string>={Player:'媒体播放器',Previous:'上一曲',Next:'下一曲',PlayPause:'播放 / 暂停',Stop:'停止播放',Mute:'静音',VolumeUp:'音量增加',VolumeDown:'音量减少',Calculator:'计算器',Email:'邮件',BrowserForward:'浏览器前进',BrowserBack:'浏览器后退',BrowserStop:'停止浏览',Computer:'此电脑',BrowserRefresh:'刷新浏览器',BrowserHome:'浏览器主页',BrowserSearch:'搜索'};
export type X11Action=keyof typeof x11Basic|{Key:{usage:number;mods:number}}|{Media:string}|{Macro:string};
export type X11Input={Key:number}|{Mouse:'Left'|'Right'|'Middle'|'Back'|'Forward'};
export interface X11Event {input:X11Input;down:boolean;delay_ms:number}
export interface X11Macro {id:string;name:string;playback:{Times:{count:number}}|'UntilAnyKey'|'WhileHeld';events:X11Event[]}
export interface X11Profile {dpi:{stages:number[];active:number;angle_snapping:boolean;ripple_control:boolean;opaque_14_15:[number,number]};report_rate:number;preferences:{light:X11Light;color:{r:number;g:number;b:number};light_speed:number;sleep_seconds:number;deep_sleep_seconds:number;debounce_ms:number};buttons:Record<string,X11Action>;macros:X11Macro[]}
export const x11DpiValues=[...Array.from({length:200},(_,i)=>(i+1)*50),...Array.from({length:120},(_,i)=>10100+i*100)];
export function x11Example():X11Profile {return {dpi:{stages:[800,1600,2400,3200,5000,22000],active:1,angle_snapping:false,ripple_control:true,opaque_14_15:[0,0]},report_rate:1000,preferences:{light:'Off',color:{r:0,g:255,b:0},light_speed:3,sleep_seconds:30,deep_sleep_seconds:600,debounce_ms:4},buttons:{'1':'Left','2':'Right','3':'Middle','4':'Back','5':'Forward','6':'DpiCycle','7':'WheelUp','8':'WheelDown'},macros:[]};}
