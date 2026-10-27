//! Host-side profiles and macros, persisted as one JSON file.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::device::{Capabilities, DeviceState, Setting};
use crate::error::Result;
use crate::model::{ButtonAction, Modifiers, MouseButton, ScrollDirection};

/// A named set of settings for any number of device models.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// Device model key (`vendor:pid`) → settings to apply, in order.
    #[serde(default)]
    pub devices: BTreeMap<String, Vec<Setting>>,
    /// Executable names (`game.exe`, `blender`) that activate this profile
    /// when their window has focus. Empty means manual only.
    #[serde(default)]
    pub apps: Vec<String>,
}

impl Profile {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: new_id("p"),
            name: name.into(),
            devices: BTreeMap::new(),
            apps: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MacroMode {
    /// Play once per press.
    Once,
    /// Loop while the button is held.
    WhileHeld,
    /// First press starts looping, second press stops.
    Toggle,
    /// Repeat a finite number of complete plays. Native ranges are driver-specific.
    Repeat(u8),
}

impl MacroMode {
    /// Parameter-free presets; finite repeat is configured separately.
    pub const ALL: [MacroMode; 3] = [MacroMode::Once, MacroMode::WhileHeld, MacroMode::Toggle];
    pub fn name(self) -> &'static str {
        match self {
            MacroMode::Once => "Play once",
            MacroMode::WhileHeld => "Repeat while held",
            MacroMode::Toggle => "Toggle repeat",
            MacroMode::Repeat(_) => "Repeat a fixed count",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MacroStep {
    KeyDown(u8),
    KeyUp(u8),
    /// Press and release, with modifiers held around it.
    KeyTap {
        usage: u8,
        mods: Modifiers,
    },
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    /// Atomic HID consumer-page tap (no held consumer state).
    Media(u16),
    /// One vertical wheel tick; no held input state.
    Scroll(ScrollDirection),
    /// Wait, in milliseconds.
    Delay(u32),
    /// Type a string (layout-dependent; best effort).
    Text(String),
    /// Open HUB internal direct-binding hold; released by its physical source.
    WaitForRelease,
}

impl MacroStep {
    pub fn label(&self) -> String {
        let key = |u: &u8| crate::keys::name(*u).unwrap_or("?").to_string();
        match self {
            MacroStep::KeyDown(u) => format!("↓ {}", key(u)),
            MacroStep::KeyUp(u) => format!("↑ {}", key(u)),
            MacroStep::KeyTap { usage, mods } => ButtonAction::Key {
                usage: *usage,
                mods: *mods,
            }
            .label(),
            MacroStep::MouseDown(b) => format!("↓ {}", b.name()),
            MacroStep::MouseUp(b) => format!("↑ {}", b.name()),
            MacroStep::Media(usage) => ButtonAction::Media(*usage).label(),
            MacroStep::Scroll(direction) => ButtonAction::Scroll(*direction).label(),
            MacroStep::Delay(ms) => format!("Wait {ms} ms"),
            MacroStep::WaitForRelease => "Hold until physical release".into(),
            MacroStep::Text(t) => format!("Type \"{t}\""),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    pub id: String,
    pub name: String,
    pub mode: MacroMode,
    pub steps: Vec<MacroStep>,
}

impl Macro {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: new_id("m"),
            name: name.into(),
            mode: MacroMode::Once,
            steps: Vec::new(),
        }
    }

    /// Total scripted delay, for display.
    pub fn duration_ms(&self) -> u64 {
        self.steps
            .iter()
            .map(|s| {
                if let MacroStep::Delay(d) = s {
                    *d as u64
                } else {
                    0
                }
            })
            .sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    pub theme: Theme,
    /// Accent color as hex.
    pub accent: String,
    /// Switch profiles automatically based on the focused application.
    pub auto_switch: bool,
    /// Show simulated devices even when real hardware is present.
    pub demo_devices: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            accent: "#00d4aa".into(),
            auto_switch: false,
            demo_devices: false,
        }
    }
}

/// Everything the app persists.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Library {
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub macros: Vec<Macro>,
    #[serde(default)]
    pub active_profile: Option<String>,
    #[serde(default)]
    pub prefs: Preferences,
}

impl Library {
    /// Load from `path`; a missing file yields a default library with one
    /// empty profile.
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::starter()),
            Err(e) => Err(e.into()),
        }
    }

    /// Write atomically (temp file + rename) so a crash never leaves a
    /// truncated library.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn starter() -> Self {
        let p = Profile::new("Default");
        Self {
            active_profile: Some(p.id.clone()),
            profiles: vec![p],
            ..Default::default()
        }
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn profile_mut(&mut self, id: &str) -> Option<&mut Profile> {
        self.profiles.iter_mut().find(|p| p.id == id)
    }

    pub fn macro_by_id(&self, id: &str) -> Option<&Macro> {
        self.macros.iter().find(|m| m.id == id)
    }

    /// The profile that should be active for a focused executable, if any.
    pub fn profile_for_app(&self, exe: &str) -> Option<&Profile> {
        let exe = exe.to_ascii_lowercase();
        self.profiles
            .iter()
            .find(|p| p.apps.iter().any(|a| a.to_ascii_lowercase() == exe))
    }
}

/// Default location of the library file.
pub fn default_library_path() -> PathBuf {
    let path = config_dir().join("gearwise").join("library.json");
    migrate_legacy_library(&path);
    path
}

