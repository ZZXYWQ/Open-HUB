import { Cpu, Usb, Wifi } from "lucide-react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { batteryPercent, type Battery, type DeviceSummary } from "@/lib/api";
import { cn } from "@/lib/utils";

/** Upright battery glyph with a fill proportional to charge. */
export function BatteryGlyph({ battery, className }: { battery: Battery; className?: string }) {
  const pct = batteryPercent(battery);
  const fill = pct == null ? 0 : Math.max(1, Math.round((pct / 100) * 9));
  const low = pct != null && pct <= 15;
  const charging = battery.status.startsWith("charging");
  return (
    <svg viewBox="0 0 8 14" className={cn("h-3.5 w-2", className)} aria-hidden="true">
      <rect x="2.5" y="0" width="3" height="1.6" rx=".4" fill="currentColor" />
      <rect x=".8" y="1.6" width="6.4" height="11.6" rx="1.2" fill="none" stroke="currentColor" strokeWidth="1.2" />
      <rect
        x="2"
        y={12 - fill}
        width="4"
        height={fill}
        rx=".5"
        className={low ? "fill-destructive" : charging ? "fill-success" : "fill-current"}
      />
    </svg>
  );
}

/** "26% [battery] [link]" line used on the device cards and rows. */
export function DeviceStatus({ device, battery }: { device: DeviceSummary; battery: Battery | null }) {
  const direct = device.connection.startsWith("Direct");
  const LinkIcon = direct ? Usb : Wifi;
  return (
    <div className="flex items-center gap-2 text-[15px] font-bold text-muted-foreground">
      {battery && (
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="flex items-center gap-1.5">
              {battery.percent != null && <span>{battery.percent}%</span>}
              <BatteryGlyph battery={battery} />
            </span>
          </TooltipTrigger>
          <TooltipContent>Battery {battery.status}</TooltipContent>
        </Tooltip>
      )}
      <Tooltip>
        <TooltipTrigger asChild>
          <LinkIcon className="size-4" aria-label={device.connection} />
        </TooltipTrigger>
        <TooltipContent>{device.connection}</TooltipContent>
      </Tooltip>
    </div>
  );
}

/** Corner badge shown when the device runs from on-board memory, as on G HUB's cards. */
export function OnboardBadge({ className }: { className?: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className={cn("grid size-12 place-items-center rounded-md border border-[#3a3b3f] text-foreground", className)}>
          <Cpu className="size-6" aria-label="On-board memory mode" />
        </span>
      </TooltipTrigger>
      <TooltipContent>On-board memory mode</TooltipContent>
    </Tooltip>
  );
}
