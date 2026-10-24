//! ATK COMPX, exact models and transports only. Protocol facts checked against ATK HUB
//! 3.2.27 (go/E2/c8/D0/Fu) and upstream sanitized captures; no vendor JS runs.
//! Unknown identities and other NearLink/BITMOUSE transports are never claimed.
use gearwise_core::model::{
    ButtonAction, ButtonCaps, Connection, DeviceKind, DpiCaps, DpiFunction, DpiStage, DpiState,
    EffectKind, LightingEffect, LightingZoneCaps, MouseButton, Rgb, Vendor, ZoneState,
};
use gearwise_core::{
    Capabilities, Device, DeviceDescriptor, DeviceState, Driver, Error, HidBackend,
    HidInterfaceInfo, HidTransport, Result, Setting,
};
#[path = "atk_macros.rs"]
mod native_macro;
use gearwise_core::profile::Macro;
use std::collections::HashMap;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub fn validate_dpi(d: &DpiState, product: u16) -> Result<()> {
    let (_, _, max) = identity(product).ok_or(Error::Unsupported("ATK model"))?;
    if d.stages.is_empty() || d.stages.len() > 8 || usize::from(d.active) >= d.stages.len() {
        return Err(Error::invalid("ATK DPI stages"));
    }
    for stage in &d.stages {
        encode_dpi(stage.x, stage.y, max)?;
    }
    Ok(())
}
pub fn validate_key(usage: u8, mods: gearwise_core::model::Modifiers) -> Result<()> {
    native_macro::encode(&native_macro::keyboard(usage, mods)).map(|_| ())
}
pub fn validate_macro(m: &Macro) -> Result<()> {
    native_macro::encode(m).map(|_| ())
}
#[derive(serde::Deserialize)]
pub struct Model {
    pub vendor: u16, pub product: u16, pub mids: Vec<u8>, pub brand: String,
    pub name: String, pub max_dpi: u16, pub buttons: u16, pub indicator: bool,
    pub wireless: bool, pub rates: Vec<u16>,
}
pub fn models() -> &'static [Model] {
    static MODELS: std::sync::LazyLock<Vec<Model>> = std::sync::LazyLock::new(||
        serde_json::from_str(include_str!("../../src/lib/compx-models.json")).expect("compiled COMPX catalog"));
    &MODELS
}
pub fn model(v: u16, p: u16) -> Option<&'static Model> {
    models().iter().find(|m| m.vendor == v && m.product == p)
}
pub fn supported(v: u16, p: u16) -> bool { model(v,p).is_some() }
fn identity(p: u16) -> Option<(u8, &'static str, u16)> {
    models().iter().find(|m|m.product==p).map(|m|(m.mids[0],m.name.as_str(),m.max_dpi))
}
fn sum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |a, b| a.wrapping_add(*b))
}
fn pair(v: u8) -> [u8; 2] {
    [v, 0x55u8.wrapping_sub(v)]
}
fn checked_pair(data: &[u8]) -> Result<u8> {
    if data.len() != 2 || sum(data) != 0x55 {
        Err(Error::invalid("ATK scalar checksum"))
    } else {
        Ok(data[0])
    }
}
fn frame(command: u8, address: u16, len: usize, data: &[u8]) -> Result<[u8; 17]> {
    if len > 10 || data.len() > len {
        return Err(Error::invalid("ATK frame length"));
    }
    let mut p = [0; 17];
    p[0] = 8;
    p[1] = command;
    p[3..5].copy_from_slice(&address.to_be_bytes());
    p[5] = len as u8;
    p[6..6 + data.len()].copy_from_slice(data);
    p[16] = 0x55u8.wrapping_sub(sum(&p[..16]));
    Ok(p)
}
fn rate_code(hz: u16) -> Result<u8> {
    match hz {
        125 => Ok(8),
        250 => Ok(4),
        500 => Ok(2),
        1000 => Ok(1),
        2000 => Ok(16),
        4000 => Ok(32),
        8000 => Ok(64),
        _ => Err(Error::invalid("ATK polling rate")),
    }
}
fn rate_value(code: u8) -> Result<u16> {
    match code {
        8 => Ok(125),
        4 => Ok(250),
        2 => Ok(500),
        1 => Ok(1000),
        16 => Ok(2000),
        32 => Ok(4000),
        64 => Ok(8000),
        _ => Err(Error::invalid("ATK polling code")),
    }
}
fn encode_dpi(x: u16, y: u16, max: u16) -> Result<[u8; 4]> {
    let axis = |v: u16| -> Result<(u8, u8)> {
        if v < if max == 36000 { 50 } else { 100 }
            || v > max
            || v % 50 != 0
            || (v > 30000 && v % 100 != 0)
        {
            return Err(Error::invalid(
                "ATK DPI range/encoding: 50 steps up to 30000, 100 above",
            ));
        }
        let doubled = v > 30000;
        let count = v / (if doubled { 100 } else { 50 }) - 1;
        Ok((count as u8, (((count >> 8) as u8) << 2) | u8::from(doubled)))
    };
    let (a, b) = axis(x)?;
    let (c, d) = axis(y)?;
    let mode = b | (d << 4);
    Ok([
        a,
        c,
        mode,
        0x55u8.wrapping_sub(a.wrapping_add(c).wrapping_add(mode)),
    ])
}
fn decode_dpi(p: &[u8], max: u16) -> Result<DpiStage> {
    if p.len() != 4 || sum(p) != 0x55 {
        return Err(Error::invalid("ATK DPI checksum"));
    }
    let axis = |lo: u8, n: u8| -> Result<u16> {
        if n & 2 != 0 {
            return Err(Error::invalid("ATK unexpected sensor DPI mode"));
        }
        let v = ((u16::from(n >> 2) << 8) | u16::from(lo)) + 1;
        v.checked_mul(if n & 1 != 0 { 100 } else { 50 })
            .ok_or_else(|| Error::invalid("ATK DPI overflow"))
    };
    let x = axis(p[0], p[2] & 15)?;
    let y = axis(p[1], p[2] >> 4)?;
    encode_dpi(x, y, max)?;
    Ok(DpiStage { x, y })
}
fn button_bytes(action: &ButtonAction, id: u16) -> Result<[u8; 4]> {
    let action = if matches!(action, ButtonAction::Default) {
        factory(id)?
    } else {
        action.clone()
    };
    let (c, v) = match action {
        ButtonAction::Disabled => (0, 0),
        ButtonAction::Mouse(b) => (
            1,
            match b {
                MouseButton::Left => 1,
                MouseButton::Right => 2,
                MouseButton::Middle => 4,
                MouseButton::Back => 8,
                MouseButton::Forward => 16,
                _ => return Err(Error::Unsupported("ATK mouse action")),
            },
        ),
        ButtonAction::Dpi(f) => (
            2,
            match f {
                DpiFunction::Cycle => 1,
                DpiFunction::Up => 2,
                DpiFunction::Down => 3,
                _ => return Err(Error::Unsupported("ATK DPI shift")),
            },
        ),
        _ => {
            return Err(Error::Unsupported(
                "ATK keyboard shortcut and macro trigger are not verified",
            ))
        }
    };
    Ok([c, v, 0, 0x55u8.wrapping_sub(c).wrapping_sub(v)])
}
fn factory(id: u16) -> Result<ButtonAction> {
    Ok(match id {
        1 => ButtonAction::Mouse(MouseButton::Left),
        2 => ButtonAction::Mouse(MouseButton::Right),
        3 => ButtonAction::Mouse(MouseButton::Middle),
        4 => ButtonAction::Mouse(MouseButton::Back),
        5 => ButtonAction::Mouse(MouseButton::Forward),
        6 => ButtonAction::Dpi(DpiFunction::Cycle),
        _ => return Err(Error::invalid("ATK button id")),
    })
}
fn decode_button(p: &[u8]) -> Option<ButtonAction> {
    if p.len() != 4 || sum(p) != 0x55 || p[2] != 0 {
        return None;
    }
    match (p[0], p[1]) {
        (0, 0) => Some(ButtonAction::Disabled),
        (1, v) => match v {
            1 => Some(MouseButton::Left),
            2 => Some(MouseButton::Right),
            4 => Some(MouseButton::Middle),
            8 => Some(MouseButton::Back),
            16 => Some(MouseButton::Forward),
            _ => None,
        }
        .map(ButtonAction::Mouse),
        (2, v) => match v {
            1 => Some(DpiFunction::Cycle),
            2 => Some(DpiFunction::Up),
            3 => Some(DpiFunction::Down),
            _ => None,
        }
        .map(ButtonAction::Dpi),
        _ => None,
    }
}

