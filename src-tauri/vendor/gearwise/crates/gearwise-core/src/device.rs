//! The contract every driver implements, and what the UI renders from.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::keys::KeyboardLayout;
use crate::model::*;
use crate::transport::{HidBackend, HidInterfaceInfo};

/// Identity of a connected device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceDescriptor {
    /// Unique for this session: `vendor:pid:serial-or-path[:index]`.
    pub id: String,
    /// Stable per model, used to key profiles: `vendor:pid`.
    pub model_key: String,
    pub vendor: Vendor,
    pub kind: DeviceKind,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub connection: Connection,
    pub firmware: Option<String>,
    pub serial: Option<String>,
    /// Simulated device (demo mode); never touches hardware.
    pub simulated: bool,
}

impl DeviceDescriptor {
    pub fn model_key_for(vendor: Vendor, pid: u16) -> String {
        format!("{}:{:04x}", vendor.key(), pid)
    }
}

/// What a device can do. Empty / `None` fields mean "not supported" and the
/// UI hides the corresponding controls.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub dpi: Option<DpiCaps>,
    /// Supported polling rates in Hz, ascending.
    pub report_rates: Vec<u16>,
    pub lighting: Vec<LightingZoneCaps>,
    /// Per-key RGB layout, for keyboards with individually addressable keys.
    pub per_key: Option<KeyboardLayout>,
    /// Physical layout to draw (macro keys, media keys, ...) when it differs
    /// from `per_key` or the keyboard has zone lighting only. `None` lets the
    /// UI fall back to `per_key` or a generic board.
    #[serde(default)]
    pub layout: Option<KeyboardLayout>,
    pub buttons: Vec<ButtonCaps>,
    /// Number of onboard (on-device) profile slots; 0 if none.
    pub onboard_profiles: u8,
    pub battery: bool,
    /// Lift-off distance choices, user-facing labels ("1 mm", "2 mm").
    pub lift_off: Vec<String>,
    pub angle_snapping: bool,
    /// Keyboard "game mode" (disables Super/Alt+Tab etc.).
    pub game_mode: bool,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub tuning: Option<crate::tuning::TuningCaps>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub vendor_profile: Option<String>,
}

/// Current values of everything in [`Capabilities`]. Drivers fill what they
/// can read back; write-only settings report the last value applied.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceState {
    pub dpi: Option<DpiState>,
    pub report_rate: Option<u16>,
    /// Keyed by zone id.
    pub lighting: BTreeMap<u8, ZoneState>,
    /// Keyed by key id (see [`crate::keys::KeyCell::id`]).
    pub per_key: BTreeMap<u16, Rgb>,
    /// Keyed by button id; absent means [`ButtonAction::Default`].
    pub buttons: BTreeMap<u16, ButtonAction>,
    pub onboard_profile: Option<u8>,
    pub battery: Option<Battery>,
    /// Index into [`Capabilities::lift_off`].
    pub lift_off: Option<u8>,
    pub angle_snapping: Option<bool>,
    pub game_mode: Option<bool>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub tuning: Option<crate::tuning::MouseTuning>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub vendor_profile: Option<serde_json::Value>,
}

/// A single change to apply to a device. Profiles are lists of these.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Setting {
    Dpi(DpiState),
    /// Switch the active stage only.
    DpiActiveStage(u8),
    ReportRate(u16),
    Lighting {
        zone: u8,
        state: ZoneState,
    },
    /// Per-key colors; keys not listed keep their color.
    PerKey(BTreeMap<u16, Rgb>),
    Button {
        id: u16,
        action: ButtonAction,
    },
    OnboardProfile(u8),
    LiftOff(u8),
    AngleSnapping(bool),
    GameMode(bool),
    Tuning(crate::tuning::MouseTuning),
    /// Strict driver-owned schema for a complete native profile. Unsupported
    /// drivers must reject this; never a raw USB command passthrough.
    VendorProfile {schema:String,data:serde_json::Value},
}

