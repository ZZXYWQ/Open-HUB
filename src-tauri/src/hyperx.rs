//! HyperX Pulsefire Haste (HP USB IDs only), experimental native adapter.
//! Protocol reference: Evan Razzaque's MIT-licensed libratbag PR #1786,
//! eceface88e0ce228eb4a335f10689e9f6677b648. No vendor executable runs.
//! DPI/rate/basic bindings read back; lighting and macro contents do not.
use gearwise_core::{
    model::*,
    profile::{Macro, MacroMode, MacroStep},
    *,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
const RATES: [u16; 4] = [125, 250, 500, 1000];
pub fn supported(v: u16, p: u16) -> bool {
    v == 0x03f0 && matches!(p, 0x048e | 0x028e)
}
fn name(p: u16) -> &'static str {
    if p == 0x028e {
        "HyperX Pulsefire Haste Wireless"
    } else {
        "HyperX Pulsefire Haste"
    }
}
fn packet(cmd: u8, sub: u8, index: u8, data: &[u8]) -> [u8; 64] {
    let mut p = [0; 64];
    p[0] = cmd;
    p[1] = sub;
    p[2] = index;
    p[3] = data.len() as u8;
    p[4..4 + data.len()].copy_from_slice(data);
    p
}
pub fn validate_dpi(d: &DpiState) -> Result<()> {
    if d.stages.is_empty()
        || d.stages.len() > 5
        || d.active as usize >= d.stages.len()
        || d.stages
            .iter()
            .any(|s| s.x != s.y || s.x < 200 || s.x > 16000 || s.x % 100 != 0)
    {
        return Err(Error::invalid(
            "HyperX Haste: 1–5 equal XY stages, 200–16000 DPI in steps of 100",
        ));
    }
    Ok(())
}
fn factory(id: u16) -> Result<ButtonAction> {
    Ok(match id {
        1 => ButtonAction::Mouse(MouseButton::Left),
        2 => ButtonAction::Mouse(MouseButton::Right),
        3 => ButtonAction::Mouse(MouseButton::Middle),
        4 => ButtonAction::Mouse(MouseButton::Back),
        5 => ButtonAction::Mouse(MouseButton::Forward),
        6 => ButtonAction::Dpi(DpiFunction::Cycle),
        _ => return Err(Error::invalid("HyperX button")),
    })
}
fn read_row(kind: u8, code: u8) -> [u8; 3] {
    [
        kind,
        code,
        if kind == 1 && (1..=5).contains(&code) {
            1 << (code - 1)
        } else {
            code
        },
    ]
}
fn raw_action(id: u16, a: &ButtonAction) -> Result<[u8; 3]> {
    let a = if matches!(a, ButtonAction::Default) {
        factory(id)?
    } else {
        a.clone()
    };
    let row = match a {
        ButtonAction::Disabled => [0, 0, 0],
        ButtonAction::Mouse(m) => [
            1,
            match m {
                MouseButton::Left => 1,
                MouseButton::Right => 2,
                MouseButton::Middle => 3,
                MouseButton::Back => 4,
                MouseButton::Forward => 5,
                _ => return Err(Error::Unsupported("HyperX mouse action")),
            },
            0,
        ],
        ButtonAction::Dpi(DpiFunction::Cycle) => [7, 8, 0],
        ButtonAction::Key { usage, mods } if (4..=0xa4).contains(&usage) && mods.0 == 0 => {
            [2, usage, 0]
        }
        _ => return Err(Error::Unsupported("HyperX native button action")),
    };
    Ok(read_row(row[0], row[1]))
}
fn decode_action(a: &[u8]) -> Option<ButtonAction> {
    if a.len() != 3 || a != read_row(a[0], a[1]) {
        return None;
    }
    match (a[0], a[1]) {
        (0, 0) => Some(ButtonAction::Disabled),
        (1, n) => match n {
            1 => Some(MouseButton::Left),
            2 => Some(MouseButton::Right),
            3 => Some(MouseButton::Middle),
            4 => Some(MouseButton::Back),
            5 => Some(MouseButton::Forward),
            _ => None,
        }
        .map(ButtonAction::Mouse),
        (2, k) if (4..=0xa4).contains(&k) => Some(ButtonAction::Key {
            usage: k,
            mods: Modifiers(0),
        }),
        (7, 8) => Some(ButtonAction::Dpi(DpiFunction::Cycle)),
        _ => None,
    }
}
/// Each 10-byte event contains a keyboard snapshot, six usages, and a LE delay.
/// No host hooks, text synthesis, mouse events or unverified repeat commands.
fn macro_events(m: &Macro) -> Result<Vec<[u8; 10]>> {
    if m.mode != MacroMode::Once || m.steps.is_empty() || m.steps.len() > 512 {
        return Err(Error::invalid("HyperX native macro: single playback only"));
    }
    let mut events: Vec<[u8; 10]> = Vec::new();
    let mut held = BTreeSet::new();
    let mut elapsed = 0u32;
    for step in &m.steps {
        match step {
            MacroStep::Delay(ms) => {
                if *ms > 10000 {
                    return Err(Error::invalid("HyperX macro delay"));
                }
                elapsed = elapsed
                    .checked_add(*ms)
                    .ok_or(Error::invalid("HyperX macro duration"))?;
                if elapsed > 60000 {
                    return Err(Error::invalid("HyperX macro duration"));
                }
                let e = events
                    .last_mut()
                    .ok_or(Error::invalid("HyperX leading delay is not encoded"))?;
                let previous = u16::from_le_bytes([e[8], e[9]]);
                let value = previous
                    .checked_add(*ms as u16)
                    .ok_or(Error::invalid("HyperX event delay overflow"))?;
                e[8..10].copy_from_slice(&value.to_le_bytes());
            }
            MacroStep::KeyDown(k) | MacroStep::KeyUp(k) => {
                if !((4..=0xa4).contains(k) || (0xe0..=0xe7).contains(k)) {
                    return Err(Error::invalid("HyperX macro keyboard usage"));
                }
                if matches!(step, MacroStep::KeyDown(_)) {
                    if !held.insert(*k) {
                        return Err(Error::invalid("HyperX duplicate key down"));
                    }
                } else if !held.remove(k) {
                    return Err(Error::invalid("HyperX unmatched key up"));
                }
                let mut e = [0; 10];
                e[0] = 0x1a;
                let mut n = 0;
                for k in &held {
                    if *k >= 0xe0 {
                        e[1] |= 1 << (*k - 0xe0);
                    } else {
                        if n >= 6 {
                            return Err(Error::invalid(
                                "HyperX native macro supports six simultaneous non-modifier keys",
                            ));
                        }
                        e[2 + n] = *k;
                        n += 1;
                    }
                }
                events.push(e);
                if events.len() > 80 {
                    return Err(Error::invalid(
                        "HyperX native macro has more than 80 keyboard snapshots",
                    ));
                }
            }
            _ => {
                return Err(Error::Unsupported(
                    "HyperX native macro only encodes keyboard press/release/delay",
                ))
            }
        }
    }
    if !held.is_empty() {
        return Err(Error::invalid("HyperX macro must release every key"));
    }
    Ok(events)
}
pub fn validate_macro(m: &Macro) -> Result<()> {
    macro_events(m).map(|_| ())
}
fn key_macro(usage: u8, mods: Modifiers) -> Macro {
    let mut steps = Vec::new();
    for n in 0..8 {
        if mods.0 & (1 << n) != 0 {
            steps.push(MacroStep::KeyDown(0xe0 + n));
        }
    }
    steps.push(MacroStep::KeyDown(usage));
    steps.push(MacroStep::Delay(20));
    steps.push(MacroStep::KeyUp(usage));
    for n in (0..8).rev() {
        if mods.0 & (1 << n) != 0 {
            steps.push(MacroStep::KeyUp(0xe0 + n));
        }
    }
    Macro {
        id: format!("hyperx-shortcut-{usage:02x}-{:02x}", mods.0),
        name: "Keyboard shortcut".into(),
        mode: MacroMode::Once,
        steps,
    }
}
pub fn validate_key(usage: u8, mods: Modifiers) -> Result<()> {
    validate_macro(&key_macro(usage, mods))
}
pub fn validate_setting(s: &Setting) -> Result<()> {
    match s {
        Setting::Dpi(d) => validate_dpi(d),
        Setting::ReportRate(r) if RATES.contains(r) => Ok(()),
        Setting::Button { id, action } if (2..=6).contains(id) => match action {
            ButtonAction::Macro(_) => Ok(()),
            ButtonAction::Key { usage, mods } => validate_key(*usage, *mods),
            _ => raw_action(*id, action).map(|_| ()),
        },
        Setting::Lighting { zone: 0, state }
            if state.brightness <= 100
                && matches!(
                    state.effect,
                    LightingEffect::Off | LightingEffect::Static { .. }
                ) =>
        {
            Ok(())
        }
        _ => Err(Error::Unsupported(
            "HyperX exact Haste setting; left click is protected",
        )),
    }
}
struct Haste {
    io: Box<dyn HidTransport>,
    desc: DeviceDescriptor,
    caps: Capabilities,
    definitions: BTreeMap<String, Macro>,
    owned: BTreeMap<u16, Macro>,
    light: Option<ZoneState>,
    physical: Vec<u8>,
}
impl Haste {
    fn send(&mut self, p: &[u8; 64]) -> Result<()> {
        let mut report = [0; 65];
        report[1..].copy_from_slice(p);
        if self.io.write(&report)? != 65 {
            return Err(Error::invalid(
                "HyperX short output write; reconnect and re-read",
            ));
        }
        Ok(())
    }
    fn request(&mut self, cmd: u8, sub: u8) -> Result<[u8; 64]> {
        self.send(&packet(cmd, sub, 0, &[]))?;
        let end = Instant::now() + Duration::from_millis(600);
        while Instant::now() < end {
            let mut p = [0; 65];
            let n = self.io.read_timeout(&mut p, 50)?;
            if n == 0 {
                continue;
            }
            if p[0] != cmd {
                continue;
            }
            if n != 64 {
                return Err(Error::invalid("HyperX input report length"));
            }
            if cmd == 0x50 && p[1] != sub {
                continue;
            }
            return Ok(p[..64].try_into().unwrap());
        }
        Err(Error::Timeout)
    }
    fn identify(&mut self) -> Result<()> {
        let info = self.request(0x50, 0)?;
        if info[3] != 60
            || u16::from_le_bytes([info[4], info[5]]) != self.desc.product_id
            || u16::from_le_bytes([info[6], info[7]]) != self.desc.vendor_id
        {
            return Err(Error::invalid(
                "HyperX hardware identity mismatch; no settings written",
            ));
        }
        Ok(())
    }
    fn snapshot(&mut self) -> Result<([u8; 64], [u8; 64])> {
        let config = self.request(0x50, 3)?;
        let dpi = self.request(0x53, 0)?;
        let bits = dpi[5];
        if bits == 0
            || bits & !31 != 0
            || dpi[4] >= 5
            || bits & (1 << dpi[4]) == 0
            || config[53] >= 4
        {
            return Err(Error::invalid(
                "HyperX malformed DPI mask, active stage or polling index",
            ));
        }
        for i in 0..5 {
            let value = u16::from_le_bytes([config[10 + i * 2], config[11 + i * 2]]);
            // All five values must be restorable, including currently disabled slots.
            if !(2..=160).contains(&value) {
                return Err(Error::invalid("HyperX DPI response out of range"));
            }
        }
        Ok((config, dpi))
    }
    fn open(io: Box<dyn HidTransport>, i: &HidInterfaceInfo) -> Result<Self> {
        if !HasteDriver.probe(i) {
            return Err(Error::Unsupported(
                "HyperX exact HP Haste control collection",
            ));
        }
        let mut d = Self {
            io,
            desc: DeviceDescriptor {
                id: format!(
                    "hyperx:{:04x}:{}",
                    i.product_id,
                    i.serial
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&i.path)
                ),
                model_key: format!("hyperx:{:04x}", i.product_id),
                vendor: Vendor::Other,
                kind: DeviceKind::Mouse,
                name: name(i.product_id).into(),
                vendor_id: i.vendor_id,
                product_id: i.product_id,
                connection: if i.product_id == 0x028e {
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
                    min: 200,
                    max: 16000,
                    step: 100,
                    max_stages: 5,
                    separate_xy: false,
                }),
                report_rates: RATES.to_vec(),
                buttons: (1..=6)
                    .map(|id| ButtonCaps {
                        id,
                        name: ["左键", "右键", "中键", "后退", "前进", "顶部 DPI"]
                            [(id - 1) as usize]
                            .into(),
                        default_action: factory(id).unwrap(),
                        remappable: id != 1,
                        hotspot: None,
                    })
                    .collect(),
                lighting: vec![LightingZoneCaps {
                    id: 0,
                    name: "滚轮 RGB · 仅会话可读".into(),
                    effects: vec![EffectKind::Off, EffectKind::Static],
                    brightness: true,
                    keys: vec![],
                }],
                ..Default::default()
            },
            definitions: BTreeMap::new(),
            owned: BTreeMap::new(),
            light: None,
            physical: vec![],
        };
        d.identify()?;
        d.refresh()?;
        Ok(d)
    }
    fn refresh(&mut self) -> Result<DeviceState> {
        let (c, d) = self.snapshot()?;
        self.physical = (0..5).filter(|n| d[5] & (1 << n) != 0).collect();
        let active = self
            .physical
            .iter()
            .position(|n| *n == d[4])
            .ok_or(Error::invalid("HyperX active DPI"))? as u8;
        let mut state = DeviceState {
            dpi: Some(DpiState {
                stages: self
                    .physical
                    .iter()
                    .map(|n| {
                        DpiStage::uniform(
                            u16::from_le_bytes([c[10 + *n as usize * 2], c[11 + *n as usize * 2]])
                                * 100,
                        )
                    })
                    .collect(),
                active,
            }),
            report_rate: Some(RATES[c[53] as usize]),
            ..Default::default()
        };
        self.owned.retain(|id, _| {
            c[35 + (*id as usize - 1) * 3..38 + (*id as usize - 1) * 3]
                == read_row(4, (*id - 1) as u8)
        });
        for id in 1..=6 {
            let row = &c[35 + (id - 1) * 3..38 + (id - 1) * 3];
            let a = decode_action(row).or_else(|| {
                self.owned
                    .get(&(id as u16))
                    .map(|m| ButtonAction::Macro(m.id.clone()))
            });
            self.caps.buttons[id - 1].remappable = id != 1 && a.is_some();
            if let Some(a) = a {
                state.buttons.insert(id as u16, a);
            }
        }
        if let Some(light) = self.light {
            state.lighting.insert(0, light);
        }
        Ok(state)
    }
    fn save(&mut self) -> Result<()> {
        let mut p = [0; 64];
        p[0] = 0xde;
        p[1] = 0xff;
        self.send(&p)
    }
    fn send_action(&mut self, id: u16, row: [u8; 3]) -> Result<()> {
        self.send(&packet(0xd4, (id - 1) as u8, row[0], &[row[1], 0]))
    }
    fn dpi_packets(d: &DpiState) -> Vec<[u8; 64]> {
        let mut packets = Vec::new();
        for (n, s) in d.stages.iter().enumerate() {
            packets.push(packet(0xd3, 2, n as u8, &(s.x / 100).to_le_bytes()));
        }
        packets.push(packet(0xd3, 1, 0, &[((1u16 << d.stages.len()) - 1) as u8]));
        packets.push(packet(0xd3, 0, 0, &[d.active]));
        packets
    }
    fn restore_dpi(&mut self, c: &[u8; 64], d: &[u8; 64]) -> Result<()> {
        self.identify()?;
        for n in 0..5 {
            self.send(&packet(0xd3, 2, n as u8, &c[10 + n * 2..12 + n * 2]))?;
        }
        self.send(&packet(0xd3, 1, 0, &[d[5]]))?;
        self.send(&packet(0xd3, 0, 0, &[d[4]]))?;
        self.save()
    }
    fn apply_dpi(&mut self, next: &DpiState) -> Result<()> {
        let (c, d) = self.snapshot()?;
        let result = (|| {
            for p in Self::dpi_packets(next) {
                self.send(&p)?;
            }
            self.save()?;
            let actual = self.refresh()?.dpi;
            if actual.as_ref() != Some(next) {
                return Err(Error::invalid("HyperX DPI write did not read back"));
            }
            Ok(())
        })();
        if let Err(e) = result {
            let restored = self.restore_dpi(&c, &d).is_ok()
                && self
                    .snapshot()
                    .is_ok_and(|(a, b)| a[10..20] == c[10..20] && b[4..6] == d[4..6]);
            return Err(Error::invalid(format!(
                "{e}; original DPI restore {}",
                if restored {
                    "verified"
                } else {
                    "not verified; reconnect and re-read"
                }
            )));
        }
        Ok(())
    }
    fn apply_button(&mut self, id: u16, action: &ButtonAction) -> Result<()> {
        let (c, _) = self.snapshot()?;
        let before: [u8; 3] = c[35 + (id as usize - 1) * 3..38 + (id as usize - 1) * 3]
            .try_into()
            .unwrap();
        if before != read_row(4, (id - 1) as u8) {
            self.owned.remove(&id);
        }
        if decode_action(&before).is_none() && !self.owned.contains_key(&id) {
            return Err(Error::invalid(
                "HyperX unknown original binding or factory macro is retained",
            ));
        }
        let native = match action {
            ButtonAction::Macro(key) => Some(
                self.definitions
                    .get(key)
                    .cloned()
                    .ok_or(Error::invalid("HyperX native macro definition missing"))?,
            ),
            ButtonAction::Key { usage, mods } if mods.0 != 0 => Some(key_macro(*usage, *mods)),
            _ => None,
        };
        if native.is_some()
            && (1..=6).any(|other| {
                other != id as usize
                    && c[35 + (other - 1) * 3] == 4
                    && c[36 + (other - 1) * 3] == (id - 1) as u8
            })
        {
            return Err(Error::invalid(
                "HyperX macro slot is referenced by another button; original macro retained",
            ));
        }
        let row = if native.is_some() {
            read_row(4, (id - 1) as u8)
        } else {
            raw_action(id, action)?
        };
        // The firmware cannot export macro contents. Do not overwrite a populated
        // slot, including one created by this app, unless the same definition repeats.
        if before[0] == 4 {
            if native.as_ref() == self.owned.get(&id) {
                return Ok(());
            }
            return Err(Error::invalid("HyperX macro slot already contains unreadable data; choose a non-macro button slot"));
        }
        let operation = (|| {
            if let Some(m) = native.as_ref() {
                let events = macro_events(m)?;
                self.send_action(id, row)?;
                for (index, chunk) in events.chunks(6).enumerate() {
                    let mut p = [0; 64];
                    p[0] = 0xd6;
                    p[1] = (id - 1) as u8;
                    p[2] = (3 * index / 2) as u8;
                    p[3] = (if index % 2 == 1 { 0x80 } else { 0 }) | chunk.len() as u8;
                    for (n, e) in chunk.iter().enumerate() {
                        p[4 + n * 10..14 + n * 10].copy_from_slice(e);
                    }
                    self.send(&p)?;
                }
                let mut p = packet(0xd5, (id - 1) as u8, 0, &[events.len() as u8]);
                p[3] = 5;
                self.send(&p)?;
            } else {
                self.send_action(id, row)?;
            }
            self.save()?;
            let (actual, _) = self.snapshot()?;
            if actual[35 + (id as usize - 1) * 3..38 + (id as usize - 1) * 3] != row {
                return Err(Error::invalid("HyperX button assignment did not read back"));
            }
            Ok(())
        })();
        if let Err(e) = operation {
            let restored = self
                .identify()
                .and_then(|_| self.send_action(id, before))
                .and_then(|_| self.save())
                .is_ok()
                && self.snapshot().is_ok_and(|(a, _)| {
                    a[35 + (id as usize - 1) * 3..38 + (id as usize - 1) * 3] == before
                });
            return Err(Error::invalid(format!(
                "{e}; original binding restore {}",
                if restored {
                    "verified"
                } else {
                    "not verified; reconnect and re-read"
                }
            )));
        }
        if let Some(m) = native {
            self.owned.insert(id, m);
        } else {
            self.owned.remove(&id);
        }
        Ok(())
    }
}
impl Device for Haste {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }
    fn capabilities(&self) -> &Capabilities {
        &self.caps
    }
    fn read_state(&mut self) -> Result<DeviceState> {
        self.refresh()
    }
    fn set_macro_definitions(&mut self, ms: &[Macro]) -> Result<()> {
        let mut out = BTreeMap::new();
        for m in ms {
            validate_macro(m)?;
            if out.insert(m.id.clone(), m.clone()).is_some() {
                return Err(Error::invalid("HyperX duplicate macro id"));
            }
        }
        self.definitions = out;
        Ok(())
    }
    fn macro_definitions(&self) -> Vec<Macro> {
        let mut out = BTreeMap::new();
        for m in self.owned.values() {
            out.insert(m.id.clone(), m.clone());
        }
        out.into_values().collect()
    }
    fn apply(&mut self, s: &Setting) -> Result<()> {
        validate_setting(s)?;
        if let Setting::Button {
            action: ButtonAction::Macro(key),
            ..
        } = s
        {
            validate_macro(
                self.definitions
                    .get(key)
                    .ok_or(Error::invalid("HyperX macro definition missing"))?,
            )?;
        }
        self.identify()?;
        match s {
            Setting::Dpi(d) => self.apply_dpi(d),
            Setting::ReportRate(hz) => {
                let (c, _) = self.snapshot()?;
                let next = RATES.iter().position(|r| r == hz).unwrap() as u8;
                let op = (|| {
                    let mut p = packet(0xd0, 0, 0, &[next]);
                    p[3] = 1;
                    self.send(&p)?;
                    self.save()?;
                    if self.snapshot()?.0[53] != next {
                        return Err(Error::invalid("HyperX polling rate did not read back"));
                    }
                    Ok(())
                })();
                if let Err(e) = op {
                    let mut restore = packet(0xd0, 0, 0, &[c[53]]);
                    restore[3] = 1;
                    let restored = self
                        .identify()
                        .and_then(|_| self.send(&restore))
                        .and_then(|_| self.save())
                        .is_ok()
                        && self.snapshot().is_ok_and(|(a, _)| a[53] == c[53]);
                    return Err(Error::invalid(format!(
                        "{e}; original rate restore {}",
                        if restored {
                            "verified"
                        } else {
                            "not verified; reconnect and re-read"
                        }
                    )));
                }
                Ok(())
            }
            Setting::Button { id, action } => self.apply_button(*id, action),
            Setting::Lighting { state, .. } => {
                self.snapshot()?;
                let color = match state.effect {
                    LightingEffect::Static { color } => [color.r, color.g, color.b]
                        .map(|v| (u16::from(v) * u16::from(state.brightness) / 100) as u8),
                    _ => [0; 3],
                };
                self.light = None;
                for n in 0..6 {
                    let mut p = [0; 64];
                    p[..4].copy_from_slice(&[0xda, 1, n, 60]);
                    if n == 0 {
                        p[4..7].copy_from_slice(&color);
                    }
                    self.send(&p)?;
                }
                self.send(&packet(0xd9, 0, 0, &[0x55, 1, 0x23]))?;
                self.save()?;
                self.light = Some(*state);
                Ok(())
            }
            _ => unreachable!(),
        }
    }
}
#[derive(Default)]
pub struct HasteDriver;
impl Driver for HasteDriver {
    fn name(&self) -> &'static str {
        "HyperX Haste exact HP USB"
    }
    fn vendor(&self) -> Vendor {
        Vendor::Other
    }
    fn probe(&self, i: &HidInterfaceInfo) -> bool {
        supported(i.vendor_id, i.product_id)
            && i.interface_number == 2
            && i.usage_page == 0xff00
            && i.bus == gearwise_core::transport::BusKind::Usb
    }
    fn open(&self, i: &HidInterfaceInfo, b: &dyn HidBackend) -> Result<Vec<Box<dyn Device>>> {
        if !self.probe(i) {
            return Err(Error::Unsupported("HyperX control collection"));
        }
        Ok(vec![Box::new(Haste::open(b.open(&i.path)?, i)?)])
    }
}
type Log = Arc<Mutex<Vec<gearwise_core::transport::MockRecord>>>;
fn memory(
    product: u16,
    alter: Option<fn(&mut [u8; 64], &mut [u8; 64])>,
    fail: Option<u8>,
) -> Result<(Haste, Log)> {
    use gearwise_core::transport::{MockOp, MockReply, MockTransport};
    let mut c = [0; 64];
    c[0] = 0x50;
    c[1] = 3;
    for (n, d) in [4u16, 8, 16, 32, 64].into_iter().enumerate() {
        c[10 + n * 2..12 + n * 2].copy_from_slice(&d.to_le_bytes());
    }
    for id in 1..=6 {
        c[35 + (id - 1) * 3..38 + (id - 1) * 3]
            .copy_from_slice(&raw_action(id as u16, &ButtonAction::Default)?);
    }
    c[53] = 3;
    let mut dpi = [0; 64];
    dpi[0] = 0x53;
    dpi[4] = 1;
    dpi[5] = 15;
    if let Some(alter) = alter {
        alter(&mut c, &mut dpi);
    }
    let mut refused = false;
    let io = MockTransport::new(move |op| {
        let MockOp::Write(p) = op else {
            return MockReply::none();
        };
        assert_eq!(p.len(), 65);
        assert_eq!(p[0], 0);
        let p = &p[1..];
        if p[0] == 0x50 {
            if p[1] == 0 {
                let mut info = [0; 64];
                info[0] = 0x50;
                info[3] = 60;
                info[4..6].copy_from_slice(&product.to_le_bytes());
                info[6..8].copy_from_slice(&0x03f0u16.to_le_bytes());
                return MockReply::input(info.to_vec());
            }
            return MockReply::input(c.to_vec());
        }
        if p[0] == 0x53 {
            return MockReply::input(dpi.to_vec());
        }
        if fail == Some(p[0]) && !refused {
            refused = true;
            return MockReply::none();
        }
        match p[0] {
            0xd0 => c[53] = p[4],
            0xd3 => match p[1] {
                0 => dpi[4] = p[4],
                1 => dpi[5] = p[4],
                2 => {
                    let n = p[2] as usize;
                    assert!(n < 5);
                    c[10 + n * 2..12 + n * 2].copy_from_slice(&p[4..6]);
                }
                _ => panic!("bad DPI command"),
            },
            0xd4 => {
                let n = p[1] as usize;
                assert!(n < 6);
                c[35 + n * 3..38 + n * 3].copy_from_slice(&read_row(p[2], p[4]));
            }
            0xd5 | 0xd6 | 0xda | 0xd9 | 0xde => {}
            _ => panic!("unrecognized HyperX command"),
        };
        MockReply::none()
    });
    let log = io.log();
    let i = HidInterfaceInfo {
        vendor_id: 0x03f0,
        product_id: product,
        path: format!("simulation:hyperx:{product:04x}"),
        usage_page: 0xff00,
        interface_number: 2,
        ..Default::default()
    };
    let mut d = Haste::open(Box::new(io), &i)?;
    d.desc.simulated = true;
    Ok((d, log))
}
pub fn simulated(product: u16) -> Result<(Box<dyn Device>, Log)> {
    let (d, l) = memory(product, None, None)?;
    Ok((Box::new(d), l))
}
#[cfg(test)]
mod tests {
    use super::*;
    use gearwise_core::transport::MockRecord;
    fn mutations(l: &Log) -> Vec<Vec<u8>> {
        l.lock()
            .unwrap()
            .iter()
            .filter_map(|r| match r {
                MockRecord::Write(p) if p.len() == 65 && p[1] >= 0xd0 => Some(p[1..].to_vec()),
                _ => None,
            })
            .collect()
    }
    #[test]
    fn both_models_dpi_rate_buttons_roundtrip() {
        for p in [0x048e, 0x028e] {
            let (mut d, l) = memory(p, None, None).unwrap();
            l.lock().unwrap().clear();
            assert!(d.read_state().unwrap().lighting.is_empty());
            assert!(mutations(&l).is_empty());
            let next = DpiState {
                stages: vec![DpiStage::uniform(200), DpiStage::uniform(16000)],
                active: 1,
            };
            d.apply(&Setting::Dpi(next.clone())).unwrap();
            d.apply(&Setting::ReportRate(500)).unwrap();
            d.apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Key {
                    usage: 5,
                    mods: Modifiers(0),
                },
            })
            .unwrap();
            let s = d.read_state().unwrap();
            assert_eq!(s.dpi, Some(next));
            assert_eq!(s.report_rate, Some(500));
            assert_eq!(
                s.buttons[&4],
                ButtonAction::Key {
                    usage: 5,
                    mods: Modifiers(0)
                }
            );
            assert!(mutations(&l).iter().all(|p| p.len() == 64));
        }
    }
    #[test]
    fn exact_usb_identity_and_vendor_collection_only() {
        let mut i = HidInterfaceInfo {
            vendor_id: 0x03f0,
            product_id: 0x048e,
            usage_page: 0xff00,
            interface_number: 2,
            ..Default::default()
        };
        assert!(HasteDriver.probe(&i));
        for p in [0x058e, 0x16a4, 0xffff] {
            i.product_id = p;
            assert!(!HasteDriver.probe(&i));
        }
        i.product_id = 0x048e;
        i.usage_page = 1;
        assert!(!HasteDriver.probe(&i));
        i.usage_page = 0xff00;
        i.bus = gearwise_core::transport::BusKind::Bluetooth;
        assert!(!HasteDriver.probe(&i));
    }
    #[test]
    fn invalid_input_is_rejected_before_any_io() {
        let (mut d, l) = memory(0x048e, None, None).unwrap();
        for s in [
            Setting::Dpi(DpiState {
                stages: vec![DpiStage::uniform(100)],
                active: 0,
            }),
            Setting::Dpi(DpiState {
                stages: vec![DpiStage { x: 800, y: 1600 }],
                active: 0,
            }),
            Setting::ReportRate(8000),
            Setting::Button {
                id: 1,
                action: ButtonAction::Disabled,
            },
            Setting::Lighting {
                zone: 0,
                state: ZoneState {
                    effect: LightingEffect::ColorCycle { period_ms: 1000 },
                    brightness: 100,
                },
            },
        ] {
            l.lock().unwrap().clear();
            assert!(d.apply(&s).is_err());
            assert!(l.lock().unwrap().is_empty());
        }
    }
    #[test]
    fn sparse_enabled_stages_map_active_index_correctly() {
        let (mut d, _) = memory(
            0x048e,
            Some(|_, p| {
                p[5] = 0b10101;
                p[4] = 4;
            }),
            None,
        )
        .unwrap();
        let s = d.read_state().unwrap().dpi.unwrap();
        assert_eq!(
            s.stages.iter().map(|s| s.x).collect::<Vec<_>>(),
            vec![400, 1600, 6400]
        );
        assert_eq!(s.active, 2);
    }
    #[test]
    fn malformed_settings_fail_closed() {
        for alter in [
            ((|c: &mut [u8; 64], _: &mut [u8; 64]| c[53] = 9) as fn(&mut [u8; 64], &mut [u8; 64])),
            |_, p| p[5] = 0,
            |_, p| p[4] = 4,
            |c, _| c[10] = 255,
            |c, _| c[18] = 0,
        ] {
            assert!(memory(0x048e, Some(alter), None).is_err());
        }
    }
    #[test]
    fn unknown_factory_macro_is_not_overwritten() {
        let (mut d, l) = memory(
            0x048e,
            Some(|c, _| c[44..47].copy_from_slice(&[4, 3, 3])),
            None,
        )
        .unwrap();
        assert!(!d.caps.buttons[3].remappable);
        l.lock().unwrap().clear();
        assert!(d
            .apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Disabled
            })
            .is_err());
        assert!(mutations(&l).is_empty());
    }
    #[test]
    fn readback_failure_restores_original_dpi() {
        let (mut d, _) = memory(0x048e, None, Some(0xd3)).unwrap();
        let original = d.read_state().unwrap().dpi;
        let e = d
            .apply(&Setting::Dpi(DpiState {
                stages: vec![DpiStage::uniform(2400)],
                active: 0,
            }))
            .unwrap_err();
        assert!(e.to_string().contains("restore verified"));
        assert_eq!(d.read_state().unwrap().dpi, original);
    }
    #[test]
    fn readback_failure_restores_original_rate() {
        let (mut d, _) = memory(0x028e, None, Some(0xd0)).unwrap();
        assert!(d
            .apply(&Setting::ReportRate(500))
            .unwrap_err()
            .to_string()
            .contains("restore verified"));
        assert_eq!(d.read_state().unwrap().report_rate, Some(1000));
    }
    #[test]
    fn static_lighting_has_six_pages_mode_and_commit_no_fake_initial_color() {
        let (mut d, l) = memory(0x048e, None, None).unwrap();
        assert!(d.read_state().unwrap().lighting.is_empty());
        l.lock().unwrap().clear();
        let z = ZoneState {
            effect: LightingEffect::Static {
                color: Rgb {
                    r: 200,
                    g: 100,
                    b: 50,
                },
            },
            brightness: 50,
        };
        d.apply(&Setting::Lighting { zone: 0, state: z }).unwrap();
        let p = mutations(&l);
        assert_eq!(p.len(), 8);
        assert_eq!(&p[0][..7], &[0xda, 1, 0, 60, 100, 50, 25]);
        for n in 1..6 {
            assert_eq!(p[n][2], n as u8);
            assert!(p[n][4..].iter().all(|v| *v == 0));
        }
        assert_eq!(&p[6][..7], &[0xd9, 0, 0, 3, 0x55, 1, 0x23]);
        assert_eq!(&p[7][..2], &[0xde, 0xff]);
        assert_eq!(d.read_state().unwrap().lighting[&0], z);
    }
    #[test]
    fn native_macro_packets_and_binding_readback_preserve_definition() {
        let (mut d, l) = memory(0x028e, None, None).unwrap();
        let m = key_macro(4, Modifiers(1));
        d.set_macro_definitions(&[m.clone()]).unwrap();
        l.lock().unwrap().clear();
        d.apply(&Setting::Button {
            id: 4,
            action: ButtonAction::Macro(m.id.clone()),
        })
        .unwrap();
        let packets = mutations(&l);
        assert_eq!(&packets[0][..6], &[0xd4, 3, 4, 2, 3, 0]);
        let event = packets.iter().find(|p| p[0] == 0xd6).unwrap();
        assert_eq!(&event[..4], &[0xd6, 3, 0, 4]);
        assert_eq!(&event[4..6], &[0x1a, 1]);
        assert_eq!(&event[14..18], &[0x1a, 1, 4, 0]);
        assert_eq!(&event[24..28], &[0x1a, 1, 0, 0]);
        assert_eq!(&event[34..38], &[0x1a, 0, 0, 0]);
        let a = packets.iter().find(|p| p[0] == 0xd5).unwrap();
        assert_eq!(&a[..5], &[0xd5, 3, 0, 5, 4]);
        assert_eq!(
            d.read_state().unwrap().buttons[&4],
            ButtonAction::Macro(m.id.clone())
        );
        assert_eq!(d.macro_definitions(), vec![m]);
    }
    #[test]
    fn populated_macro_slot_is_retained_even_in_same_session() {
        let (mut d, l) = memory(0x048e, None, None).unwrap();
        let m = key_macro(4, Modifiers(1));
        d.set_macro_definitions(&[m.clone()]).unwrap();
        d.apply(&Setting::Button {
            id: 4,
            action: ButtonAction::Macro(m.id.clone()),
        })
        .unwrap();
        l.lock().unwrap().clear();
        assert!(d
            .apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Disabled
            })
            .is_err());
        assert!(mutations(&l).is_empty());
        d.apply(&Setting::Button {
            id: 4,
            action: ButtonAction::Macro(m.id.clone()),
        })
        .unwrap();
        assert!(mutations(&l).is_empty());
    }
    #[test]
    fn macro_limits_reject_host_only_semantics_and_unbalanced_keys() {
        for m in [
            Macro {
                mode: MacroMode::Toggle,
                ..key_macro(4, Modifiers(0))
            },
            Macro {
                steps: vec![MacroStep::MouseDown(MouseButton::Left)],
                ..key_macro(4, Modifiers(0))
            },
            Macro {
                steps: vec![MacroStep::KeyDown(4)],
                ..key_macro(4, Modifiers(0))
            },
            Macro {
                steps: vec![MacroStep::KeyUp(4)],
                ..key_macro(4, Modifiers(0))
            },
            Macro {
                steps: (0..41)
                    .flat_map(|_| [MacroStep::KeyDown(4), MacroStep::KeyUp(4)])
                    .collect(),
                ..key_macro(4, Modifiers(0))
            },
        ] {
            assert!(validate_macro(&m).is_err());
        }
        let mut steps = (4..11).map(MacroStep::KeyDown).collect::<Vec<_>>();
        steps.extend((4..11).map(MacroStep::KeyUp));
        assert!(validate_macro(&Macro {
            steps,
            ..key_macro(4, Modifiers(0))
        })
        .is_err());
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;
    fn captures(name: &str) -> Vec<Vec<u8>> {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../test-data/hyperx-public-captures.json")).unwrap();
        data["captures"][name]["payloads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let s = s.as_str().unwrap();
                (0..s.len())
                    .step_by(2)
                    .map(|n| u8::from_str_radix(&s[n..n + 2], 16).unwrap())
                    .collect()
            })
            .collect()
    }
    #[test]
    fn production_rates_match_all_four_public_ngenuity_capture_payloads() {
        for (n, expected) in captures("polling_rates").iter().enumerate() {
            assert_eq!(packet(0xd0, 0, 0, &[n as u8]).to_vec(), *expected);
        }
    }
    #[test]
    fn button_output_and_readback_have_different_third_bytes() {
        assert_eq!(raw_action(2, &ButtonAction::Default).unwrap(), [1, 2, 2]);
        assert_eq!(raw_action(3, &ButtonAction::Default).unwrap(), [1, 3, 4]);
        assert_eq!(
            decode_action(&[2, 4, 4]),
            Some(ButtonAction::Key {
                usage: 4,
                mods: Modifiers(0)
            })
        );
        assert!(decode_action(&[2, 4, 0]).is_none());
        let (mut d, l) = memory(0x048e, None, None).unwrap();
        l.lock().unwrap().clear();
        d.apply(&Setting::Button {
            id: 2,
            action: ButtonAction::Default,
        })
        .unwrap();
        let expected = &captures("button_assignment")[1];
        assert!(l.lock().unwrap().iter().any(|r|matches!(r,gearwise_core::transport::MockRecord::Write(p)if p[0]==0&&p[1..]==expected[..])));
    }
    #[test]
    fn public_macro_capture_decodes_to_real_keyboard_chord_and_release() {
        let packets = captures("macro_6_key_events");
        assert_eq!(&packets[0][..6], &[0xd4, 3, 4, 2, 3, 0]);
        assert_eq!(&packets[1][..4], &[0xd6, 3, 0, 2]);
        let down = &packets[1][4..14];
        let up = &packets[1][14..24];
        assert_eq!(down[0], 0x1a);
        assert_eq!(&down[2..8], &[4, 22, 7, 9, 10, 11]);
        assert_eq!(u16::from_le_bytes([down[8], down[9]]), 1203);
        assert_eq!(up[0], 0x1a);
        assert!(up[1..8].iter().all(|v| *v == 0));
        assert_eq!(&packets[2][..9], &[0xd5, 3, 0, 5, 2, 0, 0, 0, 0]);
    }
    struct Short {
        inner: Box<dyn HidTransport>,
    }
    impl HidTransport for Short {
        fn write(&mut self, _: &[u8]) -> Result<usize> {
            Ok(64)
        }
        fn read_timeout(&mut self, b: &mut [u8], ms: i32) -> Result<usize> {
            self.inner.read_timeout(b, ms)
        }
        fn send_feature_report(&mut self, p: &[u8]) -> Result<()> {
            self.inner.send_feature_report(p)
        }
        fn get_feature_report(&mut self, p: &mut [u8]) -> Result<usize> {
            self.inner.get_feature_report(p)
        }
    }
    #[test]
    fn windows_unnumbered_short_write_is_not_accepted() {
        let (mut d, l) = memory(0x048e, None, None).unwrap();
        d.io = Box::new(Short { inner: d.io });
        l.lock().unwrap().clear();
        assert!(d.apply(&Setting::ReportRate(500)).is_err());
        assert!(l.lock().unwrap().is_empty());
    }
}

