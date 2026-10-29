import {isRival3,RIVAL3_SCHEMA,type Rival3Lights} from './rival3';
import {isSteelseries} from './steelseries';
import {compxModel} from './compx';
import {isX11,X11_SCHEMA,type X11Profile} from './x11';
export interface MouseTuning {debounce_ms?:number;motion_sync?:boolean;angle_snapping?:boolean;ripple_control?:boolean;led_idle_seconds?:number;sensor_sleep_enabled?:boolean;sensor_sleep_seconds?:number;lift_off_microns?:number;device_sleep_seconds?:number;low_power_percent?:number}
export interface TuningCaps {debounce_ms:number[];motion_sync:boolean;angle_snapping:boolean;ripple_control:boolean;led_idle_seconds:number[];sensor_sleep_enabled:boolean;sensor_sleep_seconds:number[];lift_off_microns:number[];device_sleep_seconds?:number[];low_power_percent?:number[]}
export type SensorCalibration={Standard:{distance:number}}|{Asymmetric:{lift:number;landing:number}};
export interface SensorCalibrationCaps {standard_levels:number[];lift_levels:number[];landing_levels:number[]}
export interface MultiDpi {stages:Array<{x:number;y:number}>;active:number}
export interface ShortcutEvent {kind:'Modifier'|'Key'|'Mouse'|'Media';down:boolean;value:number}
export type MultiAction='Default'|'Disabled'|'Hypershift'|'ProfileCycle'|{ProfileSelect:number}|{Mouse:'Left'|'Right'|'Middle'|'Back'|'Forward'|'WheelLeft'|'WheelRight'}|{Scroll:'Up'|'Down'}|{Key:{usage:number;mods:number}}|{Dpi:'Cycle'|'CycleDown'|'Up'|'Down'|{Shift:number}|{ShiftXY:{x:number;y:number}}}|{Macro:string}|{Media:number}|{Shortcut:ShortcutEvent[]};
export interface MultiColor {r:number;g:number;b:number}
export interface MultiColorStop{color:MultiColor;position:number}
export interface MultiGradient{stops:MultiColorStop[];count:number;period_ms:number}
export type MultiEffect={Gradient:MultiGradient}|'Off'|{Static:{color:MultiColor}}|{Breathing:{color:MultiColor;period_ms:number}}|{ColorCycle:{period_ms:number}}|{Wave:{period_ms:number;direction:'LeftToRight'|'RightToLeft'}}|{Reactive:{color:MultiColor;duration_ms:number}};
export type IndicatorLevel='Low'|'Medium'|'High';
export interface DpiIndicator {enabled:boolean;mode:'Static'|'Breathing'|'Unspecified';brightness:IndicatorLevel;speed:IndicatorLevel}
export interface MultiZoneState {effect:MultiEffect;brightness:number}
export type MultiSetting={DpiIndicator:DpiIndicator}|{DpiColor:{stage:number;color:MultiColor}}|{Dpi:MultiDpi}|{ReportRate:number}|{Button:{id:number;action:MultiAction}}|{Lighting:{zone:number;state:MultiZoneState}}|{Tuning:MouseTuning}|{SensorCalibration:SensorCalibration}|{LiftOff:number}|{AngleSnapping:boolean}|{VendorProfile:{schema:typeof X11_SCHEMA;data:X11Profile}|{schema:typeof RIVAL3_SCHEMA;data:Rival3Lights}};
export interface MultiPlan {vendor:number;product:number;settings:MultiSetting[];macros?:MultiMacro[];source?:MultiSource|null}
export interface MultiView {write_connection?:string|null;descriptor:{id:string;name:string;vendor_id:number;product_id:number;serial?:string|null;simulated:boolean};capabilities:{battery?:boolean;dpi_indicator?:boolean;dpi_color_stages?:number;calibration?:SensorCalibrationCaps|null;vendor_profile?:string;lift_off?:string[];angle_snapping?:boolean;tuning?:TuningCaps;dpi:{min:number;max:number;step:number;max_stages:number;separate_xy:boolean}|null;report_rates:number[];buttons:Array<{id:number;name:string;remappable:boolean;default_action?:MultiAction}>;lighting:Array<{id:number;name:string;effects:string[];brightness?:boolean}>};state:{dpi_indicator?:DpiIndicator|null;dpi_colors?:Record<string,MultiColor>;calibration?:SensorCalibration|null;battery?:{percent:number|null;charging:boolean}|null;vendor_profile?:X11Profile|Rival3Lights;lift_off?:number|null;angle_snapping?:boolean|null;tuning?:MouseTuning;dpi:MultiDpi|null;report_rate:number|null;buttons:Record<string,MultiAction>;lighting:Record<string,MultiZoneState>};readback:string;hardware_tested:boolean;native_macros?:MultiMacro[];session?:{hardware_save_attempted?:boolean;plan:MultiPlan;software_state:'hardware'|'running'|'stopped';temporary_dpi:boolean}}
export function fromMulti(v:MultiView):MultiPlan{
 if(v.session){const p=v.session.plan;if(p.vendor!==v.descriptor.vendor_id||p.product!==v.descriptor.product_id)throw Error('会话方案与鼠标型号不一致，未载入旧方案');return JSON.parse(JSON.stringify(p)) as MultiPlan;}
 if(isX11(v.descriptor.vendor_id,v.descriptor.product_id))return {vendor:v.descriptor.vendor_id,product:v.descriptor.product_id,macros:[],source:null,settings:v.state.vendor_profile?[{VendorProfile:{schema:X11_SCHEMA,data:v.state.vendor_profile as X11Profile}}]:[]};
 // COMPX exposes verified individual records. Keep every known writable
 // binding and light on fresh hardware reads; an omitted/unknown record stays
 // untouched. Cache-only drivers are not promoted to this readback guarantee.
 const board=compxModel(v.descriptor.vendor_id,v.descriptor.product_id);
 const steel=isSteelseries(v.descriptor.vendor_id,v.descriptor.product_id);
 const dav3=v.descriptor.vendor_id===0x1532&&[0x00b6,0x00b7].includes(v.descriptor.product_id);
 const nativeSettings:MultiSetting[]=steel?[
  ...v.capabilities.buttons.filter(b=>b.remappable&&b.id!==1).map(b=>({Button:{id:b.id,action:v.state.buttons[String(b.id)]??b.default_action??'Default'}})),
  ...(isRival3(v.descriptor.vendor_id,v.descriptor.product_id)?v.state.vendor_profile?[{VendorProfile:{schema:RIVAL3_SCHEMA as typeof RIVAL3_SCHEMA,data:v.state.vendor_profile as Rival3Lights}}]:[]:v.capabilities.lighting.filter(z=>v.state.lighting[String(z.id)]!=null).map(z=>({Lighting:{zone:z.id,state:v.state.lighting[String(z.id)]}}))),
 ]:board||dav3?[
  ...(v.capabilities.dpi_indicator&&v.state.dpi_indicator?[{DpiIndicator:v.state.dpi_indicator}]:[]),
  ...Object.entries(v.state.dpi_colors??{}).filter(([stage])=>Number(stage)<(v.capabilities.dpi_color_stages??0)).map(([stage,color])=>({DpiColor:{stage:Number(stage),color}})),
  ...v.capabilities.buttons.filter(b=>b.remappable&&v.state.buttons[String(b.id)]!=null).map(b=>({Button:{id:b.id,action:v.state.buttons[String(b.id)]}})),
  ...v.capabilities.lighting.filter(z=>v.state.lighting[String(z.id)]!=null&&!(z.id===0&&v.capabilities.dpi_indicator&&v.state.dpi_indicator)).map(z=>({Lighting:{zone:z.id,state:v.state.lighting[String(z.id)]}})),
 ]:[];
 return {vendor:v.descriptor.vendor_id,product:v.descriptor.product_id,macros:v.native_macros??[],settings:[...(v.state.dpi?[{Dpi:v.state.dpi}]:[]),...(v.state.report_rate?[{ReportRate:v.state.report_rate}]:[]),...(v.state.lift_off!=null?[{LiftOff:v.state.lift_off}]:[]),...(v.state.angle_snapping!=null?[{AngleSnapping:v.state.angle_snapping}]:[]),...(v.state.tuning&&Object.keys(v.state.tuning).length?[{Tuning:v.state.tuning}]:[]),...(dav3&&v.state.calibration&&v.capabilities.calibration?[{SensorCalibration:v.state.calibration}]:[]),...nativeSettings]};
}
export function multiProfile(plan:MultiPlan,name:string){if(isX11(plan.vendor,plan.product)){const item=plan.settings.find(s=>'VendorProfile'in s);if(!item||!('VendorProfile'in item)||item.VendorProfile.schema!==X11_SCHEMA)throw Error('请先创建或载入完整 X11 方案');const p=item.VendorProfile.data;return {id:'',name,app_path:null,target_vendor:plan.vendor,target_model:plan.product,mchose_board:null,ghub_id:null,sino:null,multi:plan,levels:p.dpi.stages.map(x=>({x,y:x})),active_level:p.dpi.active,report_rate_hz:p.report_rate};}const dpi=plan.settings.find(s=>'Dpi'in s);const rate=plan.settings.find(s=>'ReportRate'in s);if(!dpi||!('Dpi'in dpi))throw Error('缺少 DPI 设置');return {id:'',name,app_path:null,target_vendor:plan.vendor,target_model:plan.product,mchose_board:null,ghub_id:null,sino:null,multi:plan,levels:dpi.Dpi.stages.map(v=>({x:v.x,y:v.y})),active_level:dpi.Dpi.active,report_rate_hz:rate&&'ReportRate'in rate?rate.ReportRate:null};}
export interface MultiSource {vendor:number;product:number;path:string}
export type MultiStep={KeyDown:number}|{KeyUp:number}|{Delay:number}|{MouseDown:'Left'|'Right'|'Middle'|'Back'|'Forward'}|{MouseUp:'Left'|'Right'|'Middle'|'Back'|'Forward'}|{Text:string}|{Media:number}|{Scroll:'Up'|'Down'}|{ScrollTimed:{direction:'Up'|'Down';pulse_ms:number;settle_ms:number}}|{KeyTap:{usage:number;mods:number}}|'WaitForRelease';
export interface MultiMacro {id:string;name:string;mode:'Once'|'WhileHeld'|'Toggle'|{Repeat:number};steps:MultiStep[]}

export function basiliskPro(v:number,p:number){return v===0x1532&&[0x00aa,0x00ab].includes(p);}

// Stable equality for a draft and its native round trip. JSON field order and
// absent optional fields do not turn an unchanged applied plan into a new one.
export function multiPlanSignature(plan:MultiPlan|null):string{
 const ordered=(value:unknown):unknown=>Array.isArray(value)?value.map(ordered):value!==null&&typeof value==='object'?Object.fromEntries(Object.entries(value).filter(([,v])=>v!==undefined).sort(([a],[b])=>a.localeCompare(b)).map(([k,v])=>[k,ordered(v)])):value;
 return JSON.stringify(ordered(plan?{...plan,macros:plan.macros??[],source:plan.source??null}:null));
}
