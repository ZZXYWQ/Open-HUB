//! Profiles: per-application DPI and report-rate settings, modelled on G HUB.
//!
//! G HUB stores Application -> Profile -> (slot -> card) in one JSON document
//! inside `%LOCALAPPDATA%\LGHUB\settings.db`. Open GHub keeps a flatter model
//! (one settings set per profile, applied to every connected mouse) in
//! `profiles.json` under the app data directory.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::device::Device;
use crate::hidpp::Transport;

pub const DESKTOP_ID: &str = "desktop";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Level {
    pub x: u16,
    /// None means Y follows X.
    pub y: Option<u16>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// Executable path or install folder that activates this profile.
    /// None for the Desktop profile, which is used when nothing else matches.
    pub app_path: Option<String>,
    pub levels: Vec<Level>,
    pub active_level: usize,
    pub report_rate_hz: Option<u32>,
    /// G HUB profile id when imported, so re-imports update instead of duplicating.
    #[serde(default)]
    pub ghub_id: Option<String>,
}

impl Profile {
    pub fn desktop() -> Self {
        Self {
            id: DESKTOP_ID.into(),
            name: "Desktop".into(),
            app_path: None,
            levels: [400, 800, 1600, 3200, 6400].map(|x| Level { x, y: None }).to_vec(),
            active_level: 2,
            report_rate_hz: Some(1000),
            ghub_id: None,
        }
    }

    pub fn active(&self) -> Option<&Level> {
        self.levels.get(self.active_level).or_else(|| self.levels.first())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileStore {
    pub profiles: Vec<Profile>,
    pub auto_switch: bool,
    pub active_id: String,
}

impl Default for ProfileStore {
    fn default() -> Self {
        // Auto-switch starts off so DPI never changes until the user opts in.
        Self { profiles: vec![Profile::desktop()], auto_switch: false, active_id: DESKTOP_ID.into() }
    }
}

pub fn new_id() -> String {
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("p{nanos:x}{n:x}")
}

fn normalize(path: &str) -> String {
    path.trim().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

impl ProfileStore {
    pub fn load(path: &Path) -> Self {
        let mut store: Self = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        if !store.profiles.iter().any(|p| p.id == DESKTOP_ID) {
            store.profiles.insert(0, Profile::desktop());
        }
        if !store.profiles.iter().any(|p| p.id == store.active_id) {
            store.active_id = DESKTOP_ID.into();
        }
        store
    }

    /// Writes via a temporary file so a crash never leaves a truncated store.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }

    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn upsert(&mut self, mut profile: Profile) {
        profile.active_level = profile.active_level.min(profile.levels.len().saturating_sub(1));
        if profile.id == DESKTOP_ID {
            profile.app_path = None;
        }
        match self.profiles.iter_mut().find(|p| p.id == profile.id) {
            Some(existing) => *existing = profile,
            None => self.profiles.push(profile),
        }
    }

    pub fn delete(&mut self, id: &str) {
        if id == DESKTOP_ID {
            return;
        }
        self.profiles.retain(|p| p.id != id);
        if self.active_id == id {
            self.active_id = DESKTOP_ID.into();
        }
    }

    /// Profile for a foreground executable: the longest matching app path
    /// (exact exe, or any exe inside the profile's folder), else Desktop.
    pub fn match_exe(&self, exe: &str) -> &Profile {
        let exe = normalize(exe);
        self.profiles
            .iter()
            .filter_map(|p| {
                let path = normalize(p.app_path.as_deref()?);
                let hit = !path.is_empty() && (exe == path || exe.starts_with(&format!("{path}\\")));
                hit.then_some((path.len(), p))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, p)| p)
            .unwrap_or_else(|| self.get(DESKTOP_ID).unwrap_or(&self.profiles[0]))
    }
}

/// Applies a profile's active DPI level and report rate to one device.
/// Values are snapped to what the device supports. Returns per-setting errors.
pub fn apply_to_device(profile: &Profile, device: &Device, t: &Transport) -> Vec<String> {
    let mut errors = Vec::new();
    if let Some(level) = profile.active() {
        match device.dpi(t) {
            Ok(Some(dpi)) => {
                let snap = |v: u16| {
                    dpi.supported.iter().copied().min_by_key(|s| s.abs_diff(v)).unwrap_or(v)
                };
                let x = snap(level.x);
                let y = dpi.supports_y.then(|| snap(level.y.unwrap_or(level.x)));
                if let Err(e) = device.set_dpi(t, x, y, None) {
                    errors.push(format!("{}: DPI: {e}", device.name));
                }
            }
            Ok(None) => {}
            Err(e) => errors.push(format!("{}: DPI: {e}", device.name)),
        }
    }
    if let Some(hz) = profile.report_rate_hz {
        match device.report_rate(t) {
            Ok(Some(rate)) => {
                if let Some(choice) = rate.choices.iter().min_by_key(|c| c.hz.abs_diff(hz)) {
                    if choice.value != rate.current {
                        if let Err(e) = device.set_report_rate(t, choice.value) {
                            errors.push(format!("{}: report rate: {e}", device.name));
                        }
                    }
                }
            }
            Ok(None) => {}
            Err(e) => errors.push(format!("{}: report rate: {e}", device.name)),
        }
    }
    errors
}

// ---------- G HUB import ----------

#[derive(Serialize)]
pub struct ImportSummary {
    pub imported: usize,
    pub updated: usize,
    pub skipped: usize,
}

pub fn ghub_settings_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("LGHUB").join("settings.db"))
}

/// Reads G HUB's settings document without modifying it.
fn read_ghub_document(path: &Path) -> Result<Value, String> {
    use rusqlite::{Connection, OpenFlags};
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|e| format!("could not open G HUB settings: {e}"))?;
    let blob: Vec<u8> = conn
        .query_row("SELECT file FROM data ORDER BY _id DESC LIMIT 1", [], |row| row.get(0))
        .map_err(|e| format!("could not read G HUB settings: {e}"))?;
    serde_json::from_slice(&blob).map_err(|e| format!("G HUB settings are not valid JSON: {e}"))
}

