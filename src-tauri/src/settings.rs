//! App-wide settings (not per profile), stored in `settings.json` next to
//! `profiles.json` in the app data directory.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Check GitHub Releases for a newer version in the background.
    pub check_updates: bool,
    /// Download and install updates without asking.
    pub auto_install_updates: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self { check_updates: true, auto_install_updates: false }
    }
}

pub struct SettingsState {
    settings: Mutex<AppSettings>,
    path: PathBuf,
}

impl SettingsState {
    pub fn load(path: PathBuf) -> Self {
        let settings = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { settings: Mutex::new(settings), path }
    }

    fn lock(&self) -> Result<MutexGuard<'_, AppSettings>, String> {
        self.settings.lock().map_err(|_| "settings lock poisoned".to_string())
    }

    pub fn get(&self) -> AppSettings {
        self.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Replaces the settings and writes them via a temporary file.
    pub fn set(&self, settings: AppSettings) -> Result<(), String> {
        write_atomic(&self.path, &settings).map_err(|e| format!("could not save settings: {e}"))?;
        *self.lock()? = settings;
        Ok(())
    }
}

fn write_atomic(path: &Path, settings: &AppSettings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(settings)?)?;
    std::fs::rename(tmp, path)
}
