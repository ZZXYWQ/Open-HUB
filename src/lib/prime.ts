export const PRIME_SCHEMA='steelseries-prime-lighting/1';
export const isPrime=(v:number,p:number)=>v===0x1038&&p===0x182e;
export interface PrimeLights{enabled:boolean;color:{r:number;g:number;b:number};brightness:number}
export const defaultPrimeLights=():PrimeLights=>({enabled:true,color:{r:255,g:82,b:0},brightness:256});
