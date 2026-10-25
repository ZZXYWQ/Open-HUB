import {compxModel} from './compx';
import {isX11,X11_SCHEMA,type X11Profile} from './x11';
export interface MouseTuning {debounce_ms?:number;motion_sync?:boolean;angle_snapping?:boolean;ripple_control?:boolean;led_idle_seconds?:number;sensor_sleep_enabled?:boolean;sensor_sleep_seconds?:number;lift_off_microns?:number}
export interface TuningCaps {debounce_ms:number[];motion_sync:boolean;angle_snapping:boolean;ripple_control:boolean;led_idle_seconds:number[];sensor_sleep_enabled:boolean;sensor_sleep_seconds:number[];lift_off_microns:number[]}
export interface MultiDpi {stages:Array<{x:number;y:number}>;active:number}
export type MultiAction='Default'|'Disabled'|{Mouse:'Left'|'Right'|'Middle'|'Back'|'Forward'|'WheelLeft'|'WheelRight'}|{Key:{usage:number;mods:number}}|{Dpi:'Cycle'|'Up'|'Down'|{Shift:number}}|{Macro:string}|{Media:number};
export interface MultiColor {r:number;g:number;b:number}
export type MultiEffect='Off'|{Static:{color:MultiColor}}|{Breathing:{color:MultiColor;period_ms:number}}|{ColorCycle:{period_ms:number}}|{Wave:{period_ms:number;direction:'LeftToRight'|'RightToLeft'}}|{Reactive:{color:MultiColor;duration_ms:number}};
export interface MultiZoneState {effect:MultiEffect;brightness:number}
export type MultiSetting={Dpi:MultiDpi}|{ReportRate:number}|{Button:{id:number;action:MultiAction}}|{Lighting:{zone:number;state:MultiZoneState}}|{Tuning:MouseTuning}|{LiftOff:number}|{AngleSnapping:boolean}|{VendorProfile:{schema:string;data:X11Profile}};
export interface MultiPlan {vendor:number;product:number;settings:MultiSetting[];macros?:MultiMacro[];source?:MultiSource|null}
export interface MultiView {descriptor:{id:string;name:string;vendor_id:number;product_id:number;serial?:string|null;simulated:boolean};capabilities:{vendor_profile?:string;lift_off?:string[];angle_snapping?:boolean;tuning?:TuningCaps;dpi:{min:number;max:number;step:number;max_stages:number;separate_xy:boolean}|null;report_rates:number[];buttons:Array<{id:number;name:string;remappable:boolean}>;lighting:Array<{id:number;name:string;effects:string[];brightness?:boolean}>};state:{battery?:{percent:number|null;charging:boolean}|null;vendor_profile?:X11Profile;lift_off?:number|null;angle_snapping?:boolean|null;tuning?:MouseTuning;dpi:MultiDpi|null;report_rate:number|null;buttons:Record<string,MultiAction>;lighting:Record<string,MultiZoneState>};readback:string;hardware_tested:boolean;native_macros?:MultiMacro[];session?:{plan:MultiPlan;software_state:'hardware'|'running'|'stopped';temporary_dpi:boolean}}
export function fromMulti(v:MultiView):MultiPlan{
 if(v.session){const p=v.session.plan;if(p.vendor!==v.descriptor.vendor_id||p.product!==v.descriptor.product_id)throw Error('会话方案与鼠标型号不一致，未载入旧方案');return JSON.parse(JSON.stringify(p)) as MultiPlan;}
 if(isX11(v.descriptor.vendor_id,v.descriptor.product_id))return {vendor:v.descriptor.vendor_id,product:v.descriptor.product_id,macros:[],source:null,settings:v.state.vendor_profile?[{VendorProfile:{schema:X11_SCHEMA,data:v.state.vendor_profile}}]:[]};
 // COMPX exposes verified individual records. Keep every known writable
 // binding and light on fresh hardware reads; an omitted/unknown record stays
 // untouched. Cache-only drivers are not promoted to this readback guarantee.
 const board=compxModel(v.descriptor.vendor_id,v.descriptor.product_id);
 const nativeSettings:MultiSetting[]=board?[
  ...v.capabilities.buttons.filter(b=>b.remappable&&v.state.buttons[String(b.id)]!=null).map(b=>({Button:{id:b.id,action:v.state.buttons[String(b.id)]}})),
  ...v.capabilities.lighting.filter(z=>v.state.lighting[String(z.id)]!=null).map(z=>({Lighting:{zone:z.id,state:v.state.lighting[String(z.id)]}})),
 ]:[];
 return {vendor:v.descriptor.vendor_id,product:v.descriptor.product_id,macros:v.native_macros??[],settings:[...(v.state.dpi?[{Dpi:v.state.dpi}]:[]),...(v.state.report_rate?[{ReportRate:v.state.report_rate}]:[]),...(v.state.lift_off!=null?[{LiftOff:v.state.lift_off}]:[]),...(v.state.angle_snapping!=null?[{AngleSnapping:v.state.angle_snapping}]:[]),...(v.state.tuning&&Object.keys(v.state.tuning).length?[{Tuning:v.state.tuning}]:[]),...nativeSettings]};
}
export function multiProfile(plan:MultiPlan,name:string){if(isX11(plan.vendor,plan.product)){const item=plan.settings.find(s=>'VendorProfile'in s);if(!item||!('VendorProfile'in item))throw Error('请先创建或载入完整 X11 方案');const p=item.VendorProfile.data;return {id:'',name,app_path:null,target_vendor:plan.vendor,target_model:plan.product,mchose_board:null,ghub_id:null,sino:null,multi:plan,levels:p.dpi.stages.map(x=>({x,y:x})),active_level:p.dpi.active,report_rate_hz:p.report_rate};}const dpi=plan.settings.find(s=>'Dpi'in s);const rate=plan.settings.find(s=>'ReportRate'in s);if(!dpi||!('Dpi'in dpi))throw Error('缺少 DPI 设置');return {id:'',name,app_path:null,target_vendor:plan.vendor,target_model:plan.product,mchose_board:null,ghub_id:null,sino:null,multi:plan,levels:dpi.Dpi.stages.map(v=>({x:v.x,y:v.y})),active_level:dpi.Dpi.active,report_rate_hz:rate&&'ReportRate'in rate?rate.ReportRate:null};}
export interface MultiSource {vendor:number;product:number;path:string}
export type MultiStep={KeyDown:number}|{KeyUp:number}|{Delay:number}|{MouseDown:'Left'|'Right'|'Middle'|'Back'|'Forward'}|{MouseUp:'Left'|'Right'|'Middle'|'Back'|'Forward'}|{Text:string}|{Media:number}|{KeyTap:{usage:number;mods:number}}|'WaitForRelease';
export interface MultiMacro {id:string;name:string;mode:'Once'|'WhileHeld'|'Toggle';steps:MultiStep[]}

export function basiliskPro(v:number,p:number){return v===0x1532&&[0x00aa,0x00ab].includes(p);}
