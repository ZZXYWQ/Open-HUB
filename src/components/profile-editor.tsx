import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Plus, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { DESKTOP_ID, REPORT_RATES_HZ, type Profile } from "@/lib/api";
import { MAX_LEVELS } from "@/lib/levels";
import { cn } from "@/lib/utils";

const DPI_MIN = 50;
const DPI_MAX = 32000;

export const newProfile = (): Profile => ({
  id: "",
  name: "",
  app_path: "",
  levels: [400, 800, 1600, 3200].map((x) => ({ x, y: null })),
  active_level: 2,
  report_rate_hz: 1000,
});

interface Props {
  profile: Profile | null;
  onClose: () => void;
  onSave: (profile: Profile) => Promise<boolean>;
}

/** Create/edit dialog for one profile. Values are snapped to each mouse when applied. */
export function ProfileEditor({ profile, onClose, onSave }: Props) {
  const [draft, setDraft] = useState<Profile | null>(profile);
  const [saving, setSaving] = useState(false);
  useEffect(() => setDraft(profile), [profile]);
  if (!draft) return null;

  const isDesktop = draft.id === DESKTOP_ID;
  const set = (patch: Partial<Profile>) => setDraft({ ...draft, ...patch });
  const setLevel = (i: number, x: number) =>
    set({ levels: draft.levels.map((l, j) => (j === i ? { x, y: null } : l)) });
  const removeLevel = (i: number) => {
    const levels = draft.levels.filter((_, j) => j !== i);
    set({ levels, active_level: Math.min(draft.active_level, levels.length - 1) });
  };

  const browse = async () => {
    const picked = await open({
      title: "Choose the game's executable",
      multiple: false,
      directory: false,
      filters: [{ name: "Programs", extensions: ["exe"] }],
    });
    if (typeof picked === "string") {
      const exeName = picked.split(/[\\/]/).pop()?.replace(/\.exe$/i, "") ?? "";
      set({ app_path: picked, name: draft.name || exeName });
    }
  };

  const valid = draft.name.trim() !== "" && draft.levels.length > 0;
  const save = async () => {
    setSaving(true);
    const levels = draft.levels.map((l) => ({ ...l, x: Math.min(DPI_MAX, Math.max(DPI_MIN, Math.round(l.x))) }));
    const ok = await onSave({ ...draft, name: draft.name.trim(), app_path: draft.app_path?.trim() || null, levels });
    setSaving(false);
    if (ok) onClose();
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="text-xl font-extrabold tracking-tight uppercase">
            {draft.id ? "Edit profile" : "New profile"}
          </DialogTitle>
          <DialogDescription>
            {isDesktop
              ? "Used whenever no other profile matches the active application."
              : "Activated automatically when this application is in the foreground."}
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-5 py-1">
          <div className="grid gap-2">
            <Label htmlFor="profile-name">Name</Label>
            <Input id="profile-name" value={draft.name} disabled={isDesktop} onChange={(e) => set({ name: e.target.value })} placeholder="e.g. Valorant" />
          </div>

          {!isDesktop && (
            <div className="grid gap-2">
              <Label htmlFor="profile-path">Application</Label>
              <div className="flex gap-2">
                <Input
                  id="profile-path"
                  value={draft.app_path ?? ""}
                  onChange={(e) => set({ app_path: e.target.value })}
                  placeholder="C:\Games\Game\game.exe or its install folder"
                  className="font-mono text-xs"
                />
                <Button variant="secondary" size="lg" onClick={browse}>
                  <FolderOpen /> Browse
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">A folder matches every program inside it.</p>
            </div>
          )}

          <div className="grid gap-2">
            <Label>DPI levels</Label>
            <div className="flex flex-wrap items-center gap-2">
              {draft.levels.map((l, i) => (
                <div key={i} className={cn("flex items-center gap-1 rounded-lg border p-1", i === draft.active_level ? "border-primary" : "border-input")}>
                  <button
                    type="button"
                    title="Make this the active level"
                    onClick={() => set({ active_level: i })}
                    className={cn("size-5 rounded-full text-[11px] font-bold", i === draft.active_level ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground")}
                  >
                    {i + 1}
                  </button>
                  <Input
                    type="number"
                    aria-label={`Level ${i + 1} DPI`}
                    value={l.x}
                    min={DPI_MIN}
                    max={DPI_MAX}
                    step={50}
                    onChange={(e) => setLevel(i, Number(e.target.value) || DPI_MIN)}
                    className="h-7 w-20 border-0 px-1 tabular-nums dark:bg-transparent"
                  />
                  <Button variant="ghost" size="icon-xs" aria-label={`Remove level ${i + 1}`} disabled={draft.levels.length <= 1} onClick={() => removeLevel(i)}>
                    <X />
                  </Button>
                </div>
              ))}
              {draft.levels.length < MAX_LEVELS && (
                <Button
                  variant="ghost"
                  onClick={() => set({ levels: [...draft.levels, { x: Math.min(DPI_MAX, (draft.levels.at(-1)?.x ?? 800) * 2), y: null }] })}
                >
                  <Plus /> Add
                </Button>
              )}
            </div>
            <p className="text-xs text-muted-foreground">The highlighted level is applied when the profile activates. Values snap to what each mouse supports.</p>
          </div>

          <div className="grid gap-2">
            <Label>Report rate</Label>
            <Select
              value={draft.report_rate_hz == null ? "keep" : String(draft.report_rate_hz)}
              onValueChange={(v) => set({ report_rate_hz: v === "keep" ? null : Number(v) })}
            >
              <SelectTrigger className="w-60">
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectItem value="keep">Don't change</SelectItem>
                {REPORT_RATES_HZ.map((hz) => (
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
          <Button size="lg" onClick={save} disabled={!valid || saving}>
            {saving ? "Saving…" : "Save profile"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
