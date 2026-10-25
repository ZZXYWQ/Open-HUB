//! Five exact, experimental wired models. Protocol reference: libratbag
//! 8235b5bb6032dea15901ab58a8218956f4494d08 (MIT); see THIRD-PARTY notices.
//! Read selectors are volatile queries, never profile activation or reset.
use gearwise_core::transport::BusKind;
use gearwise_core::{
    model::*,
    profile::{Macro, MacroMode, MacroStep},
    *,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
#[path = "legacy/macros.rs"]
mod macros;
#[path = "legacy/protocol.rs"]
mod protocol;
#[path = "legacy/simulation.rs"]
mod simulation;
#[cfg(test)]
#[path = "legacy/tests.rs"]
mod tests;
#[derive(serde::Deserialize, Debug)]
pub struct Model {
    pub vendor: u16,
    pub product: u16,
    pub brand: String,
    pub name: String,
    pub min: u16,
    pub max: u16,
    pub step: u16,
    pub stages: u8,
    pub xy: bool,
    pub buttons: u16,
    pub rates: Vec<u16>,
    pub macros: bool,
    pub lights: Vec<EffectKind>,
    pub fixed_stages: bool,
    pub active_readonly: bool,
}
pub fn models() -> &'static [Model] {
    static MODELS: OnceLock<Vec<Model>> = OnceLock::new();
    MODELS.get_or_init(|| {
        serde_json::from_str(include_str!("../../src/lib/legacy-models.json"))
            .expect("reviewed legacy model catalog")
    })
}
pub fn model(v: u16, p: u16) -> Option<&'static Model> {
    models().iter().find(|m| m.vendor == v && m.product == p)
}
pub fn supported(v: u16, p: u16) -> bool {
    model(v, p).is_some()
}
pub fn native_macros(v: u16, p: u16) -> bool {
    model(v, p).is_some_and(|m| m.macros)
}
fn invalid(msg: &str) -> Error {
    Error::invalid(format!("Experimental wired adapter: {msg}"))
}
pub fn validate_setting(m: &Model, s: &Setting) -> Result<()> {
    match s {
        Setting::Dpi(d)
            if !d.stages.is_empty()
                && d.stages.len() <= m.stages as usize
                && (!m.fixed_stages || d.stages.len() == m.stages as usize)
                && usize::from(d.active) < d.stages.len()
                && d.stages.iter().all(|d| {
                    d.x >= m.min
                        && d.x <= m.max
                        && d.y >= m.min
                        && d.y <= m.max
                        && d.x % m.step == 0
                        && d.y % m.step == 0
                        && (m.xy || d.x == d.y)
                }) =>
        {
            Ok(())
        }
        Setting::ReportRate(r) if m.rates.contains(r) => Ok(()),
        Setting::Button { id, action } if *id >= 2 && *id <= m.buttons => {
            if matches!(action, ButtonAction::Default) {
                return Ok(());
            }
            if let ButtonAction::Macro(_) = action {
                if m.macros {
                    return Ok(());
                }
            }
            if let ButtonAction::Key { usage, mods } = action {
                if m.macros {
                    return macros::validate(
                        m,
                        &macros::shortcut(*usage, *mods, m.brand == "etekcity"),
                    );
                }
            }
            encode_action(m, *id, action).map(|_| ())
        }
        Setting::Lighting { zone: 0, state }
            if state.brightness <= 100 && m.lights.contains(&state.effect.kind()) =>
        {
            match state.effect {
                LightingEffect::Breathing { period_ms, .. }
                    if m.brand == "marsgaming"
                        && (!(2000..=18000).contains(&period_ms) || period_ms % 2000 != 0) =>
                {
                    Err(invalid(
                        "MM4 breathing period must be 2000..18000 ms in steps of 2000",
                    ))
                }
                _ => Ok(()),
            }
        }
        _ => Err(invalid(
            "unsupported setting, DPI range, stage count or protected left click",
        )),
    }
}
pub fn validate_macro(v: u16, p: u16, m: &Macro) -> Result<()> {
    macros::validate(model(v, p).ok_or(invalid("model"))?, m)
}
fn mouse(code: u8) -> Option<ButtonAction> {
    Some(ButtonAction::Mouse(match code {
        1 => MouseButton::Left,
        2 => MouseButton::Right,
        3 => MouseButton::Middle,
        4 => MouseButton::Back,
        5 => MouseButton::Forward,
        _ => return None,
    }))
}
fn default_action(id: u16) -> ButtonAction {
    mouse(id as u8).unwrap_or(ButtonAction::Dpi(DpiFunction::Cycle))
}
fn encode_action(m: &Model, id: u16, a: &ButtonAction) -> Result<Vec<u8>> {
    // No invented factory reset: Default means keep the current physical binding.
    if matches!(a, ButtonAction::Default) {
        return Err(invalid("Default is resolved from a fresh hardware read"));
    }
    let n = match a {
        ButtonAction::Mouse(b) => match b {
            MouseButton::Left => 1,
            MouseButton::Right => 2,
            MouseButton::Middle => 3,
            MouseButton::Back => 4,
            MouseButton::Forward => 5,
            _ => return Err(invalid("mouse action")),
        },
        _ => 0,
    };
    let b=match m.brand.as_str(){
 "asus"=>match a{ButtonAction::Disabled=>vec![0xff,1],ButtonAction::Mouse(_)=>vec![[0xf0,0xf1,0xf2,0xe4,0xe5][n-1],1],ButtonAction::Dpi(DpiFunction::Cycle)=>vec![0xe6,1],ButtonAction::Key{usage,mods}if mods.0==0&&asus_key(*usage)=>vec![if *usage==0x2e{0x36}else if *usage==0x57{0x2e}else{*usage},0],_=>return Err(invalid("ASUS supports a single mapped key, basic mouse action or DPI cycle; macro protocol unavailable"))},
 "gskill"=>match a{ButtonAction::Disabled=>vec![0xff,0,0,0,0],ButtonAction::Mouse(_)=>vec![1,1<<(n-1),0,0,0],ButtonAction::Dpi(f)=>vec![match f{DpiFunction::Cycle=>0x0b,DpiFunction::Up=>9,DpiFunction::Down=>10,_=>return Err(invalid("DPI action"))},0,0,0,0],ButtonAction::Key{usage,mods}if (4..=0xa4).contains(usage)=>vec![2,mods.0,*usage,0,0],_=>return Err(invalid("G.Skill binding"))},
 "marsgaming"=>match a{ButtonAction::Disabled=>vec![14,0,0,0],ButtonAction::Mouse(_)=>vec![n as u8,0,0,0],ButtonAction::Dpi(f)=>vec![match f{DpiFunction::Cycle=>8,DpiFunction::Up=>10,DpiFunction::Down=>9,_=>return Err(invalid("DPI action"))},0,0,0],ButtonAction::Key{usage,mods}if (4..=0xa4).contains(usage)=>vec![15,mods.0,*usage,0],_=>return Err(invalid("MM4 binding; arbitrary onboard macro protocol unavailable"))},
 _=>{let et=m.brand=="etekcity";let c=match a{ButtonAction::Disabled=>6,ButtonAction::Mouse(_)=>[1,2,3,7,8][n-1],ButtonAction::Dpi(f)=>match f{DpiFunction::Cycle=>if et{13}else{20},DpiFunction::Up=>if et{14}else{21},DpiFunction::Down=>if et{15}else{22},_=>return Err(invalid("DPI action"))},_=>return Err(invalid("binding requires a native macro"))};vec![c]}
 };
    let _ = id;
    Ok(b)
}
fn asus_key(k: u8) -> bool {
    matches!(k,4..=0x2e|0x35|0x38|0x3a..=0x45|0x4a..=0x4c|0x4e..=0x52|0x57|0x59..=0x61)
}
fn decode_action(m: &Model, b: &[u8]) -> Option<ButtonAction> {
    match m.brand.as_str() {
        "asus" => {
            if b[0] == 0xff {
                return Some(ButtonAction::Disabled);
            }
            if b[1] == 0 && b[0] != 0x57 {
                let usage = if b[0] == 0x36 {
                    0x2e
                } else if b[0] == 0x2e {
                    0x57
                } else {
                    b[0]
                };
                if asus_key(usage) {
                    return Some(ButtonAction::Key {
                        usage,
                        mods: Modifiers(0),
                    });
                }
            }
            if b[1] != 1 {
                return None;
            }
            match b[0] {
                0xf0 => mouse(1),
                0xf1 => mouse(2),
                0xf2 => mouse(3),
                0xe4 => mouse(4),
                0xe5 => mouse(5),
                0xe6 => Some(ButtonAction::Dpi(DpiFunction::Cycle)),
                _ => None,
            }
        }
        "gskill" => match b[0] {
            1 if b[2..].iter().all(|x| *x == 0) => match b[1] {
                1 => mouse(1),
                2 => mouse(2),
                4 => mouse(3),
                8 => mouse(4),
                16 => mouse(5),
                _ => None,
            },
            2 if (4..=0xa4).contains(&b[2]) && b[3..].iter().all(|v| *v == 0) => {
                Some(ButtonAction::Key {
                    usage: b[2],
                    mods: Modifiers(b[1]),
                })
            }
            0xff => Some(ButtonAction::Disabled),
            9 => Some(ButtonAction::Dpi(DpiFunction::Up)),
            10 => Some(ButtonAction::Dpi(DpiFunction::Down)),
            11 => Some(ButtonAction::Dpi(DpiFunction::Cycle)),
            _ => None,
        },
        "marsgaming" => match b[0] {
            1..=5 if b[1..].iter().all(|v| *v == 0) => mouse(b[0]),
            14 if b[1..].iter().all(|v| *v == 0) => Some(ButtonAction::Disabled),
            15 | 16 if (4..=0xa4).contains(&b[2]) && b[3] == 0 => Some(ButtonAction::Key {
                usage: b[2],
                mods: Modifiers(b[1]),
            }),
            8 => Some(ButtonAction::Dpi(DpiFunction::Cycle)),
            9 => Some(ButtonAction::Dpi(DpiFunction::Down)),
            10 => Some(ButtonAction::Dpi(DpiFunction::Up)),
            _ => None,
        },
        _ => match b[0] {
            1..=3 => mouse(b[0]),
            7 => mouse(4),
            8 => mouse(5),
            6 => Some(ButtonAction::Disabled),
            c if c == if m.brand == "etekcity" { 13 } else { 20 } => {
                Some(ButtonAction::Dpi(DpiFunction::Cycle))
            }
            c if c == if m.brand == "etekcity" { 14 } else { 21 } => {
                Some(ButtonAction::Dpi(DpiFunction::Up))
            }
            c if c == if m.brand == "etekcity" { 15 } else { 22 } => {
                Some(ButtonAction::Dpi(DpiFunction::Down))
            }
            _ => None,
        },
    }
}
#[derive(Clone)]
struct Snapshot {
    profile: u8,
    config: Vec<u8>,
    keys: Vec<u8>,
    light: Vec<u8>,
    dpi: DpiState,
    rate: u16,
}
struct Wired {
    io: Box<dyn HidTransport>,
    desc: DeviceDescriptor,
    caps: Capabilities,
    m: &'static Model,
    pinned: Option<u8>,
    definitions: BTreeMap<String, Macro>,
    owned: BTreeMap<u16, Macro>,
    owned_actions: BTreeMap<u16, ButtonAction>,
    pause: bool,
}
impl Wired {
    fn sleep(&self, ms: u64) {
        if self.pause {
            std::thread::sleep(Duration::from_millis(ms));
        }
    }
    fn get(&mut self, id: u8, len: usize) -> Result<Vec<u8>> {
        let mut p = vec![0; len];
        p[0] = id;
        if self.io.get_feature_report(&mut p)? != len {
            return Err(invalid("short feature response"));
        }
        Ok(p)
    }
    fn set(&mut self, p: &[u8]) -> Result<()> {
        self.io.send_feature_report(p)
    }
    fn snapshot(&mut self) -> Result<Snapshot> {
        protocol::snapshot(self)
    }
    fn open(io: Box<dyn HidTransport>, i: &HidInterfaceInfo, pause: bool) -> Result<Self> {
        let m = model(i.vendor_id, i.product_id).ok_or(invalid("exact model"))?;
        let mut d = Self {
            io,
            m,
            pause,
            pinned: None,
            definitions: BTreeMap::new(),
            owned: BTreeMap::new(),
            owned_actions: BTreeMap::new(),
            desc: DeviceDescriptor {
                id: format!(
                    "{}:{:04x}:{}",
                    m.brand,
                    m.product,
                    i.serial
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&i.path)
                ),
                model_key: format!("{}:{:04x}", m.brand, m.product),
                vendor: Vendor::Other,
                kind: DeviceKind::Mouse,
                name: m.name.clone(),
                vendor_id: m.vendor,
                product_id: m.product,
                connection: Connection::Wired,
                firmware: None,
                serial: i.serial.clone(),
                simulated: !pause,
            },
            caps: Capabilities {
                dpi: Some(DpiCaps {
                    min: m.min,
                    max: m.max,
                    step: m.step,
                    max_stages: m.stages,
                    separate_xy: m.xy,
                }),
                report_rates: m.rates.clone(),
                buttons: (1..=m.buttons)
                    .map(|id| ButtonCaps {
                        id,
                        name: if m.brand == "asus" {
                            ["左键", "右键", "中键", "DPI 键"][(id - 1) as usize].into()
                        } else {
                            format!(
                                "{}{}",
                                if id > 8 && m.brand == "roccat" {
                                    "扩展/Easy-Shift 槽 "
                                } else {
                                    "按键 "
                                },
                                id
                            )
                        },
                        remappable: id != 1,
                        default_action: default_action(id),
                        hotspot: None,
                    })
                    .collect(),
                lighting: if m.lights.is_empty() {
                    vec![]
                } else {
                    vec![LightingZoneCaps {
                        id: 0,
                        name: if m.brand == "asus" {
                            "ROG 标识"
                        } else {
                            "机身 RGB"
                        }
                        .into(),
                        effects: m.lights.clone(),
                        brightness: true,
                        keys: vec![],
                    }]
                },
                ..Default::default()
            },
        };
        d.refresh()?;
        Ok(d)
    }
    fn refresh(&mut self) -> Result<DeviceState> {
        let snap = self.snapshot()?;
        if self.pinned.is_some_and(|p| p != snap.profile) {
            return Err(invalid(
                "active hardware profile changed; reconnect and read again",
            ));
        }
        self.pinned = Some(snap.profile);
        let mut state = DeviceState {
            dpi: Some(snap.dpi.clone()),
            report_rate: Some(snap.rate),
            onboard_profile: Some(snap.profile),
            ..Default::default()
        };
        for id in 1..=self.m.buttons {
            let raw = protocol::row(self.m, &snap.keys, id)?;
            let action = if macros::is_binding(self.m, raw) {
                if macros::owned_valid(self, id)? {
                    self.owned_actions.get(&id).cloned()
                } else {
                    self.owned.remove(&id);
                    self.owned_actions.remove(&id);
                    None
                }
            } else {
                decode_action(self.m, raw)
            };
            if let Some(a) = action.clone() {
                state.buttons.insert(id, a);
            }
            self.caps.buttons[(id - 1) as usize].remappable = id != 1 && action.is_some();
        }
        if !snap.light.is_empty() {
            if let Some(s) = protocol::decode_light(self.m, &snap.light) {
                state.lighting.insert(0, s);
            }
        }
        Ok(state)
    }
    fn apply_inner(&mut self, s: &Setting) -> Result<()> {
        validate_setting(self.m, s)?;
        let before = self.snapshot()?;
        if Some(before.profile) != self.pinned {
            return Err(invalid("active profile changed before write"));
        }
        if let Setting::Button { id, action } = s {
            let old = protocol::row(self.m, &before.keys, *id)?;
            if let ButtonAction::Default = action {
                return Ok(());
            }
            if macros::is_binding(self.m, old) && !macros::owned_valid(self, *id)? {
                self.owned.remove(id);
                self.owned_actions.remove(id);
                return Err(invalid(
                    "original macro binding is preserved; contents not imported",
                ));
            }
            if decode_action(self.m, old).is_none() && !self.owned.contains_key(id) {
                return Err(invalid("unknown original binding is preserved"));
            }
        }
        if let Setting::Dpi(d) = s {
            if self.m.active_readonly && d.active != before.dpi.active {
                return Err(invalid(
                    "ASUS DPI preset can only be switched with the physical DPI button",
                ));
            }
        }
        if let Setting::Button { id, action } = s {
            let definition = match action {
                ButtonAction::Macro(key) => Some(
                    self.definitions
                        .get(key)
                        .cloned()
                        .ok_or(invalid("native macro definition missing"))?,
                ),
                ButtonAction::Key { usage, mods }
                    if self.m.brand == "roccat" || self.m.brand == "etekcity" =>
                {
                    Some(macros::shortcut(*usage, *mods, self.m.brand == "etekcity"))
                }
                _ => None,
            };
            if let Some(m) = definition {
                return macros::apply(self, *id, &m, &before, action.clone());
            }
        }
        let attempt = protocol::write_setting(self, &before, s).and_then(|_| self.verify(s));
        if let Err(e) = attempt {
            let rollback = protocol::restore_setting(self, &before, s);
            return Err(invalid(&format!(
                "write/readback failed: {e}; restoration: {rollback:?}; re-read before retrying"
            )));
        }
        if let Setting::Button { id, .. } = s {
            self.owned.remove(id);
            self.owned_actions.remove(id);
        }
        Ok(())
    }
    fn verify(&mut self, s: &Setting) -> Result<()> {
        let actual = self.refresh()?;
        let ok = match s {
            Setting::Dpi(d) => actual.dpi.as_ref() == Some(d),
            Setting::ReportRate(r) => actual.report_rate == Some(*r),
            Setting::Button { id, action } => actual.buttons.get(id) == Some(action),
            Setting::Lighting { zone, state } => {
                let expected = protocol::quantized_light(self.m, *state);
                actual.lighting.get(zone) == Some(&expected)
            }
            _ => false,
        };
        if ok {
            Ok(())
        } else {
            Err(invalid("hardware readback mismatch"))
        }
    }
}
impl Device for Wired {
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
        self.apply_inner(s)
    }
    fn set_macro_definitions(&mut self, definitions: &[Macro]) -> Result<()> {
        let mut next = BTreeMap::new();
        for m in definitions {
            macros::validate(self.m, m)?;
            if next.insert(m.id.clone(), m.clone()).is_some() {
                return Err(invalid("duplicate macro identity"));
            }
        }
        self.definitions = next;
        Ok(())
    }
    fn macro_definitions(&self) -> Vec<Macro> {
        self.owned
            .iter()
            .filter(|(id, _)| matches!(self.owned_actions.get(id), Some(ButtonAction::Macro(_))))
            .map(|(_, m)| m.clone())
            .collect()
    }
}
pub struct WiredDriver;
impl Driver for WiredDriver {
    fn name(&self) -> &'static str {
        "exact-wired-five"
    }
    fn vendor(&self) -> Vendor {
        Vendor::Other
    }
    fn probe(&self, i: &HidInterfaceInfo) -> bool {
        supported(i.vendor_id, i.product_id) && i.bus == BusKind::Usb && i.usage_page >= 0xff00
    }
    fn open(&self, i: &HidInterfaceInfo, b: &dyn HidBackend) -> Result<Vec<Box<dyn Device>>> {
        if !self.probe(i) {
            return Err(invalid("control collection"));
        }
        Ok(vec![Box::new(Wired::open(b.open(&i.path)?, i, true)?)])
    }
}
pub type MockLog = Arc<Mutex<Vec<transport::MockRecord>>>;
pub fn simulated(v: u16, p: u16) -> Result<(Box<dyn Device>, MockLog)> {
    simulation::device(v, p)
}
