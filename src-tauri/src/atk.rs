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
#[path="atk_tuning.rs"]
mod tuning;
#[path="atk_indicator.rs"]
mod indicator;
#[path="atk_battery.rs"]
mod battery;
pub fn validate_tuning(v:u16,p:u16,t:&gearwise_core::tuning::MouseTuning)->Result<()>{tuning::validate(model(v,p).ok_or(Error::Unsupported("COMPX tuning model"))?,t)}
#[path = "pulsar_shortcuts.rs"]
mod pulsar_shortcut;
use gearwise_core::profile::Macro;
use std::collections::HashMap;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub fn validate_dpi(d: &DpiState, product: u16) -> Result<()> {
    let (_, _, max) = identity(product).ok_or(Error::Unsupported("ATK model"))?;
    if d.stages.is_empty() || d.stages.len() > models().iter().find(|m|m.product==product).unwrap().max_stages as usize || usize::from(d.active) >= d.stages.len() {
        return Err(Error::invalid("ATK DPI stages"));
    }
    for stage in &d.stages {
        encode_dpi(stage.x, stage.y, max)?;
        let spec=models().iter().find(|m|m.product==product).unwrap();if !spec.separate_xy&&stage.x!=stage.y{return Err(Error::invalid("Pulsar requires equal X/Y DPI"));}
    }
    Ok(())
}
pub fn validate_key(usage: u8, mods: gearwise_core::model::Modifiers) -> Result<()> {
    native_macro::encode(&native_macro::keyboard(usage, mods)).map(|_| ())
}
pub fn horizontal_scroll_supported(v:u16,p:u16)->bool{model(v,p).is_some_and(|m|m.native_macros&&m.native_buttons&&matches!(m.brand.as_str(),"atk"|"vxe"|"vgn"))}
pub fn profile_cycle_supported(v:u16,p:u16)->bool{horizontal_scroll_supported(v,p)}
pub fn battery_supported(v:u16,p:u16)->bool{horizontal_scroll_supported(v,p)}
pub fn vertical_scroll_supported(v:u16,p:u16)->bool{horizontal_scroll_supported(v,p)}
pub fn is_pulsar(v:u16,p:u16)->bool{v==0x3554&&matches!(p,0xf507|0xf508)}
pub fn validate_pulsar_key(usage:u8,mods:gearwise_core::model::Modifiers)->Result<()>{pulsar_shortcut::encode(usage,mods).map(|_|())}
pub fn validate_media(usage:u16)->Result<()>{native_macro::validate_media(usage)}
pub fn validate_macro(m: &Macro) -> Result<()> {
    native_macro::encode(m).map(|_| ())
}
#[derive(serde::Deserialize)]
pub struct Model {
    pub vendor: u16, pub product: u16, pub mids: Vec<u8>, pub brand: String,
    pub name: String, pub max_dpi: u16, pub buttons: u16, pub indicator: bool,
    pub wireless: bool, pub rates: Vec<u16>,
    #[serde(default)] pub dpi_color_stages:u8,
    pub min_dpi:u16,pub max_stages:u8,pub separate_xy:bool,pub native_macros:bool,pub native_buttons:bool,
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
pub fn palette_stages(v:u16,p:u16)->u8 {model(v,p).filter(|m|m.indicator).map(|m|m.dpi_color_stages).unwrap_or(0)}
pub fn validate_indicator(v:u16,p:u16)->Result<()> {if palette_stages(v,p)>0{Ok(())}else{Err(Error::Unsupported("Exact COMPX indicator mode and levels"))}}
pub fn validate_indicator_state(v:u16,p:u16,state:&gearwise_core::model::DpiIndicatorState)->Result<()> {validate_indicator(v,p)?;if state.enabled&&state.mode==gearwise_core::model::DpiIndicatorMode::Unspecified{return Err(Error::invalid("Choose a known COMPX indicator mode before enabling"));}Ok(())}
pub fn validate_palette(v:u16,p:u16,stage:u8)->Result<()> {if stage>=palette_stages(v,p){Err(Error::Unsupported("Exact COMPX DPI color table"))}else{Ok(())}}

fn identity(p: u16) -> Option<(u8, &'static str, u16)> {
    models().iter().find(|m|m.product==p).map(|m|(m.mids.first().copied().unwrap_or(0),m.name.as_str(),m.max_dpi))
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
        if v < if matches!(max,36000|26000) { 50 } else { 100 }
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
        ButtonAction::ProfileCycle => (9,255),
        ButtonAction::ProfileSelect(index) if index<4 => (9,161+index),
        ButtonAction::Scroll(gearwise_core::model::ScrollDirection::Up) => (11,1),
        ButtonAction::Scroll(gearwise_core::model::ScrollDirection::Down) => (11,2),
        ButtonAction::Mouse(MouseButton::WheelLeft) => (3,1),
        ButtonAction::Mouse(MouseButton::WheelRight) => (3,2),
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
    profile:Option<u8>,
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
        if self.model.brand=="pulsar" {
            if self.exchange(3,0,0)?.first()!=Some(&1){return Err(Error::invalid("Pulsar receiver has no awake mouse"));}
            let profile=self.exchange(0x0e,0,0)?;let current=*profile.first().ok_or(Error::invalid("Pulsar active profile reply"))?;if current>3{return Err(Error::invalid("Pulsar active profile index"));}
            if self.profile.is_some_and(|p|p!=current){return Err(Error::invalid("Pulsar active profile changed; reopen device before writing"));}self.profile=Some(current);return Ok(());
        }
        let r = self.exchange(0x10, 0, 0)?;
        if r.len() < 2 || r[..2] != [2, self.mid] {
            return Err(Error::invalid(
                "ATK model/sensor identity changed or unknown; no settings written",
            ));
        }
        // EEPROM rows are scoped to the currently selected firmware bank.
        // Pin its raw token; do not infer its display numbering or switch banks.
        let selected=self.exchange(0x0e,0,0)?;
        if selected.len()!=1{return Err(Error::invalid("COMPX current board reply length"));}
        let current=selected[0];
        if self.profile.is_some_and(|p|p!=current){return Err(Error::invalid("COMPX 板载配置已改变；请断开并重新连接鼠标，再读取当前组。未继续写入"));}
        self.profile=Some(current);
        Ok(())
    }
    fn write_verified(&mut self, address: u16, next: &[u8]) -> Result<()> {
        self.identify()?;
        let before = self.read(address, next.len())?;
        self.identify()?;
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
        let (mid, name, max) = (model.mids.first().copied().unwrap_or(0),model.name.as_str(),model.max_dpi);
        let mut this = Self {
            io,
            mid,
            max,
            model,
            profile:None,
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
                battery:battery_supported(model.vendor,model.product),
                dpi: Some(DpiCaps {
                    min: model.min_dpi,
                    max,
                    step: 50,
                    max_stages: model.max_stages,
                    separate_xy: model.separate_xy,
                }),
                report_rates: model.rates.clone(),
                tuning:tuning::capabilities(model),
                buttons: (1..=model.buttons)
                    .map(|id| ButtonCaps {
                        id,
                        name: ["左键", "右键", "中键", "后退", "前进", "底部 DPI"]
                            [(id - 1) as usize]
                            .into(),
                        remappable: id != 1&&model.native_buttons,
                        default_action: factory(id).unwrap(),
                        hotspot: None,
                    })
                    .collect(),
                dpi_color_stages:palette_stages(model.vendor,model.product),
                dpi_indicator:palette_stages(model.vendor,model.product)>0,
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
        if model.brand=="pulsar" {this.identify()?;}else{
            let identity = this.exchange(0x10,0,0)?;
            if identity.len()<2 || identity[0]!=2 || !model.mids.contains(&identity[1]) {
                return Err(Error::invalid("COMPX unknown CID/MID for this USB model; no settings written"));
            }
            this.mid=identity[1];
        }
        this.refresh()?;
        if is_pulsar(this.model.vendor,this.model.product){this.desc.id=format!("{}:board:{}",this.desc.id,this.profile.ok_or(Error::invalid("Pulsar missing board identity"))?);}
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
        if profile_cycle_supported(self.model.vendor,self.model.product)&&raw==[9,255,0,77]{return Some((ButtonAction::ProfileCycle,None));}
        if profile_cycle_supported(self.model.vendor,self.model.product)&&raw.len()==4&&sum(raw)==0x55&&raw[0]==9&&raw[2]==0&&(161..=164).contains(&raw[1]){return Some((ButtonAction::ProfileSelect(raw[1]-161),None));}
        if horizontal_scroll_supported(self.model.vendor,self.model.product)&&raw.len()==4&&sum(raw)==0x55&&matches!(raw[0],3|11)&&raw[2]==0{
            use gearwise_core::model::ScrollDirection;
            let action=match (raw[0],raw[1]){(3,1)=>ButtonAction::Mouse(MouseButton::WheelLeft),(3,2)=>ButtonAction::Mouse(MouseButton::WheelRight),(11,1)=>ButtonAction::Scroll(ScrollDirection::Up),(11,2)=>ButtonAction::Scroll(ScrollDirection::Down),_=>return None};
            return Some((action,None));
        }
        if let Some(action) = decode_button(raw) {
            return Some((action, None));
        }
        if raw.len() == 4 && sum(raw) == 0x55 && raw[0] == 6 {
            let m = self.read_native_macro(raw[1], raw[2]).ok()?;
            if let Some(action)=native_macro::media_action(&m){return Some((action,None));}
            return Some((ButtonAction::Macro(m.id.clone()), Some(m)));
        }
        None
    }
    fn pulsar_action(&mut self,id:u16,raw:&[u8])->Result<Option<ButtonAction>>{
        if let Some(action)=decode_button(raw){return Ok(Some(action));}
        if raw.len()!=4||sum(raw)!=0x55||raw[0]!=5{return Ok(None);}
        let address=pulsar_shortcut::address(id)?;
        if u16::from_be_bytes([raw[1],raw[2]])!=address{return Ok(None);}
        Ok(pulsar_shortcut::decode(&self.read_blob(address,pulsar_shortcut::SIZE)?))
    }
    fn pulsar_key(&mut self,id:u16,usage:u8,mods:gearwise_core::model::Modifiers)->Result<()>{
        let payload=pulsar_shortcut::encode(usage,mods)?;let address=pulsar_shortcut::address(id)?;let target=96+(id-1)*4;
        self.identify()?;let original=self.read(target,4)?;
        if self.pulsar_action(id,&original)?.is_none(){return Err(Error::invalid("Pulsar unknown original binding retained"));}
        for other in 1..=self.model.buttons{let row=self.read(96+(other-1)*4,4)?;if sum(&row)!=0x55{return Err(Error::invalid("Pulsar button allocation checksum"));}if other!=id&&row[0]==5&&u16::from_be_bytes([row[1],row[2]])==address{return Err(Error::invalid("Pulsar shortcut slot is referenced by another button"));}}
        let before=self.read_blob(address,pulsar_shortcut::SIZE)?;
        if !before.iter().all(|v|*v==0)&&!before.iter().all(|v|*v==255)&&pulsar_shortcut::decode(&before).is_none(){return Err(Error::invalid("Pulsar unknown shortcut slot retained"));}
        let[hi,lo]=address.to_be_bytes();let binding=[5,hi,lo,0x55u8.wrapping_sub(5).wrapping_sub(hi).wrapping_sub(lo)];
        let operation=(||->Result<()>{for(n,chunk)in payload.chunks(10).enumerate(){self.write_verified(address+n as u16*10,chunk)?;}self.write_verified(target,&binding)?;
            if self.read_blob(address,pulsar_shortcut::SIZE)?!=payload||self.read(target,4)?!=binding{return Err(Error::invalid("Pulsar full shortcut did not read back"));}Ok(())})();
        if let Err(error)=operation{let restore=(||->Result<()>{self.identify()?;for(n,chunk)in before.chunks(10).enumerate(){self.write_verified(address+n as u16*10,chunk)?;}self.write_verified(target,&original)?;Ok(())})();return Err(Error::invalid(format!("{error}; Pulsar original shortcut restoration: {restore:?}")));}Ok(())
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
        // A known current slot owned only by this button is an explicit edit,
        // not a free-standing factory macro. Reuse it before consuming free slots.
        let mut selected=(original[0]==6&&original[1]<12&&!occupied.contains(&original[1])).then_some(original[1]);
        for slot in 0..12u8 {
            if selected.is_some(){break;}
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
        if !(1..=self.model.max_stages).contains(&count) || active >= count {
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
            let decoded = if self.model.native_macros{self.read_action(&raw)}else{self.pulsar_action(id,&raw)?.map(|a|(a,None))};
            let known = decoded.is_some();
            if let Some((action, definition)) = decoded {
                state.buttons.insert(id, action);
                if let Some(m) = definition {
                    if !self.read_macros.iter().any(|v| v.id == m.id) {
                        self.read_macros.push(m);
                    }
                }
            }
            self.caps.buttons[(id - 1) as usize].remappable = id != 1 && known && self.model.native_buttons;
        }
        if !self.caps.lighting.is_empty() {
            let row = self.read(76, 8)?;
            if self.caps.dpi_indicator {state.dpi_indicator=indicator::decode(&row).ok();}
            let mode = if state.dpi_indicator.is_some_and(|v|!v.enabled&&v.mode==gearwise_core::model::DpiIndicatorMode::Unspecified){0}else{checked_pair(&row[..2])?};
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
        for stage in 0..self.caps.dpi_color_stages {
            let row=self.read(44+u16::from(stage)*4,4)?;
            if sum(&row)==0x55 {state.dpi_colors.insert(stage,Rgb{r:row[0],g:row[1],b:row[2]});}
        }
        if self.caps.dpi_color_stages>0 {self.identify()?;}
        if self.caps.tuning.is_some(){state.tuning=self.read_tuning().ok();}
        if self.caps.battery {
            // Optional telemetry never prevents editing readable settings. Unknown
            // data does not become a default percentage or a cached prior value.
            state.battery=self.exchange(4,0,0).ok().and_then(|raw|battery::decode(&raw).ok());
            self.identify()?;
        }
        Ok(state)
    }
}
impl Device for Atk {
    fn set_macro_definitions(&mut self, definitions: &[Macro]) -> Result<()> {
        if !self.model.native_macros&&!definitions.is_empty(){return Err(Error::Unsupported("Pulsar native macro writes are not verified"));}
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
            Setting::Tuning(p)=>tuning::validate(self.model,p)?,
            Setting::Dpi(d) => {
                if d.stages.is_empty()
                    || d.stages.len() > self.model.max_stages as usize
                    || usize::from(d.active) >= d.stages.len()
                {
                    return Err(Error::invalid("ATK DPI stage count/index"));
                }
                validate_dpi(d,self.model.product)?;
            }
            Setting::DpiActiveStage(n) if *n < self.model.max_stages => {}
            Setting::ReportRate(hz) => {
                rate_code(*hz)?;
                if !self.caps.report_rates.contains(hz) {
                    return Err(Error::invalid("ATK exact transport polling ceiling"));
                }
            }
            Setting::Button { id, action } => {
                if !self.model.native_buttons || !(2..=self.model.buttons).contains(id) {
                    return Err(Error::invalid(
                        "ATK protects left click and unknown buttons",
                    ));
                }
                match action {
                    ButtonAction::ProfileSelect(index) => {
                        if !profile_cycle_supported(self.model.vendor,self.model.product)||*index>=4{return Err(Error::Unsupported("Exact COMPX direct profile binding"));}
                    }
                    ButtonAction::ProfileCycle => {
                        if !profile_cycle_supported(self.model.vendor,self.model.product){return Err(Error::Unsupported("Exact COMPX profile cycle binding"));}
                    }
                    ButtonAction::Scroll(_) => {
                        if !vertical_scroll_supported(self.model.vendor,self.model.product){return Err(Error::Unsupported("Exact COMPX vertical scroll binding"));}
                    }
                    ButtonAction::Mouse(MouseButton::WheelLeft|MouseButton::WheelRight) => {
                        if !horizontal_scroll_supported(self.model.vendor,self.model.product){return Err(Error::Unsupported("Exact COMPX horizontal scroll binding"));}
                    }
                    ButtonAction::Key { usage, mods } => {
                        if is_pulsar(self.model.vendor,self.model.product){validate_pulsar_key(*usage,*mods)?;}else{native_macro::encode(&native_macro::keyboard(*usage, *mods))?;}
                    }
                    ButtonAction::Media(usage)=>{if !self.model.native_macros{return Err(Error::Unsupported("Pulsar native media record is not verified"));}native_macro::validate_media(*usage)?;},
                    ButtonAction::Macro(key) => {
                        if !self.model.native_macros{return Err(Error::Unsupported("Pulsar native macro payload not verified"));}
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
            Setting::DpiColor {stage,..}=>validate_palette(self.model.vendor,self.model.product,*stage)?,
            Setting::DpiIndicator(value)=>validate_indicator_state(self.model.vendor,self.model.product,value)?,
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
            Setting::Tuning(p)=>self.apply_tuning(p)?,
            Setting::DpiIndicator(value)=>self.apply_indicator(value)?,
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
            Setting::Button{id,action:ButtonAction::Key{usage,mods}} if is_pulsar(self.model.vendor,self.model.product)=>{self.pulsar_key(*id,*usage,*mods)?;}
            Setting::Button { id, action }
                if matches!(action, ButtonAction::Macro(_) | ButtonAction::Key { .. } | ButtonAction::Media(_)) =>
            {
                self.native_binding(*id, action)?;
            }
            Setting::Button { id, action } => {
                let old = self.read(96 + (id - 1) * 4, 4)?;
                if (if self.model.native_macros{self.read_action(&old).map(|v|v.0)}else{self.pulsar_action(*id,&old)?}).is_none() {
                    return Err(Error::invalid("ATK unknown original action retained; native macro/shortcut is not overwritten"));
                }
                self.write_verified(96 + (id - 1) * 4, &button_bytes(action, *id)?)?;
            }
            Setting::DpiColor {stage,color}=>{
                let address=44+u16::from(*stage)*4;
                let current=self.read(address,4)?;
                if sum(&current)!=0x55 {return Err(Error::invalid("COMPX original DPI color checksum; unknown row retained"));}
                let next=[color.r,color.g,color.b,0x55u8.wrapping_sub(color.r).wrapping_sub(color.g).wrapping_sub(color.b)];
                self.write_verified(address,&next)?;
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
        supported(i.vendor_id,i.product_id)&&i.bus==gearwise_core::transport::BusKind::Usb&&if model(i.vendor_id,i.product_id).is_some_and(|m|m.brand=="pulsar"){i.interface_number==1&&i.usage_page>=0xff00}else{i.usage_page==0xff02&&i.usage==2}
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
    eeprom[10..12].copy_from_slice(&pair(1));
    for(index,v)in[8,1,6,0,0].into_iter().enumerate(){eeprom[169+2*index..171+2*index].copy_from_slice(&pair(v));}
    for(index,v)in[1,6,2].into_iter().enumerate(){eeprom[181+2*index..183+2*index].copy_from_slice(&pair(v));}
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
            3=>vec![1],0x0e=>vec![0],4=>vec![73,1,0x10,0],
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
        interface_number:1,
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
      for spec in models().iter().filter(|m|matches!(m.brand.as_str(),"vxe"|"vgn")){
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
      for spec in models().iter().filter(|m|matches!(m.brand.as_str(),"vxe"|"vgn")){
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

#[cfg(test)]mod pulsar_tests{
 use super::*;
 #[test]fn pulsar_real_driver_dpi_indicator_and_rate_roundtrip(){for product in [0xf507,0xf508]{let(mut d,l)=memory_device(product).unwrap();l.lock().unwrap().clear();let next=DpiState{stages:vec![DpiStage::uniform(50),DpiStage::uniform(12800),DpiStage::uniform(26000)],active:2};d.apply(&Setting::Dpi(next.clone())).unwrap();d.apply(&Setting::ReportRate(500)).unwrap();let color=Rgb{r:19,g:61,b:97};d.apply(&Setting::Lighting{zone:0,state:ZoneState{effect:LightingEffect::Static{color},brightness:100}}).unwrap();let state=d.read_state().unwrap();assert_eq!(state.dpi,Some(next));assert_eq!(state.report_rate,Some(500));assert_eq!(state.lighting[&0].effect,LightingEffect::Static{color});assert!(d.caps.buttons.iter().all(|b|b.remappable==(b.id!=1)));assert!(l.lock().unwrap().iter().all(|r|!matches!(r,gearwise_core::transport::MockRecord::Write(p)if p[1]==0x10)));}}
 #[test]fn pulsar_shared_codec_matches_public_known_vectors(){assert_eq!(encode_dpi(50,50,26000).unwrap(),[0,0,0,85]);assert_eq!(encode_dpi(12800,12800,26000).unwrap(),[255,255,0,87]);assert_eq!(encode_dpi(26000,26000,26000).unwrap(),[7,7,136,191]);for dpi in (50..=26000).step_by(50){assert_eq!(decode_dpi(&encode_dpi(dpi,dpi,26000).unwrap(),26000).unwrap(),DpiStage::uniform(dpi));}}
 #[test]fn pulsar_rejects_extra_stages_xy_keyboard_and_macros_before_io(){let(mut d,l)=memory_device(0xf507).unwrap();for setting in [Setting::Dpi(DpiState{stages:vec![DpiStage{x:800,y:1600}],active:0}),Setting::Dpi(DpiState{stages:vec![DpiStage::uniform(800);5],active:0}),Setting::Button{id:4,action:ButtonAction::Key{usage:0,mods:gearwise_core::model::Modifiers(0)}}]{l.lock().unwrap().clear();assert!(d.apply(&setting).is_err());assert!(l.lock().unwrap().is_empty());}}
 #[test]fn pulsar_profile_change_blocks_writes_instead_of_touching_other_bank(){let(mut d,l)=memory_device(0xf508).unwrap();d.profile=Some(1);l.lock().unwrap().clear();assert!(d.apply(&Setting::ReportRate(500)).unwrap_err().to_string().contains("profile changed"));assert!(!l.lock().unwrap().iter().any(|r|matches!(r,gearwise_core::transport::MockRecord::Write(p)if p[1]==7)));}
 #[test]fn pulsar_all_known_button_actions_roundtrip_without_macro_memory(){
  let actions=vec![ButtonAction::Disabled,ButtonAction::Mouse(MouseButton::Left),ButtonAction::Mouse(MouseButton::Right),ButtonAction::Mouse(MouseButton::Middle),ButtonAction::Mouse(MouseButton::Back),ButtonAction::Mouse(MouseButton::Forward),ButtonAction::Dpi(DpiFunction::Cycle),ButtonAction::Dpi(DpiFunction::Up),ButtonAction::Dpi(DpiFunction::Down)];
  for product in [0xf507,0xf508]{let(mut d,l)=memory_device(product).unwrap();l.lock().unwrap().clear();for id in 2..=5{for action in &actions{d.apply(&Setting::Button{id,action:action.clone()}).unwrap();assert_eq!(&d.read_state().unwrap().buttons[&id],action);}}
   assert!(l.lock().unwrap().iter().filter_map(|r|if let gearwise_core::transport::MockRecord::Write(p)=r{Some(p)}else{None}).filter(|p|p[1]==7).all(|p|p[3]==0&&(100..=112).contains(&p[4])&&p[5]==4));
   for id in [0,1,6,255]{l.lock().unwrap().clear();assert!(d.apply(&Setting::Button{id,action:ButtonAction::Disabled}).is_err());assert!(l.lock().unwrap().is_empty());}
  }
 }
 #[test]fn pulsar_button_records_match_independent_public_vectors(){
  for(action,expected)in[(ButtonAction::Mouse(MouseButton::Back),[1,8,0,0x4c]),(ButtonAction::Mouse(MouseButton::Forward),[1,16,0,0x44]),(ButtonAction::Disabled,[0,0,0,0x55]),(ButtonAction::Dpi(DpiFunction::Cycle),[2,1,0,0x52])]{assert_eq!(button_bytes(&action,4).unwrap(),expected);assert_eq!(decode_button(&expected),Some(action));}
 }
 #[test]fn pulsar_unknown_original_custom_record_remains_locked(){
  let(mut d,l)=memory_device(0xf507).unwrap();let original=[5,0,0,0x50];d.io.write(&frame(7,108,4,&original).unwrap()).unwrap();assert!(!d.read_state().unwrap().buttons.contains_key(&4));assert!(!d.capabilities().buttons.iter().find(|b|b.id==4).unwrap().remappable);l.lock().unwrap().clear();assert!(d.apply(&Setting::Button{id:4,action:ButtonAction::Disabled}).is_err());assert!(!l.lock().unwrap().iter().any(|r|matches!(r,gearwise_core::transport::MockRecord::Write(p)if p[1]==7)));assert_eq!(d.read(108,4).unwrap(),original);
 }
 #[test]fn pulsar_native_keys_read_import_reopen_and_replace_with_mouse_actions(){
  for product in [0xf507,0xf508]{let(mut d,log)=memory_device(product).unwrap();for id in 2..=5{let action=ButtonAction::Key{usage:0x17,mods:gearwise_core::model::Modifiers(3)};d.apply(&Setting::Button{id,action:action.clone()}).unwrap();assert_eq!(d.read_state().unwrap().buttons[&id],action);let address=pulsar_shortcut::address(id).unwrap();let raw=d.read_blob(address,32).unwrap();assert_eq!(pulsar_shortcut::decode(&raw),Some(action));}
   let info=HidInterfaceInfo{vendor_id:0x3554,product_id:product,interface_number:1,usage_page:0xff02,..Default::default()};let mut d=Atk::open(d.io,&info).unwrap();assert_eq!(d.read_state().unwrap().buttons[&4],ButtonAction::Key{usage:0x17,mods:gearwise_core::model::Modifiers(3)});d.apply(&Setting::Button{id:4,action:ButtonAction::Mouse(MouseButton::Back)}).unwrap();assert_eq!(d.read_state().unwrap().buttons[&4],ButtonAction::Mouse(MouseButton::Back));assert!(log.lock().unwrap().iter().any(|r|matches!(r,gearwise_core::transport::MockRecord::Write(p)if p[1]==7&&p[3..5]==[1,0x60])));
  }
 }
 #[test]fn pulsar_shortcut_limits_reject_before_io(){let(mut d,l)=memory_device(0xf507).unwrap();for(usage,mods)in[(0,0),(255,0),(4,31),(4,255)]{l.lock().unwrap().clear();assert!(d.apply(&Setting::Button{id:4,action:ButtonAction::Key{usage,mods:gearwise_core::model::Modifiers(mods)}}).is_err());assert!(l.lock().unwrap().is_empty());}}
 #[test]fn pulsar_unknown_shortcut_bytes_and_other_button_references_are_never_overwritten(){
  for alias in [false,true]{let(mut d,l)=memory_device(0xf507).unwrap();let address=pulsar_shortcut::address(4).unwrap();if alias{let row=[5,1,0x60,0xef];d.io.write(&frame(7,96,4,&row).unwrap()).unwrap();}else{d.io.write(&frame(7,address,4,&[7,3,4,5]).unwrap()).unwrap();}let before=d.read_blob(address,32).unwrap();l.lock().unwrap().clear();assert!(d.apply(&Setting::Button{id:4,action:ButtonAction::Key{usage:4,mods:gearwise_core::model::Modifiers(0)}}).is_err());assert!(!l.lock().unwrap().iter().any(|r|matches!(r,gearwise_core::transport::MockRecord::Write(p)if p[1]==7)));assert_eq!(d.read_blob(address,32).unwrap(),before);}
 }
 struct ShortWriteOnce{inner:Box<dyn HidTransport>,at:usize,count:usize}
 impl HidTransport for ShortWriteOnce{
  fn write(&mut self,p:&[u8])->Result<usize>{let n=self.inner.write(p)?;if p.len()==17&&p[1]==7{self.count+=1;if self.count==self.at{return Ok(16);}}Ok(n)}
  fn read_timeout(&mut self,p:&mut[u8],ms:i32)->Result<usize>{self.inner.read_timeout(p,ms)}
  fn send_feature_report(&mut self,p:&[u8])->Result<()>{self.inner.send_feature_report(p)}fn get_feature_report(&mut self,p:&mut[u8])->Result<usize>{self.inner.get_feature_report(p)}
 }
 #[test]fn pulsar_uncertain_short_write_at_every_step_restores_full_record_and_binding(){
  for at in 1..=5{let(mut d,_)=memory_device(0xf508).unwrap();let row=d.read(108,4).unwrap();let blob=d.read_blob(0x160,32).unwrap();d.io=Box::new(ShortWriteOnce{inner:d.io,at,count:0});assert!(d.apply(&Setting::Button{id:4,action:ButtonAction::Key{usage:0x17,mods:gearwise_core::model::Modifiers(15)}}).is_err());assert_eq!(d.read(108,4).unwrap(),row);assert_eq!(d.read_blob(0x160,32).unwrap(),blob);}
 }
 #[test]fn pulsar_shortcut_readback_corruption_restores_all_bytes(){let(mut d,_)=memory_device(0xf507).unwrap();let before=d.read_blob(0x160,32).unwrap();let row=d.read(108,4).unwrap();d.io=Box::new(CorruptOnce{inner:d.io,pending:false,used:false});assert!(d.apply(&Setting::Button{id:4,action:ButtonAction::Key{usage:4,mods:gearwise_core::model::Modifiers(3)}}).is_err());assert_eq!(d.read_blob(0x160,32).unwrap(),before);assert_eq!(d.read(108,4).unwrap(),row);}
 #[test]fn pulsar_broken_shortcut_or_noncanonical_pointer_is_locked_on_read_and_apply(){for pointer in [0x160u16,0x120u16]{let(mut d,l)=memory_device(0xf507).unwrap();let[hi,lo]=pointer.to_be_bytes();let row=[5,hi,lo,0x55u8.wrapping_sub(5).wrapping_sub(hi).wrapping_sub(lo)];d.io.write(&frame(7,108,4,&row).unwrap()).unwrap();assert!(!d.read_state().unwrap().buttons.contains_key(&4));assert!(!d.capabilities().buttons.iter().find(|b|b.id==4).unwrap().remappable);l.lock().unwrap().clear();assert!(d.apply(&Setting::Button{id:4,action:ButtonAction::Disabled}).is_err());assert!(!l.lock().unwrap().iter().any(|r|matches!(r,gearwise_core::transport::MockRecord::Write(p)if p[1]==7)));}}
 struct CorruptOnce{inner:Box<dyn HidTransport>,pending:bool,used:bool}
 impl HidTransport for CorruptOnce{
  fn write(&mut self,p:&[u8])->Result<usize>{let n=self.inner.write(p)?;if p.len()==17&&p[1]==7&&!self.used{self.pending=true;}Ok(n)}
  fn read_timeout(&mut self,p:&mut[u8],ms:i32)->Result<usize>{let n=self.inner.read_timeout(p,ms)?;if self.pending&&n==17&&p[1]==8{p[6]^=1;p[16]=0x55u8.wrapping_sub(sum(&p[..16]));self.pending=false;self.used=true;}Ok(n)}
  fn send_feature_report(&mut self,p:&[u8])->Result<()>{self.inner.send_feature_report(p)}
  fn get_feature_report(&mut self,p:&mut[u8])->Result<usize>{self.inner.get_feature_report(p)}
 }
 #[test]fn pulsar_failed_button_readback_restores_exact_original(){
  let(mut d,l)=memory_device(0xf508).unwrap();let old=d.read(108,4).unwrap();d.io=Box::new(CorruptOnce{inner:d.io,pending:false,used:false});l.lock().unwrap().clear();assert!(d.apply(&Setting::Button{id:4,action:ButtonAction::Disabled}).is_err());assert_eq!(d.read(108,4).unwrap(),old);let log=l.lock().unwrap();let writes=log.iter().filter_map(|r|if let gearwise_core::transport::MockRecord::Write(p)=r{Some(p)}else{None}).filter(|p|p[1]==7).collect::<Vec<_>>();assert_eq!(writes.len(),2);assert_eq!(&writes[1][6..10],old.as_slice());
 }
 #[test]fn pulsar_exact_control_interface_not_other_3554_mice(){let mut i=HidInterfaceInfo{vendor_id:0x3554,product_id:0xf507,interface_number:1,usage_page:0xff02,..Default::default()};assert!(AtkDriver.probe(&i));i.interface_number=0;assert!(!AtkDriver.probe(&i));i.interface_number=1;i.product_id=0xf509;assert!(!AtkDriver.probe(&i));}
}
#[cfg(test)]mod media_native_tests{
 use super::*;
 #[test]fn every_native_compx_transport_writes_reads_and_reopens_seven_media_actions(){
  for spec in models().iter().filter(|s|s.native_macros&&s.native_buttons){
   let(mut d,_)=memory_device(spec.product).unwrap();for usage in native_macro::MEDIA_USAGES{d.apply(&Setting::Button{id:4,action:ButtonAction::Media(*usage)}).unwrap();assert_eq!(d.read_state().unwrap().buttons[&4],ButtonAction::Media(*usage));}
   let info=HidInterfaceInfo{vendor_id:spec.vendor,product_id:spec.product,interface_number:1,usage_page:0xff02,usage:2,..Default::default()};let mut reopened=Atk::open(d.io,&info).unwrap();assert_eq!(reopened.read_state().unwrap().buttons[&4],ButtonAction::Media(0xea));
  }
 }
 #[test]fn repeated_edits_reuse_the_current_exclusive_macro_slot_and_leave_free_slots_empty(){
  let(mut d,_)=memory_device(0x1115).unwrap();d.apply(&Setting::Button{id:4,action:ButtonAction::Media(0xe2)}).unwrap();let slot=d.read(108,4).unwrap()[1];for n in 0..30{d.apply(&Setting::Button{id:4,action:ButtonAction::Media(native_macro::MEDIA_USAGES[n%7])}).unwrap();assert_eq!(d.read(108,4).unwrap()[1],slot);}for other in 0..12u16{if other!=u16::from(slot){assert_eq!(d.read(768+other*384+31,1).unwrap(),vec![0]);}}
 }
 #[test]fn a_shared_media_slot_is_not_overwritten_when_only_one_button_changes(){
  let(mut d,_)=memory_device(0x1115).unwrap();d.apply(&Setting::Button{id:4,action:ButtonAction::Media(0xe2)}).unwrap();let row=d.read(108,4).unwrap();d.write_verified(112,&row).unwrap();let base=768+u16::from(row[1])*384;let before=d.read_blob(base,43).unwrap();d.apply(&Setting::Button{id:4,action:ButtonAction::Media(0xe9)}).unwrap();let state=d.read_state().unwrap();assert_eq!(state.buttons[&4],ButtonAction::Media(0xe9));assert_eq!(state.buttons[&5],ButtonAction::Media(0xe2));assert_eq!(d.read_blob(base,43).unwrap(),before);assert_ne!(d.read(108,4).unwrap()[1],row[1]);
 }
 #[test]fn unverified_consumer_usages_pulsar_and_left_button_reject_before_any_io(){
  for(product,id,usage)in[(0x1115,4,0x183),(0x1115,1,0xe2),(0xf507,4,0xe2),(0xf508,4,0xe2)]{let(mut d,log)=memory_device(product).unwrap();log.lock().unwrap().clear();assert!(d.apply(&Setting::Button{id,action:ButtonAction::Media(usage)}).is_err());assert!(log.lock().unwrap().is_empty());}
 }
}

#[cfg(test)]
#[path="atk_palette_tests.rs"]
mod compx_palette_tests;

#[cfg(test)]
#[path="atk_indicator_tests.rs"]
mod compx_indicator_tests;

#[cfg(test)]
#[path="compx_horizontal_tests.rs"]
mod horizontal_tests;

#[cfg(test)]
#[path="compx_profile_cycle_tests.rs"]
mod profile_cycle_tests;

#[cfg(test)]#[path="compx_profile_select_tests.rs"]mod profile_select_tests;

#[cfg(test)]#[path="compx_repeat_tests.rs"]mod repeat_tests;
