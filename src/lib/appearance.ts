export const SKIN_LIMIT=8192;
export const SKIN_STORAGE='open-hub:appearance:v1';
export interface HubSkin {format:'open-hub-skin/1';name:string;accent:string;background:string;surface:string;opacity:number;panelOpacity:number;blur:number;radius:number}
export const SKINS:HubSkin[]=[
 {format:'open-hub-skin/1',name:'深海薄荷',accent:'#70e6cf',background:'#0d1520',surface:'#172434',opacity:100,panelOpacity:94,blur:0,radius:12},
 {format:'open-hub-skin/1',name:'暮光紫',accent:'#c6a4ff',background:'#181524',surface:'#2a223b',opacity:100,panelOpacity:94,blur:0,radius:14},
 {format:'open-hub-skin/1',name:'冰川蓝',accent:'#7fcbff',background:'#0e1c2b',surface:'#19334a',opacity:100,panelOpacity:94,blur:0,radius:12},
 {format:'open-hub-skin/1',name:'暖白',accent:'#16766d',background:'#f2f4f0',surface:'#ffffff',opacity:100,panelOpacity:96,blur:0,radius:12},
];
const fields=['format','name','accent','background','surface','opacity','panelOpacity','blur','radius'];
export function validateSkin(value:unknown):HubSkin {
 if(!value||typeof value!=='object'||Array.isArray(value))throw Error('皮肤必须是 Open HUB 主题文件');
 const s=value as Record<string,unknown>;
 if(Object.keys(s).some(k=>!fields.includes(k))||fields.some(k=>!(k in s)))throw Error('皮肤字段不完整或包含不支持的内容');
 if(s.format!=='open-hub-skin/1')throw Error('不支持的皮肤格式');
 if(typeof s.name!=='string'||!s.name.trim()||[...s.name].length>40||/[\u0000-\u001f]/.test(s.name))throw Error('皮肤名称应为 1–40 个字符');
 for(const k of ['accent','background','surface'])if(typeof s[k]!=='string'||!/^#[0-9a-fA-F]{6}$/.test(s[k] as string))throw Error('皮肤颜色应为 #RRGGBB');
 for(const [key,min,max] of [['opacity',0,100],['panelOpacity',20,100],['blur',0,24],['radius',0,24]] as const){const n=s[key];if(typeof n!=='number'||!Number.isInteger(n)||n<min||n>max)throw Error(`${key} 超出允许范围`);}
 return {...s} as unknown as HubSkin;
}
export function parseSkin(text:string):HubSkin {if(new TextEncoder().encode(text).length>SKIN_LIMIT)throw Error('皮肤文件不能超过 8 KB');let value:unknown;try{value=JSON.parse(text.replace(/^\uFEFF/,''));}catch{throw Error('皮肤文件不是有效的 JSON');}return validateSkin(value);}
export function getSkin():HubSkin {try{const saved=localStorage.getItem(SKIN_STORAGE);if(saved)return parseSkin(saved);}catch{/* A damaged local preference must never prevent opening settings. */}return {...SKINS[0]};}
function rgb(color:string){return [1,3,5].map(i=>parseInt(color.slice(i,i+2),16));}
function light(color:string){const [r,g,b]=rgb(color);return .2126*r+.7152*g+.0722*b>150;}
function blend(a:string,b:string,f:number){const aa=rgb(a),bb=rgb(b);return '#'+aa.map((n,i)=>Math.round(n*(1-f)+bb[i]*f).toString(16).padStart(2,'0')).join('');}
export function renderSkin(skin:HubSkin){const s=validateSkin(skin),root=document.documentElement,style=root.style,isLight=light(s.background),fg=isLight?'#17232e':'#edf5fc';
 root.dataset.hubSkin='active';root.classList.toggle('dark',!isLight);style.colorScheme=isLight?'light':'dark';
 const vars:Record<string,string>={'--hub-accent':s.accent,'--hub-base':s.background,'--hub-surface':s.surface,'--hub-text':fg,'--hub-muted':blend(s.background,fg,.64),'--hub-edge':blend(s.surface,fg,.2),'--hub-accent-text':light(s.accent)?'#0b1c22':'#ffffff','--hub-window-alpha':s.opacity+'%','--hub-panel-alpha':s.panelOpacity+'%','--hub-popup-alpha':Math.max(92,s.panelOpacity)+'%','--hub-blur':s.blur+'px','--hub-radius':s.radius+'px'};
 for(const [k,v] of Object.entries(vars))style.setProperty(k,v);
}
export function setSkin(skin:HubSkin){const s=validateSkin(skin);localStorage.setItem(SKIN_STORAGE,JSON.stringify(s));renderSkin(s);}
