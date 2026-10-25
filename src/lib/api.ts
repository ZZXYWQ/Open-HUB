import type {SinoPlan,SinoView,SinoModel} from './sino';
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
  /** Local ownership identity; connection-only when no reliable unit id exists. */
  profile_key?: string;
  vendor_id: number;
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

export interface MchoseHardware {
  raw_settings:number[]; profile:number; names:string[]; dpi_x:number[]; dpi_y:number[]; count:number; active:number; independent:boolean;
  sleep:number; never_sleep:boolean; left_debounce:number; right_debounce:number; sensor:number; rotation:number; lod:number;
  buttons:number[][]; adaptive:number[]; firmware:string; receiver_firmware:string;
}
export interface MchoseMacroEvent {kind:"key"|"mouse"|"scroll"|"delay";code:number;down:boolean;delay_ms:number}
export interface MchoseMacro {slot:number;name:string;mode:number;events:MchoseMacroEvent[];raw?:number[]}

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
 multi?: import("./multi").MultiPlan|null;
 sino?:SinoPlan|null;
 target_vendor?:number|null;
 target_model?:number|null;
  mchose_board?:number|null;
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
  profile_owners?: Record<string,string>;
  device_profiles?: Record<string,{active_id:string|null;default_id:string|null;copies:Record<string,string>}>;
}

export interface Activation {
  id: string;
  name: string;
  reason: "manual" | "auto" | "tray" | "reconnect";
  device_id?: string;
  errors: string[];
}

export interface ImportResult {
  summary: { imported: number; updated: number; skipped: number };
  view: ProfilesView;
}

export interface AppSettings {
  check_updates: boolean;
  auto_install_updates: boolean;
}

export interface UpdateInfo {
  release_url?: string;
  experimental?: boolean;
  downloaded?: boolean;
  version: string;
  current_version: string;
  notes: string | null;
  date: string | null;
}

export interface AppInfo {
  version: string;
  settings: AppSettings;
  update: UpdateInfo | null;
}

export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

// Mirrors src-tauri/src/onboard.rs (read-only on-board memory view).

export interface OnboardInfo {
  bindings_writable?:boolean;
  memory_model: number;
  profile_format: number;
  macro_format: number;
  profile_count: number;
  rom_profile_count: number;
  button_count: number;
  sector_count: number;
  sector_size: number;
  mechanical_layout: number;
  various_info: number;
}

export interface OnboardButton {
  raw: number[];
  kind: string;
  description: string;
}

export interface OnboardProfile {
  slot: number;
  sector: number;
  rom: boolean;
  enabled: boolean;
  /** null for factory (ROM) profiles, which carry no checksum. */
  crc_valid: boolean | null;
  name: string | null;
  report_rate_hz: number;
  default_dpi_index: number;
  shift_dpi_index: number;
  dpi_levels: number[];
  angle_snapping: number;
  powersave_timeout: number;
  poweroff_timeout: number;
  buttons: OnboardButton[];
  gshift_buttons: OnboardButton[];
}

export interface OnboardSnapshot {
  info: OnboardInfo;
  directory_crc_valid: boolean;
  /** 0 when no on-board profile is active (software mode). */
  active_profile: number;
  active_dpi_index: number | null;
  profiles: OnboardProfile[];
  rom_profiles: OnboardProfile[];
}

export type OnboardBindingAction='Disabled'|{Mouse:number}|{Key:{usage:number;modifiers:number}}|{Consumer:number}|{Special:'WheelLeft'|'WheelRight'|'DpiUp'|'DpiDown'|'DpiCycle'|'DpiDefault'|'DpiShift'|'ProfileUp'|'ProfileDown'|'ProfileCycle'|'GShift'};
export interface OnboardBindingChange {button:number;layer:'Normal'|'GShift';expected:number[];action:OnboardBindingAction}
export interface OnboardBindingSettings {slot:number;sector:number;changes:OnboardBindingChange[]}
export interface OnboardBindingWriteResult {report:{slot:number;sector:number;bytes_changed:number;unchanged:boolean;active_reloaded:boolean;reload_note:string|null};backup_path:string;snapshot:OnboardSnapshot|null;refresh_note:string|null}

export interface SlotSettings {
  slot: number;
  dpi_levels: number[];
  default_index: number;
  report_rate_hz: number;
}

export interface OnboardWriteResult {
  report: { slot: number; sector: number; bytes_changed: number; unchanged: boolean };
  backup_path: string;
  snapshot: OnboardSnapshot;
}

export interface OnboardRestoreResult {
  sectors_written: number;
  backup_path: string;
  snapshot: OnboardSnapshot;
}

/** Report rates the on-board format can store. */
export const ONBOARD_RATES_HZ = [125, 250, 500, 1000];

export const DESKTOP_ID = "desktop";
export const REPORT_RATES_HZ = [125, 250, 500, 1000, 2000, 4000, 8000];

