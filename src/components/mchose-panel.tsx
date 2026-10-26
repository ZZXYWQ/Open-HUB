import {LightingPanel} from "@/components/lighting-panel";
import {MchoseIdentity} from "@/components/mchose-art";
import {useEffect,useRef,useState} from "react";
import {open,save} from "@tauri-apps/plugin-dialog";
import {toast} from "sonner";
import {Button} from "@/components/ui/button";
import {Input} from "@/components/ui/input";
import {PanelShell,PanelSection} from "@/components/panel";
import {api,type MchoseHardware,type MchoseMacro,type MchoseMacroEvent} from "@/lib/api";
import {MCHOSE_MAIN_KEYS,MCHOSE_COMBINATION_MODIFIERS,mchoseKeyUsage,mchoseKeyboardBinding,mchoseKeyboardLabel} from "@/lib/mchose-keys";

const RATES=[[0,125],[2,500],[3,1000],[4,2000],[5,4000],[6,8000]];
const BUTTONS=["左键","右键","中键","前侧键","后侧键","DPI 键"];
const LOD=["0.7 mm","0.9 mm","1.2 mm","1.4 mm","1.7 mm"];
const BINDINGS:[string,number[]][]=[
 ["左键",[0,1,0]],["右键",[0,2,0]],["中键",[0,4,0]],["前进",[0,16,0]],["后退",[0,8,0]],
 ["滚轮上",[5,0,0]],["滚轮下",[5,1,0]],["DPI 循环",[1,0,0,0]],["DPI +",[1,2,0,0]],["DPI −",[1,3,0,0]],
 ["Esc",[17,41,0]],["Enter",[17,40,0]],["空格",[17,44,0]],["Tab",[17,43,0]],
 ["复制",[22,1,6]],["粘贴",[22,1,25]],["剪切",[22,1,27]],["全选",[22,1,4]],
 ["音量 +",[20,233,0]],["音量 −",[20,234,0]],["静音",[20,226,0]],["播放/暂停",[20,205,0]],
 ["上一曲",[20,182,0]],["下一曲",[20,181,0]],["板载 1",[51,0,0]],["板载 2",[51,1,0]],["板载 3",[51,2,0]],["循环板载",[51,255,0]],["禁用",[254,0,0]],
 ...Array.from({length:26},(_,i):[string,number[]]=>[String.fromCharCode(65+i),[17,4+i,0]]),
 ...Array.from({length:12},(_,i):[string,number[]]=>[`F${i+1}`,[17,58+i,0]]),
 ...Array.from({length:18},(_,i):[string,number[]]=>[`宏槽 ${i+1}`,[33,i,0]]),
];
const selectClass="h-9 min-w-0 rounded-md border border-input bg-background px-2 text-sm";
function Pick({value,onChange,children,label}:{value:number|string;onChange:(v:string)=>void;children:React.ReactNode;label:string}){
 return <select aria-label={label} className={selectClass} value={value} onChange={e=>onChange(e.target.value)}>{children}</select>;
}
function Num({value,onChange,min=0,max=255,step=1,label,disabled=false}:{value:number;onChange:(v:number)=>void;min?:number;max?:number;step?:number;label:string;disabled?:boolean}){
 return <Input disabled={disabled} aria-label={label} type="number" min={min} max={max} step={step} value={value} onChange={e=>onChange(Number(e.target.value))}/>;
}
function Check({checked,onChange,children}:{checked:boolean;onChange:(v:boolean)=>void;children:React.ReactNode}){
 return <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={checked} onChange={e=>onChange(e.target.checked)}/>{children}</label>;
}

