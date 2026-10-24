//! Updates from GitHub Releases via the Tauri updater plugin.
//!
//! The endpoint is the `latest.json` attached to the newest release (see
//! `scripts/release.mjs`). Installers are signed with the project's private
//! key and verified against the public key in `tauri.conf.json` before they
//! run. On Windows, installing starts the NSIS installer in passive mode and
//! exits the app; the installer restarts it.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt as _;

use crate::settings::SettingsState;
use crate::tray;

/// First check shortly after start, then at this interval.
const FIRST_CHECK: Duration = Duration::from_secs(30);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Clone, Serialize)]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

/// The newest update found by the last check, if any.
#[derive(Default)]
pub struct UpdateState {
    available: Mutex<Option<UpdateInfo>>,
    /// Version already announced, so each update is announced once.
    notified: Mutex<Option<String>>,
}

impl UpdateState {
    pub fn available(&self) -> Option<UpdateInfo> {
        self.available.lock().ok().and_then(|a| a.clone())
    }
}

/// Asks the release endpoint for a newer version. Remembers the result for
/// the tray and tells the UI ("update-available").
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    if app.config().identifier == "dev.local.openghub.mchose" { return Ok(None); }
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = match updater.check().await {
        Ok(update) => update,
        // The latest release has no latest.json (e.g. it predates the updater):
        // nothing to update to.
        Err(tauri_plugin_updater::Error::ReleaseNotFound) => None,
        Err(e) => return Err(format!("update check failed: {e}")),
    };
    let info = update.map(|u| UpdateInfo {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        notes: u.body.clone(),
        date: u.date.map(|d| d.to_string()),
    });
    if let Ok(mut available) = app.state::<UpdateState>().available.lock() {
        *available = info.clone();
    }
    tray::refresh(app);
    if let Some(info) = &info {
        let _ = app.emit("update-available", info.clone());
    }
    Ok(info)
}

/// Downloads, verifies and installs the latest update. On Windows this does
/// not return on success: the app exits so the installer can replace it.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    if app.config().identifier == "dev.local.openghub.mchose" { return Err("Updates are disabled for this local MCHOSE build".into()); }
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| format!("update check failed: {e}"))?
        .ok_or_else(|| "Open HUB is already up to date".to_string())?;
    let mut downloaded: u64 = 0;
    let progress_app = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress_app.emit("update-progress", Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| format!("update failed: {e}"))
}

/// Announces an update once per version (notification + tray item).
fn announce(app: &AppHandle, info: &UpdateInfo) {
    let state = app.state::<UpdateState>();
    let Ok(mut notified) = state.notified.lock() else { return };
    if notified.as_deref() == Some(info.version.as_str()) {
        return;
    }
    *notified = Some(info.version.clone());
    tray::notify(
        app,
        &format!("Open HUB {} is available", info.version),
        "Install it from the tray menu or from App settings.",
    );
}

/// Background checks: shortly after start, then every few hours. Installs
/// automatically only when the user turned that on.
pub fn spawn_background(app: AppHandle) {
    std::thread::Builder::new()
        .name("updater".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK);
            loop {
                let settings = app.state::<SettingsState>().get();
                if settings.check_updates {
                    match tauri::async_runtime::block_on(check(&app)) {
                        Ok(Some(info)) if settings.auto_install_updates => {
                            tray::notify(&app, "Updating Open HUB", &format!("Installing version {}…", info.version));
                            if let Err(e) = tauri::async_runtime::block_on(install(&app)) {
                                tray::notify(&app, "Open HUB update failed", &e);
                            }
                        }
                        Ok(Some(info)) => {
                            if crate::events::logging() {
                                eprintln!("[update] {} available", info.version);
                            }
                            announce(&app, &info)
                        }
                        Ok(None) => {
                            if crate::events::logging() {
                                eprintln!("[update] up to date ({})", app.package_info().version);
                            }
                        }
                        Err(e) => eprintln!("{e}"),
                    }
                }
                std::thread::sleep(CHECK_EVERY);
            }
        })
        .expect("could not start updater thread");
}

/// Tray "Check for updates": reports the result as a notification.
pub fn check_from_tray(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || match tauri::async_runtime::block_on(check(&app)) {
        Ok(Some(info)) => {
            tray::notify(&app, &format!("Open HUB {} is available", info.version), "Choose \"Install update\" in the tray menu.")
        }
        Ok(None) => tray::notify(&app, "Open HUB is up to date", &format!("You have version {}.", app.package_info().version)),
        Err(e) => tray::notify(&app, "Could not check for updates", &e),
    });
}

/// Tray "Install update".
pub fn install_from_tray(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        tray::notify(&app, "Updating Open HUB", "Downloading the update…");
        if let Err(e) = tauri::async_runtime::block_on(install(&app)) {
            tray::notify(&app, "Open HUB update failed", &e);
        }
    });
}
