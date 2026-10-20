import { ArrowLeft, MonitorCog, Mouse, RefreshCw } from "lucide-react";

import { AppSettingsDialog } from "@/components/app-settings-dialog";
import { BatteryGlyph } from "@/components/device-status";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { DeviceState, DeviceSummary, ProfilesView, UpdateInfo, UpdateProgress } from "@/lib/api";
import { cn } from "@/lib/utils";

export type Section = "devices" | "profiles";

interface Props {
  section: Section;
  onSection: (s: Section) => void;
  profiles: ProfilesView | null;
  onActivateProfile: (id: string) => void;
  device: DeviceSummary | null;
  state: DeviceState | null;
  scanning: boolean;
  onScan: () => void;
  onBack: () => void;
  onModeChange: (host: boolean) => void;
  updates: {
    update: UpdateInfo | null;
    onUpdateFound: (info: UpdateInfo | null) => void;
    progress: UpdateProgress | null;
    installing: boolean;
    onInstall: () => void;
  };
}

const headerClass = "grid h-20 shrink-0 grid-cols-[1fr_auto_1fr] items-center border-b border-border px-6";

const tabClass =
  "relative flex items-center text-[15px] font-semibold hover:text-primary data-[active=true]:text-primary data-[active=true]:after:absolute data-[active=true]:after:-bottom-px data-[active=true]:after:left-1/2 data-[active=true]:after:h-0.5 data-[active=true]:after:w-9 data-[active=true]:after:-translate-x-1/2 data-[active=true]:after:bg-muted-foreground";

/** G HUB-style "Desktop: Default" pill: shows and switches the active profile. */
function ProfilePicker({ profiles, onActivate }: { profiles: ProfilesView; onActivate: (id: string) => void }) {
  return (
    <Select value={profiles.active_id} onValueChange={onActivate}>
      <SelectTrigger className="h-[42px] max-w-72 gap-2.5 border-0 bg-card px-4 text-sm font-bold dark:bg-card dark:hover:bg-secondary" aria-label="Active profile">
        <MonitorCog className="size-5 text-foreground" />
        <span className="text-muted-foreground">Profile:</span>
        <SelectValue />
      </SelectTrigger>
      <SelectContent position="popper" align="end" className="max-h-80">
        {profiles.profiles.map((p) => (
          <SelectItem key={p.id} value={p.id}>
            {p.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

export function AppHeader({ section, onSection, profiles, onActivateProfile, device, state, scanning, onScan, onBack, onModeChange, updates }: Props) {
  if (!device) {
    return (
      <header className={headerClass}>
        <div className="flex h-full items-center">
          <Mouse className="ml-2.5 size-7" aria-label="Open GHub" />
          <nav className="ml-11 flex h-full gap-9" aria-label="Sections">
            {(["devices", "profiles"] as const).map((s) => (
              <button
                key={s}
                data-active={section === s}
                aria-current={section === s ? "page" : undefined}
                onClick={() => onSection(s)}
                className={tabClass}
              >
                {s === "devices" ? "Devices" : "Profiles"}
              </button>
            ))}
          </nav>
        </div>
        <div />
        <div className="flex items-center justify-end gap-2">
          {profiles && <ProfilePicker profiles={profiles} onActivate={onActivateProfile} />}
          <AppSettingsDialog {...updates} />
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon-lg" className="size-[42px] rounded-full text-muted-foreground" onClick={onScan} disabled={scanning} aria-label="Rescan devices">
                <RefreshCw className={cn("size-5", scanning && "animate-spin")} />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Rescan devices</TooltipContent>
          </Tooltip>
        </div>
      </header>
    );
  }

  const battery = state?.battery;
  return (
    <header className={headerClass}>
      <div className="flex min-w-0 items-center gap-3">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="icon-lg" className="rounded-full text-muted-foreground" onClick={onBack} aria-label="Back to devices">
              <ArrowLeft className="size-5" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Back to devices</TooltipContent>
        </Tooltip>
        <div className="flex min-w-0 flex-col">
          <span className="truncate text-xl font-extrabold tracking-tight uppercase">{device.name}</span>
          <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
            {battery && (
              <>
                <BatteryGlyph battery={battery} className="h-3 w-[7px]" />
                {battery.percent != null ? `${battery.percent}%` : battery.level}
                <span aria-hidden="true">·</span>
              </>
            )}
            {device.connection}
          </span>
        </div>
      </div>

      <div>
        {state?.onboard_mode && (
          <Select value={state.onboard_mode} onValueChange={(v) => onModeChange(v === "host")}>
            <SelectTrigger className="h-[42px] gap-2 border-0 bg-card px-4 text-sm font-bold dark:bg-card dark:hover:bg-secondary" aria-label="Settings storage">
              <span className="text-muted-foreground">Mode:</span>
              <SelectValue />
            </SelectTrigger>
            <SelectContent position="popper" align="center">
              <SelectItem value="onboard">On-board memory</SelectItem>
              <SelectItem value="host">Software control</SelectItem>
            </SelectContent>
          </Select>
        )}
      </div>

      <div className="flex justify-end">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="icon-lg" className="rounded-full text-muted-foreground" onClick={onScan} aria-label="Rescan devices">
              <RefreshCw className={cn("size-5", scanning && "animate-spin")} />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Rescan devices</TooltipContent>
        </Tooltip>
      </div>
    </header>
  );
}