struct Atk {
    io: Box<dyn HidTransport>,
    desc: DeviceDescriptor,
    caps: Capabilities,
    max: u16,
    mid: u8,
    model: &'static Model,
    definitions: HashMap<String, Macro>,
    read_macros: Vec<Macro>,
}
impl Atk {
    fn exchange(&mut self, command: u8, address: u16, len: usize) -> Result<Vec<u8>> {
        let request = frame(command, address, len, &[])?;
        if self.io.write(&request)? != request.len() {
            return Err(Error::invalid("ATK short output write"));
        }
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            let mut reply = [0; 65];
            let timeout = (deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(100)
                .max(1)) as i32;
            let n = self.io.read_timeout(&mut reply, timeout)?;
            if n == 0 {
                continue;
            }
            if reply[0] != 8 {
                continue;
            }
            if n != 17 || sum(&reply[..n]) != 0x55 {
                return Err(Error::invalid("ATK input report length/checksum"));
            }
            let r = &reply[1..17];
            if r[0] != command {
                continue;
            }
            if r[1] != 0 || r[4] > 10 {
                return Err(Error::invalid("ATK command status/length"));
            }
            if command == 8 && u16::from_be_bytes([r[2], r[3]]) != address {
                continue;
            }
            if (r[4] as usize) < len {
                return Err(Error::invalid("ATK truncated response"));
            }
            return Ok(r[5..5 + r[4] as usize].to_vec());
        }
        Err(Error::Timeout)
    }
    fn read(&mut self, address: u16, len: usize) -> Result<Vec<u8>> {
        let mut out = self.exchange(8, address, len)?;
        out.truncate(len);
        Ok(out)
    }
    fn identify(&mut self) -> Result<()> {
        let r = self.exchange(0x10, 0, 0)?;
        if r.len() < 2 || r[..2] != [2, self.mid] {
            return Err(Error::invalid(
                "ATK model/sensor identity changed or unknown; no settings written",
            ));
        }
        Ok(())
    }
    fn write_verified(&mut self, address: u16, next: &[u8]) -> Result<()> {
        let before = self.read(address, next.len())?;
        if before == next {
            return Ok(());
        }
        let p = frame(7, address, next.len(), next)?;
        if self.io.write(&p)? != 17 {
            return Err(Error::invalid(
                "ATK short EEPROM write; current state must be re-read",
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
        match self.read(address, next.len()) {
            Ok(actual) if actual == next => Ok(()),
            _ => {
                // Only this exact row is restored, and only after the model is rechecked.
                if self.identify().is_ok() {
                    let restore = frame(7, address, before.len(), &before)?;
                    let _ = self.io.write(&restore);
                }
                Err(Error::invalid("ATK write did not read back; original row restore attempted, reconnect/re-read before retry"))
            }
        }
    }
    fn open(io: Box<dyn HidTransport>, i: &HidInterfaceInfo) -> Result<Self> {
        let model = model(i.vendor_id,i.product_id).ok_or(Error::Unsupported("COMPX exact model"))?;
        let (mid, name, max) = (model.mids[0],model.name.as_str(),model.max_dpi);
        let mut this = Self {
            io,
            mid,
            max,
            model,
            definitions: HashMap::new(),
            read_macros: Vec::new(),
            desc: DeviceDescriptor {
                id: format!(
                    "{}:{:04x}:{}",
                    model.brand, i.product_id,
                    i.serial
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&i.path)
                ),
                model_key: format!("{}:{:04x}", model.brand,i.product_id),
                vendor: Vendor::Other,
                kind: DeviceKind::Mouse,
                name: name.into(),
                vendor_id: i.vendor_id,
                product_id: i.product_id,
                connection: if model.wireless {
                    Connection::Wireless
                } else {
                    Connection::Wired
                },
                firmware: None,
                serial: i.serial.clone(),
                simulated: false,
            },
            caps: Capabilities {
                dpi: Some(DpiCaps {
                    min: if max == 36000 { 50 } else { 100 },
                    max,
                    step: 50,
                    max_stages: 8,
                    separate_xy: true,
                }),
                report_rates: model.rates.clone(),
                buttons: (1..=model.buttons)
                    .map(|id| ButtonCaps {
                        id,
                        name: ["左键", "右键", "中键", "后退", "前进", "底部 DPI"]
                            [(id - 1) as usize]
                            .into(),
                        remappable: id != 1,
                        default_action: factory(id).unwrap(),
                        hotspot: None,
                    })
                    .collect(),
                lighting: if model.indicator {
                    vec![LightingZoneCaps {
                        id: 0,
                        name: "当前 DPI 档位指示灯".into(),
                        effects: vec![EffectKind::Off, EffectKind::Static],
                        brightness: false,
                        keys: vec![],
                    }]
                } else {
                    vec![]
                },
                ..Default::default()
            },
        };
        // PID alone is ambiguous: R1/SE and F1 Pro Max/MOBA share IDs.
        // Accept only the explicitly catalogued CID/MID, then pin that identity.
        let identity = this.exchange(0x10,0,0)?;
        if identity.len()<2 || identity[0]!=2 || !model.mids.contains(&identity[1]) {
            return Err(Error::invalid("COMPX unknown CID/MID for this USB model; no settings written"));
        }
        this.mid=identity[1];
        this.refresh()?;
        Ok(this)
    }
    fn read_blob(&mut self, address: u16, len: usize) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        while out.len() < len {
            let n = (len - out.len()).min(10);
            out.extend(self.read(address + out.len() as u16, n)?);
        }
        Ok(out)
    }
    fn read_native_macro(&mut self, slot: u8, play: u8) -> Result<Macro> {
        if slot >= 12 {
            return Err(Error::invalid("ATK native macro slot"));
        }
        let base = 768 + u16::from(slot) * 384;
        let count = self.read(base + 31, 1)?[0] as usize;
        if count == 0 || count > 70 {
            return Err(Error::invalid("ATK native macro count"));
        }
        let raw = self.read_blob(base, 33 + count * 5)?;
        native_macro::decode(slot, play, &raw)
    }
    fn read_action(&mut self, raw: &[u8]) -> Option<(ButtonAction, Option<Macro>)> {
        if let Some(action) = decode_button(raw) {
            return Some((action, None));
        }
        if raw.len() == 4 && sum(raw) == 0x55 && raw[0] == 6 {
            let m = self.read_native_macro(raw[1], raw[2]).ok()?;
            return Some((ButtonAction::Macro(m.id.clone()), Some(m)));
        }
        None
    }
    fn native_binding(&mut self, id: u16, action: &ButtonAction) -> Result<()> {
        let macro_def = if let Some(m) = native_macro::key_action(action) {
            m
        } else if let ButtonAction::Macro(key) = action {
            self.definitions
                .get(key)
                .cloned()
                .ok_or(Error::invalid("ATK native macro definition missing"))?
        } else {
            return Err(Error::Unsupported("ATK native macro binding"));
        };
        let payload = native_macro::encode(&macro_def)?;
        let target = 96 + (id - 1) * 4;
        let original = self.read(target, 4)?;
        if self.read_action(&original).is_none() {
            return Err(Error::invalid("ATK unknown original action is retained"));
        }
        let mut occupied = std::collections::HashSet::new();
        for button in 1..=self.model.buttons {
            let b = self.read(96 + (button - 1) * 4, 4)?;
            if sum(&b) != 0x55 {
                return Err(Error::invalid("ATK button allocation checksum"));
            }
            if b[0] == 6 && button != id {
                occupied.insert(b[1]);
            }
        }
        let mut selected = None;
        for slot in 0..12u8 {
            if occupied.contains(&slot) {
                continue;
            }
            let count = self.read(768 + u16::from(slot) * 384 + 31, 1)?[0];
            if count == 0 || count == 255 {
                selected = Some(slot);
                break;
            }
        }
        // An existing non-empty, unbound macro is never silently repurposed.
        let slot = selected
            .or_else(|| {
                (original[0] == 6 && original[1] < 12 && !occupied.contains(&original[1]))
                    .then_some(original[1])
            })
            .ok_or(Error::invalid(
                "ATK has no free native macro slot; existing macros were retained",
            ))?;
        let base = 768 + u16::from(slot) * 384;
        let backup = self.read_blob(base, payload.len())?;
        let operation = (|| -> Result<()> {
            for (index, chunk) in payload.chunks(10).enumerate() {
                self.write_verified(base + index as u16 * 10, chunk)?;
            }
            let play = native_macro::mode(macro_def.mode);
            let row = [
                6,
                slot,
                play,
                0x55u8.wrapping_sub(6).wrapping_sub(slot).wrapping_sub(play),
            ];
            self.write_verified(target, &row)?;
            Ok(())
        })();
        if let Err(e) = operation {
            if self.identify().is_ok() {
                for (index, chunk) in backup.chunks(10).enumerate() {
                    let request = frame(7, base + index as u16 * 10, chunk.len(), chunk)?;
                    let _ = self.io.write(&request);
                    std::thread::sleep(Duration::from_millis(10));
                }
                let request = frame(7, target, 4, &original)?;
                let _ = self.io.write(&request);
            }
            return Err(Error::invalid(format!(
                "{e}; ATK original macro and binding restoration attempted; re-read required"
            )));
        }
        Ok(())
    }
    fn refresh(&mut self) -> Result<DeviceState> {
        self.identify()?;
        let system = self.read(0, 10)?;
        let count = checked_pair(&system[2..4])?;
        let active = checked_pair(&system[4..6])?;
        if !(1..=8).contains(&count) || active >= count {
            return Err(Error::invalid("ATK DPI stage metadata"));
        }
        let mut stages = Vec::new();
        for n in 0..count {
            stages.push(decode_dpi(&self.read(12 + u16::from(n) * 4, 4)?, self.max)?);
        }
        let mut state = DeviceState {
            dpi: Some(DpiState { stages, active }),
            report_rate: Some(rate_value(checked_pair(&system[..2])?)?),
            ..Default::default()
        };
        self.read_macros.clear();
        for id in 1..=self.model.buttons {
            let raw = self.read(96 + (id - 1) * 4, 4)?;
            let decoded = self.read_action(&raw);
            let known = decoded.is_some();
            if let Some((action, definition)) = decoded {
                state.buttons.insert(id, action);
                if let Some(m) = definition {
                    if !self.read_macros.iter().any(|v| v.id == m.id) {
                        self.read_macros.push(m);
                    }
                }
            }
            self.caps.buttons[(id - 1) as usize].remappable = id != 1 && known;
        }
        if !self.caps.lighting.is_empty() {
            let row = self.read(76, 8)?;
            let mode = checked_pair(&row[..2])?;
            let enabled = checked_pair(&row[6..8])?;
            if enabled == 0 {
                state.lighting.insert(
                    0,
                    ZoneState {
                        effect: LightingEffect::Off,
                        brightness: 100,
                    },
                );
            } else if enabled == 1 && mode == 1 {
                let color = self.read(44 + u16::from(active) * 4, 4)?;
                if sum(&color) != 0x55 {
                    return Err(Error::invalid("ATK DPI color checksum"));
                }
                state.lighting.insert(
                    0,
                    ZoneState {
                        effect: LightingEffect::Static {
                            color: Rgb {
                                r: color[0],
                                g: color[1],
                                b: color[2],
                            },
                        },
                        brightness: 100,
                    },
                );
            }
        }
        Ok(state)
    }
}
impl Device for Atk {
    fn set_macro_definitions(&mut self, definitions: &[Macro]) -> Result<()> {
        let mut parsed = HashMap::new();
        for m in definitions {
            native_macro::encode(m)?;
            if parsed.insert(m.id.clone(), m.clone()).is_some() {
                return Err(Error::invalid("ATK duplicate native macro id"));
            }
        }
        self.definitions = parsed;
        Ok(())
    }
    fn macro_definitions(&self) -> Vec<Macro> {
        self.read_macros.clone()
    }
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }
    fn capabilities(&self) -> &Capabilities {
        &self.caps
    }
    fn read_state(&mut self) -> Result<DeviceState> {
        self.refresh()
    }
    fn apply(&mut self, s: &Setting) -> Result<()> {
        // Validate the complete operation before any write. Never trust the caller's UI.
        match s {
            Setting::Dpi(d) => {
                if d.stages.is_empty()
                    || d.stages.len() > 8
                    || usize::from(d.active) >= d.stages.len()
                {
                    return Err(Error::invalid("ATK DPI stage count/index"));
                }
                for stage in &d.stages {
                    encode_dpi(stage.x, stage.y, self.max)?;
                }
            }
            Setting::DpiActiveStage(n) if *n < 8 => {}
            Setting::ReportRate(hz) => {
                rate_code(*hz)?;
                if !self.caps.report_rates.contains(hz) {
                    return Err(Error::invalid("ATK exact transport polling ceiling"));
                }
            }
            Setting::Button { id, action } => {
                if !(2..=self.model.buttons).contains(id) {
                    return Err(Error::invalid(
                        "ATK protects left click and unknown buttons",
                    ));
                }
                match action {
                    ButtonAction::Key { usage, mods } => {
                        native_macro::encode(&native_macro::keyboard(*usage, *mods))?;
                    }
                    ButtonAction::Macro(key) => {
                        native_macro::encode(
                            self.definitions
                                .get(key)
                                .ok_or(Error::invalid("ATK native macro definition missing"))?,
                        )?;
                    }
                    _ => {
                        button_bytes(action, *id)?;
                    }
                }
            }
            Setting::Lighting { zone: 0, state }
                if self.model.indicator
                    && state.brightness == 100
                    && matches!(
                        state.effect,
                        LightingEffect::Off | LightingEffect::Static { .. }
                    ) => {}
            _ => {
                return Err(Error::Unsupported(
                    "ATK setting is not implemented for this exact model",
                ))
            }
        }
        self.identify()?;
        match s {
            Setting::Dpi(d) => {
                for (index, stage) in d.stages.iter().enumerate() {
                    self.write_verified(
                        12 + index as u16 * 4,
                        &encode_dpi(stage.x, stage.y, self.max)?,
                    )?;
                }
                let mut row = self.read(0, 10)?;
                row[2..4].copy_from_slice(&pair(d.stages.len() as u8));
                row[4..6].copy_from_slice(&pair(d.active));
                self.write_verified(0, &row)?;
            }
            Setting::DpiActiveStage(n) => {
                let mut row = self.read(0, 10)?;
                if *n >= checked_pair(&row[2..4])? {
                    return Err(Error::invalid("ATK active DPI stage"));
                }
                row[4..6].copy_from_slice(&pair(*n));
                self.write_verified(0, &row)?;
            }
            Setting::ReportRate(hz) => {
                let mut row = self.read(0, 10)?;
                row[..2].copy_from_slice(&pair(rate_code(*hz)?));
                self.write_verified(0, &row)?;
            }
            Setting::Button { id, action }
                if matches!(action, ButtonAction::Macro(_) | ButtonAction::Key { .. }) =>
            {
                self.native_binding(*id, action)?;
            }
            Setting::Button { id, action } => {
                let old = self.read(96 + (id - 1) * 4, 4)?;
                if self.read_action(&old).is_none() {
                    return Err(Error::invalid("ATK unknown original action retained; native macro/shortcut is not overwritten"));
                }
                self.write_verified(96 + (id - 1) * 4, &button_bytes(action, *id)?)?;
            }
            Setting::Lighting { state, .. } => {
                let mut row = self.read(76, 8)?;
                if let LightingEffect::Static { color } = state.effect {
                    let sys = self.read(0, 10)?;
                    let active = checked_pair(&sys[4..6])?;
                    if active >= checked_pair(&sys[2..4])? {
                        return Err(Error::invalid("ATK active DPI color index"));
                    }
                    let p = [
                        color.r,
                        color.g,
                        color.b,
                        0x55u8
                            .wrapping_sub(color.r)
                            .wrapping_sub(color.g)
                            .wrapping_sub(color.b),
                    ];
                    self.write_verified(44 + u16::from(active) * 4, &p)?;
                    row[..2].copy_from_slice(&pair(1));
                    row[6..8].copy_from_slice(&pair(1));
                } else {
                    row[6..8].copy_from_slice(&pair(0));
                }
                self.write_verified(76, &row)?;
            }
            _ => unreachable!(),
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct AtkDriver;
impl Driver for AtkDriver {
    fn name(&self) -> &'static str {
        "ATK exact COMPX transport"
    }
    fn vendor(&self) -> Vendor {
        Vendor::Other
    }
    fn probe(&self, i: &HidInterfaceInfo) -> bool {
        supported(i.vendor_id, i.product_id) && i.usage_page == 0xff02 && i.usage == 2
    }
    fn open(&self, i: &HidInterfaceInfo, b: &dyn HidBackend) -> Result<Vec<Box<dyn Device>>> {
        if !self.probe(i) {
            return Err(Error::Unsupported("ATK exact control collection"));
        }
        Ok(vec![Box::new(Atk::open(b.open(&i.path)?, i)?)])
    }
}
fn memory_device(
    product: u16,
) -> Result<(Atk, Arc<Mutex<Vec<gearwise_core::transport::MockRecord>>>)> {
    use gearwise_core::transport::{MockOp, MockReply, MockTransport};
    let spec=models().iter().find(|m|m.product==product).ok_or(Error::Unsupported("COMPX simulation"))?;
    let (mid, _, max) = identity(product).unwrap();
    let mut eeprom = vec![0; 768 + 12 * 384];
    for (index, value) in [1, 4, 1, 7, 0].into_iter().enumerate() {
        eeprom[index * 2..index * 2 + 2].copy_from_slice(&pair(value));
    }
    for (index, dpi) in [800, 1600, 3200, 6400, 8000, 12000, 16000, 24000]
        .into_iter()
        .enumerate()
    {
        eeprom[12 + index * 4..16 + index * 4].copy_from_slice(&encode_dpi(dpi, dpi, max)?);
    }
    for id in 1..=6 {
        eeprom[96 + (id - 1) * 4..100 + (id - 1) * 4]
            .copy_from_slice(&button_bytes(&ButtonAction::Default, id as u16)?);
    }
    for index in 0..8 {
        eeprom[44 + index * 4..48 + index * 4].copy_from_slice(&[0, 255, 0, 86]);
    }
    for (index, value) in [1, 128, 3, 1].into_iter().enumerate() {
        eeprom[76 + index * 2..78 + index * 2].copy_from_slice(&pair(value));
    }
    let io = MockTransport::new(move |op| {
        let MockOp::Write(req) = op else {
            return MockReply::none();
        };
        if req.len() != 17 || req[0] != 8 || sum(req) != 0x55 {
            return MockReply::none();
        }
        let addr = u16::from_be_bytes([req[3], req[4]]) as usize;
        let count = req[5] as usize;
        if count > 10 || addr + count > eeprom.len() {
            return MockReply::none();
        }
        let data = match req[1] {
            0x10 => vec![2, mid],
            8 => eeprom[addr..addr + count].to_vec(),
            7 => {
                eeprom[addr..addr + count].copy_from_slice(&req[6..6 + count]);
                return MockReply::none();
            }
            _ => return MockReply::none(),
        };
        MockReply::input(
            frame(req[1], addr as u16, data.len(), &data)
                .unwrap()
                .to_vec(),
        )
    });
    let log = io.log();
    let info = HidInterfaceInfo {
        path: format!("simulation:atk:{product:04x}"),
        vendor_id: spec.vendor,
        product_id: product,
        usage_page: 0xff02,
        usage: 2,
        ..Default::default()
    };
    let mut device = Atk::open(Box::new(io), &info)?;
    device.desc.simulated = true;
    Ok((device, log))
}

pub fn simulated(
    product: u16,
) -> Result<(
    Box<dyn Device>,
    Arc<Mutex<Vec<gearwise_core::transport::MockRecord>>>,
)> {
    let (d, log) = memory_device(product)?;
    Ok((Box::new(d), log))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atk_real_driver_dpi_buttons_and_rate_roundtrip() {
        for product in [0x1115, 0x1017, 0x10c9] {
            let (mut d, _) = simulated(product).unwrap();
            d.apply(&Setting::Dpi(DpiState {
                stages: vec![DpiStage { x: 1200, y: 1600 }, DpiStage { x: 2400, y: 3200 }],
                active: 1,
            }))
            .unwrap();
            d.apply(&Setting::ReportRate(500)).unwrap();
            d.apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Dpi(DpiFunction::Down),
            })
            .unwrap();
            let state = d.read_state().unwrap();
            assert_eq!(state.dpi.unwrap().stages[1], DpiStage { x: 2400, y: 3200 });
            assert_eq!(state.report_rate, Some(500));
            assert_eq!(state.buttons[&4], ButtonAction::Dpi(DpiFunction::Down));
        }
    }
    #[test]
    fn atk_rejects_entire_invalid_dpi_before_any_io() {
        let (mut d, log) = simulated(0x1017).unwrap();
        log.lock().unwrap().clear();
        assert!(d
            .apply(&Setting::Dpi(DpiState {
                stages: vec![
                    DpiStage { x: 1600, y: 1600 },
                    DpiStage { x: 30150, y: 30150 }
                ],
                active: 0
            }))
            .is_err());
        assert!(log.lock().unwrap().is_empty());
    }
    #[test]
    fn atk_scope_does_not_claim_other_brands_or_new_protocols() {
        let mut i = HidInterfaceInfo {
            vendor_id: 0x373b,
            product_id: 0x1115,
            usage_page: 0xff02,
            usage: 2,
            ..Default::default()
        };
        assert!(AtkDriver.probe(&i));
        i.usage_page = 0xff05;
        assert!(!AtkDriver.probe(&i));
        i.usage_page = 0xff02;
        i.vendor_id = 0x046d;
        assert!(!AtkDriver.probe(&i));
        i.vendor_id = 0x373b;
        i.product_id = 0x1263;
        assert!(!AtkDriver.probe(&i));
    }
    #[test]
    fn atk_unknown_actions_are_not_misrepresented_as_default() {
        assert!(decode_button(&[6, 1, 254, 90]).is_none());
        assert!(decode_button(&[5, 0, 0, 80]).is_none());
    }
    #[test]
    fn atk_a9_has_no_fake_rgb_and_left_click_stays_protected() {
        let (mut d, log) = simulated(0x1115).unwrap();
        assert!(d.capabilities().lighting.is_empty());
        log.lock().unwrap().clear();
        assert!(d
            .apply(&Setting::Lighting {
                zone: 0,
                state: ZoneState {
                    effect: LightingEffect::Off,
                    brightness: 100
                }
            })
            .is_err());
        assert!(d
            .apply(&Setting::Button {
                id: 1,
                action: ButtonAction::Disabled
            })
            .is_err());
        assert!(log.lock().unwrap().is_empty());
    }
    #[test]
    fn atk_x1_dpi_color_readback_preserves_other_fields() {
        let (mut d, log) = simulated(0x1017).unwrap();
        d.apply(&Setting::Lighting {
            zone: 0,
            state: ZoneState {
                effect: LightingEffect::Off,
                brightness: 100,
            },
        })
        .unwrap();
        let color = Rgb {
            r: 21,
            g: 22,
            b: 23,
        };
        d.apply(&Setting::Lighting {
            zone: 0,
            state: ZoneState {
                effect: LightingEffect::Static { color },
                brightness: 100,
            },
        })
        .unwrap();
        assert_eq!(
            d.read_state().unwrap().lighting[&0].effect,
            LightingEffect::Static { color }
        );
        assert!(log.lock().unwrap().iter().any(|r|matches!(r,gearwise_core::transport::MockRecord::Write(p) if p[1]==7&&p[3..5]==[0,76]&&p[8..12]==[128,213,3,82])));
    }
    #[test]
    fn atk_dpi_codec_has_known_wire_vectors_and_checksums() {
        assert_eq!(encode_dpi(800, 800, 30000).unwrap(), [15, 15, 0, 55]);
        assert_eq!(encode_dpi(36000, 36000, 36000).unwrap(), [103, 103, 85, 50]);
        for dpi in (100..=30000).step_by(50) {
            let p = encode_dpi(dpi, dpi, 30000).unwrap();
            assert_eq!(decode_dpi(&p, 30000).unwrap(), DpiStage { x: dpi, y: dpi });
        }
        assert!(decode_dpi(&[15, 15, 0, 0], 30000).is_err());
    }
}

