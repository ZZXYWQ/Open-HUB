//! The [`Device`] implementation shared by every Razer model; what differs
//! between models lives in the [`Model`] table.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use gearwise_core::model::{
    Battery, Connection, DpiCaps, DpiState, LightingZoneCaps, Rgb, VID_RAZER, Vendor, ZoneState,
    ButtonAction, ButtonCaps, MouseButton,
};
use gearwise_core::transport::HidInterfaceInfo;
use gearwise_core::{
    Capabilities, Device, DeviceDescriptor, DeviceEvent, DeviceState, Error, Result, Setting,
};

use crate::commands::{self, MAX_STAGES};
use crate::link::Link;
use crate::models::Model;
use crate::report::Report;

// Standard-layer records independently transcribed from the pinned OpenMouse
// Viper V3 Pro button fixtures. Scroll Click/keyboard/Hypershift remain unverified.
pub(crate) fn viper_default(id:u8)->Option<[u8;10]>{
 let mut args=[0;10];args[..3].copy_from_slice(&[1,id,0]);
 match id{1|2|4|5|9|10=>args[3..6].copy_from_slice(&[1,1,id]),0x60=>args[3..6].copy_from_slice(&[6,1,6]),_=>return None};Some(args)
}
pub(crate) fn viper_decode(id:u8,args:&[u8])->Option<ButtonAction>{
 if args.len()<10||args[..3]!=[1,id,0]||args[6..10]!=[0;4]{return None;}
 if args[3..6]==[0;3]{return Some(ButtonAction::Disabled);}
 if matches!(id,9|10|0x60){return (args[..10]==viper_default(id)?).then_some(ButtonAction::Default);}
 match args[3..6]{[1,1,1]=>Some(MouseButton::Left),[1,1,2]=>Some(MouseButton::Right),[1,1,4]=>Some(MouseButton::Back),[1,1,5]=>Some(MouseButton::Forward),_=>None}.map(ButtonAction::Mouse)
}

/// How often `poll_events` asks wireless/battery devices for their charge.
const BATTERY_INTERVAL: Duration = Duration::from_secs(30);

/// A connected Razer mouse or keyboard.
pub struct RazerDevice {
    desc: DeviceDescriptor,
    caps: Capabilities,
    model: &'static Model,
    link: Link,
    /// Last known values: read-back where the device allows it, otherwise
    /// whatever was applied last (effects, key colours).
    state: DeviceState,
    /// False after a battery poll found the device unreachable.
    online: bool,
    battery_interval: Duration,
    last_battery_poll: Option<Instant>,
    board:Option<u8>,
}

impl RazerDevice {
    /// Identify the device behind `link` and build its descriptor.
    ///
    /// Serial and firmware are best effort: a sleeping wireless mouse does not
    /// answer, which must not stop it from being listed.
    pub fn open(model: &'static Model, mut link: Link, iface: &HidInterfaceInfo) -> Result<Self> {
        let basilisk=crate::basilisk::supported(VID_RAZER,model.pid);
        if basilisk||matches!(model.pid,0x00b6|0x00b7){link.strict_model_replies();}
        let wireless = model.connection == Connection::Wireless;
        let serial = identity(link.exec(commands::serial()), wireless)?
            .and_then(|r| commands::parse_serial(&r));
        let firmware = identity(link.exec(commands::firmware()), wireless)?
            .map(|r| commands::parse_firmware(&r));

        let unique = serial
            .clone()
            .or_else(|| iface.serial.clone())
            .unwrap_or_else(|| iface.path.clone());
        let board=if basilisk{Some(Self::active_board(&mut link)?)}else{None};
        let desc = DeviceDescriptor {
            id: format!("razer:{:04x}:{unique}{}", model.pid,board.map(|n|format!(":board:{n}")).unwrap_or_default()),
            model_key: DeviceDescriptor::model_key_for(Vendor::Razer, model.pid),
            vendor: Vendor::Razer,
            kind: model.kind,
            name: model.name.to_string(),
            vendor_id: VID_RAZER,
            product_id: model.pid,
            connection: model.connection,
            firmware,
            serial,
            simulated: false,
        };
        Ok(Self {
            desc,
            caps: capabilities(model),
            model,
            link,
            state: DeviceState::default(),
            online: true,
            battery_interval: BATTERY_INTERVAL,
            last_battery_poll: None,
            board,
        })
    }

    fn active_board(link:&mut Link)->Result<u8>{
        let r=link.exec(Report::new(5,0x84,0,&[]))?;
        if !(1..=5).contains(&r.args[0]){return Err(Error::protocol("Basilisk active onboard profile outside 1..5"));}Ok(r.args[0])
    }
    fn check_board(&mut self,write:bool)->Result<()>{
        if let Some(board)=self.board{
            if write&&self.link.exec(Report::new(0,0x84,2,&[]))?.args[0]!=0{return Err(Error::protocol("Close Synapse and restore normal mouse mode before applying"));}
            if Self::active_board(&mut self.link)?!=board{return Err(Error::protocol("Basilisk onboard profile changed; switch back or rescan before applying/restoring"));}
        }Ok(())
    }
    fn basilisk_button(&mut self,id:u16)->Result<Report>{
        let board=self.board.ok_or(Error::Unsupported("Basilisk board context"))?;
        self.link.exec(Report::new(2,0x8c,10,&[board,id as u8,0]))
    }
    fn set_basilisk_button(&mut self,id:u16,action:&ButtonAction)->Result<()>{
        let block=crate::basilisk::encode(id,action)?;
        self.check_board(true)?;
        let original=self.basilisk_button(id)?;
        if crate::basilisk::decode(id,&original.args[3..10]).is_none(){return Err(Error::Unsupported("Unknown original Basilisk binding retained"));}
        let mut args=original.args;args[3..10].copy_from_slice(&block);
        self.check_board(true)?;
        let result=self.link.exec(Report::new(2,0x0c,10,&args[..10])).and_then(|_|self.basilisk_button(id)).and_then(|r|if r.args[..10]==args[..10]{Ok(())}else{Err(Error::protocol("Basilisk button readback mismatch"))}).and_then(|_|self.check_board(false));
        if let Err(error)=result{
            // Never restore into a changed active profile/mode. Preserve exact raw original.
            let restore=self.check_board(true).and_then(|_|self.link.exec(Report::new(2,0x0c,10,&original.args[..10]))).and_then(|_|self.basilisk_button(id)).and_then(|r|if r.args[..10]==original.args[..10]{Ok(())}else{Err(Error::protocol("original button restore readback mismatch"))});
            return Err(Error::protocol(format!("{error}; original Basilisk record restoration: {restore:?}")));
        }Ok(())
    }

