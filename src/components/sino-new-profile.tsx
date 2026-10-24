import {useEffect,useState} from 'react';
import {api,type Profile} from '@/lib/api';
import {profileFromSino,type SinoModel,type SinoPlan} from '@/lib/sino';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from '@/components/ui/dialog';
import {Button} from '@/components/ui/button';
import {SinoProfileEditor} from '@/components/sino-panel';
export function SinoNewProfile({onClose,onSave}:{onClose:()=>void;onSave:(p:Profile)=>Promise<boolean>}){
 const [models,setModels]=useState<SinoModel[]>([]),[draft,setDraft]=useState<Profile|null>(null),[error,setError]=useState('');
 useEffect(()=>{api.sinoModels().then(setModels,e=>setError(String(e)));},[]);
 if(draft)return <SinoProfileEditor profile={draft} onClose={onClose} onSave={onSave}/>;
 return <Dialog open onOpenChange={v=>{if(!v)onClose();}}><DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-3xl"><DialogHeader><DialogTitle>离线创建完整设备方案</DialogTitle><DialogDescription>可编辑 DPI、按键和宏并运行软件模拟测试。初始按键为软件示例，不是鼠标原厂数据；没有对应鼠标也可保存方案。当前仅下列指定型号与固件已实现适配代码。</DialogDescription></DialogHeader>{error&&<p role="alert">{error}</p>}<div className="hub-device-grid">{models.map(m=><Button key={`${m.vendor}-${m.product}-${m.firmware}`} className="h-auto min-h-24 flex-col items-start whitespace-normal" variant="outline" onClick={()=>{const buttons=Array.from({length:m.buttons},(_,i)=>[[17,1,0,0],[17,2,0,0],[17,4,0,0],[17,16,0,0],[17,8,0,0],[65,0,0,0]][i]??[80,1,0,0]);const p:SinoPlan={model:m,profile:0,dpi:[[800,800],[1600,1600]],active:0,rate:1000,buttons,macros:[]};setDraft(profileFromSino(p,`${m.brand} · 新方案`));}}><b>{m.brand}</b><span className="text-xs">{m.name}</span><small>{m.firmware} · {m.buttons} 键 · 模拟测试，未实机验证</small></Button>)}</div></DialogContent></Dialog>;
}
