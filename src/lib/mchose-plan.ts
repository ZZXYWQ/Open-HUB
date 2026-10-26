import type {MchoseMacro} from './api';
/** Portable source plan; macro slot IDs are logical until native allocation. */
export interface MchosePlan {
 vendor:number;model:number;board:number;name:string;
 dpi_x:number[];dpi_y:number[]|null;count:number;active:number;independent:boolean;
 usb_rate:number;wireless_rate:number;sleep:number;never_sleep:boolean;
 left_debounce:number;right_debounce:number;sensor:number;rotation:number;lod:number;
 buttons:number[][];macros:(MchoseMacro&{raw:number[]})[];
}

export function mchoseRateHz(code:number):number|undefined {
 return ({0:125,2:500,3:1000,4:2000,5:4000,6:8000} as Record<number,number>)[code];
}