fn ghub_list<'a>(doc: &'a Value, key: &str) -> &'a [Value] {
    doc.get(key).and_then(|v| v.get(key)).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

/// Converts G HUB profiles (one per application) into Open GHub profiles,
/// taking DPI levels and report rate from each profile's MOUSE_SETTINGS card.
pub fn import_ghub(store: &mut ProfileStore, path: &Path) -> Result<ImportSummary, String> {
    let doc = read_ghub_document(path)?;
    let apps = ghub_list(&doc, "applications");
    let cards = ghub_list(&doc, "cards");
    let mut summary = ImportSummary { imported: 0, updated: 0, skipped: 0 };

    for gp in ghub_list(&doc, "profiles") {
        let str_of = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let (Some(gid), Some(app_id)) = (str_of(gp, "id"), str_of(gp, "applicationId")) else {
            summary.skipped += 1;
            continue;
        };
        let Some(app) = apps.iter().find(|a| a.get("applicationId").and_then(Value::as_str) == Some(app_id.as_str())) else {
            summary.skipped += 1; // profile whose application no longer exists
            continue;
        };

        // The MOUSE_SETTINGS card assigned to the "<device>_mouse_settings" slot.
        let mouse = gp
            .get("assignments")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|a| a.get("slotId").and_then(Value::as_str).is_some_and(|s| s.ends_with("_mouse_settings")))
            .filter_map(|a| a.get("cardId").and_then(Value::as_str))
            .find_map(|cid| cards.iter().find(|c| c.get("id").and_then(Value::as_str) == Some(cid)))
            .and_then(|c| c.get("mouseSettings"));
        let Some(mouse) = mouse else {
            summary.skipped += 1;
            continue;
        };

        let table = mouse.get("dpiTable");
        let levels: Vec<Level> = table
            .and_then(|t| t.get("levels"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_u64().or_else(|| v.get("x").and_then(Value::as_u64)))
            .map(|x| Level { x: x.min(u64::from(u16::MAX)) as u16, y: None })
            .collect();
        if levels.is_empty() {
            summary.skipped += 1;
            continue;
        }
        let active_dpi = table.and_then(|t| t.get("activeDpi")).and_then(Value::as_u64);
        let active_level = active_dpi
            .and_then(|a| levels.iter().position(|l| u64::from(l.x) == a))
            .unwrap_or(0);
        let report_rate_hz = mouse.get("reportRate").and_then(|r| r.get("value")).and_then(Value::as_u64).map(|v| v as u32);

        let app_name = str_of(app, "name").unwrap_or_default();
        let is_desktop = app_name == "APPLICATION_NAME_DESKTOP";
        let profile_name = str_of(gp, "name").unwrap_or_default();
        let name = match (is_desktop, profile_name.as_str()) {
            (true, _) => "Desktop".to_string(),
            (false, "PROFILE_NAME_DEFAULT" | "") => app_name.clone(),
            (false, other) => format!("{app_name}: {other}"),
        };
        // Only the profile G HUB uses for the application switches automatically.
        let active_for_app = gp.get("activeForApplication").and_then(Value::as_bool).unwrap_or(false);
        let app_path = (!is_desktop && active_for_app)
            .then(|| str_of(app, "applicationPath").or_else(|| str_of(app, "applicationFolder")))
            .flatten()
            .filter(|p| !p.trim().is_empty());

        let existing_id = if is_desktop {
            Some(DESKTOP_ID.to_string())
        } else {
            store.profiles.iter().find(|p| p.ghub_id.as_deref() == Some(gid.as_str())).map(|p| p.id.clone())
        };
        let updated = existing_id.is_some();
        store.upsert(Profile {
            id: existing_id.unwrap_or_else(new_id),
            name,
            app_path,
            levels,
            active_level,
            report_rate_hz,
            ghub_id: Some(gid),
        });
        if updated {
            summary.updated += 1;
        } else {
            summary.imported += 1;
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, path: Option<&str>) -> Profile {
        Profile { id: id.into(), name: id.into(), app_path: path.map(Into::into), ..Profile::desktop() }
    }

    #[test]
    fn matches_folder_exe_and_falls_back_to_desktop() {
        let mut store = ProfileStore::default();
        store.upsert(profile("valorant", Some(r"H:\Riot Games\VALORANT\live\")));
        store.upsert(profile("game", Some(r"C:\Games\Game\game.exe")));
        assert_eq!(store.match_exe(r"h:\riot games\valorant\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe").id, "valorant");
        assert_eq!(store.match_exe(r"C:\Games\Game\game.exe").id, "game");
        assert_eq!(store.match_exe(r"C:\Games\Game\other.exe").id, DESKTOP_ID);
        assert_eq!(store.match_exe(r"H:\Riot Games\VALORANT\live2\x.exe").id, DESKTOP_ID);
    }

    #[test]
    fn desktop_cannot_be_deleted() {
        let mut store = ProfileStore::default();
        store.delete(DESKTOP_ID);
        assert!(store.get(DESKTOP_ID).is_some());
    }
}
