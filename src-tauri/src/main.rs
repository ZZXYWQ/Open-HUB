#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod device;
mod device_db;
mod events;
mod foreground;
mod hidpp;
mod manager;
mod onboard;
mod profiles;
mod settings;
mod tray;
mod updater;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager as _, State};

use device::{DeviceState, FeatureInfo};
use manager::Manager;
use profiles::{ImportSummary, Profile, ProfileStore};

/// The manager is created lazily so a HID init failure is reported to the UI
/// instead of aborting startup.
struct AppState(Mutex<Option<Manager>>);

fn manager<'a>(state: &'a State<AppState>) -> Result<MutexGuard<'a, Option<Manager>>, String> {
    let mut guard = state.0.lock().map_err(|_| "device manager lock poisoned".to_string())?;
    if guard.is_none() {
        *guard = Some(Manager::new(events::sink()).map_err(|e| e.to_string())?);
    }
    Ok(guard)
}

#[derive(Serialize)]
struct DeviceSummary {
    id: String,
    name: String,
    product: String,
    product_id: String,
    kind: &'static str,
    firmware_name: String,
    model_ids: Vec<String>,
    connection: String,
    protocol: String,
    features: Vec<FeatureInfo>,
}

#[derive(Serialize)]
struct ScanResult {
    devices: Vec<DeviceSummary>,
    warnings: Vec<String>,
}

#[tauri::command(async)]
fn scan_devices(app: AppHandle, state: State<AppState>) -> Result<ScanResult, String> {
    let mut guard = manager(&state)?;
    let warnings = guard.as_mut().expect("manager initialised").scan().map_err(|e| e.to_string())?;
    drop(guard);
    apply_to_new_devices(&app);
    let guard = manager(&state)?;
    Ok(ScanResult { devices: summaries(guard.as_ref().expect("manager initialised")), warnings })
}

/// Current devices without rescanning (the event loop keeps them current).
#[tauri::command(async)]
fn list_devices(state: State<AppState>) -> Result<Vec<DeviceSummary>, String> {
    let guard = manager(&state)?;
    Ok(summaries(guard.as_ref().expect("manager initialised")))
}

fn summaries(manager: &Manager) -> Vec<DeviceSummary> {
    manager
        .devices()
        .iter()
        .map(|d| DeviceSummary {
            id: d.id.clone(),
            name: d.name.clone(),
            product: d.product.clone(),
            product_id: format!("{:04X}", d.product_id),
            kind: d.kind,
            firmware_name: d.firmware_name.clone(),
            model_ids: d.model_ids.iter().map(|id| format!("{id:04X}")).collect(),
            connection: if d.index == hidpp::DIRECT {
                "Direct (USB)".into()
            } else {
                format!("{} · slot {}", d.product, d.index)
            },
            protocol: d.protocol.clone(),
            features: d.features.values().cloned().collect(),
        })
        .collect()
}

#[tauri::command(async)]
fn device_state(state: State<AppState>, id: String) -> Result<DeviceState, String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager.with_device(&id, |d, t| Ok(d.state(t))).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn set_dpi(state: State<AppState>, id: String, x: u16, y: Option<u16>, lod: Option<u8>) -> Result<(), String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager.with_device(&id, |d, t| d.set_dpi(t, x, y, lod)).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn set_report_rate(state: State<AppState>, id: String, value: u8) -> Result<(), String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager.with_device(&id, |d, t| d.set_report_rate(t, value)).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn set_onboard_mode(state: State<AppState>, id: String, host: bool) -> Result<(), String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager.with_device(&id, |d, t| d.set_onboard_mode(t, host)).map_err(|e| e.to_string())
}

// ---------- profiles ----------

struct ProfilesState {
    store: Mutex<ProfileStore>,
    path: PathBuf,
}

fn lock_store<'a>(state: &'a ProfilesState) -> Result<MutexGuard<'a, ProfileStore>, String> {
    state.store.lock().map_err(|_| "profile store lock poisoned".to_string())
}

