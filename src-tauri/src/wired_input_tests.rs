//! Actual SteelSeries driver + Service + recording runtime. All I/O stays in memory.
use super::*;
use crate::{
    host_macros::Source,
    input_identity::Fixture,
    macro_runtime::Runtime,
    raw_input::{Event, SourceInfo},
};
use gearwise_core::{
    model::{Modifiers, MouseButton},
    profile::{Macro, MacroMode, MacroStep},
    transport::{HidInterfaceInfo, MockBackend, MockRecord, MockReply, MockTransport},
    Device, Driver,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
#[derive(Clone, Default)]
struct Rec(Arc<Mutex<Vec<String>>>);
impl gearwise_input::InputSink for Rec {
    fn key_down(&mut self, k: u8) -> gearwise_core::Result<()> {
        self.0.lock().unwrap().push(format!("d{k}"));
        Ok(())
    }
    fn key_up(&mut self, k: u8) -> gearwise_core::Result<()> {
        self.0.lock().unwrap().push(format!("u{k}"));
        Ok(())
    }
    fn mouse_down(&mut self, _: MouseButton) -> gearwise_core::Result<()> {
        panic!("unexpected mouse output")
    }
    fn mouse_up(&mut self, _: MouseButton) -> gearwise_core::Result<()> {
        panic!("unexpected mouse output")
    }
    fn scroll(&mut self, _: i32, _: i32) -> gearwise_core::Result<()> {
        panic!("unexpected scroll")
    }
    fn media(&mut self, _: u16) -> gearwise_core::Result<()> {
        panic!("unexpected media")
    }
}
fn opened(pid: u16, unit: &str) -> (Box<dyn Device>, MockLog) {
    let iface = HidInterfaceInfo {
        path: format!("simulation:control:{unit}"),
        vendor_id: 0x1038,
        product_id: pid,
        interface_number: 0,
        usage_page: 0xffc0,
        usage: 1,
        serial: None,
        ..Default::default()
    };
    let transport = MockTransport::new(|_| MockReply::none());
    let log = transport.log();
    let slot = Mutex::new(Some(transport));
    let backend = MockBackend::new(vec![iface.clone()], move |_| {
        Ok(Box::new(slot.lock().unwrap().take().unwrap()))
    });
    (
        gearwise_steelseries::SteelSeriesDriver::with_gap(Duration::ZERO)
            .open(&iface, &backend)
            .unwrap()
            .remove(0),
        log,
    )
}
fn manager(devices: Vec<Box<dyn Device>>) -> DeviceManager {
    let mut manager = DeviceManager::new(
        Box::new(MockBackend::new(vec![], |_| {
            panic!("physical backend forbidden")
        })),
        vec![],
    );
    manager.add_virtual(devices);
    manager
}
fn metadata(pid: u16) -> Fixture {
    let sources = ["A", "B"]
        .map(|unit| SourceInfo {
            source: Source {
                vendor: 0x1038,
                product: pid,
                path: format!("simulation:keyboard:{unit}"),
            },
            serial: None,
            kind: "Keyboard report".into(),
        })
        .to_vec();
    let containers = [
        ("simulation:control:A".into(), [1; 16]),
        ("simulation:keyboard:A".into(), [1; 16]),
        ("simulation:control:B".into(), [2; 16]),
        ("simulation:keyboard:B".into(), [2; 16]),
    ]
    .into();
    Fixture {
        sources,
        containers,
    }
}
struct Case {
    service: Service,
    ids: [String; 2],
    logs: [MockLog; 2],
    runtime: Runtime,
    rec: Rec,
    root: PathBuf,
    fixture: Fixture,
}
impl Case {
    fn new(pid: u16) -> Self {
        let (a, la) = opened(pid, "A");
        let (b, lb) = opened(pid, "B");
        let ids = [a.descriptor().id.clone(), b.descriptor().id.clone()];
        assert_ne!(ids[0], ids[1]);
        let service = Service::default();
        *service.0.lock().unwrap() = Some(manager(vec![a, b]));
        let rec = Rec::default();
        let runtime = Runtime::simulation(Arc::new(Mutex::new(rec.clone())));
        let fixture = metadata(pid);
        runtime.identity_fixture(fixture.clone());
        Self {
            service,
            ids,
            logs: [la, lb],
            runtime,
            rec,
            root: std::env::temp_dir()
                .join(format!("open-hub-wired-{}", crate::profiles::new_id())),
            fixture,
        }
    }
    fn plan(&self, unit: usize) -> Plan {
        let view = self.service.read(&self.ids[unit], &self.runtime).unwrap();
        let mut settings = view
            .capabilities
            .buttons
            .iter()
            .filter(|b| b.id != 1)
            .map(|b| Setting::Button {
                id: b.id,
                action: match b.id {
                    2 => ButtonAction::Media(0xea),
                    4 => ButtonAction::Macro("unit-macro".into()),
                    5 => ButtonAction::Key {
                        usage: 7,
                        mods: Modifiers(2),
                    },
                    _ => b.default_action.clone(),
                },
            })
            .collect::<Vec<_>>();
        settings.insert(0, Setting::ReportRate(500));
        Plan {
            vendor: 0x1038,
            product: view.descriptor.product_id,
            settings,
            macros: vec![Macro {
                id: "unit-macro".into(),
                name: format!("unit-{unit}"),
                mode: MacroMode::Once,
                steps: vec![
                    MacroStep::KeyDown(6 + unit as u8),
                    MacroStep::KeyUp(6 + unit as u8),
                ],
            }],
            source: Some(self.fixture.sources[unit].source.clone()),
        }
    }
    fn apply(&self, unit: usize, plan: &Plan) -> Result<View, String> {
        self.service
            .apply_at(&self.ids[unit], plan, &self.runtime, &self.root, None)
    }
    fn press(&self, unit: usize) {
        for down in [true, false] {
            self.runtime.simulate_event(Event::Key {
                source: self.fixture.sources[unit].source.clone(),
                vkey: 0x7c,
                down,
            });
        }
    }
    fn wait(&self, n: usize) {
        let end = Instant::now() + Duration::from_secs(1);
        while self.rec.0.lock().unwrap().len() < n && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(self.rec.0.lock().unwrap().len(), n);
    }
    fn replace(&self) -> [MockLog; 2] {
        let (a, la) = opened(self.fixture.sources[0].source.product, "A");
        let (b, lb) = opened(self.fixture.sources[0].source.product, "B");
        *self.service.0.lock().unwrap() = Some(manager(vec![a, b]));
        [la, lb]
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        self.runtime.shutdown();
        for name in ["backups", "recovery"] {
            let dir = self.root.join(name);
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for e in entries {
                    let p = e.unwrap().path();
                    assert!(p.is_file());
                    std::fs::remove_file(p).unwrap();
                }
                std::fs::remove_dir(dir).unwrap();
            }
        }
        if self.root.exists() {
            std::fs::remove_dir(&self.root).unwrap();
        }
    }
}
#[test]
fn serialless_same_model_pairs_apply_only_to_own_collection_and_stop_one_without_stopping_other() {
    for pid in crate::steelseries::PRODUCTS {
        let c = Case::new(pid);
        let a = c.plan(0);
        let b = c.plan(1);
        let mut wrong = a.clone();
        wrong.source = b.source.clone();
        assert!(c.apply(0, &wrong).err().unwrap().contains("不属于"));
        assert!(c.logs.iter().all(|l| l.lock().unwrap().is_empty()));
        assert_eq!(c.runtime.status().source_count, 0);
        c.apply(0, &a).unwrap();
        c.apply(1, &b).unwrap();
        assert_eq!(c.runtime.status().source_count, 2);
        c.press(0);
        c.wait(2);
        c.press(1);
        c.wait(4);
        assert_eq!(*c.rec.0.lock().unwrap(), ["d6", "u6", "d7", "u7"]);
        c.logs[0].lock().unwrap().clear();
        c.logs[1].lock().unwrap().clear();
        c.service
            .stop_device_software(&c.ids[0], &c.runtime)
            .unwrap();
        let table = c.logs[0].lock().unwrap().clone();
        assert_eq!(table.len(), 1);
        let MockRecord::Write(table) = &table[0] else {
            panic!()
        };
        assert_eq!(&table[8..13], &[0x61, 0xea, 0, 0, 0]);
        assert_eq!(&table[18..23], &[4, 0, 0, 0, 0]);
        assert_eq!(&table[23..28], &[0x51, 0xe1, 7, 0, 0]);
        assert!(c.logs[1].lock().unwrap().is_empty());
        assert_eq!(c.runtime.status().source_count, 1);
        c.press(0);
        c.press(1);
        c.wait(6);
        assert_eq!(c.rec.0.lock().unwrap()[4..], ["d7", "u7"]);
        c.service
            .stop_device_software(&c.ids[1], &c.runtime)
            .unwrap();
    }
}
#[test]
fn missing_keyboard_and_reopened_control_keep_journal_and_never_restore_to_replacement() {
    for pid in crate::steelseries::PRODUCTS {
    for read_first in [false, true] {
        let c = Case::new(pid);
        c.apply(0, &c.plan(0)).unwrap();
        let replacement_logs = c.replace();
        let replacement = c
            .service
            .with(|m| {
                Ok(m.device_mut(&c.ids[0])
                    .unwrap()
                    .connection_key()
                    .unwrap()
                    .to_string())
            })
            .unwrap();
        assert!(!replacement.is_empty());
        if read_first {
            assert!(c
                .service
                .read(&c.ids[0], &c.runtime)
                .unwrap()
                .session
                .is_none());
            assert_eq!(c.runtime.status().source_count, 0);
        }
        assert!(c
            .service
            .stop_device_software(&c.ids[0], &c.runtime)
            .unwrap_err()
            .contains("连接已变化"));
        assert_eq!(c.runtime.status().source_count, 0);
        let state = c
            .service
            .with(|m| Ok(m.device_mut(&c.ids[0]).unwrap().read_state().unwrap()))
            .unwrap();
        assert!(state.buttons.is_empty());
        assert!(replacement_logs
            .iter()
            .all(|l| l.lock().unwrap().is_empty()));
        assert_eq!(
            crate::macro_recovery::load(&c.root.join("recovery"))
                .0
                .len(),
            1
        );
        c.press(0);
        assert!(c.rec.0.lock().unwrap().is_empty());
    }
    let c = Case::new(pid);
    c.apply(0, &c.plan(0)).unwrap();
    let mut missing = c.fixture.clone();
    missing.sources.remove(0);
    c.runtime.identity_fixture(missing);
    c.runtime.simulate_event(Event::Removed {
        path: c.fixture.sources[0].source.path.clone(),
    });
    c.logs[0].lock().unwrap().clear();
    assert!(c
        .service
        .stop_device_software(&c.ids[0], &c.runtime)
        .is_err());
    assert!(c.logs[0].lock().unwrap().is_empty());
    assert_eq!(
        crate::macro_recovery::load(&c.root.join("recovery"))
            .0
            .len(),
        1
    );
    c.press(0);
    assert!(c.rec.0.lock().unwrap().is_empty());
    }
}
#[test]
fn serialless_restart_never_auto_restores_and_reviewed_manual_recovery_keeps_every_custom_button() {
    let c = Case::new(0x1722);
    c.apply(0, &c.plan(0)).unwrap();
    c.runtime.shutdown();
    let fresh = Service::default();
    let (device, log) = opened(0x1722, "A");
    *fresh.0.lock().unwrap() = Some(manager(vec![device]));
    let runtime = Runtime::simulation(Arc::new(Mutex::new(c.rec.clone())));
    runtime.identity_fixture(c.fixture.clone());
    let dir = c.root.join("recovery");
    assert!(!fresh.recover_from(&dir, &runtime).unwrap().is_empty());
    assert!(log.lock().unwrap().is_empty());
    let list = fresh.recovery_list_from(&dir, &c.ids[0]).unwrap();
    assert_eq!(list.records.len(), 1);
    let r = &list.records[0];
    assert_eq!(r.record.version, 2);
    assert_eq!(r.record.settings.len(), 7);
    let source = &c.fixture.sources[0].source;
    assert!(fresh
        .restore_recovery_from(
            &dir,
            &c.ids[0],
            &r.key,
            &r.revision,
            &c.fixture.sources[1].source,
            true,
            &runtime
        )
        .is_err());
    assert!(fresh
        .restore_recovery_from(
            &dir,
            &c.ids[0],
            &r.key,
            &r.revision,
            source,
            false,
            &runtime
        )
        .is_err());
    assert!(log.lock().unwrap().is_empty());
    fresh
        .restore_recovery_from(&dir, &c.ids[0], &r.key, &r.revision, source, true, &runtime)
        .unwrap();
    assert_eq!(log.lock().unwrap().len(), 1);
    let state = fresh
        .with(|m| Ok(m.device_mut(&c.ids[0]).unwrap().read_state().unwrap()))
        .unwrap();
    assert_eq!(state.buttons[&2], ButtonAction::Media(0xea));
    assert_eq!(
        state.buttons[&5],
        ButtonAction::Key {
            usage: 7,
            mods: Modifiers(2)
        }
    );
    assert_eq!(
        state.buttons[&6],
        ButtonAction::Key {
            usage: 0x4e,
            mods: Modifiers(0)
        }
    );
    assert!(crate::macro_recovery::load(&dir).0.is_empty());
    assert!(c.rec.0.lock().unwrap().is_empty());
    runtime.shutdown();
}
#[test]
fn native_complete_table_validates_all_entries_before_one_send() {
    let (mut device, log) = opened(0x1722, "A");
    let table = (2..=8)
        .map(|id| {
            (
                id,
                ButtonAction::Key {
                    usage: 6,
                    mods: Modifiers(0),
                },
            )
        })
        .collect::<Vec<_>>();
    let mut invalid = table.clone();
    invalid[6].1 = ButtonAction::Media(0xffff);
    assert!(device.apply_button_table(&invalid).is_err());
    assert!(device.apply_button_table(&table[..6]).is_err());
    let mut duplicate = table.clone();
    duplicate[6].0 = 2;
    assert!(device.apply_button_table(&duplicate).is_err());
    assert!(log.lock().unwrap().is_empty());
    assert!(device.read_state().unwrap().buttons.is_empty());
    device.apply_button_table(&table).unwrap();
    assert_eq!(log.lock().unwrap().len(), 1);
    assert_eq!(device.read_state().unwrap().buttons.len(), 7);
}

