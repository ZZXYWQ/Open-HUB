export interface MultiDpi {stages:Array<{x:number;y:number}>;active:number}
export type MultiAction='Default'|'Disabled'|{Mouse:'Left'|'Right'|'Middle'|'Back'|'Forward'}|{Key:{usage:number;mods:number}}|{Dpi:'Cycle'|'Up'|'Down'|{Shift:number}}|{Macro:string};
export interface MultiColor {r:number;g:number;b:number}
export type MultiEffect='Off'|{Static:{color:MultiColor}}|{Breathing:{color:MultiColor;period_ms:number}}|{ColorCycle:{period_ms:number}}|{Wave:{period_ms:number;direction:'LeftToRight'|'RightToLeft'}}|{Reactive:{color:MultiColor;duration_ms:number}};
export interface MultiZoneState {effect:MultiEffect;brightness:number}
export type MultiSetting={Dpi:MultiDpi}|{ReportRate:number}|{Button:{id:number;action:MultiAction}}|{Lighting:{zone:number;state:MultiZoneState}};
export interface MultiPlan {vendor:number;product:number;settings:MultiSetting[];macros?:MultiMacro[];source?:MultiSource|null}
export interface MultiView {descriptor:{id:string;name:string;vendor_id:number;product_id:number;serial?:string|null;simulated:boolean};capabilities:{dpi:{min:number;max:number;step:number;max_stages:number;separate_xy:boolean}|null;report_rates:number[];buttons:Array<{id:number;name:string;remappable:boolean}>;lighting:Array<{id:number;name:string;effects:string[];brightness?:boolean}>};state:{dpi:MultiDpi|null;report_rate:number|null;buttons:Record<string,MultiAction>;lighting:Record<string,MultiZoneState>};readback:string;hardware_tested:boolean;native_macros?:MultiMacro[];session?:{plan:MultiPlan;software_state:'hardware'|'running'|'stopped';temporary_dpi:boolean}}
export function fromMulti(v:MultiView):MultiPlan{
 if(v.session){const p=v.session.plan;if(p.vendor!==v.descriptor.vendor_id||p.product!==v.descriptor.product_id)throw Error('会话方案与鼠标型号不一致，未载入旧方案');return JSON.parse(JSON.stringify(p)) as MultiPlan;}
 return {vendor:v.descriptor.vendor_id,product:v.descriptor.product_id,macros:v.native_macros??[],settings:[...(v.state.dpi?[{Dpi:v.state.dpi}]:[]),...(v.state.report_rate?[{ReportRate:v.state.report_rate}]:[])]};
}
export function multiProfile(plan:MultiPlan,name:string){const dpi=plan.settings.find(s=>'Dpi'in s);const rate=plan.settings.find(s=>'ReportRate'in s);if(!dpi||!('Dpi'in dpi))throw Error('缺少 DPI 设置');return {id:'',name,app_path:null,target_vendor:plan.vendor,target_model:plan.product,mchose_board:null,ghub_id:null,sino:null,multi:plan,levels:dpi.Dpi.stages.map(v=>({x:v.x,y:v.y})),active_level:dpi.Dpi.active,report_rate_hz:rate&&'ReportRate'in rate?rate.ReportRate:null};}
export interface MultiSource {vendor:number;product:number;path:string}
export type MultiStep={KeyDown:number}|{KeyUp:number}|{Delay:number}|{MouseDown:'Left'|'Right'|'Middle'|'Back'|'Forward'}|{MouseUp:'Left'|'Right'|'Middle'|'Back'|'Forward'}|{Text:string}|{KeyTap:{usage:number;mods:number}}|'WaitForRelease';
export interface MultiMacro {id:string;name:string;mode:'Once'|'WhileHeld'|'Toggle';steps:MultiStep[]}