#[derive(Serialize)]
struct ProfilesView {
    profiles: Vec<Profile>,
    auto_switch: bool,
    active_id: String,
}

fn view(store: &ProfileStore) -> ProfilesView {
    ProfilesView { profiles: store.profiles.clone(), auto_switch: store.auto_switch, active_id: store.active_id.clone() }
}

/// Mutates the store, persists it, and returns the new view.
fn update_store(state: &ProfilesState, f: impl FnOnce(&mut ProfileStore)) -> Result<ProfilesView, String> {
    let mut store = lock_store(state)?;
    f(&mut store);
    store.save(&state.path).map_err(|e| format!("could not save profiles: {e}"))?;
    Ok(view(&store))
}

#[derive(Clone, Serialize)]
struct Activation {
    id: String,
    name: String,
    /// "manual" or "auto"
    reason: &'static str,
    errors: Vec<String>,
}

/// Makes `id` the active profile and applies it to every connected mouse.
/// Lock order is always profiles, then devices.
fn activate(app: &AppHandle, id: &str, reason: &'static str) -> Result<Activation, String> {
    let profiles = app.state::<ProfilesState>();
    let profile = {
        let mut store = lock_store(&profiles)?;
        let profile = store.get(id).cloned().ok_or_else(|| "profile not found".to_string())?;
        store.active_id = profile.id.clone();
        store.save(&profiles.path).map_err(|e| format!("could not save profiles: {e}"))?;
        profile
    };

    let devices = app.state::<AppState>();
    let mut guard = manager(&devices)?;
    let manager = guard.as_mut().expect("manager initialised");
    if manager.devices().is_empty() {
        manager.scan().map_err(|e| e.to_string())?;
    }
    let mut errors = Vec::new();
    for d in manager.devices() {
        match manager.with_device(&d.id, |d, t| Ok(profiles::apply_to_device(&profile, d, t))) {
            Ok(errs) => errors.extend(errs),
            Err(e) => errors.push(format!("{}: {e}", d.name)),
        }
    }
    if let Ok(mut applied) = app.state::<AppliedDevices>().0.lock() {
        applied.extend(manager.devices().iter().map(|d| d.id.clone()));
    }
    drop(guard);

    let activation = Activation { id: profile.id, name: profile.name, reason, errors };
    tray::refresh(app);
    let _ = app.emit("profile-activated", activation.clone());
    Ok(activation)
}

/// Devices the active profile has been applied to since they connected.
/// A device that disconnects drops out, so it gets the profile again on return.
#[derive(Default)]
struct AppliedDevices(Mutex<HashSet<String>>);

/// Applies the active profile to devices that connected (or reconnected)
/// since it was last applied: after power-cycling the mouse, waking it, or
/// starting Open GHub. Does nothing until the user has saved settings, so a
/// first launch never changes the mouse unasked.
/// Lock order: profiles, then devices, then the applied set.
fn apply_to_new_devices(app: &AppHandle) {
    let profiles = app.state::<ProfilesState>();
    if !profiles.path.exists() {
        return;
    }
    let profile = match lock_store(&profiles) {
        Ok(store) => match store.get(&store.active_id) {
            Some(p) => p.clone(),
            None => return,
        },
        Err(_) => return,
    };

    let devices = app.state::<AppState>();
    let Ok(guard) = manager(&devices) else { return };
    let Some(manager) = guard.as_ref() else { return };
    let present: Vec<String> = manager.devices().iter().map(|d| d.id.clone()).collect();
    let applied_state = app.state::<AppliedDevices>();
    let Ok(mut applied) = applied_state.0.lock() else { return };
    applied.retain(|id| present.contains(id));
    let new: Vec<String> = present.into_iter().filter(|id| !applied.contains(id)).collect();
    for id in &new {
        match manager.with_device(id, |d, t| Ok(profiles::apply_to_device(&profile, d, t))) {
            Ok(errors) => errors.iter().for_each(|e| eprintln!("reapply {}: {e}", profile.name)),
            Err(e) => eprintln!("reapply {}: {e}", profile.name),
        }
        if events::logging() {
            eprintln!("[event] applied profile '{}' to {id}", profile.name);
        }
        applied.insert(id.clone());
    }
    drop(applied);
    drop(guard);
    for id in new {
        let _ = app.emit("device-changed", id);
    }
}

