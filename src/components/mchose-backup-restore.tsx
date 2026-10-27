import {MCHOSE_MACRO_MODES} from '@/lib/mchose-macros';
import {MCHOSE_KEYS,mchoseKeyboardLabel} from '@/lib/mchose-keys';
import {useState} from 'react';
import {open} from '@tauri-apps/plugin-dialog';
import {api,type MchoseHardware,type MchoseProfileReceipt,type MchoseRestorePreview} from '@/lib/api';
import {mchoseRateHz} from '@/lib/mchose-plan';
import {MCHOSE_BUTTONS,MCHOSE_LOD,MCHOSE_BINDINGS} from '@/lib/mchose-options';
import {Button} from '@/components/ui/button';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from '@/components/ui/dialog';

const sensorLabel=(value:number)=>[`模式 ${(value&3)+1}`,...([[4,'波纹控制'],[8,'直线修正'],[16,'Motion Sync'],[128,'玻璃模式']] as const).filter(([bit])=>value&bit).map(([,label])=>label)].join(' / ');
const binding=(b:number[])=>b[0]===0x21?`宏槽 ${b[1]+1}`:b.every(v=>v===255)?'不开放重映射':mchoseKeyboardLabel(b)??MCHOSE_BINDINGS.find(([,v])=>JSON.stringify(v)===JSON.stringify(b))?.[0]??b.map(n=>n.toString(16).padStart(2,'0')).join(' ');
type Run=<T>(f:(current:()=>boolean)=>Promise<T>,message?:string)=>Promise<T|undefined>;
export function MchoseBackupRestore({id,hardware,busy,run,onRead,onRestored}:{id:string;hardware:MchoseHardware;busy:boolean;run:Run;onRead:(value:MchoseHardware)=>void;onRestored:(value:MchoseProfileReceipt)=>void}) {
 const [visible,setVisible]=useState(false),[prepared,setPrepared]=useState<{path:string;preview:MchoseRestorePreview}|null>(null),[error,setError]=useState(''),[result,setResult]=useState<MchoseProfileReceipt|null>(null);
 const choose=()=>run(async current=>{
  setPrepared(null);setError('');setResult(null);
  const path=await open({multiple:false,filters:[{name:'Open HUB 迈从完整备份',extensions:['json']}]});
  if(typeof path!=='string'||!current())return;
  setVisible(true);
  try {const preview=await api.mchoseRestorePreview(id,path,hardware.target,hardware.profile);
   if(current()){
    if(preview.selection.target.connection_key!==hardware.target.connection_key||preview.selection.target.device_key!==hardware.target.device_key)throw Error('备份预览的连接不匹配，请重新读取设备');
    if(preview.current.profile!==hardware.profile)throw Error("备份预览的板载组不匹配");
    onRead({...preview.current,target:preview.selection.target});setPrepared({path,preview});
   }
  }catch(e){if(current()){setError(String(e));setPrepared(null);}}
 });
 const restore=()=>prepared&&run(async current=>{
  const selection=prepared;setError('');
  try {
   const value=await api.mchoseRestore(id,selection.path,selection.preview.selection);
   if(current()){
    if(value.target.connection_key!==hardware.target.connection_key||value.target.device_key!==hardware.target.device_key||value.profile!==hardware.profile)throw Error('恢复结果的设备或板载组不匹配，请重新读取设备');
    onRestored(value);setPrepared(null);setResult(value);
   }
  }catch(e){if(current()){setError(String(e));setPrepared(null);}}
 });
 const p=prepared?.preview,d=p?.desired,b=p?.current;
 const rows=d&&b?[['板载组',String(b.profile+1),String(d.profile+1)],['名称',b.names[b.profile],d.names[d.profile]],['USB / 无线 Hz',`${mchoseRateHz(b.raw_settings[1]>>4)} / ${mchoseRateHz(b.raw_settings[2]>>4)}`,`${mchoseRateHz(d.raw_settings[1]>>4)} / ${mchoseRateHz(d.raw_settings[2]>>4)}`],['启用档数 / 当前档',`${b.count} / ${b.active+1}`,`${d.count} / ${d.active+1}`],['独立 X/Y',b.independent?'开启':'关闭',d.independent?'开启':'关闭'],['LOD',MCHOSE_LOD[b.lod],MCHOSE_LOD[d.lod]],['左 / 右消抖 ms',`${b.left_debounce} / ${b.right_debounce}`,`${d.left_debounce} / ${d.right_debounce}`],['休眠',b.never_sleep?'永不休眠':`${b.sleep} 分钟`,d.never_sleep?'永不休眠':`${d.sleep} 分钟`],['旋转角度',`${b.rotation}°`,`${d.rotation}°`],['性能设置',sensorLabel(b.sensor),sensorLabel(d.sensor)],...d.dpi_x.map((x,i)=>[`DPI ${i+1} X / Y`,`${b.dpi_x[i]} / ${b.dpi_y[i]}`,`${x} / ${d.dpi_y[i]}`]),...d.buttons.map((bytes,i)=>[MCHOSE_BUTTONS[i],binding(b.buttons[i]),binding(bytes)])]:[];
 return <><Button variant="outline" disabled={busy} onClick={()=>void choose()}>从文件恢复</Button>
 <Dialog open={visible} onOpenChange={v=>{if(!busy){setVisible(v);if(!v){setPrepared(null);setResult(null);setError('');}}}}><DialogContent className="overflow-y-auto sm:max-w-3xl" style={{maxHeight:'calc(100dvh - 48px)'}} showCloseButton={!busy} onEscapeKeyDown={e=>{if(busy)e.preventDefault();}} onPointerDownOutside={e=>{if(busy)e.preventDefault();}}>
 <DialogHeader><DialogTitle>{result?'迈从配置恢复已核对':'恢复迈从板载备份'}</DialogTitle><DialogDescription>{p?`${p.device_name} · 鼠标 V${p.source_firmware} / 接收器 V${p.source_receiver_firmware}`:'先预览文件与当前鼠标，再恢复并核对。'}</DialogDescription></DialogHeader>
 {error&&<p role="alert" className="break-words whitespace-pre-wrap text-destructive">{error}</p>}
 {busy&&!prepared&&!result&&!error&&<p role="status">正在读取并核对备份…</p>}
 {p&&d&&<>
 <table className="w-full text-left text-sm" aria-label="迈从备份恢复预览"><thead><tr><th>设置</th><th>当前</th><th>恢复后</th></tr></thead><tbody>{rows.map(([name,from,to])=><tr className="border-t" key={name}><th className="py-1 font-normal">{name}</th><td>{from}</td><td className={from!==to?'font-semibold text-primary':''}>{to}</td></tr>)}</tbody></table>
 {p.macros.length>0&&<div aria-label="恢复宏槽预览"><p className="font-semibold">本组引用的宏</p>{p.macros.map(m=><details key={m.source_slot} className="py-1 text-sm"><summary className="cursor-pointer">{m.name} · {MCHOSE_MACRO_MODES.find(mode=>mode.value===m.mode)?.label??`模式 ${m.mode}`} · {m.events.length} 条事件 · 备份槽 {m.source_slot+1} → 鼠标槽 {m.target_slot+1} · {m.upload?'写入并核对':'复用已有相同内容'}</summary><ol aria-label={`宏 ${m.name} 恢复事件`} className="my-2 max-h-48 list-decimal space-y-1 overflow-y-auto pl-7">{m.events.map((e,i)=><li key={i}>{e.kind==='delay'?`等待 ${e.delay_ms} ms`:e.kind==='key'?`${e.down?'按下':'松开'} ${MCHOSE_KEYS.find(k=>k.usage===e.code)?.label??`键码 ${e.code}`}${e.delay_ms?` · 原始附加延时字段 ${e.delay_ms}（保留）`:''}`:e.kind==='mouse'?`${e.down?'按下':'松开'}鼠标键 ${({1:'左键',2:'右键',4:'中键',8:'后侧键',16:'前侧键'} as Record<number,string>)[e.code]??e.code}`:`滚轮${e.code===0?'向上':'向下'}`}</li>)}</ol></details>)}</div>}
 <p className="text-xs text-muted-foreground">只恢复当前板载组及其引用的宏。备份中另外 {p.unrelated_backup_macros} 个未被本组引用的宏保留在文件中；其他组和现有无关宏保持。宏槽冲突会重新分配并调整本组绑定。</p>
 <p className="text-xs text-muted-foreground">恢复前再次核对文件、连接及设备配置，保存完整原状备份。恢复成功后清除本组参数草稿及写入宏槽的草稿。</p>
 </>}
 {result&&<div className="space-y-2" role="status"><p>板载 {result.profile+1} 已恢复并读回核对，写入 {result.uploaded_macros.length} 个宏。</p><p>写入前原始备份：<span className="select-text break-all">{result.backup??'配置已一致，未重复写入或新建备份'}</span></p></div>}
 <div className="flex justify-end gap-2"><Button variant="ghost" disabled={busy} onClick={()=>{setVisible(false);setPrepared(null);setResult(null);}}>关闭</Button>{!result&&<Button variant="outline" disabled={busy} onClick={()=>void choose()}>重新选择备份</Button>}{prepared&&<Button disabled={busy} onClick={()=>void restore()}>{busy?'正在恢复…':'恢复本组并核对'}</Button>}</div>
 </DialogContent></Dialog></>;
}
