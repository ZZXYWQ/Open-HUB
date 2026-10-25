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
import {brandName,profileVendor,matchesDevice} from "@/lib/brands";
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
  onActivate: (id: string) => void;
  onSave: (profile: Profile) => Promise<boolean>;
  onDelete: (id: string) => void;
  onImport: () => void;
  onMhubImport:(path?:string)=>Promise<void>;
}

export function ProfilesPage({ view, devices, importing, onAutoSwitch, onActivate, onSave, onDelete, }: Props) {
  const [offline,setOffline]=useState(false);
  const [multiNew,setMultiNew]=useState(false);
  const [synapsePreview,setSynapsePreview]=useState(false);
  const [icueImport,setIcueImport]=useState(false);
  const [ggImport,setGgImport]=useState(false);
  const [editing, setEditing] = useState<Profile | null>(null);
  const brandKey=(p:Profile)=>p.sino?'sino:'+p.sino.model.brand:p.multi&&compxModel(p.multi.vendor,p.multi.product)?'compx:'+compxModel(p.multi.vendor,p.multi.product)!.brand:String(profileVendor(p));
  const brandLabel=(v:string)=>v.startsWith('sino:')?v.slice(5):v.startsWith('compx:')?compxBrand(v.slice(6)):brandName(v==='null'?null:Number(v));
  const [importChooser,setImportChooser]=useState(false);
  const chooseImport=(source:string,models:SinoModel[])=>{setImportChooser(false);setImportModels(models);if(source==='synapse')setSynapsePreview(true);else if(source==='icue')setIcueImport(true);else if(source==='gg')setGgImport(true);else if(source==='atk'||source==='vxe'||source==='vgn'||source==='hyperx'||source==='pulsar')setAtkImport(source);else setImportSource(source);};
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
            统一管理品牌、型号和配置来源。先预览再导入，配置仅应用到匹配的设备。
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
          const active = p.id === view.active_id;
          const isDesktop = p.id === DESKTOP_ID;
          const available=devices.some(d=>matchesDevice(p,d));
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

              <div className="flex flex-wrap gap-1.5">
                <Badge variant="outline">{p.sino?.model.brand??brandName(profileVendor(p),p.target_model??p.multi?.product)}</Badge>
                {p.sino&&<Badge variant="outline">完整按键 / {p.sino.macros.length} 个已记录宏 / {p.sino.model.firmware}</Badge>}
                {p.mchose_board!=null&&<Badge>迈从板载 {p.mchose_board+1}</Badge>}
                {p.levels.map((l, i) => (
                  <span
                    key={i}
                    className={cn(
                      "rounded-md px-2 py-0.5 text-xs font-semibold tabular-nums",
                      i === p.active_level ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground",
                    )}
                  >
                    {l.x}
                  </span>
                ))}
                {p.report_rate_hz != null && (
                  <span className="rounded-md border border-input px-2 py-0.5 text-xs text-muted-foreground tabular-nums">{p.report_rate_hz} Hz</span>
                )}
              </div>

              <div className="mt-1 flex items-center gap-1">
                <Button size="sm" disabled={!available} title={available?"应用到匹配品牌与型号的鼠标":"请先连接匹配的鼠标；未适配品牌无法应用参数"} variant={active ? "secondary" : "default"} onClick={() => onActivate(p.id)}>
                  <Play /> {!available?"未连接匹配鼠标":active ? "重新应用" : "应用配置"}
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
      {synapsePreview&&<SynapsePreview onClose={()=>setSynapsePreview(false)}/>}
      {icueImport&&<VendorProfileImport kind="icue" onClose={()=>setIcueImport(false)} onSave={onSave}/>}
      {ggImport&&<VendorProfileImport kind="gg" onClose={()=>setGgImport(false)} onSave={onSave}/>}
      {offline&&<SinoNewProfile onClose={()=>setOffline(false)} onSave={onSave}/>}
      {multiNew&&<MultiProfileEditor profile={null} onClose={()=>setMultiNew(false)} onSave={onSave}/>}
      {editing?.multi?<MultiProfileEditor key={editing.id} profile={editing} onClose={()=>setEditing(null)} onSave={onSave}/>:editing?.sino?<SinoProfileEditor key={editing.id} profile={editing} onClose={()=>setEditing(null)} onSave={onSave}/>:<ProfileEditor profile={editing} onClose={() => setEditing(null)} onSave={onSave} />}
    </section>
  );
}