    #[cfg(test)]
    pub(crate) fn set_battery_interval(&mut self, d: Duration) {
        self.battery_interval = d;
    }

    // ----- validation + writes -------------------------------------------------

    fn dpi_caps(&self) -> Result<&DpiCaps> {
        self.caps.dpi.as_ref().ok_or(Error::Unsupported("DPI"))
    }

    fn validate_dpi(&self, d: &DpiState) -> Result<()> {
        let c = self.dpi_caps()?;
        if d.stages.is_empty() || d.stages.len() > c.max_stages as usize {
            return Err(Error::invalid(format!(
                "{} DPI stages (this device holds {})",
                d.stages.len(),
                c.max_stages
            )));
        }
        if d.active as usize >= d.stages.len() {
            return Err(Error::invalid("active DPI stage out of range"));
        }
        for v in d.stages.iter().flat_map(|s| [s.x, s.y]) {
            if !(c.min..=c.max).contains(&v) {
                return Err(Error::invalid(format!(
                    "{v} DPI is outside {}-{}",
                    c.min, c.max
                )));
            }
        }
        Ok(())
    }

    fn write_dpi(&mut self, d: &DpiState) -> Result<()> {
        self.check_board(true)?;
        if self.model.dpi.is_some_and(|s| s.stages)&&!(matches!(self.model.pid,0x00b6|0x00b7)&&self.caps.dpi.as_ref().is_some_and(|c|c.max_stages==1)) {
            self.link.exec(commands::set_dpi_stages(d))?;
        } else {
            // Devices without a stage table hold one resolution; validation
            // guarantees exactly one stage.
            self.link.exec(commands::set_dpi(d.stages[0]))?;
        }
        Ok(())
    }

    fn read_button(&mut self, id: u8) -> Result<Report> {
        let response=self.link.exec(Report::new(2,0x8c,10,&[1,id,0]))?;
        if response.data_size!=10 || response.args[..3]!=[1,id,0] {
            return Err(Error::protocol("Viper button identity/length"));
        }
        Ok(response)
    }

    fn set_button(&mut self, id:u16, action:&ButtonAction) -> Result<()> {
        if self.board.is_some(){return self.set_basilisk_button(id,action);}
        if !matches!(self.model.pid,0x00c0|0x00c1) || !matches!(id,2|4|5|9|10|0x60) {
            return Err(Error::Unsupported("exact Viper V3 Pro remappable control"));
        }
        let mut args=viper_default(id as u8).ok_or(Error::Unsupported("unverified Viper control"))?;
        match action{
            ButtonAction::Default=>{},
            ButtonAction::Disabled=>args[3..].fill(0),
            ButtonAction::Mouse(target) if matches!(id,2|4|5)=>{
                args[5]=match target{MouseButton::Left=>1,MouseButton::Right=>2,MouseButton::Back=>4,MouseButton::Forward=>5,_=>return Err(Error::Unsupported("unverified Viper mouse target"))};
            },
            _=>return Err(Error::Unsupported("Viper: captured mouse actions, or enable/disable scroll and sensitivity controls")),
        }
        let original=self.read_button(id as u8)?;
        if viper_decode(id as u8,&original.args).is_none(){return Err(Error::Unsupported("unknown original Viper binding retained"));}
        let write=self.link.exec_with_id(Report::new(2,0x0c,10,&args),0x10);
        let verify=write.and_then(|_|self.read_button(id as u8)).and_then(|r|{
            if r.args[..10]==args{Ok(())}else{Err(Error::protocol("Viper button did not read back"))}
        });
        if let Err(error)=verify {
            // Includes uncertain writes: attempt the exact raw original record.
            let restore=self.link.exec_with_id(Report::new(2,0x0c,10,&original.args[..10]),0x10);
            return Err(Error::protocol(format!("{error}; original record restore: {restore:?}")));
        }
        Ok(())
    }

    fn set_dpi(&mut self, d: &DpiState) -> Result<()> {
        self.validate_dpi(d)?;
        self.write_dpi(d)
    }

    /// The stage table is written as a whole, so switching the active stage
    /// re-sends the known table with a new active index.
    fn set_active_stage(&mut self, index: u8) -> Result<()> {
        let mut d = match self.state.dpi.clone() {
            Some(d) => d,
            None => {
                let d = self.read_dpi()?;
                self.state.dpi = Some(d.clone());
                d
            }
        };
        if index as usize >= d.stages.len() {
            return Err(Error::invalid("active DPI stage out of range"));
        }
        d.active = index;
        self.write_dpi(&d)
    }

    fn set_rate(&mut self, hz: u16) -> Result<()> {
        let p = self.model.polling;
        if p.rates.is_empty() {
            return Err(Error::Unsupported("polling rate"));
        }
        let requests = commands::set_polling(p.proto, hz)
            .filter(|_| p.rates.contains(&hz))
            .ok_or_else(|| Error::invalid(format!("{hz} Hz polling rate")))?;
        for r in requests {
            self.check_board(true)?;
            self.link.exec(r)?;
        }
        Ok(())
    }

    fn set_lighting(&mut self, zone: u8, st: ZoneState) -> Result<()> {
        let z = self
            .model
            .zones
            .iter()
            .find(|z| z.led == zone)
            .ok_or_else(|| Error::invalid(format!("no lighting zone {zone}")))?;
        if !z.effects.contains(&st.effect.kind()) {
            return Err(Error::Unsupported("lighting effect"));
        }
        if st.brightness > 100 {
            return Err(Error::invalid(format!(
                "brightness {} is above 100",
                st.brightness
            )));
        }
        self.check_board(true)?;
        self.link
            .exec(commands::effect(zone, &st.effect, self.model.wave)?)?;
        self.check_board(true)?;
        self.link
            .exec(commands::set_brightness(zone, st.brightness))?;
        Ok(())
    }