impl Setting {
    pub fn label(&self) -> &'static str {
        match self {
            Setting::Dpi(_) => "DPI stages",
            Setting::DpiActiveStage(_) => "Active DPI stage",
            Setting::ReportRate(_) => "Report rate",
            Setting::Lighting { .. } => "Lighting",
            Setting::PerKey(_) => "Per-key lighting",
            Setting::Button { .. } => "Button assignment",
            Setting::OnboardProfile(_) => "Onboard profile",
            Setting::LiftOff(_) => "Lift-off distance",
            Setting::AngleSnapping(_) => "Angle snapping",
            Setting::GameMode(_) => "Game mode",
            Setting::Tuning(_) => "Mouse performance",
            Setting::VendorProfile{..} => "Complete native profile",
        }
    }

    /// Fold this setting into a state snapshot (used by drivers that cannot
    /// read values back, by the demo devices, and by the UI for optimistic
    /// updates).
    pub fn apply_to(&self, s: &mut DeviceState) {
        match self {
            Setting::Dpi(d) => s.dpi = Some(d.clone()),
            Setting::DpiActiveStage(i) => {
                if let Some(d) = s.dpi.as_mut() {
                    if (*i as usize) < d.stages.len() {
                        d.active = *i;
                    }
                }
            }
            Setting::ReportRate(r) => s.report_rate = Some(*r),
            Setting::Lighting { zone, state } => {
                s.lighting.insert(*zone, *state);
            }
            Setting::PerKey(m) => s.per_key.extend(m.iter().map(|(k, v)| (*k, *v))),
            Setting::Button { id, action } => {
                if *action == ButtonAction::Default {
                    s.buttons.remove(id);
                } else {
                    s.buttons.insert(*id, action.clone());
                }
            }
            Setting::OnboardProfile(p) => s.onboard_profile = Some(*p),
            Setting::LiftOff(l) => s.lift_off = Some(*l),
            Setting::AngleSnapping(b) => s.angle_snapping = Some(*b),
            Setting::GameMode(b) => s.game_mode = Some(*b),
            Setting::Tuning(patch)=>patch.merge(s.tuning.get_or_insert_with(Default::default)),
            Setting::VendorProfile{data,..}=>s.vendor_profile=Some(data.clone()),
        }
    }
}

/// Things a device reports on its own.
#[derive(Clone, Debug, PartialEq)]
pub enum DeviceEvent {
    /// A button diverted to software was pressed (macro trigger).
    ButtonDown(u16),
    ButtonUp(u16),
    BatteryChanged(Battery),
    /// The device changed a value itself (on-device DPI button, profile switch).
    StateChanged,
    /// A wireless device went to sleep / out of range.
    Disconnected,
    Reconnected,
}

/// A connected, configurable device.
pub trait Device: Send {
    /// Exact control collection and ephemeral open-instance identity. Opt-in
    /// only; neither is a durable physical mouse serial or a profile owner.
    fn control_path(&self)->Option<&str>{None}
    fn connection_key(&self)->Option<&str>{None}
    fn descriptor(&self) -> &DeviceDescriptor;
    fn capabilities(&self) -> &Capabilities;

    /// Read every value the device exposes.
    fn read_state(&mut self) -> Result<DeviceState>;

    /// Apply one setting. Drivers must validate against their capabilities and
    /// return [`crate::Error::Unsupported`] / [`crate::Error::InvalidValue`]
    /// rather than sending garbage to firmware.
    fn apply(&mut self, setting: &Setting) -> Result<()>;
    /// Send a complete reviewed table (excluding protected main click) in one
    /// native command. Opt-in only; default rejects without any I/O.
    fn apply_button_table(&mut self,_actions:&[(u16,ButtonAction)])->Result<()>{Err(crate::Error::Unsupported("complete native button table"))}


    /// Drain pending notifications without blocking for long. Called
    /// periodically by the device manager's worker thread.
    fn poll_events(&mut self) -> Vec<DeviceEvent> {
        Vec::new()
    }

    /// Persist the current settings to onboard memory where the device keeps
    /// them only in RAM until told otherwise. Default: nothing to do.
    /// Open HUB: release host-owned input routing without changing DPI or lighting.
    fn stop_software_actions(&mut self)->Result<()>{Ok(())}
    /// Explicitly reviewed recovery after restart, when in-process routing
    /// ownership is unknown. Call only after exact-target user confirmation.
    fn restore_hardware_input(&mut self)->Result<()>{Err(crate::Error::Unsupported("manual hardware input recovery"))}

    /// Prepare native macro definitions in memory. Drivers opt in explicitly.
    /// This method must not write hardware; bindings are applied separately.
    fn set_macro_definitions(&mut self, definitions:&[crate::profile::Macro])->Result<()>{
        if definitions.is_empty(){Ok(())}else{Err(crate::Error::Unsupported("native macro definitions"))}
    }
    /// Definitions decoded from the latest hardware read, excluding unknown data.
    fn macro_definitions(&self)->Vec<crate::profile::Macro>{Vec::new()}

    fn save_onboard(&mut self) -> Result<()> {
        Ok(())
    }
}

/// A vendor driver: recognises HID interfaces and builds devices from them.
pub trait Driver: Send + Sync {
    fn name(&self) -> &'static str;
    fn vendor(&self) -> Vendor;

    /// True if this driver wants to open `iface`. Physical devices expose
    /// several interfaces; a driver should claim exactly the control one.
    fn probe(&self, iface: &HidInterfaceInfo) -> bool;

    /// Open the interface and return the devices behind it. A receiver may
    /// return several (or none, if nothing paired is awake).
    fn open(
        &self,
        iface: &HidInterfaceInfo,
        backend: &dyn HidBackend,
    ) -> Result<Vec<Box<dyn Device>>>;
}
