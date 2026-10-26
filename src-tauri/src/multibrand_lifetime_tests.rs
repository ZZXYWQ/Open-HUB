//! Service lifetime tests over the production Naga B4 driver and synthetic HID.
//! Runtime::simulation has no physical identity fixture here: these tests cover
//! routing ownership and write sequencing. Serial/container proof is covered by
//! input_identity's separate tests. No OS input or physical HID is opened.
use super::*;
use crate::{host_macros::Source, macro_runtime::Runtime, raw_input::Event};
use gearwise_core::{
    model::{Modifiers, MouseButton},
    profile::{Macro, MacroMode, MacroStep},
    transport::{MockBackend, MockRecord},
    Device, Error,
};
use gearwise_input::InputSink;
use std::{collections::HashMap, path::PathBuf, time::{Duration, Instant}};

#[derive(Clone, Default)]
struct Recording(Arc<Mutex<Vec<String>>>);
impl InputSink for Recording {
    fn key_down(&mut self, key: u8) -> gearwise_core::Result<()> {
        self.0.lock().unwrap().push(format!("d{key}")); Ok(())
    }
    fn key_up(&mut self, key: u8) -> gearwise_core::Result<()> {
        self.0.lock().unwrap().push(format!("u{key}")); Ok(())
    }
    fn mouse_down(&mut self, _: MouseButton) -> gearwise_core::Result<()> { panic!("unexpected mouse output") }
    fn mouse_up(&mut self, _: MouseButton) -> gearwise_core::Result<()> { panic!("unexpected mouse output") }
    fn scroll(&mut self, _: i32, _: i32) -> gearwise_core::Result<()> { panic!("unexpected scroll output") }
    fn media(&mut self, _: u16) -> gearwise_core::Result<()> { panic!("unexpected media output") }
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("open-hub-lifetime-{}", crate::profiles::new_id()));
        std::fs::create_dir_all(&path).unwrap(); Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

#[derive(Clone, Default)]
struct ApplyHook {
    applied: Arc<Mutex<Vec<Setting>>>,
    event: Arc<Mutex<Option<(Runtime, Event)>>>,
}
impl ApplyHook {
    fn arm(&self, runtime: &Runtime, event: Event) {
        self.applied.lock().unwrap().clear();
        *self.event.lock().unwrap() = Some((runtime.clone(), event));
    }
    fn after(&self, setting: &Setting) {
        self.applied.lock().unwrap().push(setting.clone());
        let event = self.event.lock().unwrap().take();
        if let Some((runtime, event)) = event { runtime.simulate_event(event); }
    }
}