/// Switches profiles when another application comes to the foreground,
/// like G HUB's process watcher. Driven by a Windows event hook, not polling.
fn spawn_profile_watcher(app: AppHandle) {
    let own_exe = std::env::current_exe().ok().map(|p| p.to_string_lossy().to_lowercase());
    let last_exe = Mutex::new(String::new());
    foreground::watch(move || {
        let Some(exe) = foreground::foreground_exe() else { return };
        if events::logging() {
            eprintln!("[event] foreground: {exe}");
        }
        {
            let Ok(mut last) = last_exe.lock() else { return };
            if *last == exe {
                return;
            }
            *last = exe.clone();
        }
        if own_exe.as_deref() == Some(exe.to_lowercase().as_str()) {
            return;
        }
        let target = {
            let state = app.state::<ProfilesState>();
            let Ok(store) = lock_store(&state) else { return };
            if !store.auto_switch {
                return;
            }
            let profile = store.match_exe(&exe);
            if profile.id == store.active_id {
                return;
            }
            profile.id.clone()
        };
        if let Err(e) = activate(&app, &target, "auto") {
            eprintln!("auto-switch failed: {e}");
        }
    });
}

/// Reads the mouse's on-board profile memory. Read-only.
#[tauri::command(async)]
fn onboard_snapshot(state: State<AppState>, id: String) -> Result<onboard::Snapshot, String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    manager.with_device(&id, |d, t| d.onboard_snapshot(t).map(|(s, _)| s)).map_err(|e| e.to_string())
}

/// Reads the on-board memory and writes a byte-exact backup (decoded view
/// plus raw sectors) to `path`. Read-only towards the device.
#[tauri::command(async)]
fn save_onboard_backup(app: AppHandle, state: State<AppState>, id: String, path: String) -> Result<(), String> {
    let guard = manager(&state)?;
    let manager = guard.as_ref().expect("manager initialised");
    let (snapshot, raw, name, model_ids) = manager
        .with_device(&id, |d, t| d.onboard_snapshot(t).map(|(s, r)| (s, r, d.name.clone(), d.model_ids.clone())))
        .map_err(|e| e.to_string())?;
    drop(guard);
    let backup = serde_json::json!({
        "format": "open-ghub-onboard-backup/1",
        "app_version": app.package_info().version.to_string(),
        "created": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "device": name,
        "model_ids": model_ids.iter().map(|id| format!("{id:04X}")).collect::<Vec<_>>(),
        "snapshot": snapshot,
        "sectors": raw,
    });
    let bytes = serde_json::to_vec_pretty(&backup).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| format!("could not write {path}: {e}"))
}

#[tauri::command(async)]
fn get_profiles(state: State<ProfilesState>) -> Result<ProfilesView, String> {
    Ok(view(&*lock_store(&state)?))
}

#[tauri::command(async)]
fn save_profile(app: AppHandle, state: State<ProfilesState>, mut profile: Profile) -> Result<ProfilesView, String> {
    if profile.id.is_empty() {
        profile.id = profiles::new_id();
    }
    if profile.name.trim().is_empty() || profile.levels.is_empty() {
        return Err("a profile needs a name and at least one DPI level".into());
    }
    let view = update_store(&state, |store| store.upsert(profile))?;
    tray::refresh(&app);
    Ok(view)
}