#[cfg(test)]
mod faults {
    use super::*;
    struct Fault {
        inner: Box<dyn HidTransport>,
        kind: u8,
    }
    impl HidTransport for Fault {
        fn write(&mut self, p: &[u8]) -> Result<usize> {
            if self.kind == 1 {
                return Ok(16);
            }
            self.inner.write(p)
        }
        fn read_timeout(&mut self, b: &mut [u8], ms: i32) -> Result<usize> {
            let n = self.inner.read_timeout(b, ms)?;
            if n == 17 {
                match self.kind {
                    2 => b[16] ^= 1,
                    3 => return Ok(12),
                    4 => {
                        b[2] = 1;
                        b[16] = 0x55u8.wrapping_sub(sum(&b[..16]));
                    }
                    5 => {
                        if b[1] == 0x10 {
                            b[7] = 99;
                            b[16] = 0x55u8.wrapping_sub(sum(&b[..16]));
                        }
                    }
                    6 => {
                        if b[1] == 8 {
                            b[1] = 7;
                            b[16] = 0x55u8.wrapping_sub(sum(&b[..16]));
                        }
                    }
                    _ => {}
                }
            }
            Ok(n)
        }
        fn send_feature_report(&mut self, p: &[u8]) -> Result<()> {
            self.inner.send_feature_report(p)
        }
        fn get_feature_report(&mut self, p: &mut [u8]) -> Result<usize> {
            self.inner.get_feature_report(p)
        }
    }
    #[test]
    fn atk_short_write_corrupt_truncated_status_and_identity_fail_before_settings() {
        for kind in 1..=5 {
            let (mut d, log) = memory_device(0x1115).unwrap();
            log.lock().unwrap().clear();
            d.io = Box::new(Fault { inner: d.io, kind });
            assert!(d.apply(&Setting::ReportRate(500)).is_err(), "fault {kind}");
            assert!(!log
                .lock()
                .unwrap()
                .iter()
                .any(|r| matches!(r,gearwise_core::transport::MockRecord::Write(p) if p[1]==7)));
        }
    }
    #[test]
    fn atk_write_ack_cannot_be_accepted_as_readback() {
        let (mut d, _) = memory_device(0x1115).unwrap();
        d.io = Box::new(Fault {
            inner: d.io,
            kind: 6,
        });
        assert!(matches!(d.read(0, 10), Err(Error::Timeout)));
    }
    #[test]
    fn atk_packet_matches_sanitized_a9_capture() {
        assert_eq!(
            frame(0x10, 0, 0, &[]).unwrap(),
            [8, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 61]
        );
        assert_eq!(
            frame(8, 12, 4, &[]).unwrap(),
            [8, 8, 0, 0, 12, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 53]
        );
    }
}

