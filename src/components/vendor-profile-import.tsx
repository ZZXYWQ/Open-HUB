import {useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {open} from '@tauri-apps/plugin-dialog';
import {type Profile} from '@/lib/api';
import {MultiProfileEditor} from '@/components/multi-panel';
import {Button} from '@/components/ui/button';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from '@/components/ui/dialog';
interface Candidate {model:string;profile:Profile|null;issues:string[];notes:string[]}
interface Prepared {name:string;source_kind:string;candidates:Candidate[]}
const sources={
 icue:{label:'iCUE',command:'prepare_icue_import',title:'海盗船 iCUE 鼠标配置导入',choose:'选择 iCUE 导出文件',description:'转换 M65 PRO RGB 的 DPI、按键和支持的原始宏事件。读取文件后可编辑、模拟并保存；保存后由你选择是否应用到匹配鼠标。',filter:{name:'Corsair iCUE 配置',extensions:['cueprofile']}},
 gg:{label:'GG',command:'prepare_gg_import',title:'赛睿 GG 鼠标配置导入',choose:'选择 GG 配置数据库',description:'只读导入 SteelSeries GG 的 Rival 300 / Rival 310 / Sensei 310 / Sensei Ten / Rival 600 / Rival 3（原版有线） / Prime（原版有线） 配置数据库：CPI、回报率、按键、支持的原始宏事件，以及对应灯区可转换的常亮、关闭、渐变和多色呼吸。原版有线 Rival 3 1824 的循环和有限次数颜色序列可按原厂33ms步长导入，由 Open HUB 持续播放；有限播放保留原厂末帧省略规则，184C动态灯效仍未转换，不保证实际时序与GG完全一致。Prime 当前迁移已核对的原厂常亮、渐变与最多7色呼吸、五档CPI、按键和实体触发软件宏；主机流、响应效果及滚轮宏触发尚未转换。先核对并模拟，再保存到配置中心。尚未实机验证。',filter:{name:'SteelSeries GG 配置数据库',extensions:['db','sqlite','sqlite3']}},
};
export function VendorProfileImport({kind,onClose,onSave}:{kind:keyof typeof sources;onClose:()=>void;onSave:(p:Profile)=>Promise<boolean>}){
 const source=sources[kind];
 const [data,setData]=useState<Prepared|null>(null),[editing,setEditing]=useState<Profile|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(''),[note,setNote]=useState('');
 const load=async()=>{setBusy(true);setData(null);setEditing(null);setError('');setNote('');try{const path=await open({multiple:false,filters:[source.filter]});if(typeof path==='string')setData(await invoke<Prepared>(source.command,{path}));}catch(e){setError(String(e));}finally{setBusy(false);}};
 const save=async(p:Profile)=>{setBusy(true);setError('');try{const ok=await onSave(p);if(ok){setData(d=>d?{...d,candidates:d.candidates.map(c=>c.profile?.id===p.id?{...c,profile:p}:c)}:d);setNote(`${p.name} 已保存到配置中心。`);}else setError('配置未保存，请检查提示后重试。');return ok;}catch(e){setError(String(e));return false;}finally{setBusy(false);}};
 const simulate=async(p:Profile)=>{if(!p.multi)return;setBusy(true);setError('');setNote('');try{const result=await invoke<{packet_count:number;software_macros:unknown[]}>('multi_simulate',{plan:p.multi});setNote(`真实驱动模拟通过：${result.packet_count} 条报文、${result.software_macros.length} 个软件动作；未访问鼠标或注入键盘输入。`);}catch(e){setError(String(e));}finally{setBusy(false);}};
 return <><Dialog open onOpenChange={v=>{if(!v)onClose();}}><DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-3xl"><DialogHeader><DialogTitle>{source.title}</DialogTitle><DialogDescription>{source.description}</DialogDescription></DialogHeader>
 <Button disabled={busy} onClick={()=>void load()}>{busy?'正在处理…':source.choose}</Button>{error&&<p role="alert" className="lighting-error">{error}</p>}{note&&<p role="status">{note}</p>}
 {data&&<><p className="text-sm text-muted-foreground">{data.source_kind} · {data.name}</p>{data.candidates.map((candidate,i)=><section key={i} className="rounded-xl border p-4 space-y-3"><h3 className="font-bold">{candidate.model}</h3>{candidate.notes.map((n,j)=><p className="text-sm text-muted-foreground" key={j}>{n}</p>)}{candidate.issues.length>0&&<div role="status" className="lighting-error"><b>此配置暂不能完整转换：</b>{candidate.issues.map((n,j)=><p key={j}>{n}</p>)}</div>}{candidate.profile&&<><div className="flex flex-wrap gap-2 text-sm">{candidate.profile.levels.map((l,j)=><span key={j} className="rounded border px-2 py-1">档位 {j+1}：{l.x} / {l.y??l.x}{candidate.profile?.active_level===j?' · 默认':''}</span>)}</div><p className="text-sm">{candidate.profile.multi?.macros?.length??0} 个宏 · 型号绑定 {candidate.profile.target_vendor?.toString(16).toUpperCase().padStart(4,'0')}:{candidate.profile.target_model?.toString(16).toUpperCase().padStart(4,'0')} · 尚未实机验证</p><div className="flex flex-wrap gap-2"><Button disabled={busy} onClick={()=>setEditing(candidate.profile)}>检查并编辑 {source.label} 转换方案</Button><Button disabled={busy} variant="outline" onClick={()=>void simulate(candidate.profile!)}>模拟 {source.label} 转换方案</Button><Button disabled={busy} variant="outline" onClick={()=>void save(candidate.profile!)}>保存 {source.label} 转换方案</Button></div></>}</section>)}</>}
 </DialogContent></Dialog>{editing&&<MultiProfileEditor profile={editing} onClose={()=>setEditing(null)} onSave={save}/>}</>;
}
