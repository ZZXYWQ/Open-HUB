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
        let mut settings: AppSettings = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        settings.auto_install_updates = false;
        Self { settings: Mutex::new(settings), path }
    }

    fn lock(&self) -> Result<MutexGuard<'_, AppSettings>, String> {
        self.settings.lock().map_err(|_| "settings lock poisoned".to_string())
    }

    pub fn get(&self) -> AppSettings {
        self.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Replaces the settings and writes them via a temporary file.
    pub fn set(&self, mut settings: AppSettings) -> Result<(), String> {
        settings.auto_install_updates = false;
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

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn legacy_auto_install_is_disabled_on_load_and_save(){
  let dir=std::env::temp_dir().join(format!("open-hub-update-settings-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));std::fs::create_dir(&dir).unwrap();let path=dir.join("settings.json");
  std::fs::write(&path,br#"{"check_updates":true,"auto_install_updates":true}"#).unwrap();let state=SettingsState::load(path.clone());assert!(state.get().check_updates);assert!(!state.get().auto_install_updates);
  state.set(AppSettings{check_updates:false,auto_install_updates:true}).unwrap();let saved:AppSettings=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();assert!(!saved.check_updates&&!saved.auto_install_updates);
  std::fs::remove_file(path).unwrap();std::fs::remove_dir(dir).unwrap();
 }
}
