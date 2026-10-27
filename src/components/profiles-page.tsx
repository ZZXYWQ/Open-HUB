import {Sc620SourcePreview} from '@/components/sc620-source-preview';
import {MhubProfileImport} from "@/components/mhub-profile-import";
import {MchoseProfileEditor} from "@/components/mchose-profile-editor";
import {mchoseRateHz} from "@/lib/mchose-plan";
import {isLegacyBrand} from '@/lib/legacy';
import {compxModel,compxBrand} from '@/lib/compx';
import {type BoardImportBrand} from '@/lib/native-models';
import {AtkProfileImport} from '@/components/atk-profile-import';
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from "@/components/ui/dialog";
import {BrandProfileImports} from '@/components/brand-profile-imports';
import type {SinoModel} from '@/lib/sino';
import {ReconnectPlans} from '@/components/reconnect-plans';
import {MultiProfileEditor} from '@/components/multi-panel';
import {SynapsePreview} from '@/components/synapse-preview';
import {VendorProfileImport} from '@/components/vendor-profile-import';
import {SinoNewProfile} from "@/components/sino-new-profile";
import {SinoProfileEditor} from "@/components/sino-panel";
import {api} from "@/lib/api";
import {brandName,profileVendor,matchesDevice,needsProfileConnection} from "@/lib/brands";
import {save} from "@tauri-apps/plugin-dialog";
import {ProfileImportDialog} from "@/components/profile-import-dialog";
import {toast} from "sonner";
import { useState } from "react";

import { Download, Upload, Monitor, Pencil, Play, Plus, Trash2 } from "lucide-react";

import { newProfile, ProfileEditor } from "@/components/profile-editor";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { DESKTOP_ID, type Profile, type ProfilesView, type DeviceSummary } from "@/lib/api";
import { cn } from "@/lib/utils";


interface Props {
  view: ProfilesView;
  devices: DeviceSummary[];
  importing: boolean;
  onAutoSwitch: (enabled: boolean) => void;
  onActivate: (id: string,deviceId?:string,asCopy?:boolean,expectedConnection?:string) => void;
  onSave: (profile: Profile) => Promise<boolean>;
  onDelete: (id: string) => void;
  onImport: () => void;
  onMhubImport:(path?:string)=>Promise<void>;
}

