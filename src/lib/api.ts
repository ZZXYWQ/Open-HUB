import { invoke } from "@tauri-apps/api/core";

// Mirrors the Serialize types in src-tauri/src/{main,device}.rs.

export interface FeatureInfo {
  id: number;
  index: number;
  version: number;
  flags: number;
  name: string;
}

export interface DeviceSummary {
  id: string;
  name: string;
  product: string;
  product_id: string;
  kind: string;
  /** Name the firmware reports; `name` may be the marketing name instead. */
  firmware_name: string;
  /** Product ids per transport (hex), e.g. ["4093"]. */
  model_ids: string[];
  connection: string;
  protocol: string;
  features: FeatureInfo[];
}

export interface Battery {
  percent: number | null;
  level: string | null;
  status: string;
  voltage_mv: number | null;
}

export interface Dpi {
  feature: number;
  x: number;
  y: number | null;
  default: number | null;
  supported: number[];
  supports_y: boolean;
  lod: number | null;
  supports_lod: boolean;
}

export interface Choice {
  value: number;
  label: string;
  hz: number;
}

export interface ReportRate {
  feature: number;
  current: number;
  choices: Choice[];
}

export interface DeviceState {
  battery: Battery | null;
  dpi: Dpi | null;
  report_rate: ReportRate | null;
  onboard_mode: "onboard" | "host" | null;
  errors: string[];
}

export interface ScanResult {
  devices: DeviceSummary[];
  warnings: string[];
}

// Mirrors src-tauri/src/profiles.rs (fields are snake_case on the wire).

export interface ProfileLevel {
  x: number;
  /** null means Y follows X. */
  y: number | null;
}

export interface Profile {
  /** Empty string when creating a new profile. */
  id: string;
  name: string;
  /** Executable or install folder that activates the profile; null for Desktop. */
  app_path: string | null;
  levels: ProfileLevel[];
  active_level: number;
  report_rate_hz: number | null;
  ghub_id?: string | null;
}

export interface ProfilesView {
  profiles: Profile[];
  auto_switch: boolean;
  active_id: string;
}

export interface Activation {
  id: string;
  name: string;
  reason: "manual" | "auto" | "tray";
  errors: string[];
}

export interface ImportResult {
  summary: { imported: number; updated: number; skipped: number };
  view: ProfilesView;
}

export const DESKTOP_ID = "desktop";
export const REPORT_RATES_HZ = [125, 250, 500, 1000, 2000, 4000, 8000];

export const api = {
  scan: () => invoke<ScanResult>("scan_devices"),
  /** Current devices without rescanning; the backend keeps them up to date. */
  listDevices: () => invoke<DeviceSummary[]>("list_devices"),
  state: (id: string) => invoke<DeviceState>("device_state", { id }),
  setDpi: (id: string, x: number, y: number | null, lod: number | null) =>
    invoke<void>("set_dpi", { id, x, y, lod }),
  setReportRate: (id: string, value: number) => invoke<void>("set_report_rate", { id, value }),
  setOnboardMode: (id: string, host: boolean) => invoke<void>("set_onboard_mode", { id, host }),

  profiles: () => invoke<ProfilesView>("get_profiles"),
  saveProfile: (profile: Profile) => invoke<ProfilesView>("save_profile", { profile }),
  deleteProfile: (id: string) => invoke<ProfilesView>("delete_profile", { id }),
  setAutoSwitch: (enabled: boolean) => invoke<ProfilesView>("set_auto_switch", { enabled }),
  activateProfile: (id: string) => invoke<Activation>("activate_profile", { id }),
  importGhub: () => invoke<ImportResult>("import_ghub_profiles"),
};

/** Battery percent, estimated from the coarse level when the device has no exact figure. */
export function batteryPercent(b: Battery): number | null {
  if (b.percent != null) return b.percent;
  const byLevel: Record<string, number> = { full: 100, good: 70, low: 25, critical: 8 };
  return b.level ? (byLevel[b.level] ?? null) : null;
}
