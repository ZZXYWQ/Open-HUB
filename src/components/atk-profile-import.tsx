import {KEY_NAMES} from '@/lib/sino';
import {shortcutEventLabel} from '@/components/compx-shortcut';
import {compxModel} from '@/lib/compx';
import {isLegacyBrand} from '@/lib/legacy';
import {boardModel,boardModels,boardBrand,type BoardImportBrand} from '@/lib/native-models';
import {useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {open} from '@tauri-apps/plugin-dialog';
import type {DeviceSummary,Profile} from '@/lib/api';
import {type MultiPlan,type MultiView,type MultiAction,type MultiMacro,type MultiStep,multiProfile} from '@/lib/multi';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from '@/components/ui/dialog';
import {Button} from '@/components/ui/button';

export function AtkProfileImport({devices,onClose,onSave,brand='atk'}:{devices:DeviceSummary[];onClose:()=>void;onSave:(p:Profile)=>Promise<boolean>;brand?:BoardImportBrand}){
 const label=boardBrand(brand), supported=boardModels(brand);
 const peers=devices.filter(d=>boardModel(d.vendor_id,parseInt(d.product_id,16))?.brand===brand&&d.id.startsWith('multi:'+brand+':'));
 const [selected,setSelected]=useState(''),[preview,setPreview]=useState<{plan:MultiPlan;name:string;unknown:number;note?:string}|null>(null),[error,setError]=useState(''),[busy,setBusy]=useState(false);
 const [macroReview,setMacroReview]=useState<{path:string;expected:DeviceSummary;digest:string;refs:Array<{button:number;slot:number;name:string;stored_code:number;steps:number;events:MultiStep[]}>;modes:Record<string,MultiMacro['mode']>}|null>(null);
 const generation=useRef(0);
 const [includeButtons,setIncludeButtons]=useState(false);
 const [includeTuning,setIncludeTuning]=useState(false),[includeLighting,setIncludeLighting]=useState(false);
 const target=peers.find(d=>d.id===selected);const palette=target?(compxModel(target.vendor_id,parseInt(target.product_id,16))?.dpi_color_stages??0):0;
 const fileImport=['atk','vxe','vgn'].includes(brand);
 const convert=async(path:string,expected:DeviceSummary,current:number,digest?:string,modes:Record<string,MultiMacro['mode']>={})=>{
  const result=await invoke<{profile:Profile;note:string}>('prepare_compx_file_import',{path,vendor:expected.vendor_id,product:parseInt(expected.product_id,16),includeButtons,includeTuning,includeLighting,macroModes:modes,expectedDigest:digest??null});
  if(current!==generation.current)return;
  const p=result.profile,plan=p.multi;
  if(!plan||plan.vendor!==expected.vendor_id||plan.product!==parseInt(expected.product_id,16)||p.target_vendor!==plan.vendor||p.target_model!==plan.product)throw Error('转换结果与所选鼠标型号不一致');
  setPreview({plan,name:p.name,unknown:0,note:result.note});setMacroReview(null);
 };
 const confirmMacros=async()=>{if(!macroReview)return;const r=macroReview,current=++generation.current;setBusy(true);setError('');try{await convert(r.path,r.expected,current,r.digest,r.modes);}catch(e){if(current===generation.current)setError(String(e));}finally{if(current===generation.current)setBusy(false);}};
 const readFile=async()=>{const current=++generation.current;setBusy(true);setError('');setPreview(null);setMacroReview(null);try{
  const expected=peers.find(d=>d.id===selected);if(!expected)throw Error('请先选择目标鼠标型号');
  const path=await open({multiple:false,filters:[{name:'ATK HUB 配置',extensions:['bin']}]});
  if(!path||Array.isArray(path)||current!==generation.current)return;
  if(includeButtons){
   const inspection=await invoke<{sha256:string;refs:Array<{button:number;slot:number;name:string;stored_code:number;steps:number;events:MultiStep[]}>}>('inspect_compx_macro_refs',{path,vendor:expected.vendor_id,product:parseInt(expected.product_id,16)});
   if(current!==generation.current)return;
   if(!inspection||!Array.isArray(inspection.refs)||!/^[0-9a-f]{64}$/.test(inspection.sha256))throw Error('原厂宏检查结果无效，请重新选择文件');
   if(inspection.refs.length){setMacroReview({path,expected,digest:inspection.sha256,refs:inspection.refs,modes:{}});return;}
   await convert(path,expected,current,inspection.sha256);
  }else await convert(path,expected,current);
 }catch(e){if(current===generation.current)setError(String(e));}finally{if(current===generation.current)setBusy(false);}};
 const read=async()=>{const current=++generation.current;setBusy(true);setError('');setPreview(null);setMacroReview(null);try{
  const view=await invoke<MultiView>('multi_read',{id:selected});
  if(current!==generation.current)return;
  const expected=peers.find(d=>d.id===selected);
  if(!expected||'multi:'+view.descriptor.id!==expected.id||view.descriptor.product_id!==parseInt(expected.product_id,16))throw Error(`读取结果与所选 ${label} 型号或设备不一致`);
  if(boardModel(view.descriptor.vendor_id,view.descriptor.product_id)?.brand!==brand||view.descriptor.vendor_id!==expected.vendor_id||view.descriptor.simulated||!view.state.dpi)throw Error(`读取结果不是受支持的 ${label} 实机板载配置`);
  // Read the hardware fields, never substitute a saved host-macro session plan.
  const plan:MultiPlan={vendor:view.descriptor.vendor_id,product:view.descriptor.product_id,macros:view.native_macros??[],settings:[{Dpi:view.state.dpi},...(view.capabilities.tuning&&view.state.tuning&&Object.keys(view.state.tuning).length?[{Tuning:view.state.tuning}]:[]),...(view.state.report_rate&&view.capabilities.report_rates.includes(view.state.report_rate)?[{ReportRate:view.state.report_rate}]:[]),...(view.capabilities.dpi_indicator&&view.state.dpi_indicator?[{DpiIndicator:view.state.dpi_indicator}]:[]),...Object.entries(view.state.dpi_colors??{}).filter(([stage])=>Number(stage)<(view.capabilities.dpi_color_stages??0)).map(([stage,color])=>({DpiColor:{stage:Number(stage),color}})),...view.capabilities.buttons.filter(b=>b.remappable&&view.state.buttons[String(b.id)]!=null).map(b=>({Button:{id:b.id,action:view.state.buttons[String(b.id)]}})),...Object.entries(view.state.lighting).filter(([zone,state])=>!(zone==='0'&&view.capabilities.dpi_indicator&&view.state.dpi_indicator)&&(isLegacyBrand(brand)||state.effect==='Off'||(typeof state.effect==='object'&&'Static'in state.effect))).map(([zone,state])=>({Lighting:{zone:Number(zone),state}}))]};
  setPreview({plan,name:view.descriptor.name+' · 板载导入',unknown:view.capabilities.buttons.filter(b=>b.id!==1&&!b.remappable).length});
 }catch(e){if(current===generation.current)setError(String(e));}finally{if(current===generation.current)setBusy(false);}};
 return <Dialog open onOpenChange={v=>{if(!v&&!busy){generation.current++;onClose();}}}><DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl"><DialogHeader><DialogTitle>{label} 板载配置导入</DialogTitle><DialogDescription>{supported.map(m=>m.name).join('；')}。先读取并预览，再保存到配置中心；此过程不写鼠标。{fileImport?'也可选择 ATK HUB 3.2.27 导出的 .bin，按下方范围导入 DPI、已解析按键、灯效和性能。':'不解析官方软件文件。'}{isLegacyBrand(brand)?'原厂宏和未知动作保留；仅本程序会话中创建且回读通过的宏可随配置保存。':brand==='hyperx'?'原厂宏和灯效无法读回；本程序本次会话创建的宏可随配置保存。':brand==='pulsar'?'当前导入 DPI、回报率、已校验的基础按键、板载快捷键和指示灯；软件宏方案需从配置中心保存，原厂板载宏仍保留。':'仅导入能完整校验的板载宏。'}</DialogDescription></DialogHeader>
 <label>鼠标<select aria-label={`${label} 导入鼠标`} className="ml-3 max-w-full rounded border bg-background p-2" disabled={busy} value={selected} onChange={e=>{generation.current++;setSelected(e.target.value);setIncludeLighting(false);setPreview(null);setMacroReview(null);setError('');}}><option value="">请选择已连接的 {label} 鼠标</option>{peers.map(d=><option key={d.id} value={d.id}>{d.name}</option>)}</select></label>
 {!peers.length&&<p role="status" className="text-sm text-muted-foreground">未检测到这些已适配型号的 USB 配置接口。请连接鼠标，并使用主界面顶部的刷新按钮。</p>}
 <Button variant="outline" disabled={!selected||busy} onClick={()=>void read()}>{busy?'正在读取…':'读取板载配置'}</Button>
 {fileImport&&<label className="flex items-center gap-2 text-sm"><input type="checkbox" aria-label="同时导入已解析按键" checked={includeButtons} disabled={busy} onChange={e=>{generation.current++;setIncludeButtons(e.target.checked);setPreview(null);setMacroReview(null);setError('');}}/>同时导入已解析按键（支持已解析按键 / 快捷序列；文件宏需核对事件与播放方式）</label>}
 {fileImport&&<div className="space-y-2 text-sm"><label className="flex items-center gap-2"><input type="checkbox" aria-label="导入性能与省电" checked={includeTuning} disabled={!selected||busy} onChange={e=>{generation.current++;setIncludeTuning(e.target.checked);setPreview(null);setMacroReview(null);setError('');}}/>导入性能与省电（按所选型号已支持字段校验）</label><label className="flex items-center gap-2"><input type="checkbox" aria-label="导入灯效与 DPI 色表" checked={includeLighting} disabled={!selected||busy||!palette} onChange={e=>{generation.current++;setIncludeLighting(e.target.checked);setPreview(null);setMacroReview(null);setError('');}}/>导入灯效与 DPI 色表{selected&&!palette?'（此型号当前未开放）':''}</label></div>}
 {fileImport&&<Button variant="outline" disabled={!selected||busy} onClick={()=>void readFile()}>选择 ATK HUB .bin 文件{includeButtons?'（含已解析按键）':includeTuning||includeLighting?'（含所选灯效 / 性能）':'（仅 DPI / 回报率）'}</Button>}
 {macroReview&&<section aria-label="原厂宏播放确认" className="space-y-3 rounded-xl border p-4"><b>逐键确认宏播放方式</b><p className="text-sm text-muted-foreground">原厂界面编辑和鼠标回读可能使用不同编码，不能只靠文件数值判断播放方式。请按需要逐键确认；确认前不能导入，文件改变后需要重新核对。</p>{macroReview.refs.map(r=>{const mode=macroReview.modes[String(r.button)],kind=typeof mode==='object'?'Repeat':mode??'';return <div key={r.button} className="space-y-2 rounded border p-3"><p>按键 {r.button} · {r.name} · 原厂槽位 {r.slot+1} · 文件编码 {r.stored_code} · 转换后 {r.steps} 步</p><details><summary>查看宏事件</summary><ol className="list-inside list-decimal text-sm">{r.events.map((e,i)=><li key={i}>{macroStepLabel(e)}</li>)}</ol></details><select aria-label={`按键${r.button}宏播放方式`} className="rounded border bg-background p-2" disabled={busy} value={kind} onChange={e=>{const choice=e.target.value as 'Once'|'WhileHeld'|'Toggle'|'Repeat';const modes={...macroReview.modes};if(!choice)delete modes[String(r.button)];else modes[String(r.button)]=choice==='Repeat'?{Repeat:2}:choice;setMacroReview({...macroReview,modes});setError('');}}><option value="">请核对并选择</option><option value="Once">单次</option><option value="WhileHeld">按住循环</option><option value="Toggle">再次按下停止</option><option value="Repeat">固定次数</option></select>{typeof mode==='object'&&<label className="ml-3">次数<input aria-label={`按键${r.button}宏重复次数`} className="ml-2 w-24 rounded border bg-background p-2" type="number" min={2} max={252} disabled={busy} value={mode.Repeat} onChange={e=>setMacroReview({...macroReview,modes:{...macroReview.modes,[r.button]:{Repeat:Math.max(2,Math.min(252,Math.trunc(Number(e.target.value)||2)))}}})}/></label>}</div>})}<Button disabled={busy||macroReview.refs.some(r=>!macroReview.modes[String(r.button)])} onClick={()=>void confirmMacros()}>确认宏播放方式并预览</Button></section>}
 {preview&&<div aria-label={`${label} 导入预览`} className="rounded-xl border p-4 space-y-2"><b>{preview.name}</b><p>{preview.plan.settings.filter(s=>'Dpi'in s).map(s=>'Dpi'in s?s.Dpi.stages.map(d=>`${d.x} / ${d.y}`).join('、'):'')} DPI</p><div className="grid gap-2 text-sm">{preview.plan.settings.filter(s=>'ReportRate'in s).map((s,i)=><p key={'rate'+i}>{'ReportRate'in s?`回报率：${s.ReportRate} Hz`:''}</p>)}{preview.plan.settings.filter(s=>'Tuning'in s).map((s,i)=><div key={'tuning'+i} className="space-y-1" aria-label="板载性能设置预览"><b>性能与省电</b>{'Tuning'in s&&Object.entries(s.Tuning).filter(([,value])=>value!=null).map(([key,value])=><p key={key}>{({debounce_ms:'按键消抖',motion_sync:'Motion Sync',angle_snapping:'直线修正',ripple_control:'波纹修正',led_idle_seconds:'闲置熄灯时间',sensor_sleep_enabled:'自定义传感器休眠',sensor_sleep_seconds:'传感器休眠时间',lift_off_microns:'抬升高度'} as Record<string,string>)[key]??'性能参数'}：{typeof value==='boolean'?(value?'开启':'关闭'):key==='lift_off_microns'?`${Number(value)/1000} mm`:key==='debounce_ms'?`${value} ms`:`${value} 秒`}</p>)}</div>)}{preview.note&&preview.plan.settings.filter(s=>'Button'in s).map((s,i)=><p key={'binding'+i}>{'Button'in s?`按键 ${s.Button.id}：${fileActionLabel(s.Button.action,preview.plan.macros??[])}`:''}</p>)}{preview.plan.macros?.map(m=><details key={m.id}><summary>板载宏：{m.name} · {typeof m.mode==='object'?`固定重复 ${m.mode.Repeat} 次`:m.mode==='Once'?'单次':m.mode==='WhileHeld'?'按住循环':'再次按下停止'} · {m.steps.length} 步</summary><ol className="list-inside list-decimal text-sm">{m.steps.map((s,i)=><li key={i}>{macroStepLabel(s)}</li>)}</ol></details>)}{preview.plan.settings.filter(s=>'Lighting'in s).map((s,i)=><p key={'light'+i}>灯区：{'Lighting'in s&&s.Lighting.state.effect==='Off'?'关闭':typeof s.Lighting.state.effect==='object'&&'Breathing'in s.Lighting.state.effect?'呼吸':typeof s.Lighting.state.effect==='object'&&'ColorCycle'in s.Lighting.state.effect?'光谱循环':'常亮'}</p>)}</div><ImportLights plan={preview.plan}/>{preview.note?<p role="status">{preview.note}</p>:<p>将导入 {preview.plan.settings.length} 项已解析设置。支持的板载宏随配置导入；无法解析的原厂动作保持不变。</p>}{preview.unknown>0&&<p>保留 {preview.unknown} 个未开放写入的原厂按键。</p>}</div>}
 {error&&<p role="alert" className="lighting-error">{error}</p>}
 <div className="flex justify-end gap-3"><Button variant="ghost" disabled={busy} onClick={onClose}>取消</Button><Button disabled={!preview||busy} onClick={async()=>{if(!preview)return;setBusy(true);setError('');try{if(await onSave(multiProfile(preview.plan,preview.name)))onClose();}catch(e){setError(String(e));}finally{setBusy(false);}}}>导入到配置中心</Button></div>
 </DialogContent></Dialog>;
}

function fileActionLabel(a:MultiAction,macros:MultiMacro[]=[]):string{
 if(a==='Disabled')return '禁用';
 if(a==='ProfileCycle')return '循环切换板载组';
 if(typeof a==='object'){
  if('Mouse'in a)return ({Left:'鼠标左键',Right:'鼠标右键',Middle:'鼠标中键',Back:'后退',Forward:'前进',WheelLeft:'向左滚动',WheelRight:'向右滚动'})[a.Mouse];
  if('Macro'in a){const m=macros.find(m=>m.id===a.Macro);return m?`${m.name} · ${typeof m.mode==='object'?`固定重复 ${m.mode.Repeat} 次`:m.mode==='Once'?'单次':m.mode==='WhileHeld'?'按住循环':'再次按下停止'}`:'宏正文缺失';}
  if('Shortcut'in a)return a.Shortcut.map(shortcutEventLabel).join(' → ');
  if('Key'in a)return `${['Ctrl','Shift','Alt','Win','右 Ctrl','右 Shift','右 Alt','右 Win'].filter((_,i)=>a.Key.mods&(1<<i)).map(v=>v+' + ').join('')}HID 0x${a.Key.usage.toString(16).toUpperCase()}`;
  if('Scroll'in a)return a.Scroll==='Up'?'向上滚动':'向下滚动';
  if('ProfileSelect'in a)return `切换板载组 ${a.ProfileSelect+1}`;
  if('Dpi'in a&&typeof a.Dpi==='string')return ({Cycle:'循环切换 DPI',Up:'提高 DPI',Down:'降低 DPI',CycleDown:'反向切换 DPI'})[a.Dpi];
 }
 return '动作尚未转换';
}

function ImportLights({plan}:{plan:MultiPlan}){
 const indicators=plan.settings.filter(s=>'DpiIndicator'in s),colors=plan.settings.filter(s=>'DpiColor'in s);
 if(!indicators.length&&!colors.length)return null;
 const levels={Low:'低',Medium:'中',High:'高'},speeds={Low:'慢',Medium:'中',High:'快'};
 return <section aria-label="导入灯效预览" className="space-y-2"><b>灯效与指示色</b>{indicators.map((s,i)=>'DpiIndicator'in s&&<p key={i}>DPI 指示灯：{!s.DpiIndicator.enabled?'关闭':s.DpiIndicator.mode==='Breathing'?'呼吸':'常亮'} · 亮度{levels[s.DpiIndicator.brightness]} · 速度{speeds[s.DpiIndicator.speed]}</p>)}{!!colors.length&&<div aria-label="DPI 色表预览" className="grid grid-cols-2 gap-2 sm:grid-cols-4">{colors.map(s=>'DpiColor'in s&&<span key={s.DpiColor.stage} className="flex items-center gap-2 text-xs"><i aria-hidden className="inline-block h-3 w-3 rounded-full border" style={{backgroundColor:`rgb(${s.DpiColor.color.r} ${s.DpiColor.color.g} ${s.DpiColor.color.b})`}}/>档位 {s.DpiColor.stage+1}：#{[s.DpiColor.color.r,s.DpiColor.color.g,s.DpiColor.color.b].map(v=>v.toString(16).padStart(2,'0')).join('')}</span>)}</div>}</section>;
}

function macroStepLabel(s:MultiStep):string{
 if(typeof s==='string')return '等待释放';
 if('Delay'in s)return `延时 ${s.Delay} ms`;
 if('KeyDown'in s||'KeyUp'in s){const down='KeyDown'in s,code='KeyDown'in s?s.KeyDown:s.KeyUp;if(code>=224&&code<=231)return shortcutEventLabel({kind:'Modifier',down,value:1<<(code-224)});return `${KEY_NAMES.find(([v])=>v===code)?.[1]??`HID 0x${code.toString(16)}`} · ${down?'按下':'抬起'}`;}
 if('MouseDown'in s||'MouseUp'in s){const down='MouseDown'in s,name='MouseDown'in s?s.MouseDown:s.MouseUp;return shortcutEventLabel({kind:'Mouse',down,value:({Left:1,Right:2,Middle:4,Back:8,Forward:16})[name]});}
 if('Media'in s)return `媒体键 0x${s.Media.toString(16).toUpperCase()} · 点按`;
 if('ScrollTimed'in s)return `${s.ScrollTimed.direction==='Up'?'向上滚动':'向下滚动'} · 脉冲 ${s.ScrollTimed.pulse_ms} ms · 归零 ${s.ScrollTimed.settle_ms} ms`;
 if('Scroll'in s)return s.Scroll==='Up'?'向上滚动':'向下滚动';
 return '其他事件（请在宏编辑器核对）';
}