#[cfg(test)]
mod native_macro_integration {
    use super::*;
    use gearwise_core::profile::{Macro, MacroMode, MacroStep};
    fn definition(mode: MacroMode) -> Macro {
        Macro {
            id: "new-macro".into(),
            name: "测试".into(),
            mode,
            steps: vec![
                MacroStep::KeyDown(4),
                MacroStep::Delay(50),
                MacroStep::KeyUp(4),
            ],
        }
    }
    #[test]
    fn atk_macro_upload_readback_import_and_reapply_use_real_driver() {
        for product in [0x1115, 0x1017, 0x10c9] {
            for mode in [MacroMode::Once, MacroMode::WhileHeld, MacroMode::Toggle] {
                let (mut d, log) = memory_device(product).unwrap();
                d.set_macro_definitions(&[definition(mode)]).unwrap();
                d.apply(&Setting::Button {
                    id: 4,
                    action: ButtonAction::Macro("new-macro".into()),
                })
                .unwrap();
                let state = d.read_state().unwrap();
                let ButtonAction::Macro(id) = &state.buttons[&4] else {
                    panic!("native macro not read back")
                };
                let decoded = d
                    .macro_definitions()
                    .into_iter()
                    .find(|m| &m.id == id)
                    .unwrap();
                assert_eq!(decoded.mode, mode);
                assert_eq!(decoded.steps[0], MacroStep::KeyDown(4));
                assert!(decoded.steps.contains(&MacroStep::Delay(50)));
                assert_eq!(d.read(768 + 31, 1).unwrap(), [2]);
                assert!(log.lock().unwrap().iter().any(|r|matches!(r,gearwise_core::transport::MockRecord::Write(p) if p[1]==7&&p[3..5]==[0,108]&&p[6..9]==[6,0,native_macro::mode(mode)])));
                let (mut other, _) = memory_device(product).unwrap();
                other.set_macro_definitions(&[decoded.clone()]).unwrap();
                other
                    .apply(&Setting::Button {
                        id: 5,
                        action: ButtonAction::Macro(decoded.id),
                    })
                    .unwrap();
                assert!(matches!(
                    other.read_state().unwrap().buttons[&5],
                    ButtonAction::Macro(_)
                ));
            }
        }
    }
    #[test]
    fn atk_keyboard_tap_uses_board_storage_without_host_key_injection() {
        let (mut d, _) = memory_device(0x1115).unwrap();
        d.apply(&Setting::Button {
            id: 4,
            action: ButtonAction::Key {
                usage: 4,
                mods: gearwise_core::model::Modifiers(3),
            },
        })
        .unwrap();
        d.read_state().unwrap();
        let m = &d.macro_definitions()[0];
        assert_eq!(m.mode, MacroMode::Once);
        assert!(m.steps.contains(&MacroStep::KeyDown(0xe0)));
        assert!(m.steps.contains(&MacroStep::KeyUp(0xe1)));
    }
    #[test]
    fn atk_unbound_original_macros_are_never_overwritten() {
        let (mut d, _) = memory_device(0x1115).unwrap();
        let old = native_macro::encode(&definition(MacroMode::Once)).unwrap();
        for slot in 0..12 {
            for (index, chunk) in old.chunks(10).enumerate() {
                d.write_verified(768 + slot * 384 + index as u16 * 10, chunk)
                    .unwrap();
            }
        }
        d.set_macro_definitions(&[definition(MacroMode::Once)])
            .unwrap();
        let before = d.read_blob(768, 384).unwrap();
        assert!(d
            .apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Macro("new-macro".into())
            })
            .is_err());
        assert_eq!(d.read_blob(768, 384).unwrap(), before);
        assert_eq!(
            d.read(108, 4).unwrap(),
            button_bytes(&ButtonAction::Default, 4).unwrap()
        );
    }
    struct FailOnce {
        inner: Box<dyn HidTransport>,
        writes: usize,
    }
    impl HidTransport for FailOnce {
        fn write(&mut self, p: &[u8]) -> Result<usize> {
            if p[1] == 7 {
                self.writes += 1;
                if self.writes == 3 {
                    return Err(Error::invalid("synthetic macro upload failure"));
                }
            }
            self.inner.write(p)
        }
        fn read_timeout(&mut self, p: &mut [u8], ms: i32) -> Result<usize> {
            self.inner.read_timeout(p, ms)
        }
        fn send_feature_report(&mut self, p: &[u8]) -> Result<()> {
            self.inner.send_feature_report(p)
        }
        fn get_feature_report(&mut self, p: &mut [u8]) -> Result<usize> {
            self.inner.get_feature_report(p)
        }
    }
    #[test]
    fn atk_partial_macro_upload_restores_slot_and_keeps_original_button() {
        let (mut d, _) = memory_device(0x1115).unwrap();
        let before = d.read_blob(768, 384).unwrap();
        d.set_macro_definitions(&[definition(MacroMode::Once)])
            .unwrap();
        d.io = Box::new(FailOnce {
            inner: d.io,
            writes: 0,
        });
        assert!(d
            .apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Macro("new-macro".into())
            })
            .is_err());
        assert_eq!(d.read_blob(768, 384).unwrap(), before);
        assert_eq!(
            d.read(108, 4).unwrap(),
            button_bytes(&ButtonAction::Default, 4).unwrap()
        );
    }
}

