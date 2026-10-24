//! Vendor-neutral value types shared by drivers, profiles and the UI.

use std::fmt;

use serde::{Deserialize, Serialize};

pub const VID_LOGITECH: u16 = 0x046d;
pub const VID_RAZER: u16 = 0x1532;
pub const VID_CORSAIR: u16 = 0x1b1c;
pub const VID_STEELSERIES: u16 = 0x1038;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Vendor {
    Logitech,
    Razer,
    Corsair,
    SteelSeries,
    Other,
}

impl Vendor {
    pub fn from_vid(vid: u16) -> Self {
        match vid {
            VID_LOGITECH => Vendor::Logitech,
            VID_RAZER => Vendor::Razer,
            VID_CORSAIR => Vendor::Corsair,
            VID_STEELSERIES => Vendor::SteelSeries,
            _ => Vendor::Other,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Vendor::Logitech => "Logitech",
            Vendor::Razer => "Razer",
            Vendor::Corsair => "Corsair",
            Vendor::SteelSeries => "SteelSeries",
            Vendor::Other => "Other",
        }
    }

    /// Short lowercase key used in device ids and profile keys.
    pub fn key(self) -> &'static str {
        match self {
            Vendor::Logitech => "logitech",
            Vendor::Razer => "razer",
            Vendor::Corsair => "corsair",
            Vendor::SteelSeries => "steelseries",
            Vendor::Other => "other",
        }
    }
}

impl fmt::Display for Vendor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeviceKind {
    Mouse,
    Keyboard,
    Headset,
    Mousepad,
    Receiver,
    Other,
}

