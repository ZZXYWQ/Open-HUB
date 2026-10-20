import { useState } from "react";
import { Download, Monitor, Pencil, Play, Plus, Trash2 } from "lucide-react";

import { newProfile, ProfileEditor } from "@/components/profile-editor";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { DESKTOP_ID, type Profile, type ProfilesView } from "@/lib/api";
import { cn } from "@/lib/utils";

interface Props {
  view: ProfilesView;
  importing: boolean;
  onAutoSwitch: (enabled: boolean) => void;
  onActivate: (id: string) => void;
  onSave: (profile: Profile) => Promise<boolean>;
  onDelete: (id: string) => void;
  onImport: () => void;
}

export function ProfilesPage({ view, importing, onAutoSwitch, onActivate, onSave, onDelete, onImport }: Props) {
  const [editing, setEditing] = useState<Profile | null>(null);
  // Desktop first, then alphabetical.
  const profiles = [...view.profiles].sort((a, b) =>
    a.id === DESKTOP_ID ? -1 : b.id === DESKTOP_ID ? 1 : a.name.localeCompare(b.name),
  );

  return (
    <section className="flex-1 overflow-y-auto px-11 pt-8 pb-12">
      <div className="mx-auto flex max-w-6xl flex-wrap items-center gap-3">
        <div className="mr-auto">
          <h1 className="text-2xl font-extrabold tracking-tight uppercase">Profiles</h1>
          <p className="text-sm text-muted-foreground">
            Each profile stores DPI levels and a report rate. With auto-switch on, the profile for the app in the foreground is applied to every connected mouse.
          </p>
        </div>
        <Label className="gap-3 rounded-full bg-card py-2 pr-2 pl-4 text-sm font-semibold">
          Auto-switch
          <Switch checked={view.auto_switch} onCheckedChange={onAutoSwitch} />
        </Label>
        <Button variant="secondary" className="h-10 bg-card px-4 font-bold hover:bg-secondary" onClick={onImport} disabled={importing}>
          <Download /> {importing ? "Importing…" : "Import from G HUB"}
        </Button>
        <Button className="h-10 px-4 font-bold" onClick={() => setEditing(newProfile())}>
          <Plus /> New profile
        </Button>
      </div>

      <div className="mx-auto mt-6 grid max-w-6xl grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-4">
        {profiles.map((p) => {
          const active = p.id === view.active_id;
          const isDesktop = p.id === DESKTOP_ID;
          return (
            <Card key={p.id} className={cn("gap-3 rounded-[10px] px-5 py-5 ring-0", active && "ring-2 ring-primary")}>
              <div className="flex items-start gap-2">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    {isDesktop && <Monitor className="size-4 shrink-0 text-muted-foreground" />}
                    <span className="truncate text-lg leading-tight font-extrabold tracking-tight uppercase">{p.name}</span>
                  </div>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <p className="mt-1 truncate font-mono text-[11px] text-muted-foreground">
                        {isDesktop ? "Default when no app matches" : p.app_path || "No app linked · manual only"}
                      </p>
                    </TooltipTrigger>
                    {p.app_path && <TooltipContent className="max-w-md break-all">{p.app_path}</TooltipContent>}
                  </Tooltip>
                </div>
                {active && <Badge>Active</Badge>}
              </div>

              <div className="flex flex-wrap gap-1.5">
                {p.levels.map((l, i) => (
                  <span
                    key={i}
                    className={cn(
                      "rounded-md px-2 py-0.5 text-xs font-semibold tabular-nums",
                      i === p.active_level ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground",
                    )}
                  >
                    {l.x}
                  </span>
                ))}
                {p.report_rate_hz != null && (
                  <span className="rounded-md border border-input px-2 py-0.5 text-xs text-muted-foreground tabular-nums">{p.report_rate_hz} Hz</span>
                )}
              </div>

              <div className="mt-1 flex items-center gap-1">
                <Button size="sm" variant={active ? "secondary" : "default"} onClick={() => onActivate(p.id)}>
                  <Play /> {active ? "Re-apply" : "Activate"}
                </Button>
                <Button size="sm" variant="ghost" className="ml-auto" onClick={() => setEditing(p)} aria-label={`Edit ${p.name}`}>
                  <Pencil /> Edit
                </Button>
                {!isDesktop && (
                  <Button size="icon-sm" variant="ghost" className="text-muted-foreground hover:text-destructive" onClick={() => onDelete(p.id)} aria-label={`Delete ${p.name}`}>
                    <Trash2 />
                  </Button>
                )}
              </div>
            </Card>
          );
        })}
      </div>

      <ProfileEditor profile={editing} onClose={() => setEditing(null)} onSave={onSave} />
    </section>
  );
}
