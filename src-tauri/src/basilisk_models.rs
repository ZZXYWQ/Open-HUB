//! Complete native/UI/profile/macro paths over synthetic firmware. No physical HID.
use super::*;
use gearwise_core::{
    model::{DpiStage, DpiState, LightingEffect, Modifiers, MouseButton, Rgb, ZoneState},
    profile::{Macro, MacroMode, MacroStep},
    transport::MockBackend,
    Device,
};
fn plan(product: u16) -> Plan {
    Plan {
        vendor: 0x1532,
        product,
        source: None,
        macros: vec![],
        settings: vec![
            Setting::Dpi(DpiState {
                stages: vec![DpiStage::uniform(801), DpiStage { x: 1601, y: 2401 }],
                active: 1,
            }),
            Setting::ReportRate(500),
            Setting::Button {
                id: 96,
                action: ButtonAction::Key {
                    usage: 23,
                    mods: Modifiers(3),
                },
            },
            Setting::Button {
                id: 52,
                action: ButtonAction::Mouse(MouseButton::WheelRight),
            },
            Setting::Lighting {
                zone: 4,
                state: ZoneState {
                    effect: LightingEffect::Static {
                        color: Rgb::new(7, 20, 110),
                    },
                    brightness: 60,
                },
            },
        ],
    }
}
#[test]
fn both_basilisk_transports_roundtrip_profiles_and_real_dpi_buttons_lighting() {
    let mut fixtures = Vec::new();
    for product in [0x00aa, 0x00ab] {
        let (mut device, _) = simulated(0x1532, product).unwrap();
        let base = view(device.as_mut()).unwrap();
        fixtures.push(serde_json::to_value(base).unwrap());
        let p = plan(product);
        validate(&p).unwrap();
        let c = compile(&p).unwrap();
        for s in &c.settings {
            device.apply(s).unwrap();
        }
        let state = device.read_state().unwrap();
        assert_eq!(
            state.dpi,
            if let Setting::Dpi(d) = &p.settings[0] {
                Some(d.clone())
            } else {
                None
            }
        );
        assert_eq!(state.report_rate, Some(500));
        assert_eq!(
            state.buttons[&96],
            ButtonAction::Key {
                usage: 23,
                mods: Modifiers(3)
            }
        );
        assert_eq!(
            state.buttons[&52],
            ButtonAction::Mouse(MouseButton::WheelRight)
        );
        assert_eq!(state.lighting[&4].brightness, 60);
        let result = simulate(&p).unwrap();
        assert_eq!(result["hardware_tested"], false);
        assert!(result["packet_count"].as_u64().unwrap() > 20);
    }
    if let Some(path) = std::env::var_os("OPEN_HUB_BASILISK_FIXTURE") {
        std::fs::write(path, serde_json::to_vec_pretty(&fixtures).unwrap()).unwrap();
    }
}
#[test]
fn basilisk_model_layer_and_macro_limits_are_enforced_before_io() {
    for product in [0x00aa, 0x00ab] {
        let mut p = plan(product);
        for id in [1, 64, 255] {
            p.settings = vec![Setting::Button {
                id,
                action: ButtonAction::Disabled,
            }];
            assert!(validate(&p).is_err());
        }
        p = plan(product);
        p.macros = vec![Macro {
            id: "m".into(),
            name: "m".into(),
            mode: MacroMode::WhileHeld,
            steps: vec![MacroStep::KeyDown(4), MacroStep::KeyUp(4)],
        }];
        assert!(validate(&p).is_err());
    }
    assert!(!supported(0x1532, 0x0099));
    assert!(!supported(0x1532, 0x00cc));
    assert!(!supported(0x046d, 0x00ab));
}
struct Identified {
    inner: Box<dyn Device>,
    desc: DeviceDescriptor,
}
impl Device for Identified {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }
    fn capabilities(&self) -> &Capabilities {
        self.inner.capabilities()
    }
    fn read_state(&mut self) -> gearwise_core::Result<DeviceState> {
        self.inner.read_state()
    }
    fn apply(&mut self, s: &Setting) -> gearwise_core::Result<()> {
        self.inner.apply(s)
    }
}
#[derive(Clone, Default)]
struct Rec(Arc<Mutex<Vec<u8>>>);
impl gearwise_input::InputSink for Rec {
    fn key_down(&mut self, k: u8) -> gearwise_core::Result<()> {
        self.0.lock().unwrap().push(k);
        Ok(())
    }
    fn key_up(&mut self, k: u8) -> gearwise_core::Result<()> {
        self.0.lock().unwrap().push(k | 0x80);
        Ok(())
    }
    fn mouse_down(&mut self, _: MouseButton) -> gearwise_core::Result<()> {
        Ok(())
    }
    fn mouse_up(&mut self, _: MouseButton) -> gearwise_core::Result<()> {
        Ok(())
    }
    fn scroll(&mut self, _: i32, _: i32) -> gearwise_core::Result<()> {
        Ok(())
    }
    fn media(&mut self, _: u16) -> gearwise_core::Result<()> {
        Ok(())
    }
}
fn source(unit: &str) -> crate::host_macros::Source {
    crate::host_macros::Source {
        vendor: 0x1532,
        product: 0x00ab,
        path: format!("simulation:basilisk-input:{unit}"),
    }
}
fn macro_plan(unit: &str, key: u8, mode: MacroMode) -> Plan {
    Plan {
        vendor: 0x1532,
        product: 0x00ab,
        source: Some(source(unit)),
        settings: vec![Setting::Button {
            id: 15,
            action: ButtonAction::Macro("m".into()),
        }],
        macros: vec![Macro {
            id: "m".into(),
            name: "test".into(),
            mode,
            steps: vec![
                MacroStep::KeyDown(key),
                MacroStep::Delay(5),
                MacroStep::KeyUp(key),
            ],
        }],
    }
}
fn wait(rec: &Rec, n: usize) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while rec.0.lock().unwrap().len() < n && std::time::Instant::now() < end {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(rec.0.lock().unwrap().len(), n);
}
#[test]
fn two_basilisks_run_and_restore_only_their_own_macros_and_original_clutch() {
    let mut manager = DeviceManager::new(
        Box::new(MockBackend::new(vec![], |_| {
            Err(gearwise_core::Error::Unsupported("no hardware"))
        })),
        vec![],
    );
    for unit in ["a", "b"] {
        let (inner, _) = simulated(0x1532, 0x00ab).unwrap();
        let mut desc = inner.descriptor().clone();
        desc.id = format!("basilisk:{unit}:board:1");
        desc.serial = Some(unit.into());
        manager.add_virtual(vec![Box::new(Identified { inner, desc })]);
    }
    let service = Service::default();
    *service.0.lock().unwrap() = Some(manager);
    let rec = Rec::default();
    let runtime = crate::macro_runtime::Runtime::simulation(Arc::new(Mutex::new(rec.clone())));
    let root = std::env::temp_dir().join(format!(
        "basilisk-independent-{}",
        crate::profiles::new_id()
    ));
    for (unit, key) in [("a", 4), ("b", 5)] {
        service
            .apply_at(
                &format!("basilisk:{unit}:board:1"),
                &macro_plan(unit, key, MacroMode::Once),
                &runtime,
                &root,
                None,
            )
            .unwrap();
    }
    runtime.simulate_event(crate::raw_input::Event::Key {
        source: source("a"),
        vkey: 0x7c,
        down: true,
    });
    wait(&rec, 2);
    assert_eq!(*rec.0.lock().unwrap(), vec![4, 132]);
    runtime.simulate_event(crate::raw_input::Event::Key {
        source: source("a"),
        vkey: 0x7c,
        down: false,
    });
    service
        .stop_device_software("basilisk:a:board:1", &runtime)
        .unwrap();
    runtime.simulate_event(crate::raw_input::Event::Key {
        source: source("a"),
        vkey: 0x7c,
        down: true,
    });
    assert_eq!(rec.0.lock().unwrap().len(), 2);
    runtime.simulate_event(crate::raw_input::Event::Key {
        source: source("b"),
        vkey: 0x7c,
        down: true,
    });
    wait(&rec, 4);
    runtime.simulate_event(crate::raw_input::Event::Key {
        source: source("b"),
        vkey: 0x7c,
        down: false,
    });
    service
        .stop_device_software("basilisk:b:board:1", &runtime)
        .unwrap();
    service
        .with(|m| {
            for unit in ["a", "b"] {
                assert_eq!(
                    m.device_mut(&format!("basilisk:{unit}:board:1"))
                        .unwrap()
                        .read_state()
                        .unwrap()
                        .buttons[&15],
                    ButtonAction::Default
                );
            }
            Ok(())
        })
        .unwrap();
    assert!(crate::macro_recovery::load(&root.join("recovery"))
        .0
        .is_empty());
    runtime.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn basilisk_recovery_requires_original_bank_and_serial() {
    let (device, _) = simulated(0x1532, 0x00ab).unwrap();
    let d = device.descriptor().clone();
    let mut r = crate::macro_recovery::Record {
        version: 1,
        device_id: d.id.clone(),
        serial: d.serial.clone(),
        source: source("a"),
        settings: vec![Setting::Button {
            id: 15,
            action: ButtonAction::Default,
        }],
        expected: std::collections::HashMap::from([(
            15,
            ButtonAction::Key {
                usage: 104,
                mods: Modifiers(0),
            },
        )]),
        software_input: false,
        shift: None,
    };
    assert!(r.matches(&d));
    for bank in [0, 2, 5, 6] {
        r.device_id = format!("old:board:{bank}");
        assert!(!r.matches(&d));
    }
    r.device_id = d.id.clone();
    r.serial = Some("other".into());
    assert!(!r.matches(&d));
}
#[test]
#[ignore = "requires current browser-created Basilisk profiles"]
fn browser_basilisk_profiles_reload_import_and_execute_native_drivers() {
    let path = std::env::var_os("OPEN_HUB_BASILISK_UI_PROFILES").unwrap();
    let profiles: Vec<crate::profiles::Profile> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(profiles.len(), 6);
    let path =
        std::env::temp_dir().join(format!("basilisk-pack-{}.json", crate::profiles::new_id()));
    std::fs::write(
        &path,
        serde_json::to_vec(
            &serde_json::json!({"format":"open-hub-profiles/1","profiles":profiles}),
        )
        .unwrap(),
    )
    .unwrap();
    let preview =
        crate::profile_transfer::preview("openhub", Some(path.to_string_lossy().into())).unwrap();
    assert_eq!(preview.profiles.len(), 6);
    for p in preview.profiles {
        crate::profile_transfer::validate(&p).unwrap();
        assert!(!p.targets(0x046d, &[0x00ab]));
        let result = simulate(p.multi.as_ref().unwrap()).unwrap();
        assert_eq!(result["hardware_tested"], false);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn basilisk_both_led_zones_all_native_effects_and_macro_modes_execute() {
    for product in [0x00aa, 0x00ab] {
        let (mut d, log) = simulated(0x1532, product).unwrap();
        for zone in [1, 4] {
            for effect in [
                LightingEffect::Off,
                LightingEffect::Static {
                    color: Rgb::new(1, 2, 3),
                },
                LightingEffect::ColorCycle { period_ms: 4000 },
                LightingEffect::Wave {
                    period_ms: 3000,
                    direction: gearwise_core::model::WaveDirection::LeftToRight,
                },
            ] {
                let state = ZoneState {
                    effect,
                    brightness: 80,
                };
                d.apply(&Setting::Lighting { zone, state }).unwrap();
                assert_eq!(d.read_state().unwrap().lighting[&zone], state);
            }
        }
        for mode in [MacroMode::Once, MacroMode::Toggle] {
            let mut p = macro_plan("a", 4, mode);
            p.product = product;
            p.source = None;
            validate(&p).unwrap();
            let r = simulate(&p).unwrap();
            assert!(r["software_macros"].as_array().unwrap().len() == 1);
        }
        log.lock().unwrap().clear();
        assert!(d
            .apply(&Setting::Lighting {
                zone: 4,
                state: ZoneState {
                    effect: LightingEffect::Breathing {
                        color: Rgb::new(1, 2, 3),
                        period_ms: 4000
                    },
                    brightness: 100
                }
            })
            .is_err());
        assert!(!log.lock().unwrap().iter().any(
            |r| matches!(r,gearwise_core::transport::MockRecord::SetFeature(b)if b[8]&128==0)
        ));
    }
}