const DPI_DRAFT_KEYS=['dpi_x','dpi_y','count','active','independent'] as const;
const SETTINGS_DRAFT_KEYS=['sleep','never_sleep','left_debounce','right_debounce','sensor','rotation'] as const;
type DraftValues=Partial<Pick<MchoseHardware,typeof DPI_DRAFT_KEYS[number]|typeof SETTINGS_DRAFT_KEYS[number]>>;
type BoardDraft={values:DraftValues;name?:string};
type MacroValue={macro:MchoseMacro;text:string};
type MacroDraft=MacroValue&{confirmed?:MacroValue};
const confirmedMacro=(macro:MchoseMacro,text:string):MacroDraft=>({macro,text,confirmed:{macro,text}});
const emptyMacro=(slot:number):MacroDraft=>({macro:{slot,name:slot===0?'新宏':`新宏 ${slot+1}`,mode:0,events:[]},text:'[]'});

export function MchosePanel({id}:{id:string}){
 const [hardware,setHardware]=useState<MchoseHardware|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState("");
 const [page,setPage]=useState("dpi"),[boardDrafts,setBoardDrafts]=useState<Record<number,BoardDraft>>({});
 const [macroSlot,setMacroSlot]=useState(0),[macroDrafts,setMacroDrafts]=useState<Record<number,MacroDraft>>({});
 const [recording,setRecording]=useState(false);
 const draft=hardware?boardDrafts[hardware.profile]:undefined;
 const hw=hardware?{...hardware,...draft?.values}:null;
 const name=draft?.name??(hardware?.names[hardware.profile]??"");
 const macroDraft=macroDrafts[macroSlot]??emptyMacro(macroSlot);
 const {macro,text:macroText}=macroDraft;
 const macroConfirmed=macroDraft.confirmed?.macro===macro&&macroDraft.confirmed.text===macroText;
 const editMacro=(edit:(value:MacroDraft)=>MacroDraft)=>setMacroDrafts(all=>({...all,[macroSlot]:edit(all[macroSlot]??emptyMacro(macroSlot))}));
 const setMacro=(value:MchoseMacro)=>editMacro(old=>({...old,macro:value}));
 const setMacroText=(text:string)=>editMacro(old=>({...old,text}));
 const setName=(value:string)=>{if(hardware){const board=hardware.profile;setBoardDrafts(all=>({...all,[board]:{...all[board],values:all[board]?.values??{},name:value}}));}};
 const held=useRef(new Set<number>()),lastKey=useRef(0);
 const [buttonIndex,setButtonIndex]=useState(4),[binding,setBinding]=useState("0"),[capture,setCapture]=useState(false);
 const [customKey,setCustomKey]=useState<number[]|null>(null);
 const [keyboardUsage,setKeyboardUsage]=useState(4),[keyboardModifiers,setKeyboardModifiers]=useState(0),[captureHint,setCaptureHint]=useState("");
 const chooseKeyboard=(usage:number,modifiers:number)=>{setKeyboardUsage(usage);setKeyboardModifiers(modifiers);setCustomKey(mchoseKeyboardBinding(usage,modifiers));setCaptureHint("");};

 const lifetime=useRef({live:true,operation:0});
 const adopt=(v:MchoseHardware)=>setHardware(v);
 const clearSection=(board:number,kind:unknown,submitted:BoardDraft|undefined)=>setBoardDrafts(all=>{
  const previous=all[board];if(!previous)return all;
  const next:BoardDraft={...previous,values:{...previous.values}};
  if(kind==='rename'&&previous.name===submitted?.name)delete next.name;
  for(const key of kind==='dpi'?DPI_DRAFT_KEYS:kind==='settings'?SETTINGS_DRAFT_KEYS:[]){if(previous.values[key]===submitted?.values[key])delete next.values[key];}
  return {...all,[board]:next};
 });
 // DevicePage keys this panel by physical target. Async continuations also
 // retain this mount and operation, including native file-dialog results.
 const run=async<T,>(f:(current:()=>boolean)=>Promise<T>,message?:string)=>{
  const owner=lifetime.current,operation=++owner.operation;
  const current=()=>owner.live&&lifetime.current===owner&&owner.operation===operation;
  if(!current())return undefined;setBusy(true);setError("");
  try{const r=await f(current);if(current()&&message)toast.success(message);return current()?r:undefined;}
  catch(e){if(current()){setError(String(e));toast.error(String(e));}return undefined;}
  finally{if(current())setBusy(false);}
 };
 const reload=()=>run(async current=>{const value=await api.mchoseHardware(id);if(current())adopt(value);});
 useEffect(()=>{const owner={live:true,operation:0};lifetime.current=owner;void reload();return()=>{owner.live=false;held.current.clear();};},[id]);
 const apply=(action:Record<string,unknown>)=>hw&&run(async current=>{
  const board=hw.profile,submittedDraft=boardDrafts[board];
  const slot=Number(action.slot),submittedMacro=macroDrafts[slot];
  const savedMacro:MchoseMacro|undefined=action.kind==='macro'?{slot,name:String(action.name),mode:Number(action.mode),events:action.events as MchoseMacroEvent[]}:undefined;
  const value=await api.mchoseApply(id,board,action);
  if(current()){
   adopt(value);
   if(value.profile===board)clearSection(board,action.kind,submittedDraft);
   if(savedMacro){
    const receipt=value.saved_macro??savedMacro;
    const text=submittedMacro?.text??JSON.stringify(receipt.events,null,2),saved=confirmedMacro(receipt,text);
    setMacroDrafts(all=>({...all,[slot]:all[slot]===submittedMacro?saved:{...(all[slot]??emptyMacro(slot)),confirmed:saved.confirmed}}));
   }else if(action.kind==='delete_macro'){
    const empty=emptyMacro(slot),saved=confirmedMacro(empty.macro,empty.text);
    setMacroDrafts(all=>({...all,[slot]:all[slot]===submittedMacro?saved:{...(all[slot]??emptyMacro(slot)),confirmed:saved.confirmed}}));
   }
  }
 },"已写入并核对鼠标设置");
 const update=(v:DraftValues)=>{if(hw){const board=hw.profile;setBoardDrafts(all=>({...all,[board]:{...all[board],values:{...all[board]?.values,...v}}}));}};
 const discardBoard=()=>hw&&run(async current=>{const board=hw.profile,submitted=boardDrafts[board],value=await api.mchoseHardware(id);if(current()){setBoardDrafts(all=>{if(all[board]!==submitted)return all;const next={...all};delete next[board];return next;});adopt(value);}});
 const readMacro=(slot:number)=>run(async current=>{
  const submitted=macroDrafts[slot],v=await api.mchoseMacro(id,slot);if(!current())return;
  const next=v??emptyMacro(slot).macro,saved=confirmedMacro(next,JSON.stringify(next.events,null,2));
  setMacroDrafts(all=>({...all,[slot]:all[slot]===submitted?saved:{...(all[slot]??emptyMacro(slot)),confirmed:saved.confirmed}}));
 });
 const selectMacro=(slot:number)=>{setMacroSlot(slot);if(!macroDrafts[slot])void readMacro(slot);};
 const stopRecord=()=>{setRecording(false);const events=[...macro.events,...[...held.current].map(code=>({kind:"key"as const,code,down:false,delay_ms:0}))];held.current.clear();editMacro(old=>({...old,macro:{...old.macro,events},text:JSON.stringify(events,null,2)}));};
 const addRecorded=(down:boolean,code:string)=>{
  const scan=mchoseKeyUsage(code);if(scan===null)return;
  if(down&&held.current.has(scan)||!down&&!held.current.has(scan))return;
  const elapsed=Math.min(60000,Math.round(performance.now()-lastKey.current));lastKey.current=performance.now();
  if(down)held.current.add(scan);else held.current.delete(scan);
  editMacro(old=>{const events=[...old.macro.events,...(elapsed>5?[{kind:"delay"as const,code:0,down:false,delay_ms:elapsed}]:[]),{kind:"key"as const,code:scan,down,delay_ms:0}];return {...old,macro:{...old.macro,events},text:JSON.stringify(events,null,2)};});
 };
 const exportConfig=()=>run(async current=>{const path=await save({defaultPath:`A7-V3-板载${(hw?.profile??0)+1}.json`,filters:[{name:"配置备份",extensions:["json"]}]});if(path&&current())await api.mchoseExport(id,path);},"备份已导出");
 const restoreConfig=()=>run(async current=>{const path=await open({multiple:false,filters:[{name:"配置备份",extensions:["json"]}]});if(typeof path==="string"&&current()){const value=await api.mchoseRestore(id,path);if(current())adopt(value);}},"配置已恢复");
 if(!hw)return <PanelShell title="迈从控制中心"><PanelSection><Button disabled={busy} onClick={()=>void reload()}>刷新设备</Button><p className="mt-3 text-sm">{error||"正在读取鼠标现有配置…"}</p></PanelSection></PanelShell>;
 const settingsApply=()=>apply({kind:"settings",sleep:hw.sleep,never_sleep:hw.never_sleep,left_debounce:hw.left_debounce,right_debounce:hw.right_debounce,sensor:hw.sensor,rotation:hw.rotation});
 const bindingOptions=BINDINGS.map(([label,b],i)=><option value={i} key={i}>{label}{JSON.stringify(hw.buttons[buttonIndex])===JSON.stringify(b)?"（当前）":""}</option>);
 return <PanelShell title="A7 V3 Ultra+" aside={busy?"正在读写…":`板载 ${hw.profile+1}`} toolbar={<div className="flex flex-wrap gap-1">{[["dpi","DPI"],["sensor","性能"],["buttons","按键"],["macro","宏"],["boards","板载/备份"],["lighting","灯光同步"]].map(([k,n])=><Button key={k} size="sm" variant={page===k?"secondary":"ghost"} onClick={()=>setPage(k)}>{n}</Button>)}</div>}>
  {page!=="lighting"&&<MchoseIdentity dpi={hw.dpi_x[hw.active]} rate={RATES.find(([v])=>v===(hw.raw_settings[2]>>4))?.[1]??0} profile={hw.profile}/>}
  {page==="lighting"&&<LightingPanel embedded/>}
  <fieldset disabled={busy||recording} className="min-w-0 border-0 p-0">
   {error&&<p role="alert" className="mx-7 mt-3 text-sm text-destructive">{error}</p>}
   {page!=="lighting"&&<PanelSection><div className="flex items-center gap-2"><Pick label="板载配置" value={hw.profile} onChange={v=>void apply({kind:"profile",index:Number(v)})}>{hw.names.map((n,i)=><option key={i} value={i}>{n||`板载配置 ${i+1}`}</option>)}</Pick><Button size="sm" variant="outline" onClick={()=>void reload()}>重新读取</Button>{draft&&(Object.keys(draft.values).length>0||draft.name!==undefined)&&<Button size="sm" variant="outline" onClick={()=>void discardBoard()}>放弃本组草稿并重新读取</Button>}</div><p className="mt-2 text-xs text-muted-foreground">重新读取和保存其他设置会保留本组草稿；只有保存对应设置或明确放弃时才清除。{draft&&(Object.keys(draft.values).length>0||draft.name!==undefined)&&' 当前板载组有未保存草稿。'}</p></PanelSection>}
   {page==="dpi"&&<>
    <PanelSection title="六档板载 DPI" help="直接编辑鼠标内的六档设置，关闭软件后仍然有效。">
     <div className="mb-3 flex gap-3"><label className="text-sm">启用档数 <Pick label="启用 DPI 档数" value={hw.count} onChange={v=>update({count:Number(v),active:Math.min(hw.active,Number(v)-1)})}>{[1,2,3,4,5,6].map(n=><option key={n}>{n}</option>)}</Pick></label><Check checked={hw.independent} onChange={v=>update({independent:v})}>独立 X/Y</Check></div>
     <div className="grid grid-cols-[36px_1fr_1fr_48px] items-center gap-2 text-xs"><span>档位</span><span>X DPI</span><span>Y DPI</span><span>当前</span>{hw.dpi_x.flatMap((x,i)=>[<span key={`l${i}`}>{i+1}</span>,<Num key={`x${i}`} label={`档 ${i+1} X DPI`} min={50} max={50000} step={50} value={x} onChange={v=>update({dpi_x:hw.dpi_x.map((o,k)=>i===k?v:o)})}/>,<Num disabled={!hw.independent} key={`y${i}`} label={`档 ${i+1} Y DPI`} min={50} max={50000} step={50} value={hw.independent?hw.dpi_y[i]:x} onChange={v=>update({dpi_y:hw.dpi_y.map((o,k)=>i===k?v:o)})}/>,<input key={`a${i}`} aria-label={`选择档 ${i+1}`} type="radio" name="activeDpi" checked={hw.active===i} disabled={i>=hw.count} onChange={()=>update({active:i})}/>])}</div>
     <Button className="mt-3" onClick={()=>void apply({kind:"dpi",x:hw.dpi_x,y:hw.dpi_y,count:hw.count,active:hw.active,independent:hw.independent})}>保存六档 DPI</Button>
    </PanelSection>
    <PanelSection title="回报率"><p className="mb-2 text-xs text-muted-foreground">线缆和无线回报率同步设置。</p><div className="flex flex-wrap gap-2">{RATES.map(([v,hz])=><Button size="sm" variant={(hw.raw_settings[2]>>4)===v?"secondary":"outline"} key={v} onClick={()=>void run(async current=>{await api.setReportRate(id,v);if(current()){const value=await api.mchoseHardware(id);if(current())adopt(value);}},`已设为 ${hz} Hz`)}>{hz} Hz</Button>)}</div></PanelSection>
   </>}
   {page==="sensor"&&<>
    <PanelSection title="抬升高度（LOD）"><Pick label="LOD" value={hw.lod} onChange={v=>void apply({kind:"lod",index:Number(v)})}>{LOD.map((n,i)=><option key={i} value={i}>{n}</option>)}</Pick></PanelSection>
    <PanelSection title="传感器设置"><div className="space-y-3">
     <label className="flex items-center justify-between text-sm">性能模式<Pick label="性能模式" value={hw.sensor&3} onChange={v=>update({sensor:(hw.sensor&~3)|Number(v)})}>{["模式 1","模式 2","模式 3"].map((n,i)=><option key={i} value={i}>{n}</option>)}</Pick></label>
     {[[4,"波纹控制"],[8,"直线修正"],[16,"Motion Sync"],[128,"玻璃模式"]].map(([bit,label])=><Check key={bit} checked={!!(hw.sensor&Number(bit))} onChange={v=>update({sensor:v?hw.sensor|Number(bit):hw.sensor&~Number(bit)})}>{label}</Check>)}
     <label className="grid grid-cols-[1fr_120px] items-center gap-3 text-sm">传感器旋转角度（度）<Num label="旋转角度" value={hw.rotation} min={-120} max={120} step={4} onChange={v=>update({rotation:v})}/></label>
    </div></PanelSection>
    <PanelSection title="消抖和休眠"><div className="grid grid-cols-[1fr_110px] items-center gap-3 text-sm"><span>左键消抖（ms）</span><Num label="左键消抖" value={hw.left_debounce} max={50} onChange={v=>update({left_debounce:v})}/><span>右键消抖（ms）</span><Num label="右键消抖" value={hw.right_debounce} max={50} onChange={v=>update({right_debounce:v})}/><span>休眠时间（分钟）</span><Num label="休眠分钟" value={hw.sleep} max={60} onChange={v=>update({sleep:v})}/></div><div className="mt-3"><Check checked={hw.never_sleep} onChange={v=>update({never_sleep:v})}>永不休眠</Check></div><Button className="mt-3" onClick={()=>void settingsApply()}>保存性能设置</Button></PanelSection>

   </>}
   {page==="buttons"&&<>
    <PanelSection title="现有按键映射"><div className="space-y-2">{hw.buttons.map((b,i)=><div className="flex justify-between text-sm" key={i}><span>{BUTTONS[i]}</span><span>{BINDINGS.find(([,v])=>JSON.stringify(v)===JSON.stringify(b))?.[0]??mchoseKeyboardLabel(b)??(b.every(v=>v===255)?"未配置":b.map(v=>v.toString(16).padStart(2,"0")).join(" "))}</span></div>)}</div></PanelSection>
    <PanelSection title="更改按键"><div className="flex flex-col gap-3"><Pick label="选择物理按键" value={buttonIndex} onChange={v=>setButtonIndex(Number(v))}>{BUTTONS.map((n,i)=>hw.buttons[i]?.[0]!==255&&<option key={i} value={i}>{n}</option>)}</Pick><Pick label="选择按键动作" value={binding} onChange={v=>{setBinding(v);setCustomKey(null);setCapture(false);}}>{bindingOptions}</Pick><Button variant="outline" onClick={()=>{setCapture(true);chooseKeyboard(keyboardUsage,keyboardModifiers);}}>录入键盘键 / 组合键</Button>{capture&&<div role="group" aria-label="自定义键盘组合" className="space-y-3 rounded-md border p-3"><p className="text-xs text-muted-foreground">组合键使用通用修饰键，不区分左右；宏录制仍分别记录左右修饰键。</p><Pick label="自定义主键" value={keyboardUsage} onChange={v=>chooseKeyboard(Number(v),keyboardModifiers)}><option value={0}>仅修饰键</option>{MCHOSE_MAIN_KEYS.map(key=><option key={key.usage} value={key.usage}>{key.label}</option>)}</Pick><div className="flex flex-wrap gap-3">{MCHOSE_COMBINATION_MODIFIERS.map(item=><label key={item.bit} className="flex items-center gap-2 text-sm"><input type="checkbox" aria-label={item.label} checked={Boolean(keyboardModifiers&item.bit)} onChange={e=>chooseKeyboard(keyboardUsage,e.target.checked?keyboardModifiers|item.bit:keyboardModifiers&~item.bit)}/>{item.label}</label>)}</div><Input autoFocus readOnly aria-label="录入组合键" placeholder="在这里按下目标键或组合键" onKeyDown={e=>{e.preventDefault();const usage=mchoseKeyUsage(e.code);if(usage===null){chooseKeyboard(0,0);setCaptureHint("原厂键表未列出此键，请从上方列表选择。");return;}const modifiers=(e.ctrlKey?1:0)|(e.shiftKey?2:0)|(e.altKey?4:0)|(e.metaKey?8:0);chooseKeyboard(usage>=224?0:usage,modifiers);}}/><p role="status" aria-label="自定义键盘预览" className="text-xs">{customKey?`待应用：${mchoseKeyboardLabel(customKey)}`:captureHint||"请选择主键或至少一项通用修饰键。"}</p></div>}<Button disabled={capture&&!customKey} onClick={()=>void apply({kind:"button",index:buttonIndex,binding:customKey??BINDINGS[Number(binding)][1]})}>应用到{BUTTONS[buttonIndex]}</Button></div></PanelSection>
   </>}
   {page==="macro"&&<>
    <PanelSection title="板载宏"><div className="flex flex-wrap gap-2"><Pick label="宏槽" value={macro.slot} onChange={v=>selectMacro(Number(v))}>{Array.from({length:18},(_,i)=><option key={i} value={i}>宏槽 {i+1}</option>)}</Pick><Button variant="outline" onClick={()=>void readMacro(macro.slot)}>读取鼠标</Button></div><p className="mt-2 text-xs text-muted-foreground">切换宏槽会保留各槽草稿；“读取鼠标”会放弃当前槽草稿并重新读回。</p><p role="status" aria-label="宏槽保存状态" className="mt-2 text-xs text-muted-foreground">{macroConfirmed?`已核对：${macro.name} · ${macro.events.length} 条事件`:'当前宏槽有未保存草稿'}</p><div className="mt-3 space-y-3"><Input aria-label="宏名称" value={macro.name} onChange={e=>setMacro({...macro,name:e.target.value})}/><Pick label="宏循环方式" value={macro.mode} onChange={v=>setMacro({...macro,mode:Number(v)})}><option value={0}>按住重复</option><option value={1}>再次按下停止</option><option value={2}>循环执行（原厂模式 2）</option><option value={3}>其他按键停止</option></Pick><p className="text-xs text-muted-foreground">可录制键盘序列，或编辑事件 JSON。原厂宏的原始内容会保留；暂不能无损编辑的内容可原样导入导出。kind 支持 key、mouse、scroll、delay。</p><textarea aria-label="宏事件编辑器" className="h-48 w-full rounded-md border border-input bg-background p-2 font-mono text-xs" value={macroText} onChange={e=>setMacroText(e.target.value)}/><div className="flex flex-wrap gap-2"><Button variant="outline" onClick={()=>{held.current.clear();lastKey.current=performance.now();setMacro({...macro,events:[]});setMacroText("[]");setRecording(true);}}>录制键盘宏</Button><Button onClick={()=>{let events:MchoseMacroEvent[];try{events=JSON.parse(macroText);}catch{toast.error("事件 JSON 格式错误");return;}void apply({kind:"macro",slot:macro.slot,name:macro.name,mode:macro.mode,events,raw:macro.raw??null});}}>保存到宏槽</Button><Button variant="outline" onClick={()=>void run(async current=>{const events=JSON.parse(macroText);const path=await save({defaultPath:macro.name+".json",filters:[{name:"宏文件",extensions:["json"]}]});if(path&&current())await api.mchoseMacroFile(path,{...macro,events});},"宏已导出")}>导出宏</Button><Button variant="outline" onClick={()=>void apply({kind:"delete_macro",slot:macro.slot})}>删除空闲宏槽</Button></div><label className="text-sm">导入宏 JSON <input type="file" accept=".json" onChange={e=>{const file=e.target.files?.[0];if(file)void run(async current=>{const m=JSON.parse(await file.text());if(!Array.isArray(m.events))throw Error("请选择本适配版导出的宏 JSON");if(current()){setMacro({...m,slot:macro.slot});setMacroText(JSON.stringify(m.events,null,2));}});}}/></label><p className="text-xs text-muted-foreground">保存后，到“按键”页将目标按键设为对应宏槽。录制只在下面的录制区域获得焦点时工作。</p></div></PanelSection>
   </>}
   {page==="boards"&&<>
    <PanelSection title="板载配置名称"><div className="flex gap-2"><Input aria-label="板载名称" value={name} onChange={e=>setName(e.target.value)}/><Button onClick={()=>void apply({kind:"rename",name})}>重命名</Button></div></PanelSection>
    <PanelSection title="本地备份和恢复" help="导出当前板载配置及鼠标内的全部宏。其他板载配置请分别切换后导出。恢复前会再次自动备份。"><div className="flex gap-2"><Button onClick={()=>void exportConfig()}>导出配置</Button><Button variant="outline" onClick={()=>void restoreConfig()}>从文件恢复</Button></div></PanelSection>
    <PanelSection title="设备维护"><dl className="grid grid-cols-2 gap-2 text-sm"><dt>鼠标固件</dt><dd>V{hw.firmware}</dd><dt>接收器固件</dt><dd>V{hw.receiver_firmware}</dd></dl><p className="my-3 text-xs text-muted-foreground">固件刷写和磁吸底座维护通过已安装的官方 M HUB 完成。此按钮会退出适配版并打开 M HUB。</p><Button variant="outline" onClick={()=>void run(()=>api.openMhub())}>切换到 M HUB 维护设备</Button></PanelSection>
   </>}
  </fieldset>
  {recording&&<PanelSection title="正在录制"><Input autoFocus aria-label="宏录制区域" readOnly placeholder="在这里按键，完成后点击停止" onKeyDown={e=>{e.preventDefault();if(!e.repeat)addRecorded(true,e.code);}} onKeyUp={e=>{e.preventDefault();addRecorded(false,e.code);}}/><Button className="mt-3" onClick={stopRecord}>停止录制</Button></PanelSection>}
 </PanelShell>;
}

