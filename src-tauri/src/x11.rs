//! Attack Shark X11 full-profile codec and write-only executor.
//! This is not a generic Beken driver. Exact Windows collection integration is
//! deliberately separate; no reset, bootloader, WinUSB or global input hook.
//! Protocol references, limitations and MIT credit: docs/X11-ADAPTER.md.
use gearwise_core::{model::Rgb, Error, HidTransport, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};
#[path = "x11/dpi_table.rs"]
mod dpi_table;
#[cfg(test)]
#[path = "x11/tests.rs"]
mod tests;

pub const VID: u16 = 0x1d57;
pub const WIRED: u16 = 0xfa55;
pub const WIRELESS: u16 = 0xfa60;
pub fn supported(v: u16, p: u16) -> bool {
    v == VID && matches!(p, WIRED | WIRELESS)
}
fn invalid(message: &str) -> Error {
    Error::invalid(format!("X11: {message}"))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dpi {
    pub stages: [u16; 6],
    /// UI uses zero-based stages; report 04 uses 1..6.
    pub active: u8,
    pub angle_snapping: bool,
    pub ripple_control: bool,
    /// Opaque paired flags in source captures; retained, not assigned a meaning.
    pub opaque_14_15: [u8; 2],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Light {
    Off,
    Static,
    Breathing,
    Neon,
    ColorBreathing,
    StaticDpi,
    BreathingDpi,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub light: Light,
    pub color: Rgb,
    /// Firmware's five discrete levels, slowest 1 to fastest 5.
    pub light_speed: u8,
    /// 30..1800 seconds, step 30. This is sensor sleep, not LED idle.
    pub sleep_seconds: u16,
    /// 60..3600 seconds, step 60.
    pub deep_sleep_seconds: u16,
    /// Even 4..50 ms.
    pub debounce_ms: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Disabled,
    Left,
    Right,
    Middle,
    Back,
    Forward,
    DoubleClick,
    WheelUp,
    WheelDown,
    DpiCycle,
    DpiUp,
    DpiDown,
    Key { usage: u8, mods: u8 },
    Media(Media),
    Macro(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Media {
    Player,
    Previous,
    Next,
    PlayPause,
    Stop,
    Mute,
    VolumeUp,
    VolumeDown,
    Calculator,
    Email,
    BrowserForward,
    BrowserBack,
    BrowserStop,
    Computer,
    BrowserRefresh,
    BrowserHome,
    BrowserSearch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Playback {
    Times {
        count: u8,
    },
    /// Firmware stops on ANY key press, not a toggle of this trigger.
    UntilAnyKey,
    WhileHeld,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Input {
    Key(u8),
    Mouse(Mouse),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mouse {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub input: Input,
    pub down: bool,
    /// Delay after this event. Firmware quantizes using its documented code;
    /// the final event must be 10 ms (capture evidence, not silently changed).
    pub delay_ms: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Macro {
    pub id: String,
    pub name: String,
    pub playback: Playback,
    pub events: Vec<Event>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub dpi: Dpi,
    pub report_rate: u16,
    pub preferences: Preferences,
    /// Explicit complete mapping. 1 left, 2 right, 3 middle, 4 back,
    /// 5 forward, 6 DPI, 7 wheel up, 8 wheel down. No implicit defaults.
    pub buttons: BTreeMap<u16, Action>,
    pub macros: Vec<Macro>,
}
impl Profile {
    /// An OFFLINE editable example, never treated as a device read.
    pub fn example() -> Self {
        Self {
            dpi: Dpi {
                stages: [800, 1600, 2400, 3200, 5000, 22000],
                active: 1,
                angle_snapping: false,
                ripple_control: true,
                opaque_14_15: [0, 0],
            },
            report_rate: 1000,
            preferences: Preferences {
                light: Light::Off,
                color: Rgb::new(0, 255, 0),
                light_speed: 3,
                sleep_seconds: 30,
                deep_sleep_seconds: 600,
                debounce_ms: 4,
            },
            buttons: [
                Action::Left,
                Action::Right,
                Action::Middle,
                Action::Back,
                Action::Forward,
                Action::DpiCycle,
                Action::WheelUp,
                Action::WheelDown,
            ]
            .into_iter()
            .enumerate()
            .map(|(i, a)| (i as u16 + 1, a))
            .collect(),
            macros: vec![],
        }
    }
}
fn dpi_code(dpi: u16) -> Result<u8> {
    dpi_table::DPI
        .iter()
        .find(|(v, _)| *v == dpi)
        .map(|(_, c)| *c)
        .ok_or_else(|| invalid("DPI 必须选择精确支持值：50..10000 步进 50，10100..22000 步进 100"))
}
#[cfg(test)]
pub fn dpi_values() -> Vec<u16> {
    dpi_table::DPI.iter().map(|(v, _)| *v).collect()
}
fn encode_dpi(dpi: &Dpi, wireless: bool) -> Result<Vec<u8>> {
    if dpi.active >= 6 {
        return Err(invalid("当前 DPI 档位超出六档范围"));
    }
    if !matches!(dpi.opaque_14_15, [0, 0] | [1, 1]) {
        return Err(invalid("未知 DPI 元数据组合"));
    }
    let mut p = vec![0; 56];
    p[..6].copy_from_slice(&[
        4,
        0x38,
        1,
        u8::from(dpi.angle_snapping),
        u8::from(dpi.ripple_control),
        0x3f,
    ]);
    for (i, value) in dpi.stages.iter().enumerate() {
        p[8 + i] = dpi_code(*value)?;
        if *value > 12000 {
            p[6] |= 1 << i;
        }
        p[16 + i] = u8::from((10100..=12000).contains(value) || (20100..=22000).contains(value));
    }
    p[7] = p[6];
    p[14..16].copy_from_slice(&dpi.opaque_14_15);
    p[24] = dpi.active + 1;
    // Reference's reserved/color metadata. These bytes are not advertised as
    // editable per-stage colors until that sub-protocol is evidenced.
    p[25..50].copy_from_slice(&[
        0xff, 0, 0, 0, 0xff, 0, 0, 0, 0xff, 0xff, 0xff, 0, 0, 0xff, 0xff, 0xff, 0, 0xff, 0xff,
        0x40, 0, 0xff, 0xff, 0xff, 2,
    ]);
    let sum = p[3..50].iter().map(|v| u16::from(*v)).sum::<u16>();
    p[50..52].copy_from_slice(&sum.to_be_bytes());
    if !wireless {
        p.truncate(52);
    }
    Ok(p)
}
fn encode_preferences(v: &Preferences, wireless: bool) -> Result<Vec<u8>> {
    if !(1..=5).contains(&v.light_speed)
        || !(30..=1800).contains(&v.sleep_seconds)
        || v.sleep_seconds % 30 != 0
        || !(60..=3600).contains(&v.deep_sleep_seconds)
        || v.deep_sleep_seconds % 60 != 0
        || !(4..=50).contains(&v.debounce_ms)
        || v.debounce_ms % 2 != 0
    {
        return Err(invalid("灯速、休眠或消抖值不在此型号已实现范围"));
    }
    let mut p = vec![0; 15];
    p[..3].copy_from_slice(&[5, 15, 1]);
    p[3] = match v.light {
        Light::Off => 0,
        Light::Static => 0x10,
        Light::Breathing => 0x20,
        Light::Neon => 0x30,
        Light::ColorBreathing => 0x40,
        Light::StaticDpi => 0x50,
        Light::BreathingDpi => 0x60,
    };
    let minutes = v.deep_sleep_seconds / 60;
    p[4] = ((((minutes - 1) / 16) as u8) << 4) | (6 - v.light_speed);
    p[5] = (8 + minutes * 16) as u8;
    p[6..9].copy_from_slice(&[v.color.r, v.color.g, v.color.b]);
    p[9] = (v.sleep_seconds / 30) as u8;
    p[10] = v.debounce_ms / 2;
    p[11] = [v.color.r, v.color.g, v.color.b]
        .iter()
        .filter(|c| **c >= 100)
        .count() as u8
        + u8::from(v.light == Light::BreathingDpi);
    p[12] = p[3..11].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    if !wireless {
        p.truncate(13);
    }
    Ok(p)
}
fn macro_slot(id: u16) -> Result<u8> {
    match id {
        1 => Ok(1),
        2 => Ok(2),
        3 => Ok(3),
        4 => Ok(8),
        5 => Ok(7),
        _ => Err(invalid("自定义宏只证实左、右、中、前进与后退五个独立槽")),
    }
}
fn input_code(input: Input) -> Result<u8> {
    match input {
        Input::Key(k) if (4..=0xa4).contains(&k) || (0xe0..=0xe7).contains(&k) => Ok(k),
        Input::Mouse(m) => Ok(match m {
            Mouse::Left => 0xf1,
            Mouse::Right => 0xf2,
            Mouse::Middle => 0xf3,
            Mouse::Back => 0xf4,
            Mouse::Forward => 0xf5,
        }),
        _ => Err(invalid("宏键码无效")),
    }
}
fn encode_macro(slot: u8, m: &Macro) -> Result<[Vec<u8>; 3]> {
    if m.id.is_empty()
        || m.id.len() > 128
        || m.name.len() > 256
        || m.events.is_empty()
        || m.events.len() > 47
    {
        return Err(invalid("宏名称、标识或事件数无效"));
    }
    if matches!(m.playback, Playback::Times { count: 0 }) {
        return Err(invalid("宏播放次数必须为 1..255"));
    }
    if m.events.last().unwrap().delay_ms != 10 {
        return Err(invalid(
            "X11 最后一个宏事件固定间隔为 10 ms；请明确改为 10 ms",
        ));
    }
    let mut held = BTreeSet::new();
    let mut events = Vec::new();
    for e in &m.events {
        let code = input_code(e.input)?;
        if e.down {
            if !held.insert(code) {
                return Err(invalid("宏重复按下尚未释放的输入"));
            }
        } else if !held.remove(&code) {
            return Err(invalid("宏释放没有对应按下"));
        }
        if !(1..=51000).contains(&e.delay_ms) {
            return Err(invalid("事件间隔必须为 1..51000 ms"));
        }
        // Raw captures demonstrate 1269/1270 use 0x7f; 1271 adds a long
        // delay event. Upstream builder's 1070 threshold is contradicted.
        let long = e.delay_ms > 1270;
        let rem = if long { e.delay_ms % 200 } else { e.delay_ms };
        let delay = (2 * ((rem + 5) / 20) + 1) as u8;
        events.extend_from_slice(&[delay | if e.down { 0 } else { 0x80 }, code]);
        if long {
            events.extend_from_slice(&[(e.delay_ms / 200) as u8, 3]);
        }
    }
    if !held.is_empty() {
        return Err(invalid("宏含未释放输入"));
    }
    if events.len() > 94 {
        return Err(invalid("宏展开长延时后超过 47 个原生事件；不会截断"));
    }
    let mut p0 = vec![0; 64];
    let mut p1 = vec![0; 64];
    let mut p2 = vec![0; 64];
    p0[..4].copy_from_slice(&[9, 64, slot, 0]);
    p1[..4].copy_from_slice(&[9, 64, slot, 1]);
    p2[..4].copy_from_slice(&[9, 12, slot, 2]);
    p0[4] = match m.playback {
        Playback::Times { .. } => 0,
        Playback::UntilAnyKey => 1,
        Playback::WhileHeld => 2,
    };
    p0[8] = match m.playback {
        Playback::Times { count } => count,
        _ => 1,
    };
    p0[29] = (events.len() / 2) as u8;
    let split = events.len().min(34);
    p0[30..30 + split].copy_from_slice(&events[..split]);
    p1[4..4 + events.len() - split].copy_from_slice(&events[split..]);
    let sum = p0[8..]
        .iter()
        .chain(p1[4..].iter())
        .map(|b| u16::from(*b))
        .sum::<u16>();
    p2[10..12].copy_from_slice(&sum.to_be_bytes());
    Ok([p0, p1, p2])
}
fn action(id: u16, a: &Action) -> Result<[u8; 3]> {
    let code = match a {
        Action::Disabled => 1,
        Action::Left => 2,
        Action::Right => 3,
        Action::Middle => 4,
        Action::Back => 5,
        Action::Forward => 6,
        Action::DoubleClick => 7,
        Action::WheelUp => 9,
        Action::WheelDown => 10,
        Action::DpiCycle => 13,
        Action::DpiUp => 14,
        Action::DpiDown => 15,
        Action::Key { usage, mods } if (4..=0x73).contains(usage) && *mods <= 15 => {
            return Ok([0x11, *mods, *usage])
        }
        Action::Macro(_) => return Ok([0x12, 0, macro_slot(id)?]),
        Action::Media(m) => match m {
            Media::Player => 0x15,
            Media::Previous => 0x16,
            Media::Next => 0x17,
            Media::PlayPause => 0x18,
            Media::Stop => 0x19,
            Media::Mute => 0x1a,
            Media::VolumeUp => 0x1b,
            Media::VolumeDown => 0x1c,
            Media::Calculator => 0x1d,
            Media::Email => 0x1e,
            Media::BrowserForward => 0x20,
            Media::BrowserBack => 0x21,
            Media::BrowserStop => 0x22,
            Media::Computer => 0x23,
            Media::BrowserRefresh => 0x24,
            Media::BrowserHome => 0x25,
            Media::BrowserSearch => 0x26,
        },
        _ => return Err(invalid("此按键动作或修饰键尚未证实")),
    };
    Ok([code, 0, 0])
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Compiled {
    pub packets: Vec<Vec<u8>>,
    /// Firmware acceptance is unavailable; this never means readback verified.
    pub readback: &'static str,
}
pub fn compile(vendor: u16, product: u16, profile: &Profile) -> Result<Compiled> {
    if !supported(vendor, product) {
        return Err(invalid(
            "仅证实 X11 FA55 有线、FA60 接收器；不能套用 R1 或其他 Beken 鼠标",
        ));
    }
    if profile.buttons.len() != 8 || !(1..=8).all(|id| profile.buttons.contains_key(&id)) {
        return Err(invalid("必须明确提供全部八个按钮/滚轮控制的完整方案"));
    }
    if !(1..=6).any(|id| profile.buttons.get(&id) == Some(&Action::Left)) {
        return Err(invalid("至少保留一个实体按键作为左键"));
    }
    if profile.macros.len() > 5 {
        return Err(invalid("最多五个独立原生宏"));
    }
    let mut defs = BTreeMap::new();
    for m in &profile.macros {
        encode_macro(1, m)?;
        if defs.insert(m.id.as_str(), m).is_some() {
            return Err(invalid("宏标识重复"));
        }
    }
    let wireless = product == WIRELESS;
    let dpi = encode_dpi(&profile.dpi, wireless)?;
    let prefs = encode_preferences(&profile.preferences, wireless)?;
    let rate = match profile.report_rate {
        125 => 8,
        250 => 4,
        500 => 2,
        1000 => 1,
        _ => return Err(invalid("回报率只支持 125/250/500/1000 Hz")),
    };
    let mut buttons = vec![0; 59];
    buttons[..3].copy_from_slice(&[8, 59, 1]);
    for n in (3..=54).step_by(3) {
        buttons[n] = 1;
    }
    let mut macros = Vec::new();
    for (id, a) in &profile.buttons {
        let offset = match id {
            1 => 3,
            2 => 6,
            3 => 9,
            4 => 24,
            5 => 21,
            6 => 18,
            7 => 51,
            8 => 54,
            _ => unreachable!(),
        };
        buttons[offset..offset + 3].copy_from_slice(&action(*id, a)?);
        if let Action::Macro(name) = a {
            let m = defs
                .get(name.as_str())
                .ok_or_else(|| invalid("绑定的原生宏定义缺失"))?;
            macros.extend(encode_macro(macro_slot(*id)?, m)?);
        }
    }
    buttons[58] = buttons[3..58].iter().fold(0u8, |s, v| s.wrapping_add(*v));
    // Follow captured vendor order: the COMPLETE button map precedes macro
    // pages. No default map is ever inserted between individual macro writes.
    let mut packets = vec![
        dpi,
        prefs,
        vec![6, 9, 1, rate, 255 - rate, 0, 0, 0, 0],
        buttons,
    ];
    packets.extend(macros);
    Ok(Compiled {
        packets,
        readback: "write_only_session_cached",
    })
}

/// Handle scoped to one OS-opened HID collection. It must never be rebound by
/// VID/PID lookup while writing. No fallback transport or automatic retry.
pub struct Writer {
    io: Box<dyn HidTransport>,
    vendor: u16,
    product: u16,
    last_transfer: Option<Instant>,
    pause: bool,
    cached: Option<Profile>,
}
impl Writer {
    pub fn new(io: Box<dyn HidTransport>, vendor: u16, product: u16) -> Result<Self> {
        if !supported(vendor, product) {
            return Err(invalid("错误的精确型号"));
        }
        Ok(Self {
            io,
            vendor,
            product,
            last_transfer: None,
            pause: true,
            cached: None,
        })
    }
    pub fn cached(&self) -> Option<&Profile> {
        self.cached.as_ref()
    }
    pub fn apply(&mut self, p: &Profile) -> Result<usize> {
        // Compile and validate the WHOLE plan before the first transfer.
        let compiled = compile(self.vendor, self.product, p)?;
        self.cached = None;
        for (index, packet) in compiled.packets.iter().enumerate() {
            if self.pause {
                if let Some(last) = self.last_transfer {
                    std::thread::sleep(Duration::from_millis(250).saturating_sub(last.elapsed()));
                }
            }
            let result = self.io.send_feature_report(packet);
            self.last_transfer = Some(Instant::now());
            if let Err(error) = result {
                return Err(Error::invalid(format!("X11 第 {} / {} 条功能报告失败：{error}；此前报告可能已生效，原配置无法读回，未重试或虚构恢复成功",index+1,compiled.packets.len())));
            }
        }
        // This proves host transfer completion only, never device acceptance.
        self.cached = Some(p.clone());
        Ok(compiled.packets.len())
    }
}

pub const SCHEMA: &str = "attack-shark-x11/1";
pub fn decode(schema: &str, data: &serde_json::Value) -> Result<Profile> {
    if schema != SCHEMA {
        return Err(invalid("原生配置版本不匹配"));
    }
    if serde_json::to_vec(data)
        .map_err(|_| invalid("配置序列化失败"))?
        .len()
        > 65536
    {
        return Err(invalid("原生配置过大"));
    }
    serde_json::from_value(data.clone())
        .map_err(|e| Error::invalid(format!("X11 完整配置字段无效：{e}")))
}
#[path = "x11/driver.rs"]
mod driver;
pub use driver::{simulated, X11Driver};

pub fn validate_plan(plan: &crate::multibrand::Plan) -> std::result::Result<(), String> {
    if !supported(plan.vendor, plan.product) || !plan.macros.is_empty() || plan.source.is_some() {
        return Err("X11 原生完整方案不使用软件宏或其他鼠标输入来源".into());
    }
    let [gearwise_core::Setting::VendorProfile { schema, data }] = plan.settings.as_slice() else {
        return Err("X11 需要一份明确的完整配置，不能混入分项写入".into());
    };
    let p = decode(schema, data).map_err(|e| e.to_string())?;
    compile(plan.vendor, plan.product, &p).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn profile_from_plan(plan: &crate::multibrand::Plan) -> std::result::Result<Profile, String> {
    validate_plan(plan)?;
    if let gearwise_core::Setting::VendorProfile { schema, data } = &plan.settings[0] {
        decode(schema, data).map_err(|e| e.to_string())
    } else {
        unreachable!()
    }
}
