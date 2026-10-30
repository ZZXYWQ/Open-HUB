//! Original wired Prime: production bridge/native codec over memory HID only.
use super::*;
use gearwise_core::{
    model::{DpiStage, DpiState, Modifiers, Rgb},
    profile::{Macro, MacroMode, MacroStep},
    transport::{
        HidInterfaceInfo, HidTransport, MockBackend, MockRecord, MockReply, MockTransport,
    },
    Device, Driver,
};
use gearwise_steelseries::prime::{self, Profile};
fn base() -> Plan {
    let (d, _) = gearwise_steelseries::simulated_model(prime::PID).unwrap();
    Plan {
        vendor: 0x1038,
        product: prime::PID,
        settings: vec![
            Setting::Dpi(three_stages()),
            Setting::ReportRate(500),
            prime::setting(&Profile {
                enabled: true,
                color: Rgb::new(171, 205, 239),
                brightness: 37,
                program: None,
            }),
        ]
        .into_iter()
        .chain(
            d.capabilities()
                .buttons
                .iter()
                .filter(|b| b.remappable && b.id != 1)
                .map(|b| Setting::Button {
                    id: b.id,
                    action: b.default_action.clone(),
                }),
        )
        .collect(),
        macros: vec![],
        source: None,
    }
}
fn three_stages() -> DpiState {
    DpiState {
        stages: vec![
            DpiStage::uniform(50),
            DpiStage::uniform(18000),
            DpiStage::uniform(1250),
        ],
        active: 1,
    }
}
fn pad(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out.resize(65, 0);
    out
}
#[test]
fn prime_exact_model_template_exposes_native_steps_wheel_and_fixed_bottom_cpi() {
    let v = templates()
        .unwrap()
        .into_iter()
        .find(|v| v.descriptor.product_id == prime::PID && v.descriptor.vendor_id == 0x1038)
        .unwrap();
    assert_eq!(v.capabilities.dpi.as_ref().unwrap().min, 50);
    assert_eq!(v.capabilities.dpi.as_ref().unwrap().max, 18000);
    assert_eq!(v.capabilities.dpi.as_ref().unwrap().max_stages, 5);
    assert_eq!(
        v.capabilities
            .buttons
            .iter()
            .filter(|b| b.remappable)
            .map(|b| b.id)
            .collect::<Vec<_>>(),
        [2, 3, 4, 5, 7, 8]
    );
    assert_eq!(v.capabilities.lighting[0].name, "Scroll wheel");
    assert_eq!(
        v.capabilities.vendor_profile.as_deref(),
        Some(prime::SCHEMA)
    );
    assert_eq!(v.readback, "write_only_cached");
    assert!(!v.hardware_tested);
    assert!(!supported(0x1038, 0x182c) && !supported(0x1038, 0x182f));
    if let Some(path) = std::env::var_os("OPEN_HUB_PRIME_VIEWS") {
        std::fs::write(path, serde_json::to_vec_pretty(&vec![v]).unwrap()).unwrap();
    }
}
#[test]
fn prime_native_stage_rate_rgb_brightness_and_both_tables_match_source_bytes() {
    let p = base();
    validate(&p).unwrap();
    let (mut d, log) = gearwise_steelseries::simulated_model(prime::PID).unwrap();
    for s in &p.settings[..3] {
        d.apply(s).unwrap();
    }
    let mut table = p
        .settings
        .iter()
        .filter_map(|s| {
            if let Setting::Button { id, action } = s {
                Some((*id, action.clone()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    table.iter_mut().find(|(id, _)| *id == 2).unwrap().1 = ButtonAction::Key {
        usage: 6,
        mods: Modifiers(1),
    };
    table.iter_mut().find(|(id, _)| *id == 7).unwrap().1 = ButtonAction::Key {
        usage: 0x68,
        mods: Modifiers(0),
    };
    d.apply_button_table(&table).unwrap();
    let ops = log.lock().unwrap();
    assert_eq!(
        ops[0],
        MockRecord::Write(pad(&[0, 0x61, 3, 1, 1, 0, 0x68, 1, 25, 0]))
    );
    assert_eq!(ops[1], MockRecord::Write(pad(&[0, 0x5d, 2])));
    let MockRecord::SetFeature(rgb) = &ops[2] else {
        panic!()
    };
    assert_eq!(rgb.len(), 643);
    assert_eq!(&rgb[..6], &[0, 0x62, 1, 171, 205, 239]);
    assert!(rgb[6..18].iter().all(|b| *b == 0));
    assert_eq!(&rgb[18..22], &[171, 205, 239, 255]);
    assert!(rgb[22..].iter().all(|b| *b == 0));
    assert_eq!(ops[3], MockRecord::Write(pad(&[0, 0x5f, 37, 0])));
    assert_eq!(ops[4], MockRecord::Write(pad(&[0, 0x6c, 1])));
    let MockRecord::Write(keys) = &ops[5] else {
        panic!()
    };
    assert_eq!(keys.len(), 65);
    assert_eq!(&keys[7..12], &[0x51, 0xe0, 6, 0, 0]);
    assert_eq!(&keys[27..32], &[0x30, 0, 0, 0, 0]);
    let MockRecord::Write(wheel) = &ops[6] else {
        panic!()
    };
    assert_eq!(
        &wheel[..12],
        &[0, 0x5c, 0x51, 0x68, 0, 0, 0, 0x32, 0, 0, 0, 0]
    );
    drop(ops);
    assert_eq!(
        d.read_state().unwrap().vendor_profile.unwrap()["brightness"],
        37
    );
}
#[test]
fn prime_every_writable_input_supports_scoped_macro_compilation_and_fixed_input_is_rejected() {
    for id in [2, 3, 4, 5, 7, 8] {
        let mut p = base();
        p.settings
            .retain(|s| !matches!(s,Setting::Button{id:i,..}if*i==id));
        p.settings.push(Setting::Button {
            id,
            action: ButtonAction::Macro("prime-macro".into()),
        });
        p.macros.push(Macro {
            id: "prime-macro".into(),
            name: "Independent".into(),
            mode: MacroMode::Once,
            steps: vec![
                MacroStep::KeyDown(6),
                MacroStep::Delay(7),
                MacroStep::KeyUp(6),
            ],
        });
        validate(&p).unwrap();
        let r = simulate(&p).unwrap();
        assert_eq!(r["software_macros"].as_array().unwrap().len(), 1);
        assert_eq!(r["hardware_tested"], false);
    }
    let mut p = base();
    p.settings.push(Setting::Button {
        id: 6,
        action: ButtonAction::Key {
            usage: 6,
            mods: Modifiers(0),
        },
    });
    assert!(validate(&p).is_err());
    for brightness in [0, 1, 37, 128, 255, 256] {
        let p = Profile {
            brightness,
            ..Default::default()
        };
        let s = prime::setting(&p);
        let (mut a, la) = gearwise_steelseries::simulated_model(prime::PID).unwrap();
        let (mut b, lb) = gearwise_steelseries::simulated_model(prime::PID).unwrap();
        a.apply(&s).unwrap();
        assert!(lb.lock().unwrap().is_empty());
        assert_eq!(
            a.read_state().unwrap().vendor_profile.unwrap()["brightness"],
            brightness
        );
        b.apply(&prime::setting(&Profile::default())).unwrap();
        assert_eq!(la.lock().unwrap().len(), 3);
    }
}
struct Fault {
    inner: MockTransport,
    count: Arc<Mutex<usize>>,
    fail: usize,
}
impl HidTransport for Fault {
    fn write(&mut self, b: &[u8]) -> gearwise_core::Result<usize> {
        let n = self.inner.write(b)?;
        let mut c = self.count.lock().unwrap();
        *c += 1;
        Ok(n - usize::from(*c == self.fail))
    }
    fn send_feature_report(&mut self, b: &[u8]) -> gearwise_core::Result<()> {
        self.inner.send_feature_report(b)?;
        let mut c = self.count.lock().unwrap();
        *c += 1;
        if *c == self.fail {
            Err(gearwise_core::Error::protocol("injected feature failure"))
        } else {
            Ok(())
        }
    }
    fn read_timeout(&mut self, _: &mut [u8], _: i32) -> gearwise_core::Result<usize> {
        panic!("no read")
    }
    fn get_feature_report(&mut self, _: &mut [u8]) -> gearwise_core::Result<usize> {
        panic!("no read")
    }
}
fn faulty(fail: usize) -> (Box<dyn Device>, Arc<Mutex<usize>>) {
    let count = Arc::new(Mutex::new(0));
    let c = count.clone();
    let info = HidInterfaceInfo {
        path: format!("simulation:prime-fault:{fail}"),
        vendor_id: 0x1038,
        product_id: prime::PID,
        interface_number: 0,
        usage_page: 0xffc0,
        usage: 1,
        ..Default::default()
    };
    let slot = Mutex::new(Some(Fault {
        inner: MockTransport::new(|_| MockReply::none()),
        count: c,
        fail,
    }));
    let backend = MockBackend::new(vec![info.clone()], move |_| {
        Ok(Box::new(slot.lock().unwrap().take().unwrap()))
    });
    let d = gearwise_steelseries::SteelSeriesDriver::with_gap(std::time::Duration::ZERO)
        .open(&info, &backend)
        .unwrap()
        .remove(0);
    (d, count)
}
#[test]
fn prime_partial_lighting_or_table_failure_invalidates_cache_without_retry_and_explicitly_recovers()
{
    for fail in 1..=3 {
        let (mut d, count) = faulty(fail);
        let s = prime::setting(&Profile::default());
        assert!(d.apply(&s).is_err());
        assert_eq!(*count.lock().unwrap(), fail);
        let state = d.read_state().unwrap();
        assert!(state.vendor_profile.is_none() && state.lighting.is_empty());
        d.apply(&s).unwrap();
        assert_eq!(*count.lock().unwrap(), fail + 3);
    }
    let actions = base()
        .settings
        .into_iter()
        .filter_map(|s| {
            if let Setting::Button { id, action } = s {
                Some((id, action))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for fail in 1..=2 {
        let (mut d, count) = faulty(fail);
        assert!(d.apply_button_table(&actions).is_err());
        assert_eq!(*count.lock().unwrap(), fail);
        assert!(d.read_state().unwrap().buttons.is_empty());
        assert!(d
            .apply(&Setting::Button {
                id: 2,
                action: ButtonAction::Default
            })
            .is_err());
        assert_eq!(*count.lock().unwrap(), fail);
        d.apply_button_table(&actions).unwrap();
        assert_eq!(*count.lock().unwrap(), fail + 2);
    }
}
#[test]
#[ignore = "requires actual saved Prime UI profiles"]
fn prime_browser_saved_profiles_roundtrip_and_native_replay() {
    let ps = roundtrip_steelseries_browser_profiles("OPEN_HUB_PRIME_UI_PROFILES");
    assert_eq!(ps.len(), 2);
    for p in ps {
        let plan = p.multi.as_ref().unwrap();
        validate(plan).unwrap();
        assert_eq!(plan.product, prime::PID);
        assert_eq!(
            plan.settings
                .iter()
                .filter(|s| matches!(s, Setting::Button { .. }))
                .count(),
            6
        );
        let enabled = p.name == "Prime UI Static";
        assert_eq!(plan.macros.len(), usize::from(enabled));
        assert!(plan.settings.iter().any(|s| matches!(s, Setting::Dpi(d) if d.stages.len()==5 && d.active==4 && d.stages[0].x==50 && d.stages[4].x==18000)));
        assert!(plan
            .settings
            .iter()
            .any(|s| matches!(s, Setting::ReportRate(1000))));
        assert!(plan.settings.iter().any(|s| matches!(
            s,
            Setting::Button {
                id: 2,
                action: ButtonAction::Key {
                    usage: 6,
                    mods: Modifiers(1)
                }
            }
        )));
        assert!(!plan
            .settings
            .iter()
            .any(|s| matches!(s, Setting::Lighting { .. })));
        let profile = plan
            .settings
            .iter()
            .find_map(|s| match s {
                Setting::VendorProfile { schema, data } => {
                    Some(Profile::decode(plan.product, schema, data).unwrap())
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(profile.enabled, enabled);
        assert_eq!(profile.brightness, if enabled { 37 } else { 256 });
        assert_eq!(
            profile.color,
            if enabled {
                Rgb::new(171, 205, 239)
            } else {
                Rgb::new(18, 52, 86)
            }
        );
        let r = simulate(plan).unwrap();
        assert_eq!(r["hardware_tested"], false);
        assert_eq!(
            r["software_macros"].as_array().unwrap().len(),
            usize::from(enabled)
        );
    }
}

#[test]
fn prime_rejects_invalid_lighting_before_transport_io() {
    let valid = serde_json::to_value(Profile::default()).unwrap();
    let mut invalid = vec![
        serde_json::json!({}),
        serde_json::json!({"enabled":true,"color":{"r":1,"g":2,"b":3},"brightness":257}),
    ];
    let mut extra = valid.clone();
    extra["unexpected"] = true.into();
    invalid.push(extra);
    let mut rgb_extra = valid.clone();
    rgb_extra["color"]["alpha"] = 255.into();
    invalid.push(rgb_extra);
    let mut overflow = valid.clone();
    overflow["color"]["r"] = 256.into();
    invalid.push(overflow);
    let mut fractional = valid.clone();
    fractional["brightness"] = 1.5.into();
    invalid.push(fractional);
    let (mut d, log) = gearwise_steelseries::simulated_model(prime::PID).unwrap();
    for data in invalid {
        assert!(d
            .apply(&Setting::VendorProfile {
                schema: prime::SCHEMA.into(),
                data
            })
            .is_err());
        assert!(log.lock().unwrap().is_empty());
    }
    assert!(d
        .apply(&Setting::VendorProfile {
            schema: "steelseries-rival3-lighting/1".into(),
            data: valid.clone()
        })
        .is_err());
    assert!(Profile::decode(0x182c, prime::SCHEMA, &valid).is_err());
    assert!(log.lock().unwrap().is_empty());
}

struct Silent;
impl gearwise_input::InputSink for Silent {
    fn key_down(&mut self, _: u8) -> gearwise_core::Result<()> {
        panic!("no OS input")
    }
    fn key_up(&mut self, _: u8) -> gearwise_core::Result<()> {
        panic!("no OS input")
    }
    fn mouse_down(&mut self, _: gearwise_core::model::MouseButton) -> gearwise_core::Result<()> {
        panic!("no OS input")
    }
    fn mouse_up(&mut self, _: gearwise_core::model::MouseButton) -> gearwise_core::Result<()> {
        panic!("no OS input")
    }
    fn scroll(&mut self, _: i32, _: i32) -> gearwise_core::Result<()> {
        panic!("no OS input")
    }
    fn media(&mut self, _: u16) -> gearwise_core::Result<()> {
        panic!("no OS input")
    }
}
#[test]
fn prime_complete_apply_then_one_shot_flash_checks_connection_and_rejects_software_macro() {
    let (d, log) = gearwise_steelseries::simulated_model(prime::PID).unwrap();
    let id = d.descriptor().id.clone();
    let key = d.connection_key().unwrap().to_owned();
    let mut manager = DeviceManager::new(
        Box::new(MockBackend::new(vec![], |_| panic!("no HID"))),
        vec![],
    );
    manager.add_virtual(vec![d]);
    let service = Service::default();
    *service.0.lock().unwrap() = Some(manager);
    let runtime = crate::macro_runtime::Runtime::simulation(Arc::new(Mutex::new(Silent)));
    let root =
        std::env::temp_dir().join(format!("openhub-prime-flash-{}", crate::profiles::new_id()));
    let plan = base();
    assert!(service
        .save_native_hardware_at(&id, &plan, &key, &root)
        .is_err());
    assert!(log.lock().unwrap().is_empty());
    let out = service
        .apply_context(&id, &plan, &runtime, &root, None, None, Some(&key))
        .unwrap();
    assert_eq!(out.session.unwrap().plan, plan);
    let before = log.lock().unwrap().len();
    assert!(service
        .save_native_hardware_at(&id, &plan, "stale", &root)
        .is_err());
    assert_eq!(log.lock().unwrap().len(), before);
    let saved = service
        .save_native_hardware_at(&id, &plan, &key, &root)
        .unwrap();
    assert!(saved.command_sent && !saved.readback_confirmed);
    assert_eq!(
        log.lock().unwrap().last(),
        Some(&MockRecord::Write(pad(&[0, 0x59, 0])))
    );
    let sent = log.lock().unwrap().len();
    assert!(service
        .save_native_hardware_at(&id, &plan, &key, &root)
        .is_err());
    assert_eq!(log.lock().unwrap().len(), sent);
    let mut software = plan.clone();
    software
        .settings
        .iter_mut()
        .find_map(|s| {
            if let Setting::Button { id: 7, action } = s {
                Some(action)
            } else {
                None
            }
        })
        .map(|a| *a = ButtonAction::Macro("missing".into()));
    assert!(service
        .save_native_hardware_at(&id, &software, &key, &root)
        .is_err());
    assert_eq!(log.lock().unwrap().len(), sent);
    let mut dynamic = plan.clone();
    dynamic
        .settings
        .retain(|s| !matches!(s, Setting::VendorProfile { .. }));
    dynamic.settings.push(prime::setting(&Profile {
        program: Some(gradient()),
        ..Default::default()
    }));
    assert!(service
        .save_native_hardware_at(&id, &dynamic, &key, &root)
        .unwrap_err()
        .contains("渐变板载"));
    assert_eq!(log.lock().unwrap().len(), sent);
    service.stop_software(&runtime).unwrap();
    // Only files created beneath this new UUID test directory are removed.
    let resolved = root.canonicalize().unwrap();
    assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    for entry in std::fs::read_dir(root.join("backups")).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        std::fs::remove_file(entry.path()).unwrap();
    }
    std::fs::remove_dir(root.join("backups")).unwrap();
    std::fs::remove_dir(root).unwrap();
}

fn gradient() -> gearwise_steelseries::prime_gradient::Program {
    use gearwise_steelseries::prime_gradient::{Motion, Program, Section};
    Program {
        initial: Rgb::new(171, 205, 239),
        duration: 12340,
        motion: Motion {
            enabled: true,
            increase: 7,
            decrease: 8,
            minimum: 9,
        },
        sections: vec![
            Section {
                color: Rgb::new(0, 0, 0),
                weight: 127,
            },
            Section {
                color: Rgb::new(171, 205, 239),
                weight: 128,
            },
            Section {
                color: Rgb::new(16, 32, 48),
                weight: 63,
            },
        ],
    }
}
#[test]
fn prime_dynamic_native_plan_cache_static_switch_and_isolation() {
    for enabled in [true, false] {
        let profile = Profile {
            enabled,
            program: Some(gradient()),
            ..Default::default()
        };
        let (mut a, log) = gearwise_steelseries::simulated_model(prime::PID).unwrap();
        let (mut b, other) = gearwise_steelseries::simulated_model(prime::PID).unwrap();
        a.apply(&prime::setting(&profile)).unwrap();
        let records = log.lock().unwrap();
        let native = records
            .iter()
            .find_map(|r| {
                if let MockRecord::SetFeature(b) = r {
                    Some(b)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(&native[..6], &[0, 0x62, 3, 171, 205, 239]);
        assert_eq!(&native[6..10], &12340u32.to_le_bytes());
        assert_eq!(&native[14..18], &[1, 7, 8, 9]);
        assert_eq!(
            &native[18..30],
            &[0, 0, 0, 127, 171, 205, 239, 128, 16, 32, 48, 63]
        );
        assert!(other.lock().unwrap().is_empty());
        drop(records);
        let state = a.read_state().unwrap();
        assert_eq!(state.vendor_profile.unwrap()["program"]["duration"], 12340);
        if enabled {
            assert!(state.lighting.is_empty());
        }
        a.apply(&Setting::Lighting {
            zone: 0,
            state: gearwise_core::model::ZoneState {
                effect: gearwise_core::model::LightingEffect::Static {
                    color: Rgb::new(1, 2, 3),
                },
                brightness: 50,
            },
        })
        .unwrap();
        assert!(a
            .read_state()
            .unwrap()
            .vendor_profile
            .unwrap()
            .get("program")
            .is_none());
        assert!(b
            .read_state()
            .unwrap()
            .vendor_profile
            .unwrap()
            .get("program")
            .is_none());
    }
    let mut data = serde_json::to_value(Profile {
        program: Some(gradient()),
        ..Default::default()
    })
    .unwrap();
    for path in ["initial", "color"] {
        let mut bad = data.clone();
        if path == "initial" {
            bad["program"]["initial"]["alpha"] = 1.into();
        } else {
            bad["program"]["sections"][0]["color"]["alpha"] = 1.into();
        }
        assert!(Profile::decode(prime::PID, prime::SCHEMA, &bad).is_err());
    }
    data["program"]["sections"] = serde_json::json!([]);
    assert!(Profile::decode(prime::PID, prime::SCHEMA, &data).is_err());
}
#[test]
#[ignore = "requires saved native gradient production UI profiles"]
fn prime_dynamic_browser_saved_profiles_roundtrip_native_replay() {
    let profiles = roundtrip_steelseries_browser_profiles("OPEN_HUB_PRIME_DYNAMIC_UI_PROFILES");
    assert_eq!(profiles.len(), 2);
    for p in profiles {
        let plan = p.multi.as_ref().unwrap();
        validate(plan).unwrap();
        let profile = plan
            .settings
            .iter()
            .find_map(|s| {
                if let Setting::VendorProfile { schema, data } = s {
                    Some(Profile::decode(prime::PID, schema, data).unwrap())
                } else {
                    None
                }
            })
            .unwrap();
        let mut want = gradient();
        if !profile.enabled {
            want.initial = Rgb::new(18, 52, 86);
            want.sections[1].color = want.initial;
        }
        assert_eq!(profile.program, Some(want));
        assert_eq!(simulate(plan).unwrap()["hardware_tested"], false);
    }
}

#[test]
#[ignore="requires actual saved GG Prime importer UI profile"]
fn prime_gg_import_browser_saved_profile_native_replay(){
 let ps=roundtrip_steelseries_browser_profiles("OPEN_HUB_GG_PRIME_UI_PROFILES");assert_eq!(ps.len(),1);
 let p=&ps[0];let plan=p.multi.as_ref().unwrap();assert_eq!(plan.product,prime::PID);assert_eq!(p.levels[0].x,600);
 let profile=plan.settings.iter().find_map(|s|if let Setting::VendorProfile{schema,data}=s{Some(Profile::decode(prime::PID,schema,data).unwrap())}else{None}).unwrap();
 let program=profile.program.unwrap();assert_eq!(program.initial,Rgb::new(18,52,86));assert_eq!(program.duration,10000);assert_eq!(program.sections.iter().map(|s|s.weight).collect::<Vec<_>>(),[0,84,84,86]);assert_eq!(simulate(plan).unwrap()["hardware_tested"],false);
}

#[test]
#[ignore="requires all three modes saved by actual GG Prime UI"]
fn prime_gg_three_modes_browser_native_replay(){
 let ps=roundtrip_steelseries_browser_profiles("OPEN_HUB_GG_PRIME_MODES_UI_PROFILES");assert_eq!(ps.len(),3);
 for(i,p)in ps.iter().enumerate(){let plan=p.multi.as_ref().unwrap();assert_eq!(p.levels[0].x,600);let profile=plan.settings.iter().find_map(|s|if let Setting::VendorProfile{schema,data}=s{Some(Profile::decode(prime::PID,schema,data).unwrap())}else{None}).unwrap();
 if i==2{assert!(profile.program.is_none());assert_eq!(profile.color,Rgb::new(18,52,86));}else{let program=profile.program.unwrap();assert_eq!(program.initial,Rgb::new(18,52,86));assert_eq!(program.sections.iter().map(|s|s.weight).collect::<Vec<_>>(),if i==0{vec![0,84,84,86]}else{vec![0,40,43,40,43,43,43]});}
 assert_eq!(simulate(plan).unwrap()["hardware_tested"],false);}
}

#[test]
#[ignore="requires actual GG Prime chord importer UI profile"]
fn prime_gg_chord_browser_native_replay(){
 let ps=roundtrip_steelseries_browser_profiles("OPEN_HUB_GG_PRIME_CHORD_UI_PROFILES");assert_eq!(ps.len(),1);let plan=ps[0].multi.as_ref().unwrap();assert_eq!(ps[0].levels[0].x,600);assert_eq!(plan.macros.len(),1);assert_eq!(plan.macros[0].mode,MacroMode::WhileHeld);assert_eq!(plan.macros[0].steps,vec![MacroStep::KeyDown(224),MacroStep::KeyDown(4),MacroStep::KeyDown(5),MacroStep::WaitForRelease,MacroStep::KeyUp(5),MacroStep::KeyUp(4),MacroStep::KeyUp(224)]);assert_eq!(simulate(plan).unwrap()["software_macros"].as_array().unwrap().len(),1);
}
