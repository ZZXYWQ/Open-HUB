import {useEffect,useRef,useState} from 'react';
import {open} from '@tauri-apps/plugin-dialog';
import {Download,FolderOpen} from 'lucide-react';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from '@/components/ui/dialog';
import {Button} from '@/components/ui/button';
import {api,type Profile,type DeviceSummary} from '@/lib/api';
import {profileFromSino,type SinoModel} from '@/lib/sino';
import {brandName,profileVendor} from '@/lib/brands';

export function ProfileImportDialog({source,onClose,onDone,devices=[],models=[]}:{source:string|null;devices?:DeviceSummary[];models?:SinoModel[];onClose:()=>void;onDone:()=>void}) {
  const [items,setItems]=useState<Profile[]>([]),[selected,setSelected]=useState<Set<number>>(new Set()),[note,setNote]=useState(''),[error,setError]=useState(''),[busy,setBusy]=useState(false),[file,setFile]=useState(''),[deviceId,setDeviceId]=useState('');
  const generation=useRef(0);
  const boardBrand=source?.startsWith('device:')?source.slice(7):null;
  const kind=boardBrand?'device':source??'ghub';
  const nativeDevices=devices.filter(d=>d.id.startsWith('sino:')&&(!boardBrand||models.some(m=>m.brand===boardBrand&&m.vendor===d.vendor_id&&m.firmware===d.firmware_name&&d.model_ids.some(id=>parseInt(id,16)===m.product))));
  const selectedDevice=nativeDevices.find(d=>d.id===deviceId)??nativeDevices[0];
  const title=kind==='ghub'?'导入罗技 G HUB 配置':kind==='mhub'?'导入迈从 M HUB 配置':kind==='device'?(boardBrand??'鼠标')+' · 读取板载配置':'导入 Open HUB 配置包';
  useEffect(()=>{generation.current++;setItems([]);setSelected(new Set());setNote('');setError('');setFile('');setDeviceId('');setBusy(false);return()=>{generation.current++;};},[source]);
  const load=async(pick:boolean)=>{
    const current=generation.current;
    setBusy(true);setError('');setItems([]);setSelected(new Set());setNote('');setFile('');
    try {
      if(kind==='device') {
        if(!selectedDevice)throw Error('请先连接'+(boardBrand??'已适配品牌')+'的指定型号鼠标');
        const value=await api.sinoRead(selectedDevice.id,null);
        if(current!==generation.current)return;
        if(boardBrand&&(value.plan.model.brand!==boardBrand||!models.some(m=>m.brand===boardBrand&&m.vendor===value.plan.model.vendor&&m.product===value.plan.model.product&&m.firmware===value.plan.model.firmware)))throw Error('读取结果与所选品牌和型号不匹配，未导入');
        setItems([profileFromSino(value.plan,value.plan.model.brand+' · 板载'+(value.plan.profile+1))]);setSelected(new Set([0]));setFile(value.plan.model.name);
        setNote('从鼠标读取 DPI、回报率和按键；原厂宏内容不可读回，只有本程序记录的宏可带入。');return;
      }
      let path:string|null=null;
      if(pick||kind==='openhub') {
        const value=await open({multiple:false,filters:[{name:kind==='ghub'?'G HUB 配置数据库':'配置文件',extensions:kind==='ghub'?['db','json']:['json']}]});
        if(current!==generation.current||typeof value!=='string')return;
        path=value;
      }
      const value=await api.previewProfileImport(kind,path);
      if(current!==generation.current)return;
      setItems(value.profiles);setSelected(new Set(value.profiles.map((_,i)=>i)));setNote(value.note);setFile(path?.split(/[\\/]/).pop()||'自动找到的本地配置');
    } catch(e) {if(current===generation.current)setError(String(e));}
    finally {if(current===generation.current)setBusy(false);}
  };
  const commit=async()=>{
    const current=generation.current;
    setBusy(true);setError('');
    try {await api.commitProfileImport(items.filter((_,i)=>selected.has(i)));if(current!==generation.current)return;onDone();onClose();}
    catch(e){if(current===generation.current)setError(String(e));}
    finally{if(current===generation.current)setBusy(false);}
  };
  return <Dialog open={source!==null} onOpenChange={value=>{if(!value&&!busy)onClose();}}>
    <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-3xl">
      <DialogHeader><DialogTitle>{title}</DialogTitle><DialogDescription>预览内容后导入配置中心，再选择是否应用到匹配的鼠标。</DialogDescription></DialogHeader>
      {kind==='device'&&<label className="light-select-label">选择{boardBrand??''}鼠标<select aria-label="选择导入鼠标" disabled={busy} value={selectedDevice?.id??''} onChange={e=>{setDeviceId(e.target.value);setItems([]);setSelected(new Set());setError('');}}>{!nativeDevices.length&&<option value="">没有已连接的兼容鼠标</option>}{nativeDevices.map(d=><option key={d.id} value={d.id}>{d.name} · {d.firmware_name}</option>)}</select></label>}
      <p className="light-help">{kind==='device'?'读取此品牌指定型号当前使用的板载组，不修改设备。'+(models.length?'支持型号：'+[...new Set(models.map(m=>m.name+'（'+m.firmware+'）'))].join(' / '):''):kind==='ghub'?'罗技 G HUB：导入 DPI、回报率和应用关联；暂不转换官方灯效、按键及宏。':kind==='mhub'?'迈从 M HUB：导入 A7 V3 Ultra+ 的三组板载名称与引用。设备已有参数不复制、不改写。':'导入本程序导出的 Open HUB 配置包，保留每项的品牌、型号和完整方案。'}</p>
      <div className="flex flex-wrap gap-2">
        {kind!=='openhub'&&<Button disabled={busy||(kind==='device'&&!selectedDevice)} onClick={()=>void load(false)}><Download size={15}/>{kind==='device'?'读取鼠标板载配置':'读取本地配置'}</Button>}
        {kind!=='device'&&<Button variant="outline" disabled={busy} onClick={()=>void load(true)}><FolderOpen size={15}/>{busy?'正在读取…':'选择配置文件'}</Button>}
      </div>
      {error&&<div role="alert" className="lighting-error">{error}</div>}
      {items.length>0&&<>
        <div className="flex items-center justify-between gap-3 text-sm"><b>{file} · {items.length} 项</b><label className="flex items-center gap-2"><input type="checkbox" disabled={busy} checked={selected.size===items.length} onChange={e=>setSelected(e.target.checked?new Set(items.map((_,i)=>i)):new Set())}/>全选</label></div>
        <p className="light-help">{note}</p>
        <div className="hub-import-preview">{items.map((profile,i)=><label key={i} className="light-target"><input type="checkbox" disabled={busy} checked={selected.has(i)} onChange={e=>setSelected(value=>{const next=new Set(value);if(e.target.checked)next.add(i);else next.delete(i);return next;})}/><div><b>{profile.name}</b><small>{profile.sino?.model.brand??brandName(profileVendor(profile),profile.target_model)}{profile.target_model!=null?' · 型号 '+profile.target_model.toString(16).toUpperCase():''} · {profile.mchose_board!=null?'板载 '+(profile.mchose_board+1):profile.levels.map(level=>level.x).join(' / ')+' DPI'}{profile.report_rate_hz?' · '+profile.report_rate_hz+' Hz':''}{profile.sino?' · '+profile.sino.buttons.length+' 键 / '+profile.sino.macros.length+' 个已记录宏 / '+profile.sino.model.firmware:''}</small></div></label>)}</div>
      </>}
      <div className="flex justify-end gap-2"><Button variant="ghost" disabled={busy} onClick={onClose}>取消</Button><Button disabled={busy||selected.size===0} onClick={()=>void commit()}><Download size={15}/>导入 {selected.size} 项</Button></div>
    </DialogContent>
  </Dialog>;
}
