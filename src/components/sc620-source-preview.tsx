import {useEffect,useRef,useState} from 'react';
import {open,save} from '@tauri-apps/plugin-dialog';
import {invoke} from '@tauri-apps/api/core';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from '@/components/ui/dialog';
import {Button} from '@/components/ui/button';
import {type Sc620Preview,sc620ActionLabel,sc620Playback,sc620Lights} from '@/lib/sc620-source';
export function Sc620SourcePreview({onClose}:{onClose:()=>void}){
 const [data,setData]=useState<Sc620Preview|null>(null),[path,setPath]=useState(''),[error,setError]=useState(''),[note,setNote]=useState(''),[busy,setBusy]=useState(false);
 const generation=useRef(0);useEffect(()=>()=>{generation.current++;},[]);
 const load=async(file?:string,profile?:number)=>{
  const current=++generation.current;setBusy(true);setError('');setNote('');setData(null);
  try{
   const picked=file??await open({multiple:false,filters:[{name:'SC620 原厂数据库或 Open HUB 源方案',extensions:['db','sqlite','sqlite3','json']}]});
   if(typeof picked!=='string'||current!==generation.current)return;
   const value=await invoke<Sc620Preview>('preview_sc620_source',{path:picked,profile:profile??null});
   if(current!==generation.current)return;setPath(picked);setData(value);
  }catch(e){if(current===generation.current)setError(String(e));}
  finally{if(current===generation.current)setBusy(false);}
 };
 const archive=async()=>{
  if(!data)return;const current=generation.current;setBusy(true);setError('');setNote('');
  try{const dest=await save({defaultPath:`Open-HUB-SC620-源方案-${data.source.settings.profile_id}.json`,filters:[{name:'Open HUB SC620 源方案',extensions:['json']}]});
   if(!dest||current!==generation.current)return;
   await invoke('export_sc620_source',{path:dest,source:data.source});
   if(current===generation.current)setNote('源方案已保存，包含未转换内容；未写入鼠标。');
  }catch(e){if(current===generation.current)setError(String(e));}
  finally{if(current===generation.current)setBusy(false);}
 };
 const p=data?.source.settings.patch;
 return <Dialog open onOpenChange={v=>{if(!v&&!busy)onClose();}}><DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-3xl"><DialogHeader><DialogTitle>狼蛛 SC620 配置预览</DialogTitle><DialogDescription>读取原厂数据库或重新打开 Open HUB 源方案。完整保留设置、按键和宏；设备识别与写入尚未开放。</DialogDescription></DialogHeader>
 <div className="flex flex-wrap gap-2"><Button disabled={busy} onClick={()=>void load()}>{busy?'正在读取…':'选择 SC620 配置文件'}</Button>{data&&<Button variant="outline" disabled={busy} onClick={()=>void archive()}>保存 Open HUB 源方案</Button>}</div>
 {error&&<p role="alert" className="lighting-error">{error}</p>}{note&&<p role="status">{note}</p>}
 {data&&p&&<>
 <div className="flex flex-wrap items-center justify-between gap-3"><span className="text-xs text-muted-foreground break-all">{path.split(/[\\/]/).pop()}</span><label className="text-sm">文件内配置<select aria-label="SC620 文件内配置" disabled={busy} className="ml-2 rounded border bg-background p-2" value={data.source.settings.profile_id} onChange={e=>void load(path,Number(e.target.value))}>{data.profile_ids.map(id=><option key={id} value={id}>配置 {id}</option>)}</select></label></div>
 <p role="status" className="rounded-lg border p-3 text-sm">{data.static_subset_convertible?'已核对字段可进行静态转换；仍不可应用到设备。':'存在未转换内容，不能准备整份设备配置；仍可保存完整源方案。'}</p>
 <section aria-label="SC620 设置" className="space-y-3 rounded-xl border p-4"><h3 className="font-semibold">DPI、性能与灯光</h3>
 <div className="flex flex-wrap gap-2">{p.dpi.map((dpi,i)=><span key={i} className={`rounded-md border px-3 py-1 text-sm ${p.active===i?'border-primary bg-primary/10':''}`} aria-label={`SC620 档位 ${i+1}${p.active===i?' 当前':''}`}><i className="mr-2 inline-block size-2 rounded-full" style={{backgroundColor:`rgb(${p.colors[i].join(',')})`}}/>{dpi} DPI</span>)}</div>
 <p className="text-sm">{p.report_rate_hz} Hz · 消抖 {p.debounce_ms} ms · 灯效：{sc620Lights[p.light]??p.light}</p><p className="text-xs text-muted-foreground">色点为各 DPI 档位的指示颜色。</p>
 <div className="flex flex-wrap gap-x-5 gap-y-1 text-sm">{p.lod_mm!=null&&<span>LOD {p.lod_mm} mm</span>}{p.sleep_minutes!=null&&<span>休眠 {p.sleep_minutes} 分钟</span>}{p.scroll_reverse!=null&&<span>滚轮：{p.scroll_reverse?'反向':'正向'}</span>}{p.esports!=null&&<span>电竞模式：{p.esports?'开启':'关闭'}</span>}{([['ripple_control','波纹控制'],['angle_snap','直线修正'],['motion_sync','Motion Sync']] as const).map(([key,label])=>p[key]!=null&&<span key={key}>{label}：{p[key]?'开启':'关闭'}</span>)}</div>
 </section>
 <section aria-label="SC620 按键" className="space-y-2 rounded-xl border p-4"><h3 className="font-semibold">按键映射 · {data.keys.length} 项</h3><p className="text-xs text-muted-foreground">控制编号来自原厂文件，尚未核对鼠标上的物理位置。</p>{data.keys.map((k,i)=><div key={i} className="flex flex-wrap justify-between gap-2 border-t pt-2 text-sm"><span>控制 {k.source_control} · 层 {k.layer}</span><span>{k.problem?`未转换：${k.problem}`:k.macro_slot!=null?`宏 ${data.source.keys[i].value} → 槽位 ${k.macro_slot+1}`:sc620ActionLabel(k.action)}</span></div>)}{!data.keys.length&&<p className="text-sm">这组配置没有按键记录。</p>}</section>
 <section aria-label="SC620 宏" className="space-y-3 rounded-xl border p-4"><h3 className="font-semibold">宏库 · {data.source.macros.length} 个</h3>{data.source.macros.map(m=><details key={m.id} className="rounded-lg border p-3"><summary>{m.name} · {sc620Playback(m.playback)} · {m.events.length} 条事件</summary>{data.macros.find(x=>x.id===m.id)?.problem&&<p className="mt-2 text-sm text-destructive">未转换：{data.macros.find(x=>x.id===m.id)?.problem}</p>}<ol className="mt-2 space-y-1 text-xs text-muted-foreground">{m.events.map(e=><li key={e.record_id}>{e.index+1}. {e.description||`事件类型 ${e.kind}`} · 原值 {e.value} · 延时字段 {e.delay_raw}</li>)}</ol></details>)}</section>
 {data.issues.length>0&&<section aria-label="SC620 未转换内容" className="space-y-1 rounded-xl border p-4 text-sm"><h3 className="font-semibold">需要补齐的内容</h3>{data.issues.map((issue,i)=><p key={i}>{issue}</p>)}{Object.entries(data.source.settings.unconverted).map(([k,v])=><p key={k}>{k} = {v}</p>)}</section>}
 </>}
 <div className="flex justify-end"><Button variant="ghost" disabled={busy} onClick={onClose}>关闭</Button></div>
 </DialogContent></Dialog>;
}
