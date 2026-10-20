import { useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Download, HardDrive, RefreshCw } from "lucide-react";
import { toast } from "sonner";

import { PanelSection } from "@/components/panel";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { api, type DeviceSummary, type OnboardButton, type OnboardProfile, type OnboardSnapshot } from "@/lib/api";
import { cn } from "@/lib/utils";

/** Buttons that do something other than their own mouse button. */
function remapped(buttons: OnboardButton[]): OnboardButton[] {
  return buttons.filter((b, i) => b.description !== `Mouse button ${i + 1}`);
}

function ProfileCard({ profile, active }: { profile: OnboardProfile; active: boolean }) {
  const changed = remapped(profile.buttons);
  const gshift = profile.gshift_buttons.filter((b) => b.kind !== "disabled");
  return (
    <div className={cn("rounded-lg bg-secondary/60 px-3.5 py-3 text-[13px]", !profile.enabled && "opacity-60")}>
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-semibold">
          {profile.rom ? "Factory default" : `Slot ${profile.slot}`}
          {profile.name ? ` · ${profile.name}` : ""}
        </span>
        {active && <Badge>Active</Badge>}
        {!profile.enabled && <Badge variant="secondary">Disabled</Badge>}
        {profile.crc_valid === false && <Badge variant="destructive">Checksum error</Badge>}
        <span className="ml-auto text-xs text-muted-foreground tabular-nums">{profile.report_rate_hz} Hz</span>
      </div>
      <div className="mt-2 flex flex-wrap gap-1.5">
        {profile.dpi_levels.map((dpi, i) => (
          <span
            key={i}
            title={i === profile.default_dpi_index ? "Starting DPI" : undefined}
            className={cn(
              "rounded-md px-2 py-0.5 text-xs font-semibold tabular-nums",
              i === profile.default_dpi_index ? "bg-primary text-primary-foreground" : "bg-background/60 text-muted-foreground",
            )}
          >
            {dpi}
          </span>
        ))}
      </div>
      <div className="mt-2 text-xs text-muted-foreground">
        {changed.length === 0 ? (
          "Buttons: standard"
        ) : (
          <>Buttons: {changed.map((b) => `${profile.buttons.indexOf(b) + 1} → ${b.description}`).join(", ")}</>
        )}
        {gshift.length > 0 && <> · G-Shift: {gshift.length} assigned</>}
      </div>
    </div>
  );
}

/** Read-only view of the profiles stored in the mouse, with a backup. */
export function OnboardSection({ device }: { device: DeviceSummary }) {
  const [snapshot, setSnapshot] = useState<OnboardSnapshot | null>(null);
  const [reading, setReading] = useState(false);
  const [saving, setSaving] = useState(false);

  const read = async () => {
    setReading(true);
    try {
      setSnapshot(await api.onboardSnapshot(device.id));
    } catch (e) {
      toast.error(`Could not read on-board memory: ${e}`);
    } finally {
      setReading(false);
    }
  };

  const backup = async () => {
    const date = new Date().toISOString().slice(0, 10);
    const path = await save({
      title: "Save on-board memory backup",
      defaultPath: `open-ghub-onboard-${device.name.replace(/[^\w-]+/g, "_")}-${date}.json`,
      filters: [{ name: "Backup", extensions: ["json"] }],
    });
    if (!path) return;
    setSaving(true);
    try {
      await api.saveOnboardBackup(device.id, path);
      toast.success("On-board memory backed up");
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  };

  const info = snapshot?.info;
  return (
    <PanelSection
      title="On-board memory"
      help="Profiles stored inside the mouse, used when it runs without software. This view only reads the mouse; nothing is changed."
    >
      <div className="flex flex-wrap gap-2">
        <Button variant="secondary" onClick={read} disabled={reading}>
          <RefreshCw className={cn(reading && "animate-spin")} /> {snapshot ? "Read again" : "Read from mouse"}
        </Button>
        <Button variant="ghost" onClick={backup} disabled={saving}>
          <Download /> {saving ? "Saving…" : "Save backup…"}
        </Button>
      </div>

      {snapshot && info && (
        <div className="mt-4 grid gap-2">
          <p className="flex items-center gap-2 text-xs text-muted-foreground">
            <HardDrive className="size-3.5" />
            {info.profile_count} profile slots · {info.button_count} buttons · format {info.profile_format} ·{" "}
            {info.sector_count} sectors of {info.sector_size} bytes ·{" "}
            {snapshot.active_profile === 0 ? "none active (software mode)" : `slot ${snapshot.active_profile} active`}
          </p>
          {!snapshot.directory_crc_valid && (
            <p className="text-xs text-destructive">The profile directory failed its checksum; the mouse is using its factory profile.</p>
          )}
          {snapshot.profiles.map((p) => (
            <ProfileCard key={`u${p.slot}`} profile={p} active={snapshot.active_profile === p.slot} />
          ))}
          {snapshot.rom_profiles.map((p) => (
            <ProfileCard key={`r${p.slot}`} profile={p} active={false} />
          ))}
        </div>
      )}
    </PanelSection>
  );
}
