import data from './device-visuals.json';
export interface DeviceVisual {id:string;label:string;vendor:number|null;products:number[];names:string[];images:Array<{label:string;src:string}>}
export const deviceVisuals:DeviceVisual[]=data;
export function deviceVisual(vendor:number|undefined,product:number|string|undefined,name=''){
 const pid=typeof product==='string'?parseInt(product,16):product;
 const normalized=name.trim().toUpperCase();
 return deviceVisuals.find(v=>(v.vendor==null||v.vendor===vendor)&&(v.products.length&&pid!=null?v.products.includes(pid):v.names.some(n=>n.toUpperCase()===normalized)));
}

// OpenRGB exposes display names, not the native USB configuration identity.
// This is presentation only: it never chooses or routes a control target.
export function rgbArtwork(device:{name:string;vendor:string}){
 const vendors:Record<string,number>={hyperx:0x03f0,pulsar:0x3554,logitech:0x046d,razer:0x1532,corsair:0x1b1c,steelseries:0x1038,glorious:0x3794,'g-wolves':0x258a,dreammachines:0x258a,genesis:0x258a,machenike:0x258a,'mad dog':0x258a,'t-dagger':0x258a,marvo:0x258a};
 const vendorId=vendors[device.vendor.trim().toLowerCase()];
 if(vendorId==null)return undefined;
 for(const name of [device.name,device.vendor+' '+device.name]){const visual=deviceVisual(vendorId,undefined,name);if(visual)return{vendorId,name};}
 return undefined;
}
