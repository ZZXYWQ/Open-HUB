import {OnboardMacroComposer} from '@/components/onboard-macro-composer';
import type {NativeMacroRequest} from '@/lib/logitech-macro-plan';
import {OnboardBindingsEditor} from "@/components/onboard-bindings-editor";
import { useState,useEffect,useRef } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Download, HardDrive, Pencil, RefreshCw, Upload } from "lucide-react";
import { toast } from "sonner";

import { OnboardSlotEditor } from "@/components/onboard-slot-editor";
import { PanelSection } from "@/components/panel";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { api, type DeviceSummary, type OnboardButton, type OnboardProfile, type OnboardSnapshot, type SlotSettings,type OnboardBindingSettings } from "@/lib/api";
import { cn } from "@/lib/utils";

/** Buttons that do something other than their own mouse button. */
function remapped(buttons: OnboardButton[]): OnboardButton[] {
  return buttons.filter((b, i) => b.description !== `Mouse button ${i + 1}`);
}

function ProfileCard({ profile, active, onEdit,onEditBindings,onNewMacro }: { profile: OnboardProfile; active: boolean; onEdit?: () => void;onEditBindings?:()=>void;onNewMacro?:()=>void }) {
  const changed = remapped(profile.buttons);
  const gshift = profile.gshift_buttons.filter((b) => b.kind !== "disabled");
  return (
    <div className={cn("rounded-lg bg-secondary/60 px-3.5 py-3 text-[13px]", !profile.enabled && "opacity-60")}>
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-semibold">
          {profile.rom ? "Factory default" : `Slot ${profile.slot}`}
          {profile.name ? ` · ${profile.name}` : ""}
        </span>
        {active && <Badge>Active</Badge>}
        {!profile.enabled && <Badge variant="secondary">Disabled</Badge>}
        {profile.crc_valid === false && <Badge variant="destructive">Checksum error</Badge>}
        <span className="ml-auto text-xs text-muted-foreground tabular-nums">{profile.report_rate_hz} Hz</span>
        {onNewMacro&&<Button variant="outline" size="sm" onClick={onNewMacro}>新增板载宏</Button>}
        {onEditBindings&&<Button variant="outline" size="sm" onClick={onEditBindings}>编辑按键</Button>}
        {onEdit && (
          <Button variant="ghost" size="icon-sm" aria-label={`Edit slot ${profile.slot}`} onClick={onEdit}>
            <Pencil />
          </Button>
        )}
      </div>
      <div className="mt-2 flex flex-wrap gap-1.5">
        {profile.dpi_levels.map((dpi, i) => (
          <span
            key={i}
            title={i === profile.default_dpi_index ? "Starting DPI" : undefined}
            className={cn(
              "rounded-md px-2 py-0.5 text-xs font-semibold tabular-nums",
              i === profile.default_dpi_index ? "bg-primary text-primary-foreground" : "bg-background/60 text-muted-foreground",
            )}
          >
            {dpi}
          </span>
        ))}
      </div>
      <div className="mt-2 text-xs text-muted-foreground">
        {changed.length === 0 ? (
          "Buttons: standard"
        ) : (
          <>Buttons: {changed.map((b) => `${profile.buttons.indexOf(b) + 1} → ${b.description}`).join(", ")}</>
        )}
        {gshift.length > 0 && <> · G-Shift: {gshift.length} assigned</>}
      </div>
    </div>
  );
}

