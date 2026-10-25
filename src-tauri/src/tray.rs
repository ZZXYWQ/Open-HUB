//! System tray: battery and active profile at a glance, quick profile
//! switching, start-with-Windows, and low-battery notifications.
//! Closing the main window hides it; Open HUB keeps running here.

use std::sync::Mutex;

use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_notification::NotificationExt as _;

use crate::{lock_store, update_store, ProfilesState};

const TRAY_ID: &str = "main";

#[derive(Clone)]
pub struct DeviceBattery {
    pub id: String,
    pub name: String,
    pub percent: Option<u8>,
    pub charging: bool,
}

/// Latest battery readings (kept current by the event loop), shared by the
/// tray tooltip and menu.
#[derive(Default)]
pub struct BatteryCache(Mutex<Vec<DeviceBattery>>);

pub fn set_batteries(app: &AppHandle, readings: Vec<DeviceBattery>) {
    if let Ok(mut cache) = app.state::<BatteryCache>().0.lock() {
        *cache = readings;
    }
    refresh(app);
}

fn battery_label(b: &DeviceBattery) -> String {
    match (b.percent, b.charging) {
        (Some(p), true) => format!("{}: {p}% (charging)", b.name),
        (Some(p), false) => format!("{}: {p}%", b.name),
        (None, true) => format!("{}: charging", b.name),
        (None, false) => b.name.clone(),
    }
}

/// Shows the main window, creating it if needed. The window is destroyed when
/// closed, so while Open HUB sits in the tray no WebView2 processes run.
pub fn show_main(app: &AppHandle) {
    let window = match app.get_webview_window("main") {
        Some(window) => window,
        None => {
            let Some(config) = app.config().app.windows.iter().find(|w| w.label == "main") else { return };
            match tauri::WebviewWindowBuilder::from_config(app, config).and_then(|b| b.build()) {
                Ok(window) => window,
                Err(e) => return eprintln!("could not open window: {e}"),
            }
        }
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("notification failed: {e}");
    }
}

/// Shown the first time the window is closed, so the user knows the app is still running.
pub fn tray_hint_once(app: &AppHandle) {
    let Ok(dir) = app.path().app_data_dir() else { return };
    let marker = dir.join("tray-hint-shown");
    if marker.exists() {
        return;
    }
    notify(
        app,
        "Open HUB is still running",
        "It keeps applying your profiles from the system tray. Right-click the tray icon to quit.",
    );
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(marker, b"1");
}

fn tooltip(app: &AppHandle) -> String {
    let mut lines = vec!["Open HUB".to_string()];
    if let Some(cache) = app.try_state::<BatteryCache>() {
        if let Ok(batteries) = cache.0.lock() {
            lines.extend(batteries.iter().map(battery_label));
        }
    }
    if let Some(profiles) = app.try_state::<ProfilesState>() {
        if let Ok(store) = lock_store(&profiles) {
            if let Some(p) = store.get(&store.active_id) {
                lines.push(format!("最近使用配置：{}", p.name));
            }
        }
    }
    // Windows truncates tray tooltips at 127 characters.
    let mut text = lines.join("\n");
    if text.chars().count() > 127 {
        text = text.chars().take(124).collect::<String>() + "...";
    }
    text
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;

    let batteries = app
        .try_state::<BatteryCache>()
        .and_then(|c| c.0.lock().ok().map(|b| b.clone()))
        .unwrap_or_default();
    if batteries.is_empty() {
        menu.append(&MenuItem::with_id(app, "info:none", "No mouse connected", false, None::<&str>)?)?;
    }
    for (i, b) in batteries.iter().enumerate() {
        menu.append(&MenuItem::with_id(app, format!("info:{i}"), battery_label(b), false, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    let peers = crate::connected_profile_targets(app).unwrap_or_default();
    if let Some(profiles) = app.try_state::<ProfilesState>() {
        if let Ok(store) = lock_store(&profiles) {
            for (i, device) in peers.iter().enumerate() {
                let active = store.active_for(device);
                let label = format!("{}. {} · {}",i+1,device.name,active.map(|p|p.name.as_str()).unwrap_or("未分配配置"));
                let submenu = Submenu::with_id(app,format!("device-profiles:{i}"),label,true)?;
                for p in store.profiles.iter().filter(|p| device.matches(p) && store.profile_owners.get(&p.id).is_none_or(|owner| owner == &device.key)) {
                    let payload = serde_json::to_string(&(device.id.as_str(),p.id.as_str())).expect("string pair serializes");
                    submenu.append(&CheckMenuItem::with_id(app,format!("device-profile:{payload}"),&p.name,true,active.is_some_and(|a|a.id==p.id),None::<&str>)?)?;
                }
                menu.append(&submenu)?;
            }
            menu.append(&CheckMenuItem::with_id(app, "auto_switch", "按鼠标自动切换配置", true, store.auto_switch, None::<&str>)?)?;
            menu.append(&PredefinedMenuItem::separator(app)?)?;
        }
    }

    menu.append(&MenuItem::with_id(app, "show", "Show Open HUB", true, None::<&str>)?)?;
    let update = app.try_state::<crate::updater::UpdateState>().and_then(|s| s.available());
    match update {
        Some(info) => menu.append(&MenuItem::with_id(
            app,
            "update_install",
            format!("{} {}{}",if info.downloaded{"安装更新"}else{"下载更新"},info.version,if info.experimental{" · 实验版"}else{""}),
            true,
            None::<&str>,
        )?)?,
        None => menu.append(&MenuItem::with_id(app, "update_check", "检查 GitHub 更新", true, None::<&str>)?)?,
    }
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);
    menu.append(&CheckMenuItem::with_id(app, "autostart", "Start with Windows", true, autostart, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit Open HUB", true, None::<&str>)?)?;
    Ok(menu)
}

/// Rebuilds the tray menu and tooltip after battery, profile or setting changes.
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    match build_menu(app) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(e) => eprintln!("tray menu: {e}"),
    }
    let _ = tray.set_tooltip(Some(tooltip(app)));
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref().to_string();
    match id.as_str() {
        "show" => show_main(app),
        "update_check" => crate::updater::check_from_tray(app),
        "update_install" => crate::updater::install_from_tray(app),
        "quit" => app.exit(0),
        "auto_switch" => {
            let state = app.state::<ProfilesState>();
            if update_store(&state, |s| s.auto_switch = !s.auto_switch).is_ok() {
                let _ = app.emit("profiles-changed", ());
            }
            refresh(app);
        }
        "autostart" => {
            let launcher = app.autolaunch();
            let result = if launcher.is_enabled().unwrap_or(false) { launcher.disable() } else { launcher.enable() };
            if let Err(e) = result {
                notify(app, "Open HUB", &format!("Could not change Start with Windows: {e}"));
            }
            refresh(app);
        }
        _ => {
            if let Some(payload) = id.strip_prefix("device-profile:") {
                if let Ok((device_id,profile_id)) = serde_json::from_str::<(String,String)>(payload) {
                    let app = app.clone();
                    std::thread::spawn(move || {
                        match crate::activate_target(&app,&profile_id,"tray",Some(&device_id)) {
                            Err(e) => notify(&app,"Open HUB",&format!("Could not apply profile: {e}")),
                            Ok(result) if !result.errors.is_empty() => notify(&app,"Open HUB",&result.errors.join("\n")),
                            _ => {},
                        }
                    });
                }
            }
        }
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Open HUB")
        .menu(&build_menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}