    fn set_per_key(&mut self, colors: &BTreeMap<u16, Rgb>) -> Result<()> {
        let matrix = self
            .model
            .matrix
            .as_ref()
            .ok_or(Error::Unsupported("per-key lighting"))?;
        if let Some(id) = colors
            .keys()
            .find(|&&id| u8::try_from(id).ok().and_then(|u| matrix.cell(u)).is_none())
        {
            return Err(Error::invalid(format!(
                "key {id:#x} is not on this keyboard"
            )));
        }
        // The whole frame is rebuilt: keys not mentioned keep their colour.
        let mut merged = self.state.per_key.clone();
        merged.extend(colors);
        for r in matrix.frame(&merged) {
            self.link.exec(r)?;
        }
        Ok(())
    }

    fn set_game_mode(&mut self, on: bool) -> Result<()> {
        let store = self
            .model
            .game_mode
            .ok_or(Error::Unsupported("game mode"))?;
        self.link.exec(commands::set_game_mode(store, on))?;
        Ok(())
    }

    // ----- reads ---------------------------------------------------------------

    fn read_dpi(&mut self) -> Result<DpiState> {
        let dav3=matches!(self.model.pid,0x00b6|0x00b7);
        let staged=self.model.dpi.is_some_and(|s|s.stages)&&!(dav3&&self.caps.dpi.as_ref().is_some_and(|c|c.max_stages==1));
        if staged {
            match self.link.exec(commands::get_dpi_stages()) {
                Ok(r)=>{let dpi=commands::parse_dpi_stages(&r).ok_or_else(||Error::protocol("device reported an empty DPI stage table"))?;if dav3{self.validate_dpi(&dpi)?;}return Ok(dpi);},
                // Only an explicit firmware refusal authorizes this exact-model
                // fallback. Corrupt/time-out stage responses do not become a
                // guessed single-stage capability or cause another write.
                Err(Error::Unsupported(_)) if dav3=>{if let Some(c)=self.caps.dpi.as_mut(){c.max_stages=1;}},
                Err(e)=>return Err(e),
            }
        }
        let r=self.link.exec(if dav3{commands::get_dpi_variable()}else{commands::get_dpi()})?;
        let dpi=DpiState{stages:vec![commands::parse_dpi(&r)],active:0};
        if dav3 {if r.data_size!=7||r.args[0]!=1||r.args[5..7]!=[0,0]{return Err(Error::protocol("DeathAdder V3 Pro current DPI reply shape"));}self.validate_dpi(&dpi)?;}
        Ok(dpi)
    }

    fn read_rate(&mut self) -> Result<u16> {
        let proto = self.model.polling.proto;
        let r = self.link.exec(commands::get_polling(proto))?;
        commands::parse_polling(proto, &r)
            .ok_or_else(|| Error::protocol("unknown polling-rate code"))
    }

    fn read_brightness(&mut self, led: u8) -> Result<u8> {
        Ok(commands::parse_brightness(
            &self.link.exec(commands::get_brightness(led))?,
        ))
    }

    fn read_battery(&mut self) -> Result<Battery> {
        let level = self.link.exec(commands::battery_level())?;
        let charging = self.link.exec(commands::charging())?;
        Ok(commands::parse_battery(&level, &charging))
    }

    fn read_game_mode(&mut self) -> Result<bool> {
        let store = self
            .model
            .game_mode
            .ok_or(Error::Unsupported("game mode"))?;
        Ok(commands::parse_game_mode(
            &self.link.exec(commands::get_game_mode(store))?,
        ))
    }
}

impl Device for RazerDevice {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }

    fn capabilities(&self) -> &Capabilities {
        &self.caps
    }

    fn read_state(&mut self) -> Result<DeviceState> {
        self.check_board(false)?;
        let mut s = self.state.clone();
        if self.board.is_some(){
            s.buttons.clear();
            for cap in &mut self.caps.buttons{cap.remappable=false;}
            for (id,_) in crate::basilisk::BUTTONS{if let Ok(r)=self.basilisk_button(id){if let Some(action)=crate::basilisk::decode(id,&r.args[3..10]){s.buttons.insert(id,action);if let Some(c)=self.caps.buttons.iter_mut().find(|c|c.id==id){c.remappable=id!=1;}}}}
        }
        if matches!(self.model.pid,0x00c0|0x00c1) {
            s.buttons.clear();
            for cap in &mut self.caps.buttons{cap.remappable=false;}
            for id in [1,2,4,5,9,10,0x60] {
                let Ok(response)=self.read_button(id) else {continue};
                if let Some(action)=viper_decode(id,&response.args){
                    s.buttons.insert(u16::from(id),action);
                    if let Some(cap)=self.caps.buttons.iter_mut().find(|b|b.id==u16::from(id)){cap.remappable=id!=1;}
                }
            }
        }

        if self.caps.dpi.is_some() {
            if let Some(d) = soft(self.read_dpi())? {
                s.dpi = Some(d);
            }
        }
        if !self.caps.report_rates.is_empty() {
            if let Some(r) = soft(self.read_rate())? {
                s.report_rate = Some(r);
            }
        }
        if crate::power::supported(self.model.pid){s.tuning=soft(crate::power::read(&mut self.link))?;}
        // Brightness is the only lighting value that can be read back (it
        // changes with the keyboard's hotkeys); effects and colours stay as
        // last applied, and zones never set are left out rather than guessed.
        let zones: Vec<u8> = s.lighting.keys().copied().collect();
        for zone in zones {
            if let Some(b) = soft(self.read_brightness(zone))? {
                if let Some(z) = s.lighting.get_mut(&zone) {
                    z.brightness = b;
                }
            }
        }
        if self.caps.battery {
            if let Some(b) = soft(self.read_battery())? {
                s.battery = Some(b);
            }
        }
        if self.caps.game_mode {
            if let Some(g) = soft(self.read_game_mode())? {
                s.game_mode = Some(g);
            }
        }
        self.check_board(false)?;
        self.online = true;
        self.state = s.clone();
        Ok(s)
    }

    fn apply(&mut self, setting: &Setting) -> Result<()> {
        self.check_board(true)?;
        match setting {
            Setting::Dpi(d) => self.set_dpi(d)?,
            Setting::DpiActiveStage(i) => self.set_active_stage(*i)?,
            Setting::ReportRate(hz) => self.set_rate(*hz)?,
            Setting::Lighting { zone, state } => self.set_lighting(*zone, *state)?,
            Setting::PerKey(colors) => self.set_per_key(colors)?,
            Setting::GameMode(on) => self.set_game_mode(*on)?,
            Setting::Button { id, action } => self.set_button(*id,action)?,
            Setting::OnboardProfile(_) => return Err(Error::Unsupported("onboard profiles")),
            Setting::LiftOff(_) => return Err(Error::Unsupported("lift-off distance")),
            Setting::AngleSnapping(_) => return Err(Error::Unsupported("angle snapping")),
            Setting::Tuning(patch) if crate::power::supported(self.model.pid)=>{if let Err(e)=crate::power::apply(&mut self.link,self.model.pid,patch){self.state.tuning=None;return Err(e);}},
            Setting::VendorProfile{..} | Setting::Tuning(_) => return Err(Error::Unsupported("mouse performance")),
        }
        setting.apply_to(&mut self.state);
        Ok(())
    }

    /// Refreshes the battery at most every 30 s and reports
    /// wireless devices dropping out or coming back.
    fn poll_events(&mut self) -> Vec<DeviceEvent> {
        let mut events = Vec::new();
        if !self.caps.battery {
            return events;
        }
        let due = self
            .last_battery_poll
            .is_none_or(|t| t.elapsed() >= self.battery_interval);
        if !due {
            return events;
        }
        self.last_battery_poll = Some(Instant::now());

        match self.read_battery() {
            Ok(b) => {
                if !self.online {
                    self.online = true;
                    events.push(DeviceEvent::Reconnected);
                }
                if self.state.battery != Some(b) {
                    self.state.battery = Some(b);
                    events.push(DeviceEvent::BatteryChanged(b));
                }
            }
            Err(Error::Timeout | Error::Disconnected)
                if self.model.connection == Connection::Wireless =>
            {
                if self.online {
                    self.online = false;
                    events.push(DeviceEvent::Disconnected);
                }
            }
            Err(e) => log::debug!("razer: battery poll failed: {e}"),
        }
        events
    }
}