/** Profiles stored in the mouse: view, edit DPI/report rate per slot, back up and restore. */
export function OnboardSection({ device, supportedDpi }: { device: DeviceSummary; supportedDpi: number[] }) {
  const readGeneration=useRef(0);
  useEffect(()=>{readGeneration.current++;setSnapshot(null);setEditing(null);setEditingBindings(null);setComposing(null);setRestorePath(null);},[device.id]);
  const [snapshot, setSnapshot] = useState<OnboardSnapshot | null>(null);
  const [reading, setReading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [editing, setEditing] = useState<OnboardProfile | null>(null);
  const [editingBindings,setEditingBindings]=useState<OnboardProfile|null>(null);
  const [composing,setComposing]=useState<OnboardProfile|null>(null);
  const [restorePath, setRestorePath] = useState<string | null>(null);
  const [restoring, setRestoring] = useState(false);

  const writeMacro=async(request:NativeMacroRequest)=>{
    try{const result=await api.writeOnboardMacro(device.id,request);setSnapshot(result.snapshot);const note=[result.report.reload_note,result.refresh_note].filter(Boolean).join('；');if(note)toast.warning('板载宏已保存并校验',{description:note});else toast.success('板载宏已保存到此鼠标',{description:`宏与按键绑定均已读回校验。备份：${result.backup_path}`});return true;}catch(e){toast.error(String(e));return false;}
  };
  const writeBindings=async(settings:OnboardBindingSettings)=>{
    try{const result=await api.writeOnboardBindings(device.id,settings);setSnapshot(result.snapshot);
      const note=[result.report.reload_note,result.refresh_note].filter(Boolean).join('；');
      if(note)toast.warning('按键已保存并通过读回校验',{description:note});else toast.success(result.report.unchanged?'按键配置已是所选设置':'按键已保存到此鼠标',{description:`已读回校验。备份：${result.backup_path}`});return true;
    }catch(e){toast.error(String(e));return false;}
  };
  const writeSlot = async (settings: SlotSettings) => {
    try {
      const result = await api.writeOnboardSlot(device.id, settings);
      setSnapshot(result.snapshot);
      if (result.report.unchanged) {
        toast.info(`Slot ${settings.slot} already has these settings`);
      } else {
        toast.success(`Slot ${settings.slot} saved to the mouse`, {
          description: `Verified by reading it back. Backup: ${result.backup_path}`,
        });
      }
      return true;
    } catch (e) {
      toast.error(String(e));
      return false;
    }
  };

  const pickRestore = async () => {
    const path = await open({
      title: "Restore on-board memory from backup",
      multiple: false,
      directory: false,
      filters: [{ name: "Backup", extensions: ["json"] }],
    });
    if (typeof path === "string") setRestorePath(path);
  };

  const restore = async () => {
    if (!restorePath||!snapshot?.device_key) return;
    setRestoring(true);
    try {
      const result = await api.restoreOnboardBackup(device.id, restorePath,snapshot.device_key);
      setSnapshot(result.snapshot);
      toast.success(
        result.sectors_written === 0 ? "The mouse already matches this backup" : `Restored ${result.sectors_written} sector(s) from the backup`,
        { description: `The previous state was backed up to ${result.backup_path}` },
      );
      setRestorePath(null);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setRestoring(false);
    }
  };

  const read = async () => {
    setReading(true);
    const generation=++readGeneration.current;
    try {
      const next=await api.onboardSnapshot(device.id);
      if(generation===readGeneration.current)setSnapshot(next);
    } catch (e) {
      toast.error(`Could not read on-board memory: ${e}`);
    } finally {
      setReading(false);
    }
  };

  const backup = async () => {
    const date = new Date().toISOString().slice(0, 10);
    const path = await save({
      title: "Save on-board memory backup",
      defaultPath: `open-hub-onboard-${device.name.replace(/[^\w-]+/g, "_")}-${date}.json`,
      filters: [{ name: "Backup", extensions: ["json"] }],
    });
    if (!path) return;
    setSaving(true);
    try {
      await api.saveOnboardBackup(device.id, path);
      toast.success("On-board memory backed up");
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  };

  const info = snapshot?.info;
  return (
    <PanelSection
      title="On-board memory"
      help="Profiles stored inside the mouse, used in on-board memory mode even with Open HUB closed. Reading never changes the mouse; every save is backed up first."
    >
      <div className="flex flex-wrap gap-2">
        <Button variant="secondary" onClick={read} disabled={reading}>
          <RefreshCw className={cn(reading && "animate-spin")} /> {snapshot ? "Read again" : "Read from mouse"}
        </Button>
        <Button variant="ghost" onClick={backup} disabled={saving}>
          <Download /> {saving ? "Saving…" : "Save backup…"}
        </Button>
        <Button variant="ghost" onClick={pickRestore} disabled={restoring||!snapshot?.device_key||!snapshot?.info.bindings_writable}>
          <Upload /> Restore from backup…
        </Button>
      </div>

      {snapshot && info && (
        <div className="mt-4 grid gap-2">
          <p className="flex items-center gap-2 text-xs text-muted-foreground">
            <HardDrive className="size-3.5" />
            {info.profile_count} profile slots · {info.button_count} buttons · format {info.profile_format} ·{" "}
            {info.sector_count} sectors of {info.sector_size} bytes ·{" "}
            {snapshot.active_profile === 0 ? "none active (software mode)" : `slot ${snapshot.active_profile} active`}
          </p>
          {!snapshot.directory_crc_valid && (
            <p className="text-xs text-destructive">The profile directory failed its checksum; the mouse is using its factory profile.</p>
          )}
          {info.bindings_writable===false&&<p className="text-xs text-muted-foreground">此连接目前仅开放板载读取；按键、DPI 及备份恢复需要经过身份核对的有线 USB 连接。</p>}
          {snapshot.profiles.map((p) => (
            <ProfileCard
              key={`u${p.slot}`}
              profile={p}
              active={snapshot.active_profile === p.slot}
              onEdit={p.crc_valid&&info.bindings_writable ? () => setEditing(p) : undefined}
              onNewMacro={p.crc_valid&&info.bindings_writable&&snapshot.device_key.startsWith('unit:')&&info.memory_model===1&&info.profile_format>=1&&info.profile_format<=5&&info.macro_format===1&&[255,256].includes(info.sector_size)&&info.sector_count>0&&info.sector_count<=32&&info.button_count>0&&info.button_count<=16?()=>setComposing(p):undefined}
              onEditBindings={p.crc_valid&&info.bindings_writable&&info.memory_model===1&&info.profile_format>=1&&info.profile_format<=5&&info.macro_format===1&&info.button_count>0&&info.button_count<=16?()=>setEditingBindings(p):undefined}
            />
          ))}
          {snapshot.rom_profiles.map((p) => (
            <ProfileCard key={`r${p.slot}`} profile={p} active={false} />
          ))}
        </div>
      )}

      {composing&&snapshot&&info&&<OnboardMacroComposer profile={composing} info={info} deviceKey={snapshot.device_key} deviceId={device.id} onClose={()=>setComposing(null)} onSave={writeMacro}/>}
      <OnboardBindingsEditor device={device} deviceKey={snapshot?.device_key??''} profile={editingBindings} info={snapshot?.info??null} onClose={()=>setEditingBindings(null)} onSave={writeBindings}/>
      <OnboardSlotEditor profile={editing} deviceKey={snapshot?.device_key??''} supportedDpi={supportedDpi} onClose={() => setEditing(null)} onSave={writeSlot} />

      <Dialog open={restorePath !== null} onOpenChange={(o) => !o && setRestorePath(null)}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>Restore on-board memory?</DialogTitle>
            <DialogDescription>
              The mouse's profile directory and user profiles will be replaced with the ones in this backup. The factory
              profile is never touched, and the current memory is backed up first.
            </DialogDescription>
          </DialogHeader>
          <p className="font-mono text-xs break-all text-muted-foreground">{restorePath}</p>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setRestorePath(null)}>
              Cancel
            </Button>
            <Button onClick={restore} disabled={restoring}>
              <Upload /> {restoring ? "Restoring…" : "Restore"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </PanelSection>
  );
}