#[cfg(test)]
mod failure_tests {
    use super::*;
    use gearwise_core::transport::MockRecord;
    struct BadReply {
        inner: Box<dyn HidTransport>,
        kind: u8,
    }
    impl HidTransport for BadReply {
        fn write(&mut self, p: &[u8]) -> Result<usize> {
            self.inner.write(p)
        }
        fn read_timeout(&mut self, p: &mut [u8], ms: i32) -> Result<usize> {
            let n = self.inner.read_timeout(p, ms)?;
            if n == 64 {
                match self.kind {
                    1 if p[0] == 0x50 && p[1] == 0 => p[4] ^= 1,
                    2 => return Ok(12),
                    3 => p[0] = 0x42,
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
    fn changed_hardware_truncated_or_stale_replies_fail_before_mutations() {
        for kind in 1..=3 {
            let (mut d, l) = memory(0x048e, None, None).unwrap();
            d.io = Box::new(BadReply { inner: d.io, kind });
            l.lock().unwrap().clear();
            assert!(d.apply(&Setting::ReportRate(500)).is_err());
            assert!(l
                .lock()
                .unwrap()
                .iter()
                .all(|r| !matches!(r,MockRecord::Write(p)if p[1]>=0xd0)));
        }
    }
    #[test]
    fn a_macro_referenced_by_another_button_is_never_overwritten() {
        let (mut d, l) = memory(
            0x048e,
            Some(|c, _| c[47..50].copy_from_slice(&[4, 3, 3])),
            None,
        )
        .unwrap();
        let m = key_macro(4, Modifiers(0));
        d.set_macro_definitions(&[m.clone()]).unwrap();
        l.lock().unwrap().clear();
        assert!(d
            .apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Macro(m.id)
            })
            .unwrap_err()
            .to_string()
            .contains("another button"));
        assert!(l
            .lock()
            .unwrap()
            .iter()
            .all(|r| !matches!(r,MockRecord::Write(p)if p[1]>=0xd0)));
    }
}
