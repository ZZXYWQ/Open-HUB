export const PRIME_SCHEMA='steelseries-prime-lighting/1';
export const isPrime=(v:number,p:number)=>v===0x1038&&p===0x182e;
export interface PrimeLights{enabled:boolean;color:{r:number;g:number;b:number};brightness:number;program?:PrimeProgram}
export const defaultPrimeLights=():PrimeLights=>({enabled:true,color:{r:255,g:82,b:0},brightness:256});

export interface PrimeProgram{initial:{r:number;g:number;b:number};duration:number;motion:{enabled:boolean;increase:number;decrease:number;minimum:number};sections:{color:{r:number;g:number;b:number};weight:number}[]}