impl DeviceKind {
    pub fn name(self) -> &'static str {
        match self {
            DeviceKind::Mouse => "Mouse",
            DeviceKind::Keyboard => "Keyboard",
            DeviceKind::Headset => "Headset",
            DeviceKind::Mousepad => "Mousepad",
            DeviceKind::Receiver => "Receiver",
            DeviceKind::Other => "Device",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Connection {
    Wired,
    /// 2.4 GHz through a vendor receiver (Lightspeed, HyperSpeed, Slipstream).
    Wireless,
    Bluetooth,
}

impl Connection {
    pub fn name(self) -> &'static str {
        match self {
            Connection::Wired => "Wired",
            Connection::Wireless => "Wireless",
            Connection::Bluetooth => "Bluetooth",
        }
    }
}

// ---------------------------------------------------------------------------
// Color
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);
    pub const WHITE: Rgb = Rgb::new(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 {
            return None;
        }
        let v = u32::from_str_radix(s, 16).ok()?;
        Some(Rgb::new((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }

    /// Scale by a 0–100 brightness percentage.
    pub fn scaled(self, brightness: u8) -> Self {
        let b = brightness.min(100) as u16;
        Rgb::new(
            (self.r as u16 * b / 100) as u8,
            (self.g as u16 * b / 100) as u8,
            (self.b as u16 * b / 100) as u8,
        )
    }

    /// Hue wheel position in degrees (0–360), full saturation and value.
    pub fn from_hue(hue: f32) -> Self {
        let h = hue.rem_euclid(360.0) / 60.0;
        let x = 1.0 - (h % 2.0 - 1.0).abs();
        let (r, g, b) = match h as u32 {
            0 => (1.0, x, 0.0),
            1 => (x, 1.0, 0.0),
            2 => (0.0, 1.0, x),
            3 => (0.0, x, 1.0),
            4 => (x, 0.0, 1.0),
            _ => (1.0, 0.0, x),
        };
        Rgb::new((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
    }
}

// ---------------------------------------------------------------------------
// Sensor
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DpiCaps {
    pub min: u16,
    pub max: u16,
    pub step: u16,
    /// How many DPI stages the device can hold (onboard or emulated).
    pub max_stages: u8,
    /// Whether X and Y resolution can differ.
    pub separate_xy: bool,
}

impl DpiCaps {
    /// Clamp and snap a value to this sensor's range.
    pub fn snap(&self, dpi: u16) -> u16 {
        let step = self.step.max(1);
        let v = dpi.clamp(self.min, self.max);
        let snapped = self.min + ((v - self.min) + step / 2) / step * step;
        snapped.min(self.max)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DpiStage {
    pub x: u16,
    pub y: u16,
}

impl DpiStage {
    pub const fn uniform(dpi: u16) -> Self {
        Self { x: dpi, y: dpi }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DpiState {
    pub stages: Vec<DpiStage>,
    /// Index into `stages`.
    pub active: u8,
}

impl DpiState {
    pub fn current(&self) -> Option<DpiStage> {
        self.stages.get(self.active as usize).copied()
    }
}

// ---------------------------------------------------------------------------
// Lighting
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectKind {
    Off,
    Static,
    Breathing,
    ColorCycle,
    Wave,
    Reactive,
}

impl EffectKind {
    pub const ALL: [EffectKind; 6] = [
        EffectKind::Off,
        EffectKind::Static,
        EffectKind::Breathing,
        EffectKind::ColorCycle,
        EffectKind::Wave,
        EffectKind::Reactive,
    ];

    pub fn name(self) -> &'static str {
        match self {
            EffectKind::Off => "Off",
            EffectKind::Static => "Static",
            EffectKind::Breathing => "Breathing",
            EffectKind::ColorCycle => "Color cycle",
            EffectKind::Wave => "Wave",
            EffectKind::Reactive => "Reactive",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaveDirection {
    LeftToRight,
    RightToLeft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightingEffect {
    Off,
    Static {
        color: Rgb,
    },
    Breathing {
        color: Rgb,
        period_ms: u16,
    },
    ColorCycle {
        period_ms: u16,
    },
    Wave {
        period_ms: u16,
        direction: WaveDirection,
    },
    /// Keys light up when pressed and fade out.
    Reactive {
        color: Rgb,
        duration_ms: u16,
    },
}

impl LightingEffect {
    pub fn kind(&self) -> EffectKind {
        match self {
            LightingEffect::Off => EffectKind::Off,
            LightingEffect::Static { .. } => EffectKind::Static,
            LightingEffect::Breathing { .. } => EffectKind::Breathing,
            LightingEffect::ColorCycle { .. } => EffectKind::ColorCycle,
            LightingEffect::Wave { .. } => EffectKind::Wave,
            LightingEffect::Reactive { .. } => EffectKind::Reactive,
        }
    }

    /// A sensible default instance of an effect kind, keeping `color` where
    /// the effect uses one.
    pub fn default_for(kind: EffectKind, color: Rgb) -> Self {
        match kind {
            EffectKind::Off => LightingEffect::Off,
            EffectKind::Static => LightingEffect::Static { color },
            EffectKind::Breathing => LightingEffect::Breathing {
                color,
                period_ms: 4000,
            },
            EffectKind::ColorCycle => LightingEffect::ColorCycle { period_ms: 10_000 },
            EffectKind::Wave => LightingEffect::Wave {
                period_ms: 3000,
                direction: WaveDirection::LeftToRight,
            },
            EffectKind::Reactive => LightingEffect::Reactive {
                color,
                duration_ms: 500,
            },
        }
    }

    pub fn color(&self) -> Option<Rgb> {
        match *self {
            LightingEffect::Static { color }
            | LightingEffect::Breathing { color, .. }
            | LightingEffect::Reactive { color, .. } => Some(color),
            _ => None,
        }
    }

    /// Speed/period in milliseconds, for effects that animate.
    pub fn period_ms(&self) -> Option<u16> {
        match *self {
            LightingEffect::Breathing { period_ms, .. }
            | LightingEffect::ColorCycle { period_ms }
            | LightingEffect::Wave { period_ms, .. } => Some(period_ms),
            LightingEffect::Reactive { duration_ms, .. } => Some(duration_ms),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightingZoneCaps {
    pub id: u8,
    /// User-facing name: "Logo", "Primary", "Scroll wheel", "Keys".
    pub name: String,
    pub effects: Vec<EffectKind>,
    pub brightness: bool,
    /// Keys this zone lights, as ids in the device's layout (see
    /// [`crate::Capabilities::layout`]). Empty when unknown or not a key zone.
    #[serde(default)]
    pub keys: Vec<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneState {
    pub effect: LightingEffect,
    /// 0–100.
    pub brightness: u8,
}

// ---------------------------------------------------------------------------
// Buttons and actions
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    WheelLeft,
    WheelRight,
}

impl MouseButton {
    pub const ALL: [MouseButton; 7] = [
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::Back,
        MouseButton::Forward,
        MouseButton::WheelLeft,
        MouseButton::WheelRight,
    ];

    pub fn name(self) -> &'static str {
        match self {
            MouseButton::Left => "Left click",
            MouseButton::Right => "Right click",
            MouseButton::Middle => "Middle click",
            MouseButton::Back => "Back",
            MouseButton::Forward => "Forward",
            MouseButton::WheelLeft => "Scroll left",
            MouseButton::WheelRight => "Scroll right",
        }
    }
}

/// HID keyboard modifier bits (usage page 0x07, report byte 0 layout).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Modifiers(pub u8);

impl Modifiers {
    pub const LCTRL: u8 = 0x01;
    pub const LSHIFT: u8 = 0x02;
    pub const LALT: u8 = 0x04;
    pub const LGUI: u8 = 0x08;
    pub const RCTRL: u8 = 0x10;
    pub const RSHIFT: u8 = 0x20;
    pub const RALT: u8 = 0x40;
    pub const RGUI: u8 = 0x80;

    pub fn has(self, bit: u8) -> bool {
        self.0 & bit != 0
    }

    pub fn label(self) -> String {
        let mut parts = Vec::new();
        if self.has(Self::LCTRL | Self::RCTRL) {
            parts.push("Ctrl");
        }
        if self.has(Self::LSHIFT | Self::RSHIFT) {
            parts.push("Shift");
        }
        if self.has(Self::LALT | Self::RALT) {
            parts.push("Alt");
        }
        if self.has(Self::LGUI | Self::RGUI) {
            parts.push("Super");
        }
        parts.join("+")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DpiFunction {
    Up,
    Down,
    Cycle,
    /// "Sniper"/DPI-shift: hold for this DPI, release to restore.
    Shift(u16),
}

/// HID consumer-page usages worth offering in a picker.
pub mod media {
    pub const PLAY_PAUSE: u16 = 0x00cd;
    pub const NEXT: u16 = 0x00b5;
    pub const PREVIOUS: u16 = 0x00b6;
    pub const STOP: u16 = 0x00b7;
    pub const MUTE: u16 = 0x00e2;
    pub const VOLUME_UP: u16 = 0x00e9;
    pub const VOLUME_DOWN: u16 = 0x00ea;
    pub const CALCULATOR: u16 = 0x0192;
    pub const BROWSER_HOME: u16 = 0x0223;

    pub const ALL: [(u16, &str); 9] = [
        (PLAY_PAUSE, "Play / Pause"),
        (NEXT, "Next track"),
        (PREVIOUS, "Previous track"),
        (STOP, "Stop"),
        (MUTE, "Mute"),
        (VOLUME_UP, "Volume up"),
        (VOLUME_DOWN, "Volume down"),
        (CALCULATOR, "Calculator"),
        (BROWSER_HOME, "Browser home"),
    ];

    pub fn name(usage: u16) -> Option<&'static str> {
        ALL.iter().find(|(u, _)| *u == usage).map(|(_, n)| *n)
    }
}

/// What a button or key does.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ButtonAction {
    /// Factory behaviour.
    Default,
    Disabled,
    Mouse(MouseButton),
    /// Keystroke: HID keyboard usage (page 0x07) plus modifiers.
    Key {
        usage: u8,
        mods: Modifiers,
    },
    /// HID consumer usage (page 0x0C).
    Media(u16),
    Dpi(DpiFunction),
    ProfileCycle,
    /// Software macro by id; played back by the host while the app runs.
    Macro(String),
}

impl ButtonAction {
    pub fn label(&self) -> String {
        match self {
            ButtonAction::Default => "Default".into(),
            ButtonAction::Disabled => "Disabled".into(),
            ButtonAction::Mouse(b) => b.name().into(),
            ButtonAction::Key { usage, mods } => {
                let key = crate::keys::name(*usage).unwrap_or("?");
                if mods.0 == 0 {
                    key.to_string()
                } else {
                    format!("{}+{}", mods.label(), key)
                }
            }
            ButtonAction::Media(u) => media::name(*u)
                .map(str::to_string)
                .unwrap_or_else(|| format!("Media 0x{u:03x}")),
            ButtonAction::Dpi(DpiFunction::Up) => "DPI up".into(),
            ButtonAction::Dpi(DpiFunction::Down) => "DPI down".into(),
            ButtonAction::Dpi(DpiFunction::Cycle) => "DPI cycle".into(),
            ButtonAction::Dpi(DpiFunction::Shift(d)) => format!("DPI shift ({d})"),
            ButtonAction::ProfileCycle => "Profile cycle".into(),
            ButtonAction::Macro(id) => format!("Macro: {id}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ButtonCaps {
    /// Driver-defined stable id (Logitech control id, Razer button index...).
    pub id: u16,
    pub name: String,
    pub remappable: bool,
    /// Action reported when the button is set to [`ButtonAction::Default`].
    pub default_action: ButtonAction,
    /// Position on the device illustration, normalised 0–1 (x, y). The UI
    /// draws hotspots here; `None` lists the button without a hotspot.
    pub hotspot: Option<(f32, f32)>,
}

// ---------------------------------------------------------------------------
// Power
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battery {
    pub percent: Option<u8>,
    pub charging: bool,
}
