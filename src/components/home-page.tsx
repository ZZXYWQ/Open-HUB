import { useState } from "react";
import { LayoutGrid, RefreshCw, Rows2 } from "lucide-react";

import { DeviceArt } from "@/components/device-art";
import { DeviceStatus, OnboardBadge } from "@/components/device-status";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import type { DeviceState, DeviceSummary } from "@/lib/api";

type Layout = "grid" | "list";

function readLayout(): Layout {
  try {
    return localStorage.getItem("layout") === "list" ? "list" : "grid";
  } catch {
    return "grid";
  }
}

interface Props {
  devices: DeviceSummary[];
  states: Record<string, DeviceState>;
  scanning: boolean;
  onOpen: (id: string) => void;
  onScan: () => void;
}

export function HomePage({ devices, states, scanning, onOpen, onScan }: Props) {
  const [layout, setLayout] = useState<Layout>(readLayout);

  const changeLayout = (value: string) => {
    if (value !== "grid" && value !== "list") return;
    setLayout(value);
    try {
      localStorage.setItem("layout", value);
    } catch {
      // Non-fatal.
    }
  };

  if (!devices.length) {
    return (
      <section className="flex flex-1 flex-col items-center justify-center gap-3 text-center">
        <h2 className="text-2xl font-extrabold tracking-tight uppercase">
          {scanning ? "Looking for devices" : "No devices detected"}
        </h2>
        <p className="max-w-sm text-muted-foreground">
          Connect a Logitech mouse by cable or receiver. Wireless mice may need a click to wake up.
        </p>
        <Button className="mt-2" onClick={onScan} disabled={scanning}>
          <RefreshCw className={scanning ? "animate-spin" : undefined} />
          Rescan
        </Button>
      </section>
    );
  }

  return (
    <section className="flex-1 overflow-y-auto px-11 pt-8 pb-12">
      <div className="flex justify-end">
        <ToggleGroup
          type="single"
          value={layout}
          onValueChange={changeLayout}
          className="rounded-full bg-card p-1"
          aria-label="Layout"
        >
          <ToggleGroupItem value="grid" aria-label="Grid view" className="rounded-full text-muted-foreground data-[state=on]:bg-transparent data-[state=on]:text-primary">
            <LayoutGrid className="size-5" />
          </ToggleGroupItem>
          <ToggleGroupItem value="list" aria-label="List view" className="rounded-full text-muted-foreground data-[state=on]:bg-transparent data-[state=on]:text-primary">
            <Rows2 className="size-5" />
          </ToggleGroupItem>
        </ToggleGroup>
      </div>

      {layout === "grid" ? (
        <div className="mt-6 flex flex-wrap justify-center gap-6">
          {devices.map((d) => {
            const s = states[d.id];
            return (
              <button key={d.id} onClick={() => onOpen(d.id)} className="rounded-[10px] text-left outline-none focus-visible:ring-3 focus-visible:ring-ring" aria-label={`Open ${d.name}`}>
                <Card className="relative h-[450px] w-[296px] gap-0 rounded-[10px] px-6 pt-8.5 pb-6 ring-0 transition-colors hover:bg-[#2a2b2f]">
                  <span className="text-2xl leading-tight font-extrabold tracking-tight uppercase">{d.name}</span>
                  <div className="mt-3.5">
                    <DeviceStatus device={d} battery={s?.battery ?? null} />
                  </div>
                  <DeviceArt kind={d.kind} className="min-h-0 flex-1 pt-1.5 pb-13 [&_svg]:max-h-60" />
                  {s?.onboard_mode === "onboard" && <OnboardBadge className="absolute right-6 bottom-6" />}
                </Card>
              </button>
            );
          })}
        </div>
      ) : (
        <div className="mx-auto mt-6 flex max-w-3xl flex-col gap-3">
          {devices.map((d) => {
            const s = states[d.id];
            return (
              <button key={d.id} onClick={() => onOpen(d.id)} className="rounded-[10px] text-left outline-none focus-visible:ring-3 focus-visible:ring-ring" aria-label={`Open ${d.name}`}>
                <Card className="grid h-26 grid-cols-[80px_1fr_auto_48px] items-center gap-5 rounded-[10px] px-5 py-0 ring-0 transition-colors hover:bg-[#2a2b2f]">
                  <DeviceArt kind={d.kind} className="h-19" />
                  <div className="flex min-w-0 flex-col">
                    <span className="truncate text-xl font-extrabold tracking-tight uppercase">{d.name}</span>
                    <span className="text-xs text-muted-foreground">{d.connection}</span>
                  </div>
                  <DeviceStatus device={d} battery={s?.battery ?? null} />
                  {s?.onboard_mode === "onboard" ? <OnboardBadge /> : <span />}
                </Card>
              </button>
            );
          })}
        </div>
      )}
    </section>
  );
}
