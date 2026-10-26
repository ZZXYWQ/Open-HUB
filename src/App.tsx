import {ProfileApplicationResult} from '@/components/profile-application-result';
import {activeDeviceProfile,brandName} from "@/lib/brands";
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";

import { AppHeader, type Section } from "@/components/app-header";
import { DevicePage } from "@/components/device-page";
import {LightingPanel} from "@/components/lighting-panel";
import { HomePage } from "@/components/home-page";
import { ProfilesPage } from "@/components/profiles-page";
import {
  api,
  type Activation,
  type DeviceState,
  type DeviceSummary,
  type Profile,
  type ProfilesView,
  type UpdateInfo,
  type UpdateProgress,
} from "@/lib/api";
import type { Level } from "@/lib/levels";

// The backend pushes changes; this only catches anything an event missed.
const FALLBACK_REFRESH_MS = 5 * 60 * 1000;

export default function App() {
  const [devices, setDevices] = useState<DeviceSummary[]>([]);
  const [states, setStates] = useState<Record<string, DeviceState>>({});
  const [profiles, setProfiles] = useState<ProfilesView | null>(null);
  const [activationResult,setActivationResult] = useState<Activation|null>(null);
  const [section, setSection] = useState<Section>("devices");
  const [openId, setOpenId] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const [importing, setImporting] = useState(false);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress | null>(null);
  const [installing, setInstalling] = useState(false);
  const announcedUpdate = useRef<string | null>(null);
  const busy = useRef(false);
  const scanningRef = useRef(false);
  const openIdRef = useRef(openId);
  openIdRef.current = openId;
  const devicesRef = useRef(devices);
  devicesRef.current = devices;

  const refreshState = useCallback(async (id: string, quiet = false) => {
    try {
      const s = await api.state(id);
      setStates((prev) => ({ ...prev, [id]: s }));
    } catch (e) {
      if (!quiet) toast.error(String(e));
    }
  }, []);

  const refreshAllStates = useCallback(async () => {
    for (const d of devicesRef.current) await refreshState(d.id, true);
  }, [refreshState]);

  const scan = useCallback(async () => {
    if (scanningRef.current) return;
    scanningRef.current = true;
    setScanning(true);
    try {
      const result = await api.scan();
      result.warnings.forEach((w) => console.warn(w));
      if (result.warnings.length) toast.warning(result.warnings[0]);
      setDevices(result.devices);
      devicesRef.current = result.devices;
      setStates((prev) => Object.fromEntries(Object.entries(prev).filter(([id]) => result.devices.some((d) => d.id === id))));
      const open = openIdRef.current;
      if (open && !result.devices.some((d) => d.id === open)) {
        setOpenId(null);
        toast.error("Device disconnected");
      }
      await refreshAllStates();
    } catch (e) {
      toast.error(String(e));
    } finally {
      scanningRef.current = false;
      setScanning(false);
    }
  }, [refreshAllStates]);

  /** Runs one device write, reports the outcome, then re-reads the device. */
  const run = useCallback(
    async (label: string, action: (id: string) => Promise<void>) => {
      const id = openIdRef.current;
      if (!id || busy.current) return;
      busy.current = true;
      try {
        await action(id);
        toast.success(`${label} applied`);
      } catch (e) {
        toast.error(`${label}: ${e}`);
      } finally {
        busy.current = false;
        await refreshState(id);
      }
    },
    [refreshState],
  );

  /** Wraps a profile command: updates the view or reports the error. */
  const profileCall = useCallback(async (call: () => Promise<ProfilesView>, success?: string) => {
    try {
      setProfiles(await call());
      if (success) toast.success(success);
      return true;
    } catch (e) {
      toast.error(String(e));
      return false;
    }
  }, []);

  const reportActivation = useCallback(
    async (a: Activation) => {
      setActivationResult(a);
      if (a.errors.length) toast.warning(`${a.name}: ${a.errors[0]}`);
      else toast.success(a.mchose?`${a.name} 已读回核对`:a.reason === "manual" ? `${a.name} applied` : `Switched to ${a.name}`);
      setProfiles(await api.profiles());
      await refreshAllStates();
    },
    [refreshAllStates],
  );

  useEffect(() => {
    scan();
    api.profiles().then(setProfiles, (e) => toast.error(String(e)));
  }, [scan]);

  // Auto-switches (Rust watcher) and tray menu actions happen outside this
  // window; the backend tells us via these events.
  useEffect(() => {
    const unlistenActivated = listen<Activation>("profile-activated", (e) => {
      if (e.payload.reason !== "manual") reportActivation(e.payload);
    });
    const unlistenChanged = listen("profiles-changed", () => {
      api.profiles().then(setProfiles, (err) => toast.error(String(err)));
    });
    return () => {
      unlistenActivated.then((f) => f());
      unlistenChanged.then((f) => f());
    };
  }, [reportActivation]);

  // Device events from the backend: plug/unplug/wake (list changed) and
  // per-device notifications such as battery or DPI changes.
  useEffect(() => {
    const unlistenDevices = listen("devices-changed", async () => {
      try {
        const list = await api.listDevices();
        setDevices(list);
        devicesRef.current = list;
        const open = openIdRef.current;
        if (open && !list.some((d) => d.id === open)) setOpenId(null);
        await refreshAllStates();
      } catch (err) {
        console.warn(err);
      }
    });
    const unlistenDevice = listen<string>("device-changed", (e) => {
      if (!busy.current) refreshState(e.payload, true);
    });
    return () => {
      unlistenDevices.then((f) => f());
      unlistenDevice.then((f) => f());
    };
  }, [refreshAllStates, refreshState]);

  /** Installs only the version explicitly selected and already downloaded. */
  const installUpdate = useCallback(async (version:string) => {
    setInstalling(true);
    setUpdateProgress(null);
    try {
      await api.installUpdate(version);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setInstalling(false);
    }
  }, []);

  const downloadUpdate = useCallback(async(version:string)=>{
    setInstalling(true);setUpdateProgress(null);
    try{const found=await api.downloadUpdate(version);setUpdate(found);toast.success('更新已下载并验证签名',{description:'你可以立即安装，也可以稍后再安装。',action:{label:'安装并重启',onClick:()=>void installUpdate(found.version)}});}
    catch(e){toast.error(String(e));}finally{setInstalling(false);}
  },[installUpdate]);

  // Updates: the backend checks in the background and reports what it finds.
  useEffect(() => {
    api.appInfo().then((info) => setUpdate(info.update), () => {});
    const unlistenAvailable = listen<UpdateInfo>("update-available", (e) => {
      setUpdate(e.payload);
      if (announcedUpdate.current === e.payload.version) return;
      announcedUpdate.current = e.payload.version;
      toast(`Open HUB ${e.payload.version}${e.payload.experimental?' 实验版':''} 可用`, {
        description: "可在设置查看更新说明，自行选择下载；安装需要另行确认。",
        duration: 15000,
        action: { label: "下载更新", onClick: () => void downloadUpdate(e.payload.version) },
        cancel:{label:"稍后",onClick:()=>{}},
      });
    });
    const unlistenDownloaded = listen<UpdateInfo>("update-downloaded",e=>setUpdate(e.payload));
    const unlistenProgress = listen<UpdateProgress>("update-progress", (e) => setUpdateProgress(e.payload));
    return () => {
      unlistenAvailable.then((f) => f());
      unlistenProgress.then((f) => f());
      unlistenDownloaded.then((f)=>f());
    };
  }, [downloadUpdate]);

  // Fallback refresh in case an event was missed.
  useEffect(() => {
    const timer = setInterval(() => {
      if (document.hidden || busy.current || scanningRef.current) return;
      if (document.activeElement?.matches("input, [role=slider], [role=combobox]")) return;
      const ids = openIdRef.current ? [openIdRef.current] : devicesRef.current.map((d) => d.id);
      ids.reduce((p, id) => p.then(() => refreshState(id, true)), Promise.resolve());
    }, FALLBACK_REFRESH_MS);
    return () => clearInterval(timer);
  }, [refreshState]);

  const device = devices.find((d) => d.id === openId) ?? null;
  const state = device ? (states[device.id] ?? null) : null;
  // An unassigned device shows its actual current DPI, never another mouse's
  // global/default profile. Saving creates a private copy for this exact unit.
  const activeProfile:Profile|null = activeDeviceProfile(profiles,device) ?? (device ? {
    id:"device-default:"+device.id,name:brandName(device.vendor_id)+" · 默认配置",target_vendor:device.vendor_id,
    target_model:parseInt(device.model_ids[0]??device.product_id,16)||null,app_path:null,
    levels:[{x:state?.dpi?.x??1600,y:state?.dpi?.y??null}],active_level:0,report_rate_hz:null,
  } : null);

  const activateProfile = async (id: string,deviceId?:string,asCopy=false,expectedConnection?:string) => {
    try {
      await reportActivation(await api.activateProfile(id,deviceId,asCopy,expectedConnection));
    } catch (e) {
      toast.error(String(e));
    }
  };

  /** Saves edited DPI levels into the active profile. */
  const saveLevels = (levels: Level[], activeLevel: number) => {
    if (!activeProfile || !device) return;
    profileCall(() => api.saveProfile({ ...activeProfile, levels, active_level: activeLevel },device.id));
  };

  /** Saves the levels with `index` active, then applies that level to the open device. */
  const applyLevel = (levels: Level[], index: number) => {
    const dpi = state?.dpi;
    const level = levels[index];
    if (!dpi || !level) return;
    saveLevels(levels, index);
    run("DPI", (id) => api.setDpi(id, level.x, dpi.supports_y ? (level.y ?? level.x) : null, dpi.supports_lod ? dpi.lod : null));
  };

  const setLod = (lod: number) => {
    const dpi = state?.dpi;
    if (!dpi) return;
    run("Lift-off distance", (id) => api.setDpi(id, dpi.x, dpi.supports_y ? (dpi.y ?? dpi.x) : null, lod));
  };
  const setMode = (host: boolean) =>
    run(host ? "Software control" : "On-board memory", (id) => api.setOnboardMode(id, host));

  const setReportRate = (value: number) => {
    const hz = state?.report_rate?.choices.find((c) => c.value === value)?.hz;
    if (activeProfile && device && hz) profileCall(() => api.saveProfile({ ...activeProfile, report_rate_hz: hz },device.id));
    run("Report rate", (id) => api.setReportRate(id, value));
  };

  const importGhub = async () => {
    setImporting(true);
    try {
      const { summary, view } = await api.importGhub();
      setProfiles(view);
      toast.success(`Imported ${summary.imported} profiles from G HUB, updated ${summary.updated}, skipped ${summary.skipped}`);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setImporting(false);
    }
  };

  const open = (id: string) => {
    setOpenId(id);
    refreshState(id);
  };

  return (
    <div id="hub-shell" className={`flex h-screen flex-col overflow-hidden bg-background text-foreground select-none ${section==="lighting"||device?.vendor_id===0x3837||(!device&&devices.length>0&&devices.every(d=>d.vendor_id===0x3837))?"mchose-theme":""}`}>
      <AppHeader
        section={section}
        onSection={(s) => {
          setSection(s);
          setOpenId(null);
        }}
        profiles={profiles}
        onActivateProfile={activateProfile}
        device={device}
        state={state}
        scanning={scanning}
        onScan={scan}
        onBack={() => setOpenId(null)}
        onModeChange={setMode}
        updates={{ update, onUpdateFound: setUpdate, progress: updateProgress, installing, onInstall: ()=>{if(update)void installUpdate(update.version)}, onDownload:()=>{if(update)void downloadUpdate(update.version)} }}
      />
      <ProfileApplicationResult activation={activationResult} devices={devices} onDismiss={()=>setActivationResult(null)}/>
      {section === "lighting" ? <LightingPanel/> : device && activeProfile ? (
        <DevicePage
          key={device.id}
          onProfiles={()=>{setOpenId(null);setSection("profiles");}}
          device={device}
          state={state}
          profile={activeProfile}
          onLevelsChange={saveLevels}
          onApplyLevel={applyLevel}
          onReportRate={setReportRate}
          onLod={setLod}
          onModeChange={setMode}
        />
      ) : section === "profiles" && profiles ? (
        <ProfilesPage
          devices={devices}
          view={profiles}
          importing={importing}
          onAutoSwitch={(enabled) => profileCall(() => api.setAutoSwitch(enabled), enabled ? "Auto-switch on" : "Auto-switch off")}
          onActivate={activateProfile}
          onSave={(p: Profile) => profileCall(() => api.saveProfile(p), "Profile saved")}
          onDelete={(id) => profileCall(() => api.deleteProfile(id), "Profile deleted")}
          onImport={importGhub}
          onMhubImport={async(path)=>{setImporting(true);try{const result=await api.importMhub(path??null);setProfiles(result.view);toast.success(`M HUB 已导入 ${result.summary.imported} 组，更新 ${result.summary.updated} 组；未改动鼠标`);}catch(e){toast.error(String(e));}finally{setImporting(false);}}}
        />
      ) : (
        <HomePage onLighting={key=>{sessionStorage.setItem('lighting-focus-key',key);setOpenId(null);setSection('lighting');}} devices={devices} states={states} scanning={scanning} onOpen={open} onScan={scan} />
      )}
    </div>
  );
}
