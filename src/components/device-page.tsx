import {MultiPanel} from '@/components/multi-panel';
import { useState } from "react";
import { Crosshair, Settings, SlidersHorizontal, type LucideIcon } from "lucide-react";
import {SinoPanel} from "@/components/sino-panel";
import {MchosePanel} from "@/components/mchose-panel";

import { DeviceArt } from "@/components/device-art";
import { DpiPanel } from "@/components/dpi-panel";
import { SettingsPanel } from "@/components/settings-panel";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { DeviceState, DeviceSummary, Profile } from "@/lib/api";
import { levelLabel, levelMatches, type Level } from "@/lib/levels";
import { cn } from "@/lib/utils";

type Tab = "dpi" | "settings" | "hardware";

interface Props {
  device: DeviceSummary;
  state: DeviceState | null;
  profile: Profile;
  onLevelsChange: (levels: Level[], activeLevel: number) => void;
  onApplyLevel: (levels: Level[], index: number) => void;
  onReportRate: (value: number) => void;
  onLod: (lod: number) => void;
  onModeChange: (host: boolean) => void;
  onProfiles?:()=>void;
}

function RailItem({ icon: Icon, label, active, onClick }: { icon: LucideIcon; label: string; active: boolean; onClick: () => void }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          aria-label={label}
          aria-pressed={active}
          onClick={onClick}
          className={cn(
            "relative size-12 rounded-lg text-muted-foreground",
            active && "bg-sidebar-accent text-primary before:absolute before:top-2.5 before:bottom-2.5 before:-left-3 before:w-0.75 before:rounded-full before:bg-primary hover:bg-sidebar-accent hover:text-primary",
          )}
        >
          <Icon className="size-6" />
        </Button>
      </TooltipTrigger>
      <TooltipContent side="right">{label}</TooltipContent>
    </Tooltip>
  );
}

export function DevicePage({ device, state, profile, onLevelsChange, onApplyLevel, onReportRate, onLod, onModeChange,onProfiles }: Props) {
  const [tab, setTab] = useState<Tab | null>(device.vendor_id===0x3837?"hardware":"dpi");
  const toggle = (t: Tab) => setTab((cur) => (cur === t ? null : t));
  const dpi = state?.dpi;
  const levels = profile.levels;
  if(device.id.startsWith("multi:"))return <MultiPanel key={device.id} id={device.id} onImported={onProfiles}/>;
  if(device.id.startsWith("sino:"))return <SinoPanel key={device.id} id={device.id} onImported={onProfiles}/>;
  if(device.vendor_id===0x3837)return <div className="mchose-studio min-h-0 flex-1"><MchosePanel id={device.id}/></div>;

  return (
    <div className="grid min-h-0 flex-1 grid-cols-[72px_auto_1fr]">
      <nav className="flex flex-col items-center gap-1 border-r border-border pt-4.5" aria-label="Device features">
        {device.vendor_id===0x3837 ? <RailItem icon={SlidersHorizontal} label="迈从控制中心" active={tab==="hardware"} onClick={()=>toggle("hardware")}/> : <RailItem icon={Crosshair} label="Sensitivity (DPI)" active={tab === "dpi"} onClick={() => toggle("dpi")} />}
        <RailItem icon={Settings} label="Device settings" active={tab === "settings"} onClick={() => toggle("settings")} />
      </nav>

      {tab==="hardware"&&device.vendor_id===0x3837 ? <MchosePanel id={device.id}/> : state && tab === "dpi" ? (
        <DpiPanel device={device} state={state} profile={profile} onLevelsChange={onLevelsChange} onApplyLevel={onApplyLevel} onReportRate={onReportRate} onLod={onLod} />
      ) : state && tab === "settings" ? (
        <SettingsPanel device={device} state={state} onModeChange={onModeChange} />
      ) : (
        <div />
      )}

      <div className="relative flex min-w-0 flex-col items-center justify-center gap-6 bg-[radial-gradient(ellipse_55%_50%_at_50%_42%,#0c1824_0%,#000_72%)]">
        {dpi && (
          <div className="absolute top-6 right-8 text-right">
            <div className="text-[40px] leading-none font-extrabold tracking-tighter tabular-nums">
              {levelLabel({ x: dpi.x, y: dpi.supports_y ? dpi.y : null })}
            </div>
            <div className="mt-1 text-xs tracking-widest text-muted-foreground uppercase">Current DPI</div>
          </div>
        )}
        <DeviceArt kind={device.kind} className="h-[50vh] max-h-[460px] min-h-[200px]" />
        {!state && <p className="text-xs tracking-widest text-muted-foreground uppercase">Reading device…</p>}
        {dpi && device.vendor_id!==0x3837 && (
          <>
            <div className="flex items-center gap-4.5">
              {levels.map((l, i) => (
                <button
                  key={i}
                  title={`Switch to ${levelLabel(l)} DPI`}
                  onClick={() => onApplyLevel(levels, i)}
                  className={cn(
                    "border-b-2 border-transparent pb-1.5 text-base leading-4 tabular-nums hover:text-primary",
                    levelMatches(l, dpi) && "border-white",
                    dpi.default === l.x && "text-level-default",
                  )}
                >
                  {levelLabel(l)}
                </button>
              ))}
            </div>
            <div className="text-xs tracking-widest text-muted-foreground uppercase">DPI levels</div>
          </>
        )}
      </div>
    </div>
  );
}