export function ProfilesPage({ view, devices, importing, onAutoSwitch, onActivate, onSave, onDelete, }: Props) {
  const [targets,setTargets]=useState<Record<string,string>>({});
  const [offline,setOffline]=useState(false);
  const [multiNew,setMultiNew]=useState(false);
  const [synapsePreview,setSynapsePreview]=useState(false);
  const [mhubImport,setMhubImport]=useState(false);
  const [sc620Preview,setSc620Preview]=useState(false);
  const [icueImport,setIcueImport]=useState(false);
  const [ggImport,setGgImport]=useState(false);
  const [editing, setEditing] = useState<Profile | null>(null);
  const brandKey=(p:Profile)=>p.sino?'sino:'+p.sino.model.brand:p.multi&&compxModel(p.multi.vendor,p.multi.product)?'compx:'+compxModel(p.multi.vendor,p.multi.product)!.brand:String(profileVendor(p));
  const brandLabel=(v:string)=>v.startsWith('sino:')?v.slice(5):v.startsWith('compx:')?compxBrand(v.slice(6)):brandName(v==='null'?null:Number(v));
  const [importChooser,setImportChooser]=useState(false);
  const chooseImport=(source:string,models:SinoModel[])=>{setImportChooser(false);setImportModels(models);if(source==='sc620')setSc620Preview(true);else if(source==='mhub')setMhubImport(true);else if(source==='synapse')setSynapsePreview(true);else if(source==='icue')setIcueImport(true);else if(source==='gg')setGgImport(true);else if(isLegacyBrand(source)||source==='atk'||source==='vxe'||source==='vgn'||source==='hyperx'||source==='pulsar')setAtkImport(source);else setImportSource(source);};
  const [atkImport,setAtkImport]=useState<BoardImportBrand|null>(null);
  const [importModels,setImportModels]=useState<SinoModel[]>([]);
  const [importSource,setImportSource]=useState<string|null>(null),[vendor,setVendor]=useState('all');
  const exportPack=async()=>{try{const path=await save({defaultPath:'Open-HUB-配置包.json',filters:[{name:'Open HUB 配置包',extensions:['json']}]});if(path){await api.exportProfilePack(path,view.profiles.filter(p=>vendor==='all'||brandKey(p)===vendor));toast.success('配置包已导出');}}catch(e){toast.error(String(e));}};
  // Desktop first, then alphabetical.
  const profiles = view.profiles.filter(p=>vendor==='all'||brandKey(p)===vendor).sort((a, b) =>
    a.id === DESKTOP_ID ? -1 : b.id === DESKTOP_ID ? 1 : a.name.localeCompare(b.name),
  );

  return (
    <section className="flex-1 overflow-y-auto px-11 pt-8 pb-12">
      <div className="mx-auto flex max-w-6xl flex-wrap items-center gap-3">
        <div className="mr-auto min-w-64 flex-1">
          <h1 className="text-2xl font-extrabold tracking-tight uppercase">配置中心</h1>
          <p className="text-sm text-muted-foreground">
            按品牌管理配置，每只鼠标使用独立副本；自动切换仅对可唯一识别且已分配配置的鼠标生效。
          </p>
        </div>
        <Label className="gap-3 rounded-full bg-card py-2 pr-2 pl-4 text-sm font-semibold">
          自动切换
          <Switch checked={view.auto_switch} onCheckedChange={onAutoSwitch} />
        </Label>
        <Button variant="outline" className="h-10" onClick={()=>void exportPack()}><Upload/>导出配置包</Button>
        <Button variant="secondary" className="h-10" disabled={importing} onClick={()=>setImportChooser(true)}><Download/>导入配置</Button>

        <Button variant="outline" className="h-10" onClick={()=>setMultiNew(true)}>按品牌新建配置</Button>
        <Button variant="outline" className="h-10" onClick={()=>setOffline(true)}>离线设备方案</Button>
        <Button className="h-10 px-4 font-bold" onClick={() => setEditing(newProfile())}>
          <Plus /> 新建配置
        </Button>
      </div>

      <div className="mx-auto mt-6 max-w-6xl">
        <div className="hub-brand-filter" aria-label="按品牌筛选配置">{['all',...new Set(view.profiles.map(p=>brandKey(p)))].map(v=><button key={v} aria-pressed={vendor===v} className={vendor===v?'selected':''} onClick={()=>setVendor(v)}>{v==='all'?'全部品牌':brandLabel(v)}<span>{view.profiles.filter(p=>v==='all'||brandKey(p)===v).length}</span></button>)}</div>
      </div>
      <div className="mx-auto mt-6 max-w-6xl"><ReconnectPlans/></div>
      <div className="mx-auto mt-6 grid max-w-6xl grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-4">
        {profiles.map((p) => {
          const owner=view.profile_owners?.[p.id];
          const ownerDevice=owner?devices.find(d=>d.profile_key===owner):undefined;
          const active=!!owner&&view.device_profiles?.[owner]?.active_id===p.id&&!!ownerDevice;
          const isDesktop = p.id === DESKTOP_ID;
          const candidates=devices.filter(d=>matchesDevice(p,d));const available=candidates.length>0;
          const needsTarget=candidates.length>1;
          const target=candidates.some(d=>d.id===targets[p.id])?targets[p.id]:ownerDevice?.id??(candidates.length===1?candidates[0].id:'');
          const selected=candidates.find(d=>d.id===target);
          const asCopy=!!owner&&!!selected&&selected.profile_key!==owner;
          return (
            <Card key={p.id} className={cn("gap-3 rounded-[10px] px-5 py-5 ring-0", active && "ring-2 ring-primary")}>
              <div className="flex items-start gap-2">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    {isDesktop && <Monitor className="size-4 shrink-0 text-muted-foreground" />}
                    <span className="truncate text-lg leading-tight font-extrabold tracking-tight uppercase">{p.name}</span>
                  </div>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <p className="mt-1 truncate font-mono text-[11px] text-muted-foreground">
                        {isDesktop ? "无应用匹配时使用" : p.app_path || "未关联应用 · 手动切换"}
                      </p>
                    </TooltipTrigger>
                    {p.app_path && <TooltipContent className="max-w-md break-all">{p.app_path}</TooltipContent>}
                  </Tooltip>
                </div>
                {active && <Badge>当前使用</Badge>}
              </div>

              {owner&&<p className="text-xs text-muted-foreground" aria-label={`配置 ${p.name} 归属`}>{ownerDevice?`独立配置 · ${candidates.findIndex(d=>d.id===ownerDevice.id)+1}. ${ownerDevice.name}`:"独立配置 · 所属鼠标未连接"}{owner.startsWith('session:')?' · 本次连接':''}</p>}
              <div className="flex flex-wrap gap-1.5">
                <Badge variant="outline">{p.sino?.model.brand??brandName(profileVendor(p),p.target_model??p.multi?.product)}</Badge>
                {p.sino&&<Badge variant="outline">完整按键 / {p.sino.macros.length} 个已记录宏 / {p.sino.model.firmware}</Badge>}
                {p.mchose_board!=null&&<Badge>迈从板载 {p.mchose_board+1}</Badge>}
                {p.mchose&&<Badge variant="outline">板载 {p.mchose.board+1} · {p.mchose.count}/6 档 · {p.mchose.buttons.filter(b=>b[0]!==255).length} 个按键 · {p.mchose.macros.length} 个宏</Badge>}
                {p.levels.map((l, i) => (
                  <span
                    key={i}
                    className={cn(
                      "rounded-md px-2 py-0.5 text-xs font-semibold tabular-nums",
                      i === p.active_level ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground",
                      p.mchose&&i>=p.mchose.count&&"opacity-40",
                    )}
                  >
                    {l.x}{p.mchose&&l.y!=null&&l.y!==l.x?` × ${l.y}`:""}
                  </span>
                ))}
                {p.mchose&&<span className="rounded-md border border-input px-2 py-0.5 text-xs text-muted-foreground tabular-nums">USB {mchoseRateHz(p.mchose.usb_rate)} Hz / 无线 {mchoseRateHz(p.mchose.wireless_rate)} Hz</span>}
                {!p.mchose&&p.report_rate_hz != null && (
                  <span className="rounded-md border border-input px-2 py-0.5 text-xs text-muted-foreground tabular-nums">{p.report_rate_hz} Hz</span>
                )}
              </div>

              {needsTarget&&<label className="text-xs text-muted-foreground">应用到哪只鼠标<select aria-label={`配置 ${p.name} 目标鼠标`} className="mt-1 w-full rounded border bg-background p-2 text-sm" value={target} onChange={e=>setTargets({...targets,[p.id]:e.target.value})}><option value="">请选择目标鼠标</option>{candidates.map((d,i)=><option key={d.id} value={d.id}>{i+1}. {d.name} · {d.id.replace(/:board:\d+$/,'').split(':').at(-1)?.slice(-48)}</option>)}</select><span>仅应用到所选鼠标。导入的模板会生成独立副本。</span></label>}
              {asCopy&&<p className="text-xs text-muted-foreground">将保留原配置，为所选鼠标创建独立副本。</p>}
              <div className="mt-1 flex items-center gap-1">
                <Button size="sm" disabled={!available||needsTarget&&!target||needsProfileConnection(selected)&&!selected?.connection_key} title={available?"应用到匹配品牌与型号的鼠标":"请先连接匹配的鼠标；未适配品牌无法应用参数"} variant={active ? "secondary" : "default"} onClick={() => onActivate(p.id,target||undefined,asCopy,needsProfileConnection(selected)?selected?.connection_key:undefined)}>
                  <Play /> {!available?"未连接匹配鼠标":asCopy?"复制并应用":active ? "重新应用" : "应用配置"}
                </Button>
                <Button size="sm" variant="ghost" className="ml-auto" onClick={() => setEditing(p)} aria-label={`Edit ${p.name}`}>
                  <Pencil /> 编辑
                </Button>
                {!isDesktop && (
                  <Button size="icon-sm" variant="ghost" className="text-muted-foreground hover:text-destructive" onClick={() => onDelete(p.id)} aria-label={`Delete ${p.name}`}>
                    <Trash2 />
                  </Button>
                )}
              </div>
            </Card>
          );
        })}
      </div>

      {profiles.length===0&&<p className="py-12 text-center text-muted-foreground">此品牌暂无配置，点击上方“导入配置”选择品牌。</p>}
      <Dialog open={importChooser} onOpenChange={setImportChooser}>
        <DialogContent className="hub-import-chooser overflow-y-auto" style={{width:'calc(100vw - 32px)',maxWidth:896,maxHeight:'calc(100dvh - 48px)'}}>
          <DialogHeader><DialogTitle>导入配置</DialogTitle><DialogDescription>按品牌选择来源，再预览并导入到配置中心。</DialogDescription></DialogHeader>
          <BrandProfileImports onChoose={chooseImport}/>
          <div className="flex flex-wrap items-center justify-between gap-3 border-t pt-4">
            <span className="text-xs text-muted-foreground">已有 Open HUB 导出的备份文件？</span>
            <Button variant="outline" onClick={()=>chooseImport('openhub',[])}><Download size={15}/>恢复 Open HUB 备份</Button>
          </div>
        </DialogContent>
      </Dialog>
      <ProfileImportDialog key={importSource??'closed'} devices={devices} models={importModels} source={importSource} onClose={()=>setImportSource(null)} onDone={()=>toast.success('配置已导入，可按品牌查看并手动应用')}/>
      {atkImport&&<AtkProfileImport brand={atkImport} devices={devices} onClose={()=>setAtkImport(null)} onSave={onSave}/>}
      {sc620Preview&&<Sc620SourcePreview onClose={()=>setSc620Preview(false)}/>}
      {synapsePreview&&<SynapsePreview onClose={()=>setSynapsePreview(false)}/>}
      {mhubImport&&<MhubProfileImport onClose={()=>setMhubImport(false)} onSave={onSave}/>}
      {icueImport&&<VendorProfileImport kind="icue" onClose={()=>setIcueImport(false)} onSave={onSave}/>}
      {ggImport&&<VendorProfileImport kind="gg" onClose={()=>setGgImport(false)} onSave={onSave}/>}
      {offline&&<SinoNewProfile onClose={()=>setOffline(false)} onSave={onSave}/>}
      {multiNew&&<MultiProfileEditor profile={null} onClose={()=>setMultiNew(false)} onSave={onSave}/>}
      {editing?.mchose?<MchoseProfileEditor key={editing.id} profile={editing} onClose={()=>setEditing(null)} onSave={onSave}/>:editing?.multi?<MultiProfileEditor key={editing.id} profile={editing} onClose={()=>setEditing(null)} onSave={onSave}/>:editing?.sino?<SinoProfileEditor key={editing.id} profile={editing} onClose={()=>setEditing(null)} onSave={onSave}/>:<ProfileEditor profile={editing} onClose={() => setEditing(null)} onSave={onSave} />}
    </section>
  );
}