export interface DockLight{enabled:boolean;mode:number;mode_count:number;speed:number;brightness:number;sync:boolean;color:number[];direction:number}
export interface DockView{light:DockLight;firmware:string;music_running:boolean;audio_level:number;music_error:string|null}
export interface RgbDevice{id:number;key:string;name:string;vendor:string;kind:number;leds:number;direct:boolean}
export interface EffectConfig{kind:string;style:number;color:number[];palette:number[][];brightness:number;speed:number;gain:number;dock:boolean;rgb:{id:number;key:string}[]}
export interface EffectStatus{running:boolean;kind:string;level:number;pixels:number[][];error:string|null;config:EffectConfig|null}
export const api = {
 sinoRead:(id:string,profile:number|null=null)=>invoke<SinoView>('sino_read',{id,profile}),
 sinoApply:(id:string,plan:SinoPlan)=>invoke<SinoView>('sino_apply',{id,plan}),
 sinoModels:()=>invoke<SinoModel[]>('sino_models'),
 sinoSimulate:(plan:SinoPlan)=>invoke<{simulated:boolean;hardware_tested:boolean;packet_count:number;note:string}>('sino_simulate',{plan}),
 previewProfileImport:(source:string,path:string|null)=>invoke<{profiles:Profile[];note:string}>('preview_profile_import',{source,path}),
 commitProfileImport:(profiles:Profile[])=>invoke<ProfilesView>('commit_profile_import',{profiles}),
 exportProfilePack:(path:string,profiles:Profile[])=>invoke<void>('export_profile_pack',{path,profiles}),
 dockState:()=>invoke<DockView>('magdock_state'),
 dockApply:(light:DockLight)=>invoke<DockView>('magdock_apply',{light}),
 dockReset:()=>invoke<DockView>('magdock_reset'),
 rgbDevices:()=>invoke<RgbDevice[]>('rgb_devices'),
 effectsStart:(config:EffectConfig)=>invoke<EffectStatus>('effects_start',{config}),
 effectsStop:()=>invoke<EffectStatus>('effects_stop'),
 effectsStatus:()=>invoke<EffectStatus>('effects_status'),
 effectsPreset:(path:string,config:EffectConfig,targetKeys:string[]=[])=>invoke<void>('effects_preset',{path,config,targetKeys}),
  mchoseHardware:(id:string)=>invoke<MchoseHardware>("mchose_hardware",{id}),
  mchoseMacro:(id:string,slot:number)=>invoke<MchoseMacro|null>("mchose_macro",{id,slot}),
  mchoseMacroFile:(path:string,macro:MchoseMacro)=>invoke<void>("mchose_macro_file",{path,...macro}),
  mchoseApply:(id:string,expectedProfile:number,action:Record<string,unknown>)=>invoke<MchoseHardware>("mchose_apply",{id,expectedProfile,action}),
  mchoseExport:(id:string,path:string)=>invoke<void>("mchose_export",{id,path}),
  mchoseRestore:(id:string,path:string)=>invoke<MchoseHardware>("mchose_restore",{id,path}),
  importMhub:(path:string|null=null)=>invoke<ImportResult>("import_mhub_profiles",{path}),
  openMhub:()=>invoke<void>("open_mhub"),
  scan: () => invoke<ScanResult>("scan_devices"),
  /** Current devices without rescanning; the backend keeps them up to date. */
  listDevices: () => invoke<DeviceSummary[]>("list_devices"),
  state: (id: string) => invoke<DeviceState>("device_state", { id }),
  setDpi: (id: string, x: number, y: number | null, lod: number | null) =>
    invoke<void>("set_dpi", { id, x, y, lod }),
  setReportRate: (id: string, value: number) => invoke<void>("set_report_rate", { id, value }),
  setOnboardMode: (id: string, host: boolean) => invoke<void>("set_onboard_mode", { id, host }),

  profiles: () => invoke<ProfilesView>("get_profiles"),
  saveProfile: (profile: Profile, deviceId?:string, makeActive?:boolean) => invoke<ProfilesView>("save_profile", { profile,...(deviceId?{deviceId}:{}),...(makeActive==null?{}:{makeActive}) }),
  deleteProfile: (id: string) => invoke<ProfilesView>("delete_profile", { id }),
  setAutoSwitch: (enabled: boolean) => invoke<ProfilesView>("set_auto_switch", { enabled }),
  activateProfile: (id: string, deviceId?:string, asCopy=false) => invoke<Activation>("activate_profile", { id,...(deviceId?{deviceId}:{}),...(asCopy?{asCopy:true}:{}) }),
  importGhub: () => invoke<ImportResult>("import_ghub_profiles"),

  onboardSnapshot: (id: string) => invoke<OnboardSnapshot>("onboard_snapshot", { id }),
  saveOnboardBackup: (id: string, path: string) => invoke<void>("save_onboard_backup", { id, path }),
  writeOnboardBindings:(id:string,settings:OnboardBindingSettings)=>invoke<OnboardBindingWriteResult>("write_onboard_bindings",{id,settings}),
  writeOnboardSlot: (id: string, settings: SlotSettings) =>
    invoke<OnboardWriteResult>("write_onboard_slot", { id, settings }),
  restoreOnboardBackup: (id: string, path: string) =>
    invoke<OnboardRestoreResult>("restore_onboard_backup", { id, path }),

  appInfo: () => invoke<AppInfo>("get_app_info"),
  setAppSettings: (settings: AppSettings) => invoke<void>("set_app_settings", { settings }),
  checkForUpdate: () => invoke<UpdateInfo | null>("check_for_update"),
  /** On Windows the app exits and the installer restarts it when this succeeds. */
  installUpdate: (expectedVersion:string) => invoke<void>("install_update",{expectedVersion}),
  downloadUpdate: (expectedVersion:string) => invoke<UpdateInfo>("download_update",{expectedVersion}),
};

/** Battery percent, estimated from the coarse level when the device has no exact figure. */
export function batteryPercent(b: Battery): number | null {
  if (b.percent != null) return b.percent;
  const byLevel: Record<string, number> = { full: 100, good: 70, low: 25, critical: 8 };
  return b.level ? (byLevel[b.level] ?? null) : null;
}