#[tauri::command(async)]
fn delete_profile(app: AppHandle, state: State<ProfilesState>, id: String) -> Result<ProfilesView, String> {
    let view = update_store(&state, |store| store.delete(&id))?;
    tray::refresh(&app);
    Ok(view)
}

#[tauri::command(async)]
fn set_auto_switch(app: AppHandle, state: State<ProfilesState>, enabled: bool) -> Result<ProfilesView, String> {
    let view = update_store(&state, |store| store.auto_switch = enabled)?;
    tray::refresh(&app);
    Ok(view)
}

#[tauri::command(async)]
fn activate_profile(app: AppHandle, id: String) -> Result<Activation, String> {
    activate(&app, &id, "manual")
}

#[derive(Serialize)]
struct ImportResult {
    summary: ImportSummary,
    view: ProfilesView,
}

#[tauri::command(async)]
fn import_ghub_profiles(app: AppHandle, state: State<ProfilesState>) -> Result<ImportResult, String> {
    let path = profiles::ghub_settings_path()
        .filter(|p| p.exists())
        .ok_or_else(|| "G HUB settings not found (expected %LOCALAPPDATA%\\LGHUB\\settings.db)".to_string())?;
    let mut summary = None;
    let view = update_store(&state, |store| summary = Some(profiles::import_ghub(store, &path)))?;
    let summary = summary.expect("import ran")?;
    tray::refresh(&app);
    Ok(ImportResult { summary, view })
}

// ---------- app settings and updates ----------

#[derive(Serialize)]
struct AppInfo {
    version: String,
    settings: settings::AppSettings,
    update: Option<updater::UpdateInfo>,
}

#[tauri::command(async)]
fn get_app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        settings: app.state::<settings::SettingsState>().get(),
        update: app.state::<updater::UpdateState>().available(),
    }
}

#[tauri::command(async)]
fn set_app_settings(app: AppHandle, settings: settings::AppSettings) -> Result<(), String> {
    app.state::<settings::SettingsState>().set(settings)
}

#[tauri::command]
async fn check_for_update(app: AppHandle) -> Result<Option<updater::UpdateInfo>, String> {
    updater::check(&app).await
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    updater::install(&app).await
}

/// `open-ghub --onboard-dump [backup.json]`: read-only. Prints the decoded
/// on-board profiles; with a path, also writes the raw sectors there.
fn onboard_dump(backup_path: Option<String>) {
    let mut manager = match Manager::new(events::sink()) {
        Ok(m) => m,
        Err(e) => return eprintln!("HID init failed: {e}"),
    };
    if let Err(e) = manager.scan() {
        return eprintln!("scan failed: {e}");
    }
    for d in manager.devices() {
        if !d.has(device::feature::ONBOARD_PROFILES) {
            continue;
        }
        match manager.with_device(&d.id, |d, t| d.onboard_snapshot(t)) {
            Ok((snapshot, raw)) => {
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "device": d.name, "snapshot": snapshot })).unwrap_or_default());
                if let Some(path) = &backup_path {
                    let backup = serde_json::json!({
                        "device": d.name, "model_ids": d.model_ids, "info": snapshot.info, "sectors": raw,
                    });
                    match std::fs::write(path, serde_json::to_vec_pretty(&backup).unwrap_or_default()) {
                        Ok(()) => eprintln!("raw sectors written to {path}"),
                        Err(e) => eprintln!("could not write {path}: {e}"),
                    }
                }
            }
            Err(e) => eprintln!("{}: {e}", d.name),
        }
    }
}

