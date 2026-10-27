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
    Gradient,
    Off,
    Static,
    Breathing,
    ColorCycle,
    Wave,
    Reactive,
}

impl EffectKind {
    pub const ALL: [EffectKind; 7] = [
        EffectKind::Gradient,
        EffectKind::Off,
        EffectKind::Static,
        EffectKind::Breathing,
        EffectKind::ColorCycle,
        EffectKind::Wave,
        EffectKind::Reactive,
    ];

    pub fn name(self) -> &'static str {
        match self {
            EffectKind::Gradient => "Custom gradient",
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

/// A percent position in exact half-percent units. JSON keeps the original
/// numeric `position` field (whole percentages stay integers). No float is
/// retained internally, rounded or accepted outside the 0..100 / 0.5 grid.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,PartialOrd,Ord)]
pub struct ColorPosition(u16);
impl ColorPosition{
    pub const fn whole(percent:u8)->Self{Self(percent as u16*2)}
    pub const fn from_half_percent(halves:u16)->Self{Self(halves)}
    pub const fn half_percent(self)->u16{self.0}
}
impl From<u8> for ColorPosition{fn from(percent:u8)->Self{Self::whole(percent)}}
impl PartialEq<u8> for ColorPosition{fn eq(&self,other:&u8)->bool{self.0==u16::from(*other)*2}}
impl PartialOrd<u8> for ColorPosition{fn partial_cmp(&self,other:&u8)->Option<std::cmp::Ordering>{self.0.partial_cmp(&(u16::from(*other)*2))}}
impl Serialize for ColorPosition{
    fn serialize<S:serde::Serializer>(&self,s:S)->std::result::Result<S::Ok,S::Error>{
        if self.0>200{return Err(serde::ser::Error::custom("gradient position exceeds 100 percent"));}
        if self.0%2==0{s.serialize_u16(self.0/2)}else{s.serialize_f64(f64::from(self.0)/2.0)}
    }
}
impl<'de> Deserialize<'de> for ColorPosition{
    fn deserialize<D:serde::Deserializer<'de>>(d:D)->std::result::Result<Self,D::Error>{
        struct Position;
        impl<'de> serde::de::Visitor<'de> for Position{
            type Value=ColorPosition;
            fn expecting(&self,f:&mut std::fmt::Formatter)->std::fmt::Result{f.write_str("0–100 percent in exact 0.5-percent steps")}
            fn visit_u64<E:serde::de::Error>(self,v:u64)->std::result::Result<Self::Value,E>{if v>100{Err(E::custom("gradient position exceeds 100 percent"))}else{Ok(ColorPosition::whole(v as u8))}}
            fn visit_i64<E:serde::de::Error>(self,v:i64)->std::result::Result<Self::Value,E>{if v<0{Err(E::custom("negative gradient position"))}else{self.visit_u64(v as u64)}}
            fn visit_f64<E:serde::de::Error>(self,v:f64)->std::result::Result<Self::Value,E>{if !v.is_finite()||!(0.0..=100.0).contains(&v)||(v*2.0).fract()!=0.0{Err(E::custom("gradient position must use exact half-percent steps"))}else{Ok(ColorPosition::from_half_percent((v*2.0)as u16))}}
        }
        d.deserialize_any(Position)
    }
}
/// One native colour-loop control point. Codecs perform the documented
/// floor(percent * 255 / 100), including exact half-percent positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorStop {pub color:Rgb,pub position:ColorPosition}
impl ColorStop{pub const EMPTY:Self=Self{color:Rgb::BLACK,position:ColorPosition::whole(0)};}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightingEffect {
    /// Fixed bound keeps this enum Copy and bounds serialised native palettes.
    Gradient { stops:[ColorStop;14],count:u8,period_ms:u16 },
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
            LightingEffect::Gradient { .. } => EffectKind::Gradient,
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
            EffectKind::Gradient => {let mut stops=[ColorStop::EMPTY;14];stops[0]=ColorStop{color,position:ColorPosition::whole(0)};stops[1]=ColorStop{color:Rgb::BLACK,position:ColorPosition::whole(50)};stops[2]=ColorStop{color,position:ColorPosition::whole(100)};LightingEffect::Gradient{stops,count:3,period_ms:1000}},
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
            LightingEffect::Gradient{stops,count,..} => (count>0).then_some(stops[0].color),
            LightingEffect::Static { color }
            | LightingEffect::Breathing { color, .. }
            | LightingEffect::Reactive { color, .. } => Some(color),
            _ => None,
        }
    }

    /// Speed/period in milliseconds, for effects that animate.
    pub fn period_ms(&self) -> Option<u16> {
        match *self {
            LightingEffect::Gradient{period_ms,..}
            | LightingEffect::Breathing { period_ms, .. }
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

/// A one-shot vertical scroll output, available only where a driver supports it.
/// Kept separate from mouse-button holds and macro press/release steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScrollDirection {
    Up,
    Down,
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
    /// Next stage, wrapping to the first stage after the last.
    Cycle,
    /// "Sniper"/DPI-shift: hold for this DPI, release to restore.
    Shift(u16),
    /// Native DPI clutch with independently configured axes. Driver support is explicit.
    ShiftXY { x: u16, y: u16 },
    /// Previous stage, wrapping to the last stage before the first.
    CycleDown,
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
    /// Select a zero-based native onboard slot, only for an exact verified driver.
    ProfileSelect(u8),
    /// Hold the device-native secondary mapping layer (only advertised by exact drivers).
    Hypershift,
    /// Software macro by id; played back by the host while the app runs.
    Macro(String),
    /// A scroll output; support is advertised by the exact device driver.
    Scroll(ScrollDirection),
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
            ButtonAction::Dpi(DpiFunction::ShiftXY { x, y }) => format!("DPI shift ({x} × {y})"),
            ButtonAction::Dpi(DpiFunction::CycleDown) => "DPI cycle down".into(),
            ButtonAction::ProfileCycle => "Profile cycle".into(),
            ButtonAction::ProfileSelect(index) => format!("Profile {}",u16::from(*index)+1),
            ButtonAction::Hypershift => "Hold Hypershift".into(),
            ButtonAction::Macro(id) => format!("Macro: {id}"),
            ButtonAction::Scroll(ScrollDirection::Up) => "Scroll up".into(),
            ButtonAction::Scroll(ScrollDirection::Down) => "Scroll down".into(),
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

#[cfg(test)]mod color_position_tests{
 use super::*;
 #[test]fn half_percent_positions_round_trip_without_changing_integer_json(){for (text,halves)in[("0",0),("33",66),("100",200),("16.5",33),("49.5",99)]{let p:ColorPosition=serde_json::from_str(text).unwrap();assert_eq!(p.half_percent(),halves);assert_eq!(serde_json::to_string(&p).unwrap(),text);}}
 #[test]fn positions_reject_out_of_range_quarter_percent_non_numeric_and_non_finite_inputs(){for s in["-0.5","100.5","101","16.25","true","null","\"16.5\"","1e100"]{assert!(serde_json::from_str::<ColorPosition>(s).is_err(),"{s}");}assert!(serde_json::to_string(&ColorPosition::from_half_percent(201)).is_err());}
}

#[cfg(test)]
mod button_action_serialization_tests {
    use super::*;
    use crate::profile::MacroStep;
    use serde_json::json;

    #[test]
    fn existing_button_action_json_remains_compatible() {
        let fixtures = [
            (ButtonAction::Default, json!("Default")),
            (ButtonAction::Disabled, json!("Disabled")),
            (ButtonAction::Mouse(MouseButton::Left), json!({"Mouse": "Left"})),
            (ButtonAction::Mouse(MouseButton::WheelLeft), json!({"Mouse": "WheelLeft"})),
            (ButtonAction::Mouse(MouseButton::WheelRight), json!({"Mouse": "WheelRight"})),
            (ButtonAction::Key { usage: 4, mods: Modifiers(0x81) }, json!({"Key": {"usage": 4, "mods": 129}})),
            (ButtonAction::Media(0xe9), json!({"Media": 233})),
            (ButtonAction::Dpi(DpiFunction::Up), json!({"Dpi": "Up"})),
            (ButtonAction::Dpi(DpiFunction::Down), json!({"Dpi": "Down"})),
            (ButtonAction::Dpi(DpiFunction::Cycle), json!({"Dpi": "Cycle"})),
            (ButtonAction::Dpi(DpiFunction::Shift(400)), json!({"Dpi": {"Shift": 400}})),
            (ButtonAction::ProfileCycle, json!("ProfileCycle")),
            (ButtonAction::Hypershift, json!("Hypershift")),
            (ButtonAction::Macro("saved-macro".into()), json!({"Macro": "saved-macro"})),
        ];
        for (action, value) in fixtures {
            assert_eq!(serde_json::to_value(&action).unwrap(), value);
            assert_eq!(serde_json::from_value::<ButtonAction>(value).unwrap(), action);
        }
        assert_eq!(ButtonAction::Dpi(DpiFunction::Cycle).label(), "DPI cycle");
    }

    #[test]
    fn direct_native_profile_selection_has_distinct_typed_json(){
        for (index,label) in [(0,"Profile 1"),(3,"Profile 4")]{let action=ButtonAction::ProfileSelect(index);let value=json!({"ProfileSelect":index});assert_eq!(serde_json::to_value(&action).unwrap(),value);assert_eq!(serde_json::from_value::<ButtonAction>(value).unwrap(),action);assert_eq!(action.label(),label);}
        for value in [json!({"ProfileSelect":-1}),json!({"ProfileSelect":256}),json!({"ProfileSelect":1.5}),json!({"ProfileSelect":"1"}),json!({"ProfileSelect":{"index":1}})]{assert!(serde_json::from_value::<ButtonAction>(value).is_err());}
    }
    #[test]
    fn scoped_scroll_and_reverse_cycle_have_distinct_strict_json() {
        for (action, value, label) in [
            (ButtonAction::Scroll(ScrollDirection::Up), json!({"Scroll": "Up"}), "Scroll up"),
            (ButtonAction::Scroll(ScrollDirection::Down), json!({"Scroll": "Down"}), "Scroll down"),
            (ButtonAction::Dpi(DpiFunction::CycleDown), json!({"Dpi": "CycleDown"}), "DPI cycle down"),
        ] {
            assert_eq!(serde_json::to_value(&action).unwrap(), value);
            assert_eq!(serde_json::from_value::<ButtonAction>(value).unwrap(), action);
            assert_eq!(action.label(), label);
        }
        for value in [json!({"Scroll": "Left"}), json!({"Scroll": "up"}), json!({"Scroll": 1})] {
            assert!(serde_json::from_value::<ButtonAction>(value).is_err());
        }
    }

    #[test]
    fn independent_clutch_json_preserves_legacy_uniform_shape() {
        for (action, value) in [
            (ButtonAction::Dpi(DpiFunction::Shift(800)), json!({"Dpi":{"Shift":800}})),
            (ButtonAction::Dpi(DpiFunction::ShiftXY { x:800,y:1600 }), json!({"Dpi":{"ShiftXY":{"x":800,"y":1600}}})),
        ] {
            assert_eq!(serde_json::to_value(&action).unwrap(), value);
            assert_eq!(serde_json::from_value::<ButtonAction>(value).unwrap(), action);
        }
        assert_eq!(ButtonAction::Dpi(DpiFunction::ShiftXY { x:800,y:1600 }).label(), "DPI shift (800 × 1600)");
        for value in [json!({"Dpi":{"ShiftXY":{"x":800}}}), json!({"Dpi":{"ShiftXY":{"x":800,"y":65536}}})] {
            assert!(serde_json::from_value::<ButtonAction>(value).is_err());
        }
    }

    #[test]
    fn scroll_outputs_do_not_expand_mouse_or_macro_hold_inputs() {
        assert_eq!(MouseButton::ALL, [
            MouseButton::Left, MouseButton::Right, MouseButton::Middle,
            MouseButton::Back, MouseButton::Forward,
            MouseButton::WheelLeft, MouseButton::WheelRight,
        ]);
        for direction in ["WheelUp", "WheelDown"] {
            assert!(serde_json::from_value::<MouseButton>(json!(direction)).is_err());
            assert!(serde_json::from_value::<MacroStep>(json!({"MouseDown": direction})).is_err());
            assert!(serde_json::from_value::<MacroStep>(json!({"MouseUp": direction})).is_err());
        }
        assert_eq!(serde_json::from_value::<MacroStep>(json!({"Scroll": "Up"})).unwrap(), MacroStep::Scroll(ScrollDirection::Up));
    }
}

/// Discrete firmware levels; no unverified percentage or timing is implied.
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub enum IndicatorLevel {Low,Medium,High}
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub enum DpiIndicatorMode {
    Static,
    Breathing,
    /// Source-defined disabled record with both mode bytes zero. Never enabled implicitly.
    Unspecified,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DpiIndicatorState {pub enabled:bool,pub mode:DpiIndicatorMode,pub brightness:IndicatorLevel,pub speed:IndicatorLevel}