#[test]
fn wired_live_browser_fixture_contains_two_real_driver_views_without_serial_or_persistent_owner() {
    let c = Case::new(0x1722);
    let views = c
        .ids
        .iter()
        .map(|id| c.service.read(id, &c.runtime).unwrap())
        .collect::<Vec<_>>();
    for v in &views {
        assert!(!v.descriptor.simulated);
        assert!(v.descriptor.serial.is_none());
        assert!(crate::profile_routing::serial_key(0x1038, 0x1722, None).is_empty());
    }
    if let Some(path) = std::env::var_os("OPEN_HUB_WIRED_INPUT_FIXTURE") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &serde_json::json!({"views":views,"sources":c.fixture.sources}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}
struct Short {
    writes: Arc<Mutex<usize>>,
}
impl gearwise_core::HidTransport for Short {
    fn write(&mut self, b: &[u8]) -> gearwise_core::Result<usize> {
        *self.writes.lock().unwrap() += 1;
        Ok(b.len() - 1)
    }
    fn send_feature_report(&mut self, _: &[u8]) -> gearwise_core::Result<()> {
        panic!("button table is an output")
    }
    fn read_timeout(&mut self, _: &mut [u8], _: i32) -> gearwise_core::Result<usize> {
        Ok(0)
    }
    fn get_feature_report(&mut self, _: &mut [u8]) -> gearwise_core::Result<usize> {
        panic!("no feature read")
    }
}
#[test]
fn native_complete_table_short_send_does_not_retry_or_commit_any_cached_button() {
    let writes = Arc::new(Mutex::new(0));
    let counter = writes.clone();
    let iface = HidInterfaceInfo {
        path: "simulation:short-table".into(),
        vendor_id: 0x1038,
        product_id: 0x1722,
        interface_number: 0,
        usage_page: 0xffc0,
        usage: 1,
        ..Default::default()
    };
    let backend = MockBackend::new(vec![iface.clone()], move |_| {
        Ok(Box::new(Short {
            writes: counter.clone(),
        }))
    });
    let mut d = gearwise_steelseries::SteelSeriesDriver::with_gap(Duration::ZERO)
        .open(&iface, &backend)
        .unwrap()
        .remove(0);
    let table = (2..=8)
        .map(|id| (id, ButtonAction::Disabled))
        .collect::<Vec<_>>();
    assert!(d.apply_button_table(&table).is_err());
    assert_eq!(*writes.lock().unwrap(), 1);
    assert!(d.read_state().unwrap().buttons.is_empty());
}

#[test]
#[ignore = "requires exact current wired input browser saves"]
fn wired_browser_profiles_persist_reload_and_run_independently_through_service_and_real_native_driver(
) {
    let path = std::env::var_os("OPEN_HUB_WIRED_INPUT_UI_PROFILES").unwrap();
    let profiles: Vec<crate::profiles::Profile> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(profiles.len(), 2);
    let c = Case::new(0x1722);
    let mut store = crate::profiles::ProfileStore::default();
    for mut p in profiles {
        crate::profile_transfer::validate(&p).unwrap();
        if p.id.is_empty() {
            p.id = crate::profiles::new_id();
        }
        store.upsert(p);
    }
    std::fs::create_dir(&c.root).unwrap();
    let file = c.root.join("profiles.json");
    store.save(&file).unwrap();
    let reloaded = crate::profiles::ProfileStore::load(&file);
    for (unit, name) in [(0, "A 独立宏"), (1, "B 独立宏")] {
        let p = reloaded
            .profiles
            .iter()
            .find(|p| {
                p.multi
                    .as_ref()
                    .is_some_and(|m| m.macros.iter().any(|m| m.name == name))
            })
            .unwrap();
        let plan = p.multi.as_ref().unwrap();
        assert_eq!(plan.source.as_ref(), Some(&c.fixture.sources[unit].source));
        assert_eq!(plan.macros[0].steps[0], MacroStep::KeyDown(6 + unit as u8));
        c.apply(unit, plan).unwrap();
    }
    c.press(0);
    c.wait(2);
    c.press(1);
    c.wait(4);
    assert_eq!(*c.rec.0.lock().unwrap(), ["d6", "u6", "d7", "u7"]);
    c.service
        .stop_device_software(&c.ids[0], &c.runtime)
        .unwrap();
    assert_eq!(c.runtime.status().source_count, 1);
    c.service
        .stop_device_software(&c.ids[1], &c.runtime)
        .unwrap();
    std::fs::remove_file(file).unwrap();
}
