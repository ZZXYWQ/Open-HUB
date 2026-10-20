//! Event loop for everything that happens without the user asking:
//! HID++ notifications from mice and receivers, devices plugged in or out,
//! and a slow safety check. Nothing here polls the hardware on a short timer.
//!
//! - Receiver "device connection" notification (HID++ 1.0 sub id 0x41) or a
//!   HID device arriving/leaving: rescan, then refresh battery, tray and UI.
//! - Any other notification from a device (battery change, DPI/profile change,
//!   wake-up): re-read that device and tell the UI ("device-changed").

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager as _};

use crate::hidpp::{Event, EventSink};
use crate::tray::{self, DeviceBattery};
use crate::AppState;

/// Coalesces the burst of reports a reconnect or plug-in produces.
const RESCAN_DELAY: Duration = Duration::from_millis(600);
const DEVICE_REFRESH_DELAY: Duration = Duration::from_millis(300);
/// Catch devices that never send battery events.
const SAFETY_INTERVAL: Duration = Duration::from_secs(15 * 60);
/// Notify once at each threshold per discharge; re-arm above this level.
const WARN_AT: [u8; 2] = [15, 5];
const REARM_ABOVE: u8 = 20;
/// HID++ 1.0 receiver notification: device connected or disconnected.
const DEVICE_CONNECTION: u8 = 0x41;
/// Feature events worth acting on. Others (e.g. 0x8110 button spy, which
/// reports every click) are ignored so ordinary use costs nothing.
const WATCHED_FEATURES: &[u16] = &[
    0x1000, 0x1001, 0x1004, // battery
    0x1D4B, // wireless status (reconnect)
    0x2201, 0x2202, // DPI
    0x8060, 0x8061, // report rate
    0x8100, // onboard profiles (profile / DPI level changes)
];

enum Msg {
    Hid(Event),
    SystemDevicesChanged,
}

static TX: OnceLock<Sender<Msg>> = OnceLock::new();

/// Set OPEN_GHUB_LOG=1 to print every event to stderr (diagnostics).
pub fn logging() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("OPEN_GHUB_LOG").is_some_and(|v| v == "1"))
}

/// Where HID readers deliver events. Before [`start`] (e.g. `--probe`),
/// events are dropped.
pub fn sink() -> EventSink {
    match TX.get() {
        Some(tx) => {
            let tx = tx.clone();
            Arc::new(move |event| {
                let _ = tx.send(Msg::Hid(event));
            })
        }
        None => Arc::new(|_| {}),
    }
}

/// Starts the event loop and the OS device watcher. Call before the device
/// manager is created so its readers get a live sink.
pub fn start(app: AppHandle) {
    let (tx, rx) = channel();
    if TX.set(tx.clone()).is_err() {
        return;
    }
    watch_system_devices(tx);
    std::thread::Builder::new()
        .name("events".into())
        .spawn(move || run(app, rx))
        .expect("could not start event loop");
}