/// What the app may show for `model`.
fn capabilities(model: &Model) -> Capabilities {
    Capabilities {
        dpi: model.dpi.map(|d| DpiCaps {
            min: 100,
            max: d.max,
            step: if matches!(model.pid,0x007a|0x007b|0x00a5|0x00a6|0x00c0|0x00c1|0x00aa|0x00ab){1}else{50},
            max_stages: if d.stages { MAX_STAGES as u8 } else { 1 },
            separate_xy: true,
        }),
        report_rates: model.polling.rates.to_vec(),
        tuning:crate::power::supported(model.pid).then(crate::power::caps),
        buttons: if crate::basilisk::supported(VID_RAZER,model.pid){crate::basilisk::BUTTONS.into_iter().map(|(id,name)|ButtonCaps{id,name:name.into(),remappable:id!=1,default_action:crate::basilisk::decode(id,&crate::basilisk::default_block(id).unwrap()).unwrap(),hotspot:None}).collect()}else if matches!(model.pid,0x00c0|0x00c1) {
            [(1,"左键"),(2,"右键"),(4,"后退"),(5,"前进"),(9,"滚轮向上"),(10,"滚轮向下"),(0x60,"底部 DPI 键")].into_iter().map(|(id,name)|ButtonCaps{id,name:name.into(),remappable:id!=1,default_action:viper_decode(id as u8,&viper_default(id as u8).unwrap()).unwrap(),hotspot:None}).collect()
        } else {Vec::new()},
        lighting: model
            .zones
            .iter()
            .map(|z| LightingZoneCaps {
                keys: Vec::new(),
                id: z.led,
                name: z.name.to_string(),
                effects: z.effects.to_vec(),
                brightness: true,
            })
            .collect(),
        per_key: model.matrix.map(|m| m.layout()),
        battery: model.battery,
        game_mode: model.game_mode.is_some(),
        ..Default::default()
    }
}

/// Serial and firmware queries may fail without making the device unusable:
/// firmware that rejects them, and wireless devices that are asleep.
fn identity(r: Result<Report>, wireless: bool) -> Result<Option<Report>> {
    match r {
        Ok(r) => Ok(Some(r)),
        Err(Error::Unsupported(_) | Error::Protocol(_)) => Ok(None),
        Err(Error::Timeout) if wireless => Ok(None),
        Err(e) => Err(e),
    }
}

