import { OnboardSection } from "@/components/onboard-section";
import { PanelSection, PanelShell, StateErrors } from "@/components/panel";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import type { DeviceState, DeviceSummary } from "@/lib/api";

interface Props {
  device: DeviceSummary;
  state: DeviceState;
  onModeChange: (host: boolean) => void;
}

export function SettingsPanel({ device, state, onModeChange }: Props) {
  const b = state.battery;
  const info: [string, string][] = [
    ["Name", device.name],
    ...(device.firmware_name && device.firmware_name !== device.name
      ? ([["Firmware name", device.firmware_name]] as [string, string][])
      : []),
    ["Type", device.kind],
    ["Connection", device.connection],
    ["Product ID", device.model_ids.length ? device.model_ids.join(", ") : device.product_id],
    ...(device.model_ids.length ? ([["Receiver ID", device.product_id]] as [string, string][]) : []),
    ["Protocol", `HID++ ${device.protocol}`],
  ];
  if (b) {
    const parts = [b.percent != null ? `${b.percent}%` : b.level, b.voltage_mv ? `${b.voltage_mv} mV` : null, b.status];
    info.push(["Battery", parts.filter(Boolean).join(" · ")]);
  }

  return (
    <PanelShell title="Settings">
      <StateErrors errors={state.errors} />

      {state.onboard_mode && (
        <PanelSection>
          <div className="flex items-center justify-between gap-4 rounded-lg bg-card px-4 py-3.5">
            <Label htmlFor="onboard" className="flex-col items-start gap-0.5">
              <span className="text-sm font-semibold">On-board memory mode</span>
              <span className="text-[12.5px] font-normal text-muted-foreground">
                On: the mouse runs the profile stored on it. Off: Open GHub controls it live, and settings reset when it loses power.
              </span>
            </Label>
            <Switch id="onboard" checked={state.onboard_mode === "onboard"} onCheckedChange={(on) => onModeChange(!on)} />
          </div>
        </PanelSection>
      )}

      {device.features.some((f) => f.id === 0x8100) && <OnboardSection device={device} />}

      <PanelSection title="Device info">
        <dl className="grid grid-cols-[140px_1fr] gap-x-4 gap-y-2 text-[13px]">
          {info.map(([k, v]) => (
            <div key={k} className="contents">
              <dt className="text-muted-foreground">{k}</dt>
              <dd className="tabular-nums select-text">{v}</dd>
            </div>
          ))}
        </dl>
      </PanelSection>

      <PanelSection title={`HID++ features (${device.features.length})`}>
        <Table className="text-[12.5px]">
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className="text-[11px] tracking-widest text-muted-foreground uppercase">ID</TableHead>
              <TableHead className="text-[11px] tracking-widest text-muted-foreground uppercase">Feature</TableHead>
              <TableHead className="text-[11px] tracking-widest text-muted-foreground uppercase">Index</TableHead>
              <TableHead className="text-[11px] tracking-widest text-muted-foreground uppercase">Ver</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {device.features.map((f) => (
              <TableRow key={f.id}>
                <TableCell className="font-mono">0x{f.id.toString(16).padStart(4, "0").toUpperCase()}</TableCell>
                <TableCell>{f.name || "—"}</TableCell>
                <TableCell className="font-mono">{f.index}</TableCell>
                <TableCell className="font-mono">{f.version}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </PanelSection>
    </PanelShell>
  );
}