struct IdentifiedNaga {
    inner: Box<dyn Device>,
    descriptor: DeviceDescriptor,
    hook: ApplyHook,
}
impl Device for IdentifiedNaga {
    fn control_path(&self) -> Option<&str> { self.inner.control_path() }
    fn connection_key(&self) -> Option<&str> { self.inner.connection_key() }
    fn descriptor(&self) -> &DeviceDescriptor { &self.descriptor }
    fn capabilities(&self) -> &Capabilities { self.inner.capabilities() }
    fn read_state(&mut self) -> gearwise_core::Result<DeviceState> { self.inner.read_state() }
    fn apply(&mut self, setting: &Setting) -> gearwise_core::Result<()> {
        self.inner.apply(setting)?;
        self.hook.after(setting);
        Ok(())
    }
    fn stop_software_actions(&mut self) -> gearwise_core::Result<()> { self.inner.stop_software_actions() }
    fn restore_hardware_input(&mut self) -> gearwise_core::Result<()> { self.inner.restore_hardware_input() }
}
type HidLog = Arc<Mutex<Vec<MockRecord>>>;
fn key(usage: u8) -> ButtonAction { ButtonAction::Key { usage, mods: Modifiers(0) } }
fn button(id: u16, usage: u8) -> Setting { Setting::Button { id, action: key(usage) } }
fn naga(id: &str, serial: &str) -> (Box<dyn Device>, HidLog, ApplyHook) {
    let (mut inner, log) = crate::naga::simulated();
    inner.apply(&button(64, 4)).unwrap();
    inner.apply(&button(65, 5)).unwrap();
    let mut descriptor = inner.descriptor().clone();
    descriptor.id = id.into(); descriptor.serial = Some(serial.into());
    let hook = ApplyHook::default();
    log.lock().unwrap().clear();
    (Box::new(IdentifiedNaga { inner, descriptor, hook: hook.clone() }), log, hook)
}
fn service(devices: Vec<Box<dyn Device>>) -> Service {
    let backend = MockBackend::new(vec![], |_| Err(Error::Unsupported("no physical HID in lifetime test")));
    let mut manager = DeviceManager::new(Box::new(backend), vec![]);
    manager.add_virtual(devices);
    let service = Service::default();
    *service.0.lock().unwrap() = Some(manager);
    service
}
fn source() -> Source { Source { vendor: 0x1532, product: 0x00b4, path: "simulation:lifetime:shared".into() } }
fn plan(source: &Source, usages: &[u8]) -> Plan {
    let macros = usages.iter().enumerate().map(|(index, usage)| Macro {
        id: format!("lifetime-{index}"), name: format!("Lifetime {index}"), mode: MacroMode::Once,
        steps: vec![MacroStep::KeyDown(*usage), MacroStep::KeyUp(*usage)],
    }).collect::<Vec<_>>();
    Plan { vendor: 0x1532, product: 0x00b4,
        settings: macros.iter().enumerate().map(|(index, action)| Setting::Button {
            id: 64 + index as u16, action: ButtonAction::Macro(action.id.clone()),
        }).collect(), macros, source: Some(source.clone()) }
}
fn runtime() -> (Runtime, Recording) {
    let recording = Recording::default();
    (Runtime::simulation(Arc::new(Mutex::new(recording.clone()))), recording)
}
fn journal(service: &Service, id: &str) -> PathBuf {
    service.1.lock().unwrap().get(id).unwrap().journal.as_ref().unwrap().path.clone()
}
fn writes(log: &HidLog) -> Vec<u16> {
    log.lock().unwrap().iter().filter_map(|record| match record {
        MockRecord::SetFeature(packet) if packet.len() == 91 && packet[7] == 2 && packet[8] == 0x0c => Some(u16::from(packet[10])),
        _ => None,
    }).collect()
}
fn assert_buttons(service: &Service, id: &str, first: u8, second: u8) {
    let state = service.with(|manager| manager.device_mut(id).unwrap().read_state().map_err(|e| e.to_string())).unwrap();
    assert_eq!(state.buttons.get(&64), Some(&key(first)));
    assert_eq!(state.buttons.get(&65), Some(&key(second)));
}
fn fire(runtime: &Runtime, recording: &Recording, source: &Source, total: usize) {
    runtime.simulate_event(Event::Key { source: source.clone(), vkey: 0x7c, down: true });
    let deadline = Instant::now() + Duration::from_secs(2);
    while recording.0.lock().unwrap().len() < total && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    runtime.simulate_event(Event::Key { source: source.clone(), vkey: 0x7c, down: false });
    assert_eq!(recording.0.lock().unwrap().len(), total);
}

#[test]
fn stale_control_cleanup_and_session_mismatch_cannot_cancel_reused_source_owner() {
    let scratch = Scratch::new(); let source = source(); let (runtime, recording) = runtime();
    let (a, a_log, _) = naga("lifetime:A", "unit-A");
    let service = service(vec![a]);
    let mut old_view = service.apply_at("lifetime:A", &plan(&source, &[4]), &runtime, &scratch.0, None).unwrap();
    let a_journal = journal(&service, "lifetime:A");
    runtime.simulate_event(Event::Removed { path: source.path.clone() });
    let (b, b_log, _) = naga("lifetime:B", "unit-B");
    service.with(|manager| { manager.drop_device("lifetime:A"); manager.add_virtual(vec![b]); Ok(()) }).unwrap();
    runtime.simulate_event(Event::Arrived { source: source.clone() });
    service.apply_at("lifetime:B", &plan(&source, &[5]), &runtime, &scratch.0, None).unwrap();
    let b_cancelled = service.1.lock().unwrap().get("lifetime:B").unwrap().cancelled.clone();
    a_log.lock().unwrap().clear(); b_log.lock().unwrap().clear();

    assert!(service.stop_device_software("lifetime:A", &runtime).is_err());
    assert!(a_journal.exists());
    assert!(runtime.is_armed(&source)); assert!(!b_cancelled.load(Ordering::SeqCst));
    assert!(a_log.lock().unwrap().is_empty()); assert!(b_log.lock().unwrap().is_empty());
    fire(&runtime, &recording, &source, 2);

    // Exercise the cached-A connection-mismatch branch after B owns this path.
    old_view.connection_key = Some("different-open-instance-A".into());
    service.attach_session(&mut old_view, &runtime).unwrap();
    assert!(old_view.session.is_none()); assert!(a_journal.exists());
    assert!(runtime.is_armed(&source)); assert!(!b_cancelled.load(Ordering::SeqCst));
    assert!(a_log.lock().unwrap().is_empty()); assert!(b_log.lock().unwrap().is_empty());
    fire(&runtime, &recording, &source, 4);
    assert_eq!(*recording.0.lock().unwrap(), ["d5", "u5", "d5", "u5"]);
    assert!(service.1.lock().unwrap().contains_key("lifetime:A"));
    runtime.shutdown();
}

