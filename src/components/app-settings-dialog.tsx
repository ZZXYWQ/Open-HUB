import {AppearanceSettings} from '@/components/appearance-settings';
import {useEffect,useState} from 'react';
import {Download,RefreshCw,Settings} from 'lucide-react';
import {toast} from 'sonner';
import {Button} from '@/components/ui/button';
import {Dialog,DialogContent,DialogDescription,DialogHeader,DialogTitle,DialogTrigger} from '@/components/ui/dialog';
import {Label} from '@/components/ui/label';
import {Separator} from '@/components/ui/separator';
import {Switch} from '@/components/ui/switch';
import {Tooltip,TooltipContent,TooltipTrigger} from '@/components/ui/tooltip';
import {api,type AppInfo,type UpdateInfo,type UpdateProgress} from '@/lib/api';
interface Props{update:UpdateInfo|null;onUpdateFound:(info:UpdateInfo|null)=>void;progress:UpdateProgress|null;installing:boolean;onInstall:()=>void;onDownload:()=>void}
export function AppSettingsDialog({update,onUpdateFound,progress,installing,onInstall,onDownload}:Props){
 const [info,setInfo]=useState<AppInfo|null>(null),[checking,setChecking]=useState(false),[checkedOnce,setCheckedOnce]=useState(false),[error,setError]=useState('');
 const load=()=>api.appInfo().then(setInfo,e=>toast.error(String(e)));
 useEffect(()=>{void load();},[]);
 const check=async()=>{setChecking(true);setCheckedOnce(false);setError('');try{onUpdateFound(await api.checkForUpdate());setCheckedOnce(true);}catch(e){setError(String(e));}finally{setChecking(false);}};
 const toggle=async(value:boolean)=>{if(!info)return;const settings={...info.settings,check_updates:value,auto_install_updates:false};try{await api.setAppSettings(settings);setInfo({...info,settings});}catch(e){setError(String(e));}};
 const pct=progress?.total?Math.min(100,Math.round(progress.downloaded/progress.total*100)):null;
 return <Dialog onOpenChange={open=>{if(open)void load();}}><Tooltip><TooltipTrigger asChild><DialogTrigger asChild><Button variant="ghost" size="icon-lg" className="relative size-[42px] rounded-full text-muted-foreground" aria-label="App settings"><Settings className="size-5"/>{update&&<span className="absolute top-2 right-2 size-2 rounded-full bg-primary" aria-label="Update available"/>}</Button></DialogTrigger></TooltipTrigger><TooltipContent>外观与设置</TooltipContent></Tooltip>
 <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl"><DialogHeader><DialogTitle className="text-xl font-extrabold">外观与设置</DialogTitle><DialogDescription>Open HUB {info?.version} · 实验性版本</DialogDescription></DialogHeader><AppearanceSettings/><Separator/>
 <section aria-label="GitHub 版本更新" className="grid gap-3"><h3 className="font-bold">版本更新</h3><p className="text-xs text-muted-foreground">来源：GitHub · ZZXYWQ/Open-HUB。自动检查只读取版本信息，不自动下载或安装。</p>
 <div className="rounded-xl border p-4 space-y-3">{update?<><b>发现新版本 {update.version}{update.experimental?' · 实验版':''}</b><p className="text-sm text-muted-foreground">当前 {info?.version??update.current_version}。{update.downloaded?'安装包已下载，安装前将再次验证签名。':'下载完成后，你仍可选择是否安装。'}</p>{update.notes&&<details><summary className="cursor-pointer text-sm">查看更新说明</summary><pre className="mt-2 max-h-60 overflow-y-auto whitespace-pre-wrap break-words text-xs">{update.notes}</pre></details>}<div className="flex flex-wrap gap-2">{update.downloaded?<Button disabled={installing} onClick={()=>onInstall()}>{installing?'正在安装…':'安装并重启'}</Button>:<Button disabled={installing} onClick={()=>onDownload()}><Download size={16}/>{installing?(pct==null?'正在下载…':`正在下载… ${pct}%`):'下载更新'}</Button>}{update.downloaded&&<Button disabled={installing} variant="outline" onClick={()=>onDownload()}>重新下载</Button>}<Button disabled={installing} variant="ghost" onClick={()=>toast.info('已保留当前版本，可稍后从此处继续更新')}>稍后</Button></div></>:<p className="text-sm text-muted-foreground">{checkedOnce?'当前已是最新版本。':'检查已发布的稳定版或实验版；预发布会明确标识。'}</p>}
 {installing&&progress&&<p role="status" className="text-xs">已下载 {(progress.downloaded/1048576).toFixed(1)} MiB{pct!=null?` · ${pct}%`:''}</p>}
 <Button variant="outline" disabled={checking||installing} onClick={()=>void check()}><RefreshCw className={checking?'animate-spin':''} size={16}/>{checking?'正在检查…':'立即检查更新'}</Button></div>
 {error&&<p role="alert" className="text-sm text-destructive">{error}</p>}
 {info&&<div className="flex items-center justify-between gap-4 rounded-lg bg-secondary/60 px-4 py-3"><Label htmlFor="check-updates" className="flex-col items-start gap-1"><span>自动检查 GitHub 更新</span><span className="text-xs font-normal text-muted-foreground">启动约 30 秒后及每 6 小时检查；下载、安装均由你选择。</span></Label><Switch id="check-updates" checked={info.settings.check_updates} onCheckedChange={v=>void toggle(v)}/></div>}
 <p className="text-xs text-muted-foreground">更新包使用 Open HUB 独立密钥验证完整性与来源。此更新签名不同于 Windows 代码签名，实验安装包仍未取得 Windows 代码签名证书。</p></section></DialogContent></Dialog>;
}