/// `open-ghub --probe`: read-only diagnostic that prints every detected
/// device and its current settings as JSON, without opening a window.
fn probe() {
    let mut manager = match Manager::new(events::sink()) {
        Ok(m) => m,
        Err(e) => return eprintln!("HID init failed: {e}"),
    };
    match manager.scan() {
        Ok(warnings) => warnings.iter().for_each(|w| eprintln!("warning: {w}")),
        Err(e) => return eprintln!("scan failed: {e}"),
    }
    for d in manager.devices() {
        let state = manager.with_device(&d.id, |d, t| Ok(d.state(t))).ok();
        let features: Vec<String> = d.features.values().map(|f| format!("{:04X}", f.id)).collect();
        let report = serde_json::json!({
            "name": d.name, "firmware_name": d.firmware_name,
            "model_ids": d.model_ids.iter().map(|id| format!("{id:04X}")).collect::<Vec<_>>(), "kind": d.kind, "product_id": format!("{:04X}", d.product_id),
            "index": d.index, "protocol": d.protocol, "features": features, "state": state,
        });
        println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    }
    if manager.devices().is_empty() {
        println!("no HID++ devices found");
    }
    println!("foreground app: {}", foreground::foreground_exe().unwrap_or_else(|| "(none)".into()));
}

fn main() {
    if std::env::args().any(|a| a == "--probe") {
        return probe();
    }
    // Read-only: dump on-board profile memory as JSON (and raw sectors to a file).
    if let Some(pos) = std::env::args().position(|a| a == "--onboard-dump") {
        return onboard_dump(std::env::args().nth(pos + 1));
    }
    // Read-only: print what an import from G HUB would produce, without saving.
    if std::env::args().any(|a| a == "--ghub-profiles") {
        let mut store = ProfileStore::default();
        let result = profiles::ghub_settings_path()
            .ok_or_else(|| "LOCALAPPDATA not set".to_string())
            .and_then(|p| profiles::import_ghub(&mut store, &p));
        match result {
            Ok(s) => println!(
                "{}\nimported {}, updated {}, skipped {}",
                serde_json::to_string_pretty(&store.profiles).unwrap_or_default(),
                s.imported,
                s.updated,
                s.skipped
            ),
            Err(e) => eprintln!("{e}"),
        }
        return;
    }
    tauri::Builder::default()
        // Must be first: a second launch focuses this window instead of
        // starting another instance that would also talk to the mouse.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main(app)))
        .plugin(tauri_plugin_autostart::Builder::new().args(["--minimized"]).build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updater::UpdateState::default())
        .manage(tray::BatteryCache::default())
        .manage(AppliedDevices::default())
        .manage(AppState(Mutex::new(None)))
        .setup(|app| {
            let path = app.path().app_data_dir()?.join("profiles.json");
            let store = ProfileStore::load(&path);
            app.manage(ProfilesState { store: Mutex::new(store), path });
            let settings_path = app.path().app_data_dir()?.join("settings.json");
            app.manage(settings::SettingsState::load(settings_path));
            // Before anything creates the device manager, so HID readers get a live sink.
            events::start(app.handle().clone());
            spawn_profile_watcher(app.handle().clone());
            tray::create(app.handle())?;
            updater::spawn_background(app.handle().clone());
            // Started by "Start with Windows": stay in the tray.
            if !std::env::args().any(|a| a == "--minimized") {
                tray::show_main(app.handle());
            }
            Ok(())
        })
        // Closing the window destroys it (freeing WebView2); the app keeps
        // running in the tray, see the ExitRequested handler below.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                tray::tray_hint_once(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            scan_devices,
            list_devices,
            device_state,
            set_dpi,
            set_report_rate,
            set_onboard_mode,
            get_profiles,
            save_profile,
            delete_profile,
            set_auto_switch,
            activate_profile,
            import_ghub_profiles,
            onboard_snapshot,
            save_onboard_backup,
            get_app_info,
            set_app_settings,
            check_for_update,
            install_update
        ])
        .build(tauri::generate_context!())
        .expect("error while building Open GHub")
        .run(|_app, event| {
            // The last window closing would normally quit; stay in the tray
            // instead. Quit from the tray menu passes an exit code and goes through.
            if let tauri::RunEvent::ExitRequested { api, code: None, .. } = event {
                api.prevent_exit();
            }
        });
}