#[cfg(test)]
mod dpi_limits_tests {
    use super::*;
    #[test]
    fn atk_eight_stage_native_layout_and_model_specific_rate_limits() {
        for product in [0x1115, 0x1017, 0x10c9] {
            let (mut d, log) = memory_device(product).unwrap();
            let dpi = DpiState {
                stages: (1..=8)
                    .map(|n| DpiStage {
                        x: n * 400,
                        y: n * 800,
                    })
                    .collect(),
                active: 7,
            };
            d.apply(&Setting::Dpi(dpi.clone())).unwrap();
            assert_eq!(d.read_state().unwrap().dpi, Some(dpi));
            if product == 0x1017 {
                d.apply(&Setting::Dpi(DpiState {
                    stages: vec![DpiStage { x: 50, y: 36000 }],
                    active: 0,
                }))
                .unwrap();
                d.apply(&Setting::ReportRate(8000)).unwrap();
                assert_eq!(d.read_state().unwrap().report_rate, Some(8000));
            } else {
                log.lock().unwrap().clear();
                assert!(d.apply(&Setting::ReportRate(8000)).is_err());
                assert!(log.lock().unwrap().is_empty());
            }
        }
    }
    #[test]
    fn compx_new_models_use_real_dpi_button_macro_and_indicator_packets(){
      use gearwise_core::profile::{MacroMode,MacroStep};
      for spec in models().iter().filter(|m|m.brand!="atk"){
        let (mut d,log)=memory_device(spec.product).unwrap();
        assert_eq!(d.descriptor().vendor_id,spec.vendor);
        assert!(d.descriptor().id.starts_with(&format!("{}:",spec.brand)));
        for mode in [MacroMode::Once,MacroMode::WhileHeld,MacroMode::Toggle]{
          let definition=Macro{id:"compx-native-test".into(),name:"compx-native-test".into(),mode:mode.clone(),steps:vec![MacroStep::KeyDown(4),MacroStep::Delay(10),MacroStep::KeyUp(4)]};d.set_macro_definitions(&[definition.clone()]).unwrap();
          d.apply(&Setting::Button{id:4,action:ButtonAction::Macro(definition.id.clone())}).unwrap();
          let state=d.read_state().unwrap();assert!(matches!(state.buttons[&4],ButtonAction::Macro(_)));
          assert_eq!(d.macro_definitions()[0].mode,mode);
        }
        let dpi=DpiState{stages:vec![DpiStage{x:100,y:150},DpiStage{x:30000,y:25000}],active:1};
        d.apply(&Setting::Dpi(dpi.clone())).unwrap();d.apply(&Setting::ReportRate(1000)).unwrap();
        let state=d.read_state().unwrap();assert_eq!(state.dpi,Some(dpi));assert_eq!(state.report_rate,Some(1000));
        assert_eq!(d.capabilities().buttons.len(),spec.buttons as usize);
        if spec.indicator{
          let light=ZoneState{effect:LightingEffect::Static{color:Rgb{r:21,g:62,b:203}},brightness:100};
          d.apply(&Setting::Lighting{zone:0,state:light.clone()}).unwrap();assert_eq!(d.read_state().unwrap().lighting[&0],light);
        }else{assert!(d.capabilities().lighting.is_empty());}
        log.lock().unwrap().clear();
        assert!(d.apply(&Setting::ReportRate(8000)).is_err());
        assert!(d.apply(&Setting::Button{id:1,action:ButtonAction::Disabled}).is_err());
        if spec.buttons==5{assert!(d.apply(&Setting::Button{id:6,action:ButtonAction::Disabled}).is_err());}
        assert!(log.lock().unwrap().is_empty());
      }
    }
    struct ModelIdentity{inner:Box<dyn HidTransport>,cid:u8,mid:u8}
    impl HidTransport for ModelIdentity{
      fn write(&mut self,p:&[u8])->Result<usize>{self.inner.write(p)}
      fn read_timeout(&mut self,p:&mut[u8],ms:i32)->Result<usize>{let n=self.inner.read_timeout(p,ms)?;if n==17&&p[1]==0x10{p[6]=self.cid;p[7]=self.mid;p[16]=0x55u8.wrapping_sub(sum(&p[..16]));}Ok(n)}
      fn send_feature_report(&mut self,p:&[u8])->Result<()>{self.inner.send_feature_report(p)}
      fn get_feature_report(&mut self,p:&mut[u8])->Result<usize>{self.inner.get_feature_report(p)}
    }
    #[test]
    fn compx_shared_pids_reject_unimplemented_r1_se_r1_max_and_f1_moba_before_eeprom(){
      for (product,mid) in [(0xf58f,29),(0xf58c,27),(0xf506,5)]{
        let (d,log)=memory_device(product).unwrap();log.lock().unwrap().clear();
        let info=HidInterfaceInfo{vendor_id:0x3554,product_id:product,usage_page:0xff02,usage:2,..Default::default()};
        assert!(Atk::open(Box::new(ModelIdentity{inner:d.io,cid:2,mid}),&info).is_err());
        assert!(log.lock().unwrap().iter().all(|p|!matches!(p,gearwise_core::transport::MockRecord::Write(v) if v[1]==7||v[1]==8)));
      }
    }
    #[test]
    fn compx_all_allowed_color_identities_open_but_swapped_identity_blocks_writes(){
      for spec in models().iter().filter(|m|m.brand!="atk"){
        for &mid in &spec.mids{
          let (d,log)=memory_device(spec.product).unwrap();
          let info=HidInterfaceInfo{vendor_id:spec.vendor,product_id:spec.product,usage_page:0xff02,usage:2,..Default::default()};
          let mut d=Atk::open(Box::new(ModelIdentity{inner:d.io,cid:2,mid}),&info).unwrap();assert_eq!(d.mid,mid);
          d.io=Box::new(ModelIdentity{inner:d.io,cid:2,mid:255});log.lock().unwrap().clear();
          assert!(d.apply(&Setting::ReportRate(500)).is_err());
          assert!(log.lock().unwrap().iter().all(|p|!matches!(p,gearwise_core::transport::MockRecord::Write(v) if v[1]==7)));
        }
      }
    }
    #[test]
    fn compx_registry_is_unambiguous_and_does_not_open_ff04_or_bitmouse(){
      let mut keys=std::collections::HashSet::new();
      for spec in models(){assert!(keys.insert(spec.product),"future shared PID models need explicit plan identity");
        for page in [0xff04,0xff05,1]{let info=HidInterfaceInfo{vendor_id:spec.vendor,product_id:spec.product,usage_page:page,usage:2,..Default::default()};assert!(!AtkDriver.probe(&info));}
      }
      assert!(!supported(0x373b,0x124c));assert!(!supported(0xa8a4,0x2255));
    }

}
