// Exact cached M HUB qM table (renderer SHA256 c3e91c28...7ad61bc).
// These are interoperability facts. No OEM code is imported or executed.
export interface MchoseKey {usage:number;label:string;code:string}
export const MCHOSE_KEYS:readonly MchoseKey[]=[
 ...Array.from({length:26},(_,i)=>({usage:4+i,label:String.fromCharCode(65+i),code:`Key${String.fromCharCode(65+i)}`})),
 ...Array.from({length:9},(_,i)=>({usage:30+i,label:String(i+1),code:`Digit${i+1}`})),
 {usage:39,label:'0',code:'Digit0'},
 ...([
  [40,'Enter','Enter'],[41,'Esc','Escape'],[42,'Backspace','Backspace'],[43,'Tab','Tab'],[44,'空格','Space'],
  [45,'-','Minus'],[46,'=','Equal'],[47,'[','BracketLeft'],[48,']','BracketRight'],[49,'\\','Backslash'],
  [51,';','Semicolon'],[52,"'",'Quote'],[53,'`','Backquote'],[54,',','Comma'],[55,'.','Period'],[56,'/','Slash'],[57,'Caps Lock','CapsLock'],
 ] as const).map(([usage,label,code])=>({usage,label,code})),
 ...Array.from({length:12},(_,i)=>({usage:58+i,label:`F${i+1}`,code:`F${i+1}`})),
 ...([
  [70,'Print Screen','PrintScreen'],[71,'Scroll Lock','ScrollLock'],[72,'Pause','Pause'],[73,'Insert','Insert'],[74,'Home','Home'],
  [75,'Page Up','PageUp'],[76,'Delete','Delete'],[77,'End','End'],[78,'Page Down','PageDown'],
  [79,'右方向键','ArrowRight'],[80,'左方向键','ArrowLeft'],[81,'下方向键','ArrowDown'],[82,'上方向键','ArrowUp'],
  [83,'Num Lock','NumLock'],[84,'数字键盘 /','NumpadDivide'],[85,'数字键盘 *','NumpadMultiply'],[86,'数字键盘 -','NumpadSubtract'],[87,'数字键盘 +','NumpadAdd'],[88,'数字键盘 Enter','NumpadEnter'],
 ] as const).map(([usage,label,code])=>({usage,label,code})),
 ...Array.from({length:9},(_,i)=>({usage:89+i,label:`数字键盘 ${i+1}`,code:`Numpad${i+1}`})),
 {usage:98,label:'数字键盘 0',code:'Numpad0'},{usage:99,label:'数字键盘 .',code:'NumpadDecimal'},{usage:101,label:'菜单键',code:'ContextMenu'},
 ...([
  [224,'左 Ctrl','ControlLeft'],[225,'左 Shift','ShiftLeft'],[226,'左 Alt','AltLeft'],[227,'左 Win','MetaLeft'],
  [228,'右 Ctrl','ControlRight'],[229,'右 Shift','ShiftRight'],[230,'右 Alt','AltRight'],[231,'右 Win','MetaRight'],
 ] as const).map(([usage,label,code])=>({usage,label,code})),
];
export const MCHOSE_MAIN_KEYS=MCHOSE_KEYS.filter(key=>key.usage<224);
export const MCHOSE_COMBINATION_MODIFIERS=[{bit:1,label:'Ctrl（通用）'},{bit:2,label:'Shift（通用）'},{bit:4,label:'Alt（通用）'},{bit:8,label:'Win（通用）'}] as const;
const byCode=new Map(MCHOSE_KEYS.map(key=>[key.code,key.usage]));
const mainByUsage=new Map(MCHOSE_MAIN_KEYS.map(key=>[key.usage,key]));
export function mchoseKeyUsage(code:string):number|null{return byCode.get(code)??null;}
// The combination picker has four generic modifiers. Macro E0..E7 usages do
// not establish eight-bit left/right combination masks or plain assignments.
export function mchoseKeyboardBinding(usage:number,modifiers:number):number[]|null{
 if(!Number.isInteger(modifiers)||modifiers<0||modifiers>15)return null;
 if(usage===0)return modifiers?[0x12,modifiers,0]:null;
 if(!mainByUsage.has(usage))return null;
 return modifiers?[0x13,modifiers,usage]:[0x11,usage,0];
}
export function mchoseKeyboardLabel(binding:readonly number[]):string|null{
 if(binding.length!==3)return null;
 const [kind,a,b]=binding;
 if(kind===0x11&&b===0)return mainByUsage.get(a)?.label??null;
 if((kind!==0x12&&kind!==0x13)||a<1||a>15||!Number.isInteger(a))return null;
 if(kind===0x12&&b!==0||kind===0x13&&!mainByUsage.has(b))return null;
 const modifiers=MCHOSE_COMBINATION_MODIFIERS.filter(item=>a&item.bit).map(item=>item.label);
 return [...modifiers,...(kind===0x13?[mainByUsage.get(b)!.label]:[])].join(' + ');
}
