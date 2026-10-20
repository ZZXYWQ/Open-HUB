import { useEffect, useState } from "react";
import { Lock, LockOpen, Plus, Trash2 } from "lucide-react";

import { PanelShell, PanelSection, StateErrors } from "@/components/panel";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Slider } from "@/components/ui/slider";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { DeviceState, DeviceSummary, Dpi, Profile } from "@/lib/api";
import { levelMatches, MAX_LEVELS, nearest, supportedValues, type Level } from "@/lib/levels";
import { cn } from "@/lib/utils";

const LOD_LABELS = ["Low", "Medium", "High"];

interface Props {
  device: DeviceSummary;
  state: DeviceState;
  /** The active profile, whose DPI levels this panel edits. */
  profile: Profile;
  /** Saves edited levels (and the active level) into the profile without touching the device. */
  onLevelsChange: (levels: Level[], activeLevel: number) => void;
  /** Saves the levels, makes `index` the active level, and applies it to the device. */
  onApplyLevel: (levels: Level[], index: number) => void;
  onReportRate: (value: number) => void;
  onLod: (lod: number) => void;
}

/** Number field that accepts free typing and snaps to a supported DPI on commit. */
function DpiInput({ value, supported, onCommit, label }: { value: number; supported: number[]; onCommit: (v: number) => void; label: string }) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const v = nearest(supported, Number(text) || supported[0]);
    setText(String(v));
    if (v !== value) onCommit(v);
  };
  return (
    <Input
      type="number"
      inputMode="numeric"
      aria-label={label}
      value={text}
      min={supported[0]}
      max={supported[supported.length - 1]}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      className="h-10 text-[15px] font-semibold tabular-nums"
    />
  );
}

export function DpiPanel(props: Props) {
  const { state } = props;
  const dpi = state.dpi;
  if (!dpi) {
    return (
      <PanelShell title="Sensitivity (DPI)">
        <PanelSection>
          <p className="text-sm text-muted-foreground">This device does not expose an adjustable DPI feature.</p>
        </PanelSection>
        <StateErrors errors={state.errors} />
      </PanelShell>
    );
  }
  return (
    <DpiEditor key={`${props.device.id}:${props.profile.id}`} {...props} dpi={dpi} />
  );
}

