import { useEffect, useRef, useState } from "react";
import { LayoutGrid, RefreshCw, Rows2, Mouse, Sparkles } from "lucide-react";

import { DeviceArt } from "@/components/device-art";
import { DeviceStatus, OnboardBadge } from "@/components/device-status";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import {api,type RgbDevice,type DeviceState,type DeviceSummary} from "@/lib/api";
import {rgbBrand} from "@/lib/lighting-targets";

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
  onLighting:(key:string)=>void;
}

export function HomePage({ devices, states, scanning, onOpen, onScan,onLighting }: Props) {
  const [rgbMice,setRgbMice]=useState<RgbDevice[]>([]),[rgbBusy,setRgbBusy]=useState(false),[rgbScanned,setRgbScanned]=useState(false),[rgbError,setRgbError]=useState('');
  const rgbGeneration=useRef(0),rgbPending=useRef(false);
  useEffect(()=>()=>{rgbGeneration.current++;},[]);
  const scanRgb=async()=>{
    if(rgbPending.current)return;
    const generation=rgbGeneration.current;
    rgbPending.current=true;setRgbBusy(true);setRgbError('');
    try{const found=await api.rgbDevices();if(generation===rgbGeneration.current){setRgbMice(found.filter(d=>d.kind===6));setRgbScanned(true);}}
    catch(e){if(generation===rgbGeneration.current){setRgbError(String(e));setRgbScanned(true);}}
    finally{rgbPending.current=false;if(generation===rgbGeneration.current)setRgbBusy(false);}
  };
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

  if (!devices.length&&!rgbMice.length) {
    return (
      <section className="flex flex-1 flex-col items-center justify-center gap-3 text-center">
        <h2 className="text-2xl font-extrabold tracking-tight uppercase">
          {scanning||rgbBusy ? "正在查找设备" : "未检测到鼠标"}
        </h2>
        <p className="max-w-sm text-muted-foreground">
          连接支持的鼠标后可编辑参数；仅控制灯光的设备可单独扫描。{rgbError&&<span className="mt-2 block text-xs">灯光接口：{rgbError}</span>}
        </p>
        <Button className="mt-2" onClick={onScan} disabled={scanning}>
          <RefreshCw className={scanning ? "animate-spin" : undefined} />
          重新检测
        </Button>
        <Button variant="outline" disabled={rgbBusy} onClick={()=>void scanRgb()}><Sparkles size={16}/>{rgbBusy?'正在扫描灯光…':'扫描灯光鼠标'}</Button>
      </section>
    );
  }

  return (
    <section className="flex-1 overflow-y-auto px-11 pt-8 pb-12"><div className="mx-auto max-w-6xl"><h1 className="text-2xl font-extrabold">设备中心</h1><p className="mt-2 text-sm text-muted-foreground">按设备实际能力显示参数控制和灯光入口。</p></div>
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
                  <DeviceArt kind={d.kind} vendorId={d.vendor_id} productId={d.product_id} name={d.name} className="min-h-0 flex-1 pt-1.5 pb-13 [&_svg]:max-h-60" />
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
                  <DeviceArt kind={d.kind} vendorId={d.vendor_id} productId={d.product_id} name={d.name} className="h-19" />
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
      <div className="mx-auto mt-8 max-w-6xl"><div className="flex items-center gap-2"><Sparkles size={18}/><h2 className="font-semibold">鼠标灯光设备</h2><Button variant="outline" size="sm" className="ml-auto" disabled={rgbBusy} onClick={()=>void scanRgb()}><RefreshCw size={14}/>{rgbBusy?'正在扫描灯光…':'扫描灯光鼠标'}</Button></div><p className="mt-2 text-xs text-muted-foreground">按需扫描或进入“灯光同步”后才检测灯光设备。此处只控制灯光，参数设置以原生适配为准。</p>{rgbBusy&&<p className="light-help">正在识别灯光接口…</p>}{rgbError&&<p className="light-help">{rgbError}</p>}<div className="hub-device-grid">{rgbMice.map(d=><button key={d.key} className="hub-device-card" disabled={!d.direct||!d.leds} onClick={()=>onLighting(d.key)}><Mouse size={36}/><div className="hub-device-copy"><small>{rgbBrand(d)}</small><b>{d.name}</b><small>{d.direct&&d.leds?'打开灯光控制':'该型号无可用的实时灯光接口'}</small></div></button>)}</div>{rgbScanned&&!rgbBusy&&!rgbMice.length&&!rgbError&&<p className="mt-3 text-xs text-muted-foreground">未发现其他鼠标灯光接口。底座与电脑 RGB 可从顶部“灯光同步”进入。</p>}</div>
    </section>
  );
}
