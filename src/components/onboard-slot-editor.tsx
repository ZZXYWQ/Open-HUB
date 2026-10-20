import { useEffect, useState } from "react";
import { Plus, Save, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { api, ONBOARD_RATES_HZ, type OnboardProfile, type SlotSettings } from "@/lib/api";
import { cn } from "@/lib/utils";

const MAX_LEVELS = 5;

interface Props {
  profile: OnboardProfile | null;
  /** Valid DPI values for this mouse, to snap typed values. */
  supportedDpi: number[];
  onClose: () => void;
  onSave: (settings: SlotSettings) => Promise<boolean>;
}

function nearest(list: number[], v: number): number {
  return list.length ? list.reduce((a, b) => (Math.abs(b - v) < Math.abs(a - v) ? b : a)) : v;
}

/** Edits DPI levels, starting level and report rate of one on-board slot. */
export function OnboardSlotEditor({ profile, supportedDpi, onClose, onSave }: Props) {
  const [levels, setLevels] = useState<number[]>([]);
  const [start, setStart] = useState(0);
  const [rate, setRate] = useState(1000);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!profile) return;
    setLevels(profile.dpi_levels.length ? profile.dpi_levels : [800]);
    setStart(Math.min(profile.default_dpi_index, Math.max(0, profile.dpi_levels.length - 1)));
    setRate(ONBOARD_RATES_HZ.includes(profile.report_rate_hz) ? profile.report_rate_hz : 1000);
  }, [profile]);

  if (!profile) return null;

  const snap = (v: number) => nearest(supportedDpi, Math.max(1, Math.round(v)));
  const fromActiveProfile = async () => {
    const view = await api.profiles();
    const active = view.profiles.find((p) => p.id === view.active_id);
    if (!active) return;
    const next = active.levels.slice(0, MAX_LEVELS).map((l) => snap(l.x));
    setLevels(next);
    setStart(Math.min(active.active_level, next.length - 1));
    if (active.report_rate_hz && ONBOARD_RATES_HZ.includes(active.report_rate_hz)) setRate(active.report_rate_hz);
  };

  const save = async () => {
    setSaving(true);
    const ok = await onSave({ slot: profile.slot, dpi_levels: levels.map(snap), default_index: start, report_rate_hz: rate });
    setSaving(false);
    if (ok) onClose();
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="text-xl font-extrabold tracking-tight uppercase">On-board slot {profile.slot}</DialogTitle>
          <DialogDescription>
            Saved inside the mouse and used in on-board memory mode, even with Open GHub closed. A backup of the mouse's
            memory is saved first, and the write is checked by reading it back.
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-5 py-1">
          <div className="grid gap-2">
            <div className="flex items-center justify-between">
              <Label>DPI levels</Label>
              <Button variant="link" size="sm" className="h-auto p-0" onClick={fromActiveProfile}>
                Use my active Open GHub profile
              </Button>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              {levels.map((dpi, i) => (
                <div key={i} className={cn("flex items-center gap-1 rounded-lg border p-1", i === start ? "border-primary" : "border-input")}>
                  <button
                    type="button"
                    title="Start on this level"
                    onClick={() => setStart(i)}
                    className={cn("size-5 rounded-full text-[11px] font-bold", i === start ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground")}
                  >
                    {i + 1}
                  </button>
                  <Input
                    type="number"
                    aria-label={`Level ${i + 1} DPI`}
                    value={dpi}
                    onChange={(e) => setLevels(levels.map((v, j) => (j === i ? Number(e.target.value) || 0 : v)))}
                    onBlur={() => setLevels(levels.map((v, j) => (j === i ? snap(v) : v)))}
                    className="h-7 w-20 border-0 px-1 tabular-nums dark:bg-transparent"
                  />
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    aria-label={`Remove level ${i + 1}`}
                    disabled={levels.length <= 1}
                    onClick={() => {
                      const next = levels.filter((_, j) => j !== i);
                      setLevels(next);
                      setStart(Math.min(start > i ? start - 1 : start, next.length - 1));
                    }}
                  >
                    <X />
                  </Button>
                </div>
              ))}
              {levels.length < MAX_LEVELS && (
                <Button variant="ghost" onClick={() => setLevels([...levels, snap((levels.at(-1) ?? 800) * 2)])}>
                  <Plus /> Add
                </Button>
              )}
            </div>
            <p className="text-xs text-muted-foreground">The highlighted level is where the mouse starts. Values snap to what the sensor supports.</p>
          </div>

          <div className="grid gap-2">
            <Label>Report rate</Label>
            <Select value={String(rate)} onValueChange={(v) => setRate(Number(v))}>
              <SelectTrigger className="w-60">
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                {ONBOARD_RATES_HZ.map((hz) => (
                  <SelectItem key={hz} value={String(hz)}>
                    {hz} Hz
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        <DialogFooter>
          <Button variant="ghost" size="lg" onClick={onClose}>
            Cancel
          </Button>
          <Button size="lg" onClick={save} disabled={saving || levels.length === 0}>
            <Save /> {saving ? "Writing to mouse…" : "Save to mouse"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