function DpiEditor({ state, dpi, profile, onLevelsChange, onApplyLevel, onReportRate, onLod }: Props & { dpi: Dpi }) {
  const supported = supportedValues(dpi);
  // Local copy so sliders move smoothly; committed edits are saved to the profile.
  const [levels, setLevels] = useState<Level[]>(profile.levels);
  const [selected, setSelected] = useState(profile.active_level);
  const [advanced, setAdvanced] = useState(false);
  const showAdvanced = advanced && dpi.supports_y;

  // Pick up profile changes made elsewhere (auto-switch, profile editor).
  const profileLevels = JSON.stringify(profile.levels);
  useEffect(() => {
    setLevels(profile.levels);
    setSelected(profile.active_level);
  }, [profileLevels, profile.active_level]);

  // Follow on-mouse DPI button presses picked up by polling.
  useEffect(() => {
    const idx = levels.findIndex((l) => levelMatches(l, dpi));
    if (idx >= 0) setSelected(idx);
  }, [dpi.x, dpi.y]);

  /** Saves levels without applying anything to the device. */
  const update = (next: Level[], active = Math.min(selected, next.length - 1)) => {
    setLevels(next);
    setSelected(active);
    onLevelsChange(next, active);
  };
  /** Saves levels and applies level `idx` to the device. */
  const select = (idx: number, next = levels) => {
    setLevels(next);
    setSelected(idx);
    onApplyLevel(next, idx);
  };
  const withLevel = (idx: number, patch: Partial<Level>) => levels.map((l, i) => (i === idx ? { ...l, ...patch } : l));
  const commitLevel = (idx: number, patch: Partial<Level>) => select(idx, withLevel(idx, patch));
  const indexOf = (v: number) => Math.max(0, supported.indexOf(nearest(supported, v)));

  const addLevel = () => {
    const last = levels[levels.length - 1]?.x ?? 800;
    const next = [...levels, { x: nearest(supported, Math.min(supported[supported.length - 1], last * 2)), y: null }];
    update(next, next.length - 1);
  };
  const deleteLevel = () => update(levels.filter((_, i) => i !== selected));

  return (
    <PanelShell
      title="Sensitivity (DPI)"
      aside={`Profile: ${profile.name} · ${supported[0]}–${supported[supported.length - 1]}${dpi.default ? ` · default ${dpi.default}` : ""}`}
      toolbar={
        <>
          <Button variant="ghost" size="lg" className="font-bold tracking-wide uppercase" onClick={addLevel} disabled={levels.length >= MAX_LEVELS}>
            <Plus /> Add level
          </Button>
          <Button variant="ghost" size="lg" className="font-bold tracking-wide uppercase" onClick={deleteLevel} disabled={levels.length <= 1 || selected < 0}>
            <Trash2 /> Delete
          </Button>
          {dpi.supports_y && (
            <Label className="ml-auto gap-2 text-[13px] font-normal">
              <Checkbox checked={advanced} onCheckedChange={(v) => setAdvanced(v === true)} />
              Advanced
            </Label>
          )}
        </>
      }
    >
      <StateErrors errors={state.errors} />

      <div className="flex flex-col gap-2 px-3.5 pt-3">
        <div className={cn("grid h-10 items-center gap-3.5 px-3 text-[11px] font-bold tracking-widest text-muted-foreground uppercase",
          showAdvanced ? "grid-cols-[72px_1fr_28px_1fr]" : "grid-cols-[72px_1fr_96px]")}>
          <span>DPI slot</span>
          {showAdvanced ? (
            <>
              <span>X-axis</span>
              <span />
              <span>Y-axis</span>
            </>
          ) : (
            <>
              <span>DPI slider</span>
              <span>DPI value</span>
            </>
          )}
        </div>

        {levels.map((l, idx) => {
          const current = levelMatches(l, dpi);
          const locked = l.y == null;
          return (
            <div
              key={idx}
              role="button"
              tabIndex={0}
              aria-label={`DPI level ${idx + 1}: ${l.x}`}
              onClick={() => select(idx)}
              onKeyDown={(e) => e.key === "Enter" && e.target === e.currentTarget && select(idx)}
              className={cn(
                "grid h-16 cursor-pointer items-center gap-3.5 rounded-lg px-3 outline-none hover:bg-white/10 focus-visible:ring-3 focus-visible:ring-ring/50",
                showAdvanced ? "grid-cols-[72px_1fr_28px_1fr]" : "grid-cols-[72px_1fr_96px]",
                idx === selected && "bg-row-active hover:bg-row-active",
              )}
            >
              <div className="flex items-center gap-3">
                <span className={cn("h-11 w-1 rounded-full", current ? "bg-white" : "bg-[#393939]")} />
                <span className="w-4.5 text-center font-bold tabular-nums">{idx + 1}</span>
                {dpi.default === l.x && (
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <span className="size-2.5 rotate-45 rounded-[1px] bg-level-default" aria-label="Device default DPI" />
                    </TooltipTrigger>
                    <TooltipContent>Device default DPI</TooltipContent>
                  </Tooltip>
                )}
              </div>

              {showAdvanced ? (
                <>
                  <DpiInput label={`Level ${idx + 1} X DPI`} value={l.x} supported={supported} onCommit={(x) => commitLevel(idx, { x })} />
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        className={locked ? "text-primary" : "text-muted-foreground"}
                        aria-label={locked ? "Unlock Y from X" : "Lock Y to X"}
                        onClick={(e) => {
                          e.stopPropagation();
                          update(withLevel(idx, { y: locked ? l.x : null }));
                        }}
                      >
                        {locked ? <Lock /> : <LockOpen />}
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>{locked ? "X and Y locked together" : "Separate X and Y"}</TooltipContent>
                  </Tooltip>
                  {locked ? (
                    <span className="text-xs tracking-wider text-muted-foreground uppercase">Same as X</span>
                  ) : (
                    <DpiInput label={`Level ${idx + 1} Y DPI`} value={l.y ?? l.x} supported={supported} onCommit={(y) => commitLevel(idx, { y })} />
                  )}
                </>
              ) : (
                <>
                  <Slider
                    aria-label={`Level ${idx + 1} DPI`}
                    min={0}
                    max={supported.length - 1}
                    step={1}
                    value={[indexOf(l.x)]}
                    onClick={(e) => e.stopPropagation()}
                    onValueChange={([i]) => setLevels(withLevel(idx, { x: supported[i], y: l.y == null ? null : supported[i] }))}
                    onValueCommit={([i]) => commitLevel(idx, { x: supported[i], y: l.y == null ? null : supported[i] })}
                  />
                  <DpiInput label={`Level ${idx + 1} DPI value`} value={l.x} supported={supported} onCommit={(x) => commitLevel(idx, { x, y: l.y == null ? null : x })} />
                </>
              )}
            </div>
          );
        })}
      </div>

      {state.report_rate && (
        <>
          <Separator className="mx-7 mt-6 w-auto!" />
          <PanelSection title="Report rate (per second)" help="How often the mouse sends its position to the PC.">
            <Select value={String(state.report_rate.current)} onValueChange={(v) => onReportRate(Number(v))}>
              <SelectTrigger className="h-10 w-60" aria-label="Report rate">
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                {state.report_rate.choices.map((c) => (
                  <SelectItem key={c.value} value={String(c.value)}>
                    {c.label.replace(" Hz", "")}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </PanelSection>
        </>
      )}

      {dpi.supports_lod && dpi.lod != null && (
        <PanelSection title="Lift-off distance" help="Height at which the sensor stops tracking when you lift the mouse.">
          <Select value={String(dpi.lod)} onValueChange={(v) => onLod(Number(v))}>
            <SelectTrigger className="h-10 w-60" aria-label="Lift-off distance">
              <SelectValue />
            </SelectTrigger>
            <SelectContent position="popper">
              {LOD_LABELS.map((label, i) => (
                <SelectItem key={label} value={String(i)}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </PanelSection>
      )}
    </PanelShell>
  );
}