fn run(app: AppHandle, rx: std::sync::mpsc::Receiver<Msg>) {
    let mut warned: HashMap<String, u8> = HashMap::new();
    let mut rescan_at: Option<Instant> = None;
    let mut refresh_at: HashMap<(CString, u8), Instant> = HashMap::new();
    // First battery read shortly after start, then only on events or the safety interval.
    let mut safety_at = Instant::now() + Duration::from_secs(3);

    loop {
        let next = refresh_at.values().chain(rescan_at.iter()).copied().fold(safety_at, Instant::min);
        let msg = rx.recv_timeout(next.saturating_duration_since(Instant::now()));
        if logging() {
            match &msg {
                Ok(Msg::Hid(Event::Notification { path, report })) => {
                    let feature = feature_of(&app, path, report[1], report[2]);
                    eprintln!("[event] hid notification {report:02X?} feature={feature:04X?}")
                }
                Ok(Msg::Hid(Event::Gone)) => eprintln!("[event] transport gone"),
                Ok(Msg::SystemDevicesChanged) => eprintln!("[event] system HID devices changed"),
                Err(_) => {}
            }
        }
        match msg {
            Ok(Msg::Hid(Event::Notification { path, report })) => {
                let now = Instant::now();
                if report[2] == DEVICE_CONNECTION {
                    rescan_at.get_or_insert(now + RESCAN_DELAY);
                } else if feature_of(&app, &path, report[1], report[2]).is_some_and(|f| WATCHED_FEATURES.contains(&f)) {
                    refresh_at.entry((path, report[1])).or_insert(now + DEVICE_REFRESH_DELAY);
                }
            }
            Ok(Msg::Hid(Event::Gone) | Msg::SystemDevicesChanged) => {
                rescan_at.get_or_insert(Instant::now() + RESCAN_DELAY);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }

        let now = Instant::now();
        if rescan_at.is_some_and(|t| t <= now) {
            rescan_at = None;
            refresh_at.clear();
            rescan(&app);
            update_batteries(&app, &mut warned);
            if logging() {
                eprintln!("[event] rescanned");
            }
            let _ = app.emit("devices-changed", ());
            safety_at = now + SAFETY_INTERVAL;
        }
        let due: Vec<(CString, u8)> = refresh_at.iter().filter(|(_, t)| **t <= now).map(|(k, _)| k.clone()).collect();
        if !due.is_empty() {
            for key in &due {
                refresh_at.remove(key);
            }
            update_batteries(&app, &mut warned);
            for id in device_ids(&app, &due) {
                let _ = app.emit("device-changed", id);
            }
        }
        if safety_at <= now {
            safety_at = now + SAFETY_INTERVAL;
            update_batteries(&app, &mut warned);
        }
    }
}

fn rescan(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(mut guard) = crate::manager(&state) else { return };
    if let Some(manager) = guard.as_mut() {
        if let Err(e) = manager.scan() {
            eprintln!("rescan failed: {e}");
        }
    }
}

/// Feature id behind a feature index on the device at `path`/`index`.
fn feature_of(app: &AppHandle, path: &CString, index: u8, feature_index: u8) -> Option<u16> {
    let state = app.state::<AppState>();
    let guard = crate::manager(&state).ok()?;
    let device = guard.as_ref()?.device_at(path, index)?;
    device.features.values().find(|f| f.index == feature_index).map(|f| f.id)
}

fn device_ids(app: &AppHandle, keys: &[(CString, u8)]) -> Vec<String> {
    let state = app.state::<AppState>();
    let Ok(guard) = crate::manager(&state) else { return Vec::new() };
    let Some(manager) = guard.as_ref() else { return Vec::new() };
    keys.iter().filter_map(|(path, index)| manager.device_at(path, *index).map(|d| d.id.clone())).collect()
}

fn estimated_percent(b: &crate::device::Battery) -> Option<u8> {
    b.percent.or(match b.level {
        Some("full") => Some(100),
        Some("good") => Some(50),
        Some("low") => Some(15),
        Some("critical") => Some(5),
        _ => None,
    })
}

fn read_batteries(app: &AppHandle) -> Vec<DeviceBattery> {
    let state = app.state::<AppState>();
    let Ok(guard) = crate::manager(&state) else { return Vec::new() };
    let Some(manager) = guard.as_ref() else { return Vec::new() };
    manager
        .devices()
        .iter()
        .filter_map(|d| {
            let battery = manager.with_device(&d.id, |dev, t| dev.battery(t)).ok().flatten()?;
            Some(DeviceBattery {
                id: d.id.clone(),
                name: d.name.clone(),
                percent: estimated_percent(&battery),
                charging: battery.status.starts_with("charging") || battery.status == "full",
            })
        })
        .collect()
}

/// Reads batteries, warns at 15% and 5%, and refreshes the tray.
fn update_batteries(app: &AppHandle, warned: &mut HashMap<String, u8>) {
    let readings = read_batteries(app);
    for b in &readings {
        let Some(pct) = b.percent else { continue };
        if b.charging || pct > REARM_ABOVE {
            warned.remove(&b.id);
            continue;
        }
        let already = warned.get(&b.id).copied().unwrap_or(u8::MAX);
        if let Some(&level) = WARN_AT.iter().filter(|&&w| pct <= w && w < already).min() {
            warned.insert(b.id.clone(), level);
            let body = if level <= 5 {
                format!("{} battery is at {pct}%. Charge it now to avoid disconnects.", b.name)
            } else {
                format!("{} battery is at {pct}%.", b.name)
            };
            tray::notify(app, "Low mouse battery", &body);
        }
    }
    tray::set_batteries(app, readings);
}

/// Windows tells us when any HID device interface arrives or leaves
/// (receiver or wired mouse plugged in / out).
#[cfg(windows)]
fn watch_system_devices(tx: Sender<Msg>) {
    use windows_sys::core::GUID;
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Register_Notification, CM_NOTIFY_ACTION, CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL,
        CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL, CM_NOTIFY_EVENT_DATA, CM_NOTIFY_FILTER,
        CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE, CR_SUCCESS, HCMNOTIFICATION,
    };

    static OS_TX: OnceLock<Sender<Msg>> = OnceLock::new();
    /// GUID_DEVINTERFACE_HID
    const HID_INTERFACE: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);

    unsafe extern "system" fn on_change(
        _notify: HCMNOTIFICATION,
        _context: *const core::ffi::c_void,
        action: CM_NOTIFY_ACTION,
        _data: *const CM_NOTIFY_EVENT_DATA,
        _size: u32,
    ) -> u32 {
        if action == CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL || action == CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL {
            if let Some(tx) = OS_TX.get() {
                let _ = tx.send(Msg::SystemDevicesChanged);
            }
        }
        0 // ERROR_SUCCESS
    }

    if OS_TX.set(tx).is_err() {
        return;
    }
    let mut filter = CM_NOTIFY_FILTER {
        cbSize: std::mem::size_of::<CM_NOTIFY_FILTER>() as u32,
        FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
        ..Default::default()
    };
    filter.u.DeviceInterface.ClassGuid = HID_INTERFACE;
    let mut handle: HCMNOTIFICATION = std::ptr::null_mut();
    // SAFETY: filter is fully initialised; the registration lives for the
    // whole process, so the handle is intentionally never unregistered.
    let result = unsafe { CM_Register_Notification(&filter, std::ptr::null(), Some(on_change), &mut handle) };
    if result != CR_SUCCESS {
        eprintln!("device change notifications unavailable (CONFIGRET {result})");
    }
}

#[cfg(not(windows))]
fn watch_system_devices(_tx: Sender<Msg>) {}
