import {AppearanceSettings} from '@/components/appearance-settings';
import { useEffect, useState } from "react";
import { Download, RefreshCw, Settings } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { api, type AppInfo, type AppSettings, type UpdateInfo, type UpdateProgress } from "@/lib/api";
import { cn } from "@/lib/utils";

interface Props {
  /** Latest update found (from the backend's background check or an event). */
  update: UpdateInfo | null;
  onUpdateFound: (info: UpdateInfo | null) => void;
  progress: UpdateProgress | null;
  installing: boolean;
  onInstall: () => void;
}

function SettingRow({ id, title, description, checked, onChange }: { id: string; title: string; description: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg bg-secondary/60 px-4 py-3">
      <Label htmlFor={id} className="flex-col items-start gap-0.5">
        <span className="text-sm font-semibold">{title}</span>
        <span className="text-[12.5px] font-normal text-muted-foreground">{description}</span>
      </Label>
      <Switch id={id} checked={checked} onCheckedChange={onChange} />
    </div>
  );
}

/** App-wide settings: version, update checks and automatic installs. */
export function AppSettingsDialog({ update, onUpdateFound, progress, installing, onInstall }: Props) {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [checkedOnce, setCheckedOnce] = useState(false);

  const load = () => api.appInfo().then(setInfo, (e) => toast.error(String(e)));
  useEffect(() => {
    load();
  }, []);

  const save = async (patch: Partial<AppSettings>) => {
    if (!info) return;
    const settings = { ...info.settings, ...patch };
    try {
      await api.setAppSettings(settings);
      setInfo({ ...info, settings });
    } catch (e) {
      toast.error(String(e));
    }
  };

  const checkNow = async () => {
    setChecking(true);
    try {
      onUpdateFound(await api.checkForUpdate());
      setCheckedOnce(true);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setChecking(false);
    }
  };

  const pct = progress?.total ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100)) : null;

  return (
    <Dialog onOpenChange={(open) => open && load()}>
      <Tooltip>
        <TooltipTrigger asChild>
          <DialogTrigger asChild>
            <Button variant="ghost" size="icon-lg" className="relative size-[42px] rounded-full text-muted-foreground" aria-label="App settings">
              <Settings className="size-5" />
              {update && <span className="absolute top-2 right-2 size-2 rounded-full bg-primary" aria-label="Update available" />}
            </Button>
          </DialogTrigger>
        </TooltipTrigger>
        <TooltipContent>App settings</TooltipContent>
      </Tooltip>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="text-xl font-extrabold tracking-tight uppercase">外观与设置</DialogTitle>
          <DialogDescription>Open HUB {info ? `version ${info.version}` : ""}</DialogDescription>
        </DialogHeader>

        <AppearanceSettings/>
        <Separator/>
        <section className="grid gap-3">
          <h3 className="text-[13px] font-bold tracking-widest uppercase">Updates</h3>

          <div className={cn("flex items-center justify-between gap-3 rounded-lg px-4 py-3", update ? "bg-row-active" : "bg-secondary/60")}>
            <div className="min-w-0 text-sm">
              {update ? (
                <>
                  <div className="font-semibold">Version {update.version} is available</div>
                  {installing ? (
                    <div className="text-muted-foreground">
                      {pct != null ? `Downloading… ${pct}%` : "Downloading…"} Open HUB will restart.
                    </div>
                  ) : (
                    update.notes && <div className="line-clamp-2 text-muted-foreground">{update.notes.split("\n")[0]}</div>
                  )}
                </>
              ) : (
                <div className="text-muted-foreground">{checkedOnce ? "You're up to date." : "Updates come from the project's GitHub releases."}</div>
              )}
            </div>
            {update ? (
              <Button onClick={onInstall} disabled={installing}>
                <Download /> {installing ? "Installing…" : "Install"}
              </Button>
            ) : (
              <Button variant="secondary" onClick={checkNow} disabled={checking}>
                <RefreshCw className={cn(checking && "animate-spin")} /> {checking ? "Checking…" : "Check now"}
              </Button>
            )}
          </div>

          {info && (
            <>
              <SettingRow
                id="check-updates"
                title="Check for updates automatically"
                description="Shortly after start and every 6 hours. You get a notification when one is available."
                checked={info.settings.check_updates}
                onChange={(v) => save({ check_updates: v, auto_install_updates: v && info.settings.auto_install_updates })}
              />
              <SettingRow
                id="auto-install"
                title="Install updates automatically"
                description="Downloads and installs new versions without asking. Open HUB restarts briefly."
                checked={info.settings.auto_install_updates}
                onChange={(v) => save({ auto_install_updates: v, check_updates: v || info.settings.check_updates })}
              />
            </>
          )}
          <Separator className="mt-1" />
          <p className="text-xs text-muted-foreground">
            Updates are signed. Open HUB verifies each download before installing it.
          </p>
        </section>
      </DialogContent>
    </Dialog>
  );
}