/// The app used to be called Periph Studio; carry its library over once.
fn migrate_legacy_library(path: &Path) {
    let legacy = config_dir().join("periph-studio").join("library.json");
    if !path.exists() && legacy.exists() {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Err(e) = fs::copy(&legacy, path) {
            log::warn!("could not migrate {}: {e}", legacy.display());
        }
    }
}

fn config_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(p) = std::env::var_os("APPDATA") {
            return PathBuf::from(p);
        }
    }
    if let Some(p) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(p);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config");
    }
    PathBuf::from(".")
}

/// Convert a device snapshot into the settings that recreate it — used to
/// save "current device state" into a profile.
pub fn settings_from_state(caps: &Capabilities, state: &DeviceState) -> Vec<Setting> {
    let mut out = Vec::new();
    if caps.onboard_profiles > 0 {
        if let Some(p) = state.onboard_profile {
            out.push(Setting::OnboardProfile(p));
        }
    }
    if let (Some(_), Some(d)) = (&caps.dpi, &state.dpi) {
        out.push(Setting::Dpi(d.clone()));
    }
    if let Some(r) = state.report_rate.filter(|r| caps.report_rates.contains(r)) {
        out.push(Setting::ReportRate(r));
    }
    for z in &caps.lighting {
        if let Some(s) = state.lighting.get(&z.id) {
            out.push(Setting::Lighting {
                zone: z.id,
                state: *s,
            });
        }
    }
    if caps.per_key.is_some() && !state.per_key.is_empty() {
        out.push(Setting::PerKey(state.per_key.clone()));
    }
    for b in caps.buttons.iter().filter(|b| b.remappable) {
        let action = state
            .buttons
            .get(&b.id)
            .cloned()
            .unwrap_or(ButtonAction::Default);
        out.push(Setting::Button { id: b.id, action });
    }
    if !caps.lift_off.is_empty() {
        if let Some(l) = state.lift_off {
            out.push(Setting::LiftOff(l));
        }
    }
    if caps.angle_snapping {
        if let Some(a) = state.angle_snapping {
            out.push(Setting::AngleSnapping(a));
        }
    }
    if caps.game_mode {
        if let Some(g) = state.game_mode {
            out.push(Setting::GameMode(g));
        }
    }
    out
}

/// Short unique id: prefix + time-based hex + counter.
pub fn new_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}{:x}{:03x}", t, c & 0xfff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    #[test]
    fn library_round_trips_through_json() {
        let mut lib = Library::starter();
        let mut m = Macro::new("Combo");
        m.steps = vec![
            MacroStep::KeyTap {
                usage: 0x04,
                mods: Modifiers(Modifiers::LCTRL),
            },
            MacroStep::Delay(50),
            MacroStep::Text("gg".into()),
        ];
        lib.macros.push(m);
        lib.profiles[0].apps.push("game.exe".into());
        lib.profiles[0].devices.insert(
            "logitech:c094".into(),
            vec![
                Setting::ReportRate(1000),
                Setting::Lighting {
                    zone: 0,
                    state: ZoneState {
                        effect: LightingEffect::Static {
                            color: Rgb::new(1, 2, 3),
                        },
                        brightness: 80,
                    },
                },
                Setting::Button {
                    id: 0x52,
                    action: ButtonAction::Macro("m1".into()),
                },
            ],
        );
        let dir = std::env::temp_dir().join(format!("gearwise-test-{}", new_id("t")));
        let path = dir.join("library.json");
        lib.save(&path).unwrap();
        let back = Library::load(&path).unwrap();
        assert_eq!(lib, back);
        assert!(back.profile_for_app("GAME.EXE").is_some());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_file_gives_starter() {
        let lib = Library::load(Path::new("/nonexistent/gearwise/library.json")).unwrap();
        assert_eq!(lib.profiles.len(), 1);
        assert_eq!(
            lib.active_profile.as_deref(),
            Some(lib.profiles[0].id.as_str())
        );
    }

    #[test]
    fn ids_are_unique() {
        let a = new_id("p");
        let b = new_id("p");
        assert_ne!(a, b);
    }
}

#[cfg(test)]mod finite_mode_json_tests{use super::*;#[test]fn finite_mode_keeps_existing_mode_json_and_has_an_explicit_count(){for (mode,json) in [(MacroMode::Once,serde_json::json!("Once")),(MacroMode::Repeat(3),serde_json::json!({"Repeat":3}))]{assert_eq!(serde_json::to_value(mode).unwrap(),json);assert_eq!(serde_json::from_value::<MacroMode>(json).unwrap(),mode);}for json in [serde_json::json!({"Repeat":-1}),serde_json::json!({"Repeat":256}),serde_json::json!({"Repeat":2.5}),serde_json::json!("Repeat")]{assert!(serde_json::from_value::<MacroMode>(json).is_err());}}}

#[cfg(test)]mod scroll_json_tests{use super::*;#[test]fn atomic_scroll_json_preserves_direction_and_existing_step_format(){for direction in[ScrollDirection::Up,ScrollDirection::Down]{let step=MacroStep::Scroll(direction);let value=serde_json::to_value(&step).unwrap();assert_eq!(value,serde_json::json!({"Scroll":if direction==ScrollDirection::Up{"Up"}else{"Down"}}));assert_eq!(serde_json::from_value::<MacroStep>(value).unwrap(),step);}for v in[serde_json::json!({"Scroll":"Left"}),serde_json::json!({"Scroll":1}),serde_json::json!({"Scroll":{"direction":"Up"}})]{assert!(serde_json::from_value::<MacroStep>(v).is_err());}assert_eq!(serde_json::to_value(MacroStep::KeyDown(4)).unwrap(),serde_json::json!({"KeyDown":4}));}}
