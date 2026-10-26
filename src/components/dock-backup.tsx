import {useEffect,useRef,useState} from 'react';
import {open,save} from '@tauri-apps/plugin-dialog';
import {toast} from 'sonner';
import {Button} from '@/components/ui/button';
import {Dialog,DialogContent,DialogDescription,DialogHeader,DialogTitle} from '@/components/ui/dialog';
import {api,type DockLight,type DockRestorePreview,type DockView} from '@/lib/api';
const modes=['常亮','呼吸','七彩闪耀','循环变色','流水','音乐'];
const hex=(c:number[])=>'#'+c.map(v=>v.toString(16).padStart(2,'0')).join('');
function values(l:DockLight){return [l.enabled?'开启':'关闭',modes[l.mode]??String(l.mode),l.sync?'开启':'关闭',hex(l.color),`${l.brightness+1}/5`,`${5-l.speed}/5`,l.direction?'顺时针':'逆时针',String(l.mode_count)];}
export function DockBackup({connectionKey,active,run,onRestored}:{connectionKey:string;active:boolean;run:(action:()=>Promise<void>)=>Promise<void>;onRestored:(view:DockView)=>void}){
 const [prepared,setPrepared]=useState<{path:string;view:DockRestorePreview}|null>(null),[pending,setPending]=useState(false);
 const generation=useRef(0),current=useRef(connectionKey),mounted=useRef(true);current.current=connectionKey;
 useEffect(()=>{generation.current++;setPrepared(null);},[connectionKey]);
 useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;generation.current++;};},[]);
 const valid=(turn:number,connection:string)=>mounted.current&&generation.current===turn&&current.current===connection;
 const exportFile=()=>run(async()=>{const turn=++generation.current,connection=connectionKey;const path=await save({defaultPath:'Open-HUB-MagDock-灯效.json',filters:[{name:'底座灯效备份',extensions:['json']}]});if(!path||!valid(turn,connection))return;await api.dockExport(path,connection);if(valid(turn,connection))toast.success('底座灯效备份已导出');});
 const choose=()=>run(async()=>{const turn=++generation.current,connection=connectionKey;setPrepared(null);const path=await open({multiple:false,filters:[{name:'底座灯效备份',extensions:['json']}]});if(typeof path!=='string'||!valid(turn,connection))return;const view=await api.dockRestorePreview(path,connection);if(valid(turn,connection)&&view.connection_key===connection)setPrepared({path,view});});
 const restore=async()=>{if(!prepared||pending)return;const selection=prepared.view,turn=++generation.current,connection=connectionKey;setPending(true);try{await run(async()=>{if(!valid(turn,connection))return;const result=await api.dockRestore(prepared.path,{connection_key:selection.connection_key,sha256:selection.sha256,expected_light:selection.expected_light,expected_firmware:selection.expected_firmware});if(!valid(turn,connection)||result.connection_key!==connection)return;onRestored(result);setPrepared(null);toast.success('底座灯效已恢复并读回核对');});}finally{if(mounted.current)setPending(false);}};
 const source=prepared?values(prepared.view.light):[],before=prepared?values(prepared.view.expected_light):[];
 return <div className="mt-4 border-t pt-4" aria-label="底座灯效备份"><div className="lighting-actions"><Button variant="outline" disabled={!connectionKey||active} onClick={()=>void exportFile()}>导出底座灯效</Button><Button variant="outline" disabled={!connectionKey||active} onClick={()=>void choose()}>从备份恢复灯效</Button></div><p className="light-help">{active?'请先停止底座同步或律动，再导出或恢复设置。':'恢复前可预览变化；软件音乐、屏幕同步方案在灯光方案中另行载入。'}</p><p className="light-help">早期自动备份可从 %LOCALAPPDATA%\Open HUB\backups 文件夹选择。</p>
 <Dialog open={!!prepared} onOpenChange={v=>{if(!v&&!pending){generation.current++;setPrepared(null);}}}><DialogContent style={{width:'calc(100vw - 32px)',maxWidth:640,maxHeight:'calc(100dvh - 48px)'}} className="overflow-y-auto" onEscapeKeyDown={e=>{if(pending)e.preventDefault();}} onPointerDownOutside={e=>{if(pending)e.preventDefault();}} showCloseButton={!pending}>
 <DialogHeader><DialogTitle>恢复底座灯效</DialogTitle><DialogDescription>MCHOSE MagDock · 当前固件 {prepared?.view.expected_firmware} · {prepared?.view.legacy?'旧版设置快照（未记录设备及固件）':`备份固件 ${prepared?.view.source_firmware}`}</DialogDescription></DialogHeader>
 <table className="w-full text-left text-sm" aria-label="底座恢复预览"><thead><tr><th>设置</th><th>当前</th><th>备份</th></tr></thead><tbody>{['灯光','基础模式','音乐同步开关','颜色','亮度','速度','方向','原厂模式参数'].map((label,i)=><tr key={label} className="border-t"><td className="py-2">{label}</td><td>{before[i]}</td><td className={source[i]!==before[i]?'text-primary font-semibold':''}>{source[i]}</td></tr>)}</tbody></table>
 <p className="text-xs text-muted-foreground">应用前再次核对文件、连接与当前设置，并备份写入前状态。</p><div className="flex justify-end gap-3"><Button variant="ghost" disabled={pending} onClick={()=>{generation.current++;setPrepared(null);}}>取消</Button><Button disabled={pending||active||!connectionKey} onClick={()=>void restore()}>{pending?'正在恢复…':'恢复并核对'}</Button></div>
 </DialogContent></Dialog></div>;
}