/// A read the firmware refuses is left out of the refreshed state instead of
/// failing the whole refresh; transport errors still propagate.
fn soft<T>(r: Result<T>) -> Result<Option<T>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(e @ (Error::Unsupported(_) | Error::Protocol(_))) => {
            log::debug!("razer: read skipped: {e}");
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models;
    use crate::testutil::{Fake, INSTANT, sent};
    use gearwise_core::model::{DpiStage, EffectKind, LightingEffect, WaveDirection};
    use gearwise_core::transport::MockRecord;
    use std::sync::{Arc, Mutex};

    type Log = Arc<Mutex<Vec<MockRecord>>>;

    /// Open the model with `pid` against `fake`; the identity traffic of
    /// `open` is dropped from the returned log.
    fn open(pid: u16, fake: &Fake) -> (RazerDevice, Log) {
        let model = models::find(pid).unwrap();
        let fake=if crate::basilisk::supported(VID_RAZER,pid){fake.clone().on(5,0x84,2,&[1])}else{fake.clone()};
        let (io, log) = fake.transport();
        let link = Link::new(Box::new(io), model.transaction_id, INSTANT);
        let iface = HidInterfaceInfo {
            path: "test-path".into(),
            ..Default::default()
        };
        let dev = RazerDevice::open(model, link, &iface).unwrap();
        log.lock().unwrap().clear();
        (dev, log)
    }

    /// Wire bytes: report id, status, then `rest` (transaction id onwards),
    /// zero padded, with the checksum byte set.
    fn wire(rest: &[u8], crc: u8) -> Vec<u8> {
        let mut v = vec![0u8, 0];
        v.extend_from_slice(rest);
        v.resize(91, 0);
        v[89] = crc;
        v
    }

    fn stages(v: &[u16], active: u8) -> DpiState {
        DpiState {
            stages: v.iter().map(|&d| DpiStage::uniform(d)).collect(),
            active,
        }
    }

    #[test]
    fn every_model_accepts_everything_it_advertises() {
        for m in models::MODELS {
            let (mut dev, _) = open(m.pid, &Fake::default());
            let caps = dev.capabilities().clone();
            let ctx = m.name;
            if let Some(d) = &caps.dpi {
                let table: Vec<_> = (0..d.max_stages as u16)
                    .map(|i| DpiStage::uniform(d.min + 100 * i))
                    .collect();
                let last = table.len() as u8 - 1;
                dev.apply(&Setting::Dpi(DpiState {
                    stages: table,
                    active: last,
                }))
                .unwrap_or_else(|e| panic!("{ctx}: {e}"));
                let ends = DpiStage { x: d.max, y: d.min };
                dev.apply(&Setting::Dpi(DpiState {
                    stages: vec![ends],
                    active: 0,
                }))
                .unwrap_or_else(|e| panic!("{ctx}: {e}"));
                dev.apply(&Setting::DpiActiveStage(0))
                    .unwrap_or_else(|e| panic!("{ctx}: {e}"));
            }
            for &hz in &caps.report_rates {
                dev.apply(&Setting::ReportRate(hz))
                    .unwrap_or_else(|e| panic!("{ctx}: {hz} Hz: {e}"));
            }
            for z in &caps.lighting {
                for &kind in &z.effects {
                    let effect = LightingEffect::default_for(kind, Rgb::new(10, 20, 30));
                    let state = ZoneState {
                        effect,
                        brightness: 70,
                    };
                    dev.apply(&Setting::Lighting { zone: z.id, state })
                        .unwrap_or_else(|e| panic!("{ctx}: {kind:?}: {e}"));
                }
            }
            if let Some(layout) = &caps.per_key {
                let all = layout
                    .keys
                    .iter()
                    .map(|k| (k.id, Rgb::new(1, 2, 3)))
                    .collect();
                dev.apply(&Setting::PerKey(all))
                    .unwrap_or_else(|e| panic!("{ctx}: {e}"));
            }
            if caps.game_mode {
                dev.apply(&Setting::GameMode(true))
                    .unwrap_or_else(|e| panic!("{ctx}: {e}"));
            }
        }
    }

    #[test]
    fn devices_are_send() {
        fn is_send<T: Send>() {}
        is_send::<RazerDevice>();
    }

    #[test]
    fn single_resolution_mouse_sends_set_dpi() {
        // DeathAdder V2: transaction id 0x3f; 800 = 0x0320 for both axes.
        // crc = 07 ^ 04 ^ 05 ^ 01 ^ 03 ^ 20 ^ 03 ^ 20 = 0x07
        let fake = Fake::default().on(0x04, 0x85, 2, &[0, 0x03, 0x20, 0x03, 0x20]);
        let (mut dev, log) = open(0x0084, &fake);
        dev.apply(&Setting::Dpi(stages(&[800], 0))).unwrap();
        assert_eq!(
            sent(&log),
            [wire(
                &[
                    0x3f, 0, 0, 0, 0x07, 0x04, 0x05, 0x01, 0x03, 0x20, 0x03, 0x20
                ],
                0x07
            )]
        );
        // The refresh reads the resolution back as one stage.
        assert_eq!(dev.read_state().unwrap().dpi, Some(stages(&[800], 0)));
    }

    #[test]
    fn dpi_values_are_validated_before_anything_is_sent() {
        let (mut dev, log) = open(0x0084, &Fake::default());
        let bad = |dev: &mut RazerDevice, s: DpiState| dev.apply(&Setting::Dpi(s));
        assert!(
            matches!(
                bad(&mut dev, stages(&[800, 1600], 0)),
                Err(Error::InvalidValue(_))
            ),
            "one stage only"
        );
        assert!(
            matches!(bad(&mut dev, stages(&[50], 0)), Err(Error::InvalidValue(_))),
            "below minimum"
        );
        assert!(
            matches!(
                bad(&mut dev, stages(&[25_000], 0)),
                Err(Error::InvalidValue(_))
            ),
            "above DA V2 maximum"
        );
        assert!(
            matches!(
                bad(&mut dev, stages(&[800], 1)),
                Err(Error::InvalidValue(_))
            ),
            "active out of range"
        );
        assert!(matches!(
            bad(&mut dev, stages(&[], 0)),
            Err(Error::InvalidValue(_))
        ));
        assert!(sent(&log).is_empty());
    }

    #[test]
    fn stage_table_is_written_whole() {
        // DeathAdder V2 Pro: transaction id 0x3f. Same packet as the
        // command-level test: active stage 2 of 3, checksum 0xb2.
        let (mut dev, log) = open(0x007c, &Fake::default());
        let table = DpiState {
            stages: vec![
                DpiStage::uniform(800),
                DpiStage::uniform(1800),
                DpiStage { x: 3200, y: 6400 },
            ],
            active: 1,
        };
        dev.apply(&Setting::Dpi(table.clone())).unwrap();
        let want = wire(
            &[
                0x3f, 0, 0, 0, 0x26, 0x04, 0x06, 0x01, 0x02, 0x03, 0x00, 0x03, 0x20, 0x03, 0x20, 0,
                0, 0x01, 0x07, 0x08, 0x07, 0x08, 0, 0, 0x02, 0x0c, 0x80, 0x19, 0x00,
            ],
            0xb2,
        );
        assert_eq!(sent(&log), [want]);

        // Switching the active stage re-sends the table with the new index.
        log.lock().unwrap().clear();
        dev.apply(&Setting::DpiActiveStage(2)).unwrap();
        let reqs = sent(&log);
        assert_eq!(reqs.len(), 1);
        assert_eq!(
            &reqs[0][9..12],
            &[0x01, 0x03, 0x03],
            "store, active = stage 3, count"
        );
        assert_eq!(dev.state.dpi.as_ref().unwrap().active, 2);

        assert!(matches!(
            dev.apply(&Setting::DpiActiveStage(3)),
            Err(Error::InvalidValue(_))
        ));
        assert!(matches!(
            dev.apply(&Setting::Dpi(stages(&[800; 6], 0))),
            Err(Error::InvalidValue(_))
        ));
    }

    #[test]
    fn active_stage_without_a_known_table_reads_it_first() {
        let table = [
            1, 1, 2, //
            1, 0x01, 0x90, 0x01, 0x90, 0, 0, //
            2, 0x03, 0x20, 0x03, 0x20, 0, 0,
        ];
        let (mut dev, log) = open(0x007c, &Fake::default().on(0x04, 0x86, 2, &table));
        dev.apply(&Setting::DpiActiveStage(1)).unwrap();
        let reqs = sent(&log);
        assert_eq!((reqs[0][7], reqs[0][8]), (0x04, 0x86));
        assert_eq!((reqs[1][7], reqs[1][8]), (0x04, 0x06));
        assert_eq!(&reqs[1][9..12], &[0x01, 0x02, 0x02]);
    }

    #[test]
    fn legacy_polling_rate_is_one_command() {
        // DeathAdder V2, 500 Hz = code 02. crc = 01 ^ 00 ^ 05 ^ 02 = 0x06
        let (mut dev, log) = open(0x0084, &Fake::default().on(0x00, 0x85, 2, &[0x02]));
        dev.apply(&Setting::ReportRate(500)).unwrap();
        assert_eq!(
            sent(&log),
            [wire(&[0x3f, 0, 0, 0, 0x01, 0x00, 0x05, 0x02], 0x06)]
        );
        assert_eq!(dev.read_state().unwrap().report_rate, Some(500));
        assert!(matches!(
            dev.apply(&Setting::ReportRate(2000)),
            Err(Error::InvalidValue(_))
        ));
        assert!(matches!(
            dev.apply(&Setting::ReportRate(1)),
            Err(Error::InvalidValue(_))
        ));
    }

    #[test]
    fn hyper_polling_on_a_mouse_sends_both_selectors() {
        // DeathAdder V3, 8000 Hz = code 01. crc = 02 ^ 00 ^ 40 ^ sel ^ 01
        let (mut dev, log) = open(0x00b2, &Fake::default());
        dev.apply(&Setting::ReportRate(8000)).unwrap();
        assert_eq!(
            sent(&log),
            [
                wire(&[0x1f, 0, 0, 0, 0x02, 0x00, 0x40, 0x00, 0x01], 0x43),
                wire(&[0x1f, 0, 0, 0, 0x02, 0x00, 0x40, 0x01, 0x01], 0x42),
            ]
        );
    }

    #[test]
    fn hyper_polling_on_a_keyboard_sends_one_command() {
        let (mut dev, log) = open(0x026c, &Fake::default());
        dev.apply(&Setting::ReportRate(1000)).unwrap();
        assert_eq!(sent(&log).len(), 1);
        assert!(matches!(
            open(0x024e, &Fake::default())
                .0
                .apply(&Setting::ReportRate(1000)),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn static_colour_zone_sends_effect_then_brightness() {
        // BlackWidow V3 keys zone (LED 5), transaction id 0x3f.
        // Effect packet is the capture documented by openrazer (checksum 0xff);
        // brightness 100 % = 0xff, crc = 03 ^ 0f ^ 04 ^ 01 ^ 05 ^ ff = 0xf3.
        let (mut dev, log) = open(0x024e, &Fake::default());
        let st = ZoneState {
            effect: LightingEffect::Static {
                color: Rgb::new(255, 0, 0),
            },
            brightness: 100,
        };
        dev.apply(&Setting::Lighting { zone: 5, state: st })
            .unwrap();
        assert_eq!(
            sent(&log),
            [
                wire(
                    &[
                        0x3f, 0, 0, 0, 0x09, 0x0f, 0x02, 0x01, 0x05, 0x01, 0, 0, 0x01, 0xff, 0, 0
                    ],
                    0xff
                ),
                wire(&[0x3f, 0, 0, 0, 0x03, 0x0f, 0x04, 0x01, 0x05, 0xff], 0xf3),
            ]
        );
        assert_eq!(dev.state.lighting.get(&5), Some(&st));
    }

    #[test]
    fn lighting_requests_are_validated() {
        let (mut dev, log) = open(0x0084, &Fake::default()); // DA V2: logo + scroll, no wave
        let st = |effect| ZoneState {
            effect,
            brightness: 50,
        };
        let wave = LightingEffect::Wave {
            period_ms: 3000,
            direction: WaveDirection::LeftToRight,
        };
        assert!(matches!(
            dev.apply(&Setting::Lighting {
                zone: 4,
                state: st(wave)
            }),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            dev.apply(&Setting::Lighting {
                zone: 9,
                state: st(LightingEffect::Off)
            }),
            Err(Error::InvalidValue(_))
        ));
        let loud = ZoneState {
            effect: LightingEffect::Off,
            brightness: 101,
        };
        assert!(matches!(
            dev.apply(&Setting::Lighting {
                zone: 4,
                state: loud
            }),
            Err(Error::InvalidValue(_))
        ));
        assert!(sent(&log).is_empty());
        // Scroll wheel is LED 1 and does react.
        let fx = LightingEffect::Reactive {
            color: Rgb::WHITE,
            duration_ms: 500,
        };
        dev.apply(&Setting::Lighting {
            zone: 1,
            state: st(fx),
        })
        .unwrap();
        assert_eq!(sent(&log)[0][10], 0x01, "LED id");
    }

    #[test]
    fn mice_without_lighting_reject_it() {
        let (mut dev, _) = open(0x00c0, &Fake::default()); // Viper V3 Pro: no LEDs
        assert!(dev.capabilities().lighting.is_empty());
        let st = ZoneState {
            effect: LightingEffect::Off,
            brightness: 0,
        };
        assert!(matches!(
            dev.apply(&Setting::Lighting { zone: 4, state: st }),
            Err(Error::InvalidValue(_))
        ));
    }

    #[test]
    fn per_key_frame_is_sent_row_by_row_then_drawn() {
        // BlackWidow V3: 6 x 22 grid, Esc at column 1 of row 0.
        let (mut dev, log) = open(0x024e, &Fake::default());
        let mut keys = BTreeMap::new();
        keys.insert(0x29u16, Rgb::new(255, 0, 0));
        dev.apply(&Setting::PerKey(keys)).unwrap();

        let reqs = sent(&log);
        assert_eq!(reqs.len(), 7, "6 rows + draw");
        // Row 0: class 0f id 03 size 0x47; args 00 00 row=0 first=0 last=21, then
        // column 0 black and column 1 red.
        // crc = 47 ^ 0f ^ 03 ^ 15 ^ ff = 0xa1
        let mut head = vec![
            0x3f, 0, 0, 0, 0x47, 0x0f, 0x03, 0, 0, 0, 0, 0x15, 0, 0, 0, 0xff, 0, 0,
        ];
        assert_eq!(reqs[0], wire(&head, 0xa1));
        // Row 3: only the header differs (crc = 47 ^ 0f ^ 03 ^ 03 ^ 15 = 0x5d).
        head.truncate(11);
        head[9] = 3;
        head.extend_from_slice(&[0x15]);
        assert_eq!(reqs[3], wire(&head, 0x5d));
        // Draw: 0f 02, size 0c, args 00 00 08. crc = 0c ^ 0f ^ 02 ^ 08 = 0x09
        assert_eq!(
            reqs[6],
            wire(&[0x3f, 0, 0, 0, 0x0c, 0x0f, 0x02, 0x00, 0x00, 0x08], 0x09)
        );

        // A second update keeps the first key.
        log.lock().unwrap().clear();
        let mut more = BTreeMap::new();
        more.insert(0x3au16, Rgb::new(0, 255, 0)); // F1 at (0, 3)
        dev.apply(&Setting::PerKey(more)).unwrap();
        let row0 = &sent(&log)[0];
        assert_eq!(
            &row0[14 + 3..14 + 6],
            &[0xff, 0, 0],
            "Esc still red at column 1"
        );
        assert_eq!(
            &row0[14 + 9..14 + 12],
            &[0, 0xff, 0],
            "F1 green at column 3"
        );
        assert_eq!(dev.state.per_key.len(), 2);
    }

    #[test]
    fn per_key_rejects_keys_the_board_lacks() {
        let (mut dev, log) = open(0x0a24, &Fake::default()); // V3 Tenkeyless
        assert_eq!(dev.capabilities().per_key.as_ref().unwrap().keys.len(), 87);
        let mut numpad = BTreeMap::new();
        numpad.insert(0x5fu16, Rgb::WHITE);
        assert!(matches!(
            dev.apply(&Setting::PerKey(numpad)),
            Err(Error::InvalidValue(_))
        ));
        let mut not_a_key = BTreeMap::new();
        not_a_key.insert(0x1000u16, Rgb::WHITE);
        assert!(matches!(
            dev.apply(&Setting::PerKey(not_a_key)),
            Err(Error::InvalidValue(_))
        ));
        assert!(sent(&log).is_empty());
        // Mice have no per-key lighting.
        let (mut mouse, _) = open(0x0084, &Fake::default());
        assert!(matches!(
            mouse.apply(&Setting::PerKey(BTreeMap::new())),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn game_mode_uses_the_models_storage_flag() {
        // BlackWidow V3 persists it: args 01 08 01, crc = 03 ^ 03 ^ 00 ^ 01 ^ 08 ^ 01 = 0x08
        let (mut dev, log) = open(0x024e, &Fake::default());
        dev.apply(&Setting::GameMode(true)).unwrap();
        assert_eq!(
            sent(&log),
            [wire(
                &[0x3f, 0, 0, 0, 0x03, 0x03, 0x00, 0x01, 0x08, 0x01],
                0x08
            )]
        );
        // Huntsman V2 does not: args 00 08 01, crc = 03 ^ 03 ^ 00 ^ 00 ^ 08 ^ 01 = 0x09
        let (mut v2, log) = open(0x026c, &Fake::default());
        v2.apply(&Setting::GameMode(true)).unwrap();
        assert_eq!(
            sent(&log),
            [wire(
                &[0x1f, 0, 0, 0, 0x03, 0x03, 0x00, 0x00, 0x08, 0x01],
                0x09
            )]
        );
        assert!(matches!(
            open(0x0084, &Fake::default())
                .0
                .apply(&Setting::GameMode(true)),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn unsupported_settings_are_refused() {
        let (mut dev, log) = open(0x0084, &Fake::default());
        for s in [
            Setting::Button {
                id: 1,
                action: gearwise_core::model::ButtonAction::Disabled,
            },
            Setting::OnboardProfile(0),
            Setting::LiftOff(0),
            Setting::AngleSnapping(true),
        ] {
            assert!(matches!(dev.apply(&s), Err(Error::Unsupported(_))), "{s:?}");
        }
        assert!(sent(&log).is_empty());
        assert!(dev.capabilities().buttons.is_empty());
    }

    #[test]
    fn read_state_parses_what_the_device_reports() {
        // DeathAdder V2 Pro (wired): stage table, polling 500 Hz, 50 % charging.
        let fake = Fake::default()
            .on(
                0x04,
                0x86,
                2,
                &[
                    1, 2, 2, 1, 0x03, 0x20, 0x03, 0x20, 0, 0, 2, 0x06, 0x40, 0x06, 0x40, 0, 0,
                ],
            )
            .on(0x00, 0x85, 2, &[0x02])
            .on(0x07, 0x80, 2, &[0, 0x80])
            .on(0x07, 0x84, 2, &[0, 1]);
        let (mut dev, _) = open(0x007c, &fake);
        let s = dev.read_state().unwrap();
        let dpi = s.dpi.unwrap();
        assert_eq!(dpi.active, 1);
        assert_eq!(
            dpi.stages,
            vec![DpiStage::uniform(800), DpiStage::uniform(1600)]
        );
        assert_eq!(s.report_rate, Some(500));
        assert_eq!(
            s.battery,
            Some(Battery {
                percent: Some(50),
                charging: true
            })
        );
        assert_eq!(s.game_mode, None);
        assert!(
            s.lighting.is_empty(),
            "effects cannot be read back and are not invented"
        );
    }

    #[test]
    fn read_state_keeps_applied_effects_and_refreshes_their_brightness() {
        // Brightness hotkey on a keyboard: the device now reports 0x80 (50 %).
        let fake = Fake::default()
            .on(0x0f, 0x84, 2, &[1, 5, 0x80])
            .on(0x03, 0x80, 2, &[1, 8, 1]);
        let (mut dev, _) = open(0x024e, &fake);
        let fx = LightingEffect::ColorCycle { period_ms: 10_000 };
        dev.apply(&Setting::Lighting {
            zone: 5,
            state: ZoneState {
                effect: fx,
                brightness: 100,
            },
        })
        .unwrap();
        let s = dev.read_state().unwrap();
        assert_eq!(
            s.lighting[&5],
            ZoneState {
                effect: fx,
                brightness: 50
            }
        );
        assert_eq!(s.game_mode, Some(true));
    }

    #[test]
    fn firmware_refusals_do_not_fail_the_refresh() {
        // Polling reads are "not supported" (status 5): the field stays empty.
        let fake =
            Fake::default()
                .on(0x00, 0x85, 5, &[])
                .on(0x04, 0x85, 2, &[0, 0x03, 0x20, 0x03, 0x20]);
        let (mut dev, _) = open(0x0084, &fake);
        let s = dev.read_state().unwrap();
        assert_eq!(s.report_rate, None);
        assert!(s.dpi.is_some());
        // A dead link, on the other hand, is reported.
        let dead = Fake::default().on(0x04, 0x85, 4, &[]);
        let (mut dev, _) = open(0x0084, &dead);
        assert!(matches!(dev.read_state(), Err(Error::Timeout)));
    }

    #[test]
    fn capabilities_follow_the_model_table() {
        let (dev, _) = open(0x00ab, &Fake::default().on(5,0x84,2,&[1])); // Basilisk V3 Pro (wireless)
        let c = dev.capabilities();
        let dpi = c.dpi.as_ref().unwrap();
        assert_eq!(
            (dpi.min, dpi.max, dpi.max_stages, dpi.separate_xy),
            (100, 30_000, 5, true)
        );
        assert_eq!(c.report_rates, [125, 500, 1000]);
        assert!(c.battery && !c.game_mode && c.per_key.is_none());
        let zones: Vec<_> = c.lighting.iter().map(|z| (z.id, z.name.as_str())).collect();
        assert_eq!(zones, [(4, "Logo"), (1, "Scroll wheel")]);
        assert_eq!(
            c.lighting[0].effects,
            [
                EffectKind::Off,
                EffectKind::Static,
                EffectKind::ColorCycle,
                EffectKind::Wave
            ]
        );
        assert_eq!(dev.descriptor().connection, Connection::Wireless);

        let (kb, _) = open(0x028d, &Fake::default()); // BlackWidow V4 Pro
        assert_eq!(kb.capabilities().report_rates.last(), Some(&8000));
        assert!(kb.capabilities().per_key.is_some() && kb.capabilities().game_mode);
        assert!(kb.capabilities().dpi.is_none());
    }

    #[test]
    fn battery_polling_is_rate_limited() {
        let fake = Fake::default()
            .on(0x07, 0x80, 2, &[0, 0x80])
            .on(0x07, 0x84, 2, &[0, 0]);
        let (mut dev, log) = open(0x007d, &fake); // DeathAdder V2 Pro (wireless)
        let events = dev.poll_events();
        assert_eq!(
            events,
            [DeviceEvent::BatteryChanged(Battery {
                percent: Some(50),
                charging: false
            })]
        );
        let n = sent(&log).len();
        assert_eq!(n, 2);
        // Within the interval: no I/O at all, no events.
        assert!(dev.poll_events().is_empty());
        assert_eq!(sent(&log).len(), n);
        // Interval elapsed but nothing changed: I/O, no event.
        dev.set_battery_interval(Duration::ZERO);
        assert!(dev.poll_events().is_empty());
        assert_eq!(sent(&log).len(), n + 2);
    }

    #[test]
    fn devices_without_a_battery_never_poll() {
        let (mut dev, log) = open(0x0084, &Fake::default());
        dev.set_battery_interval(Duration::ZERO);
        assert!(dev.poll_events().is_empty());
        assert!(sent(&log).is_empty());
    }

    #[test]
    fn a_sleeping_wireless_mouse_is_reported_once_and_again_on_return() {
        let fake = Fake::default()
            .on(0x07, 0x80, 4, &[])
            .on(0x07, 0x80, 2, &[0, 0xff])
            .on(0x07, 0x84, 2, &[0, 0]);
        let (mut dev, _) = open(0x007d, &fake);
        dev.set_battery_interval(Duration::ZERO);
        assert_eq!(dev.poll_events(), [DeviceEvent::Disconnected]);
        // Now awake: the script moved on to success.
        assert_eq!(
            dev.poll_events(),
            [
                DeviceEvent::Reconnected,
                DeviceEvent::BatteryChanged(Battery {
                    percent: Some(100),
                    charging: false
                })
            ]
        );
        assert!(dev.poll_events().is_empty());
    }

    #[test]
    fn a_wired_timeout_is_not_a_disconnect() {
        let (mut dev, _) = open(0x007c, &Fake::default().on(0x07, 0x80, 4, &[]));
        dev.set_battery_interval(Duration::ZERO);
        assert!(dev.poll_events().is_empty());
        assert!(dev.poll_events().is_empty());
    }

    #[test]
    fn a_sleeping_wireless_mouse_can_still_be_opened() {
        let fake = Fake::default()
            .on(0x00, 0x82, 4, &[])
            .on(0x00, 0x81, 4, &[]);
        let (dev, _) = open(0x007d, &fake);
        assert_eq!(dev.descriptor().serial, None);
        assert_eq!(dev.descriptor().id, "razer:007d:test-path");
    }

    #[test]
    fn a_wired_device_that_does_not_answer_fails_to_open() {
        let model = models::find(0x0084).unwrap();
        let (io, _) = Fake::default().on(0x00, 0x82, 4, &[]).transport();
        let link = Link::new(Box::new(io), model.transaction_id, INSTANT);
        let iface = HidInterfaceInfo::default();
        assert!(matches!(
            RazerDevice::open(model, link, &iface),
            Err(Error::Timeout)
        ));
    }
}