#[derive(Clone, Copy, Debug)]
enum Invalidation { Removed, Stopped }
impl Invalidation {
    fn event(self, source: &Source) -> Event { match self {
        Self::Removed => Event::Removed { path: source.path.clone() }, Self::Stopped => Event::Stopped,
    } }
}
#[derive(Clone, Copy, Debug)]
enum RestoreEntry { RestoreOne, StopDevice }

fn live_restore_stops_after_first_setting(invalidation: Invalidation, entry: RestoreEntry) {
    let scratch = Scratch::new(); let source = source(); let (runtime, _) = runtime();
    let (device, log, hook) = naga("lifetime:restore", "unit-restore");
    let service = service(vec![device]);
    service.apply_at("lifetime:restore", &plan(&source, &[6, 7]), &runtime, &scratch.0, None).unwrap();
    let journal = journal(&service, "lifetime:restore");
    hook.arm(&runtime, invalidation.event(&source)); log.lock().unwrap().clear();
    let result = match entry {
        RestoreEntry::RestoreOne => service.with(|manager| service.restore_one(manager, "lifetime:restore", &runtime)),
        RestoreEntry::StopDevice => service.stop_device_software("lifetime:restore", &runtime),
    };
    assert!(result.is_err(), "{entry:?} / {invalidation:?}");
    assert_eq!(*hook.applied.lock().unwrap(), [button(64, 4)]);
    assert_eq!(writes(&log), [64]);
    assert!(journal.exists()); assert!(service.1.lock().unwrap().contains_key("lifetime:restore"));
    assert_buttons(&service, "lifetime:restore", 4, 0x69);
    assert!(!runtime.is_armed(&source));
    let before_retry = writes(&log);
    assert!(service.stop_device_software("lifetime:restore", &runtime).is_err());
    assert_eq!(writes(&log), before_retry); assert!(journal.exists());
    runtime.shutdown();
}
#[test]
fn restore_one_stops_after_removed_during_first_native_restore() {
    live_restore_stops_after_first_setting(Invalidation::Removed, RestoreEntry::RestoreOne);
}
#[test]
fn restore_one_stops_after_stopped_during_first_native_restore() {
    live_restore_stops_after_first_setting(Invalidation::Stopped, RestoreEntry::RestoreOne);
}
#[test]
fn stop_device_stops_after_removed_during_first_native_restore() {
    live_restore_stops_after_first_setting(Invalidation::Removed, RestoreEntry::StopDevice);
}
#[test]
fn stop_device_stops_after_stopped_during_first_native_restore() {
    live_restore_stops_after_first_setting(Invalidation::Stopped, RestoreEntry::StopDevice);
}

fn manual_restore_stops_after_first_setting(invalidation: Invalidation) {
    let scratch = Scratch::new(); let source = source(); let (runtime, _) = runtime();
    let (device, log, hook) = naga("lifetime:manual", "unit-manual");
    let service = service(vec![device]);
    service.with(|manager| {
        let device = manager.device_mut("lifetime:manual").unwrap();
        device.apply(&button(64, 0x68)).map_err(|e| e.to_string())?;
        device.apply(&button(65, 0x69)).map_err(|e| e.to_string())
    }).unwrap();
    let dir = scratch.0.join("recovery");
    let journal = crate::macro_recovery::Journal::create(&dir, crate::macro_recovery::Record {
        version: 1, device_id: "lifetime:manual".into(), serial: Some("unit-manual".into()),
        source: source.clone(), settings: vec![button(64, 4), button(65, 5)],
        expected: HashMap::from([(64, key(0x68)), (65, key(0x69))]), software_input: false, shift: None,
    }).unwrap();
    let review = service.recovery_list_from(&dir, "lifetime:manual").unwrap();
    assert!(review.errors.is_empty()); assert_eq!(review.records.len(), 1);
    let item = &review.records[0];
    hook.arm(&runtime, invalidation.event(&source)); log.lock().unwrap().clear();
    assert!(service.restore_recovery_from(&dir, "lifetime:manual", &item.key, &item.revision,
        &source, true, &runtime).is_err());
    assert_eq!(*hook.applied.lock().unwrap(), [button(64, 4)]);
    assert_eq!(writes(&log), [64]);
    assert_buttons(&service, "lifetime:manual", 4, 0x69);
    assert!(journal.path.exists()); assert_eq!(journal.record().unwrap().settings.len(), 2);
    let retained = service.recovery_list_from(&dir, "lifetime:manual").unwrap();
    assert_eq!(retained.records.len(), 1); assert_eq!(retained.records[0].key, item.key);
    runtime.shutdown();
}
#[test]
fn confirmed_manual_restore_stops_after_removed_during_first_native_restore() {
    manual_restore_stops_after_first_setting(Invalidation::Removed);
}
#[test]
fn confirmed_manual_restore_stops_after_stopped_during_first_native_restore() {
    manual_restore_stops_after_first_setting(Invalidation::Stopped);
}
