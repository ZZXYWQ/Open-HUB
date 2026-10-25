use super::*;
fn open(m: &Model) -> (Wired, MockLog, Arc<Mutex<simulation::Memory>>) {
    simulation::open(m.vendor, m.product).unwrap()
}
fn actions(log: &MockLog) -> Vec<Vec<u8>> {
    log.lock()
        .unwrap()
        .iter()
        .filter_map(|r| match r {
            transport::MockRecord::SetFeature(p) | transport::MockRecord::Write(p) => {
                Some(p.clone())
            }
            _ => None,
        })
        .collect()
}
#[test]
fn exact_five_models_and_vendor_control_collection_only() {
    assert_eq!(models().len(), 5);
    for m in models() {
        assert!(WiredDriver.probe(&HidInterfaceInfo {
            vendor_id: m.vendor,
            product_id: m.product,
            usage_page: 0xff00,
            ..Default::default()
        }));
        for (v, p, page, bus) in [
            (m.vendor, m.product, 1, BusKind::Usb),
            (m.vendor, m.product + 1, 0xff00, BusKind::Usb),
            (m.vendor, m.product, 0xff00, BusKind::Bluetooth),
        ] {
            assert!(!WiredDriver.probe(&HidInterfaceInfo {
                vendor_id: v,
                product_id: p,
                usage_page: page,
                bus,
                ..Default::default()
            }));
        }
    }
}
#[test]
fn reads_never_reset_activate_profiles_or_write_configuration() {
    for m in models() {
        let (mut d, log, _) = open(m);
        log.lock().unwrap().clear();
        let state = d.read_state().unwrap();
        assert_eq!(state.dpi.unwrap().stages[0], DpiStage::uniform(800));
        assert!(!actions(&log).iter().any(|p| match m.brand.as_str() {
            "asus" => p[0] != 0x12,
            "gskill" => p[0] != 12 || p[2] == 7 && p[4] == 0 || p[2] == 12 && p[4] != 0,
            "marsgaming" => p[1] != 3,
            _ => p[0] != 4,
        }));
    }
}
#[test]
fn all_five_dpi_and_rates_roundtrip_through_real_drivers() {
    for m in models() {
        let (mut d, _, _) = open(m);
        let v = DpiState {
            stages: vec![
                DpiStage {
                    x: 1200,
                    y: if m.xy { 1300 } else { 1200 },
                },
                DpiStage::uniform(2400),
            ],
            active: if m.active_readonly { 0 } else { 1 },
        };
        d.apply(&Setting::Dpi(v.clone()))
            .unwrap_or_else(|e| panic!("{}: {e}", m.name));
        assert_eq!(d.read_state().unwrap().dpi, Some(v));
        d.apply(&Setting::ReportRate(500))
            .unwrap_or_else(|e| panic!("{}: {e}", m.name));
        assert_eq!(d.read_state().unwrap().report_rate, Some(500));
    }
}
#[test]
fn native_basic_buttons_roundtrip_and_keep_unrelated_fields() {
    for m in models() {
        let (mut d, _, mem) = open(m);
        let before = mem.lock().unwrap().clone();
        d.apply(&Setting::Button {
            id: 4,
            action: ButtonAction::Mouse(MouseButton::Forward),
        })
        .unwrap();
        assert_eq!(
            d.read_state().unwrap().buttons[&4],
            ButtonAction::Mouse(MouseButton::Forward)
        );
        let after = mem.lock().unwrap().clone();
        for id in [1, 2, 3, 5].into_iter().filter(|id| *id <= m.buttons) {
            assert_eq!(
                protocol::row(m, &before.keys, id).unwrap(),
                protocol::row(
                    m,
                    if m.brand == "gskill" {
                        &after.config
                    } else {
                        &after.keys
                    },
                    id
                )
                .unwrap()
            );
        }
        if m.brand == "gskill" {
            assert_eq!(&before.config[18..303], &after.config[18..303]);
            assert_eq!(&before.config[388..], &after.config[388..]);
        }
    }
}
#[test]
fn bad_settings_and_protected_left_are_rejected_without_io() {
    for m in models() {
        let (mut d, log, _) = open(m);
        for s in [
            Setting::Button {
                id: 1,
                action: ButtonAction::Disabled,
            },
            Setting::ReportRate(8000),
            Setting::Dpi(DpiState {
                stages: vec![DpiStage::uniform(1)],
                active: 0,
            }),
        ] {
            log.lock().unwrap().clear();
            assert!(d.apply(&s).is_err());
            assert!(actions(&log).is_empty());
        }
    }
}
#[test]
fn hardware_profile_change_blocks_all_persistent_writes() {
    for m in models().iter().filter(|m| m.brand != "asus") {
        let (mut d, log, mem) = open(m);
        mem.lock().unwrap().profile = 1;
        log.lock().unwrap().clear();
        assert!(d.apply(&Setting::ReportRate(500)).is_err());
        assert!(actions(&log).iter().all(|p| match m.brand.as_str() {
            "gskill" => p[0] == 12 && p[2] == 7 && p[4] == 1,
            "marsgaming" => p[1] == 3,
            _ => false,
        }));
    }
}
#[test]
fn asus_wire_commands_follow_published_field_offsets() {
    let m = &models()[0];
    let (mut d, log, _) = open(m);
    log.lock().unwrap().clear();
    d.apply(&Setting::Button {
        id: 4,
        action: ButtonAction::Key {
            usage: 4,
            mods: Modifiers(0),
        },
    })
    .unwrap();
    let packets = actions(&log);
    let p = packets.iter().find(|p| p[..2] == [0x51, 0x21]).unwrap();
    assert_eq!(&p[..8], &[0x51, 0x21, 0, 0, 0xe6, 1, 4, 0]);
    assert!(packets.iter().any(|p| p[..2] == [0x50, 3]));
}
#[test]
fn known_macro_slots_only_support_once_and_balanced_keys() {
    for m in models().iter().filter(|m| m.macros) {
        let mut def = macros::shortcut(4, Modifiers(1), m.brand == "etekcity");
        macros::validate(m, &def).unwrap();
        def.mode = MacroMode::Toggle;
        assert!(macros::validate(m, &def).is_err());
        def.mode = MacroMode::Once;
        def.steps.pop();
        assert!(macros::validate(m, &def).is_err());
    }
}
#[test]
fn native_macros_verify_actual_payload_and_preserve_other_slots() {
    for m in models().iter().filter(|m| m.macros) {
        let (mut d, _, mem) = open(m);
        let def = macros::shortcut(5, Modifiers(1), m.brand == "etekcity");
        d.set_macro_definitions(&[def.clone()]).unwrap();
        d.apply(&Setting::Button {
            id: 4,
            action: ButtonAction::Macro(def.id.clone()),
        })
        .unwrap_or_else(|e| panic!("{}: {e}", m.name));
        assert_eq!(
            d.read_state().unwrap().buttons[&4],
            ButtonAction::Macro(def.id.clone())
        );
        assert_eq!(d.macro_definitions(), vec![def]);
        let state = mem.lock().unwrap();
        assert_eq!(state.slots.len(), 1);
        assert!(state.slots.contains_key(&3));
    }
}
#[test]
fn firmware_macro_binding_is_locked_without_overwriting_contents() {
    for m in models().iter().filter(|m| m.macros) {
        let (mut d, log, mem) = open(m);
        {
            let mut s = mem.lock().unwrap();
            let mut keys = if m.brand == "gskill" {
                s.config.clone()
            } else {
                s.keys.clone()
            };
            protocol::replace_row(
                m,
                &mut keys,
                4,
                &[if m.brand == "gskill" {
                    6
                } else if m.brand == "roccat" {
                    48
                } else {
                    16
                }],
            )
            .unwrap();
            if m.brand == "gskill" {
                protocol::gskill_checksum(&mut keys);
                s.config = keys;
            } else {
                if m.brand == "roccat" {
                    protocol::checksum(&mut keys)
                }
                s.keys = keys;
            }
        }
        assert!(!d.read_state().unwrap().buttons.contains_key(&4));
        assert!(!d.capabilities().buttons[3].remappable);
        log.lock().unwrap().clear();
        assert!(d
            .apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Disabled
            })
            .is_err());
        assert!(actions(&log).iter().all(|p| p[0] == 4 || p[0] == 12));
    }
}
#[test]
fn etekcity_rejects_timing_instead_of_dropping_user_delays() {
    let m = models().iter().find(|m| m.brand == "etekcity").unwrap();
    assert!(macros::validate(m, &macros::shortcut(4, Modifiers(0), false)).is_err());
}
#[test]
fn only_reviewed_lighting_and_quantized_brightness_are_exposed() {
    for m in models() {
        let (mut d, log, _) = open(m);
        let s = ZoneState {
            effect: LightingEffect::Static {
                color: Rgb::new(24, 40, 220),
            },
            brightness: 80,
        };
        if m.lights.is_empty() {
            log.lock().unwrap().clear();
            assert!(d.apply(&Setting::Lighting { zone: 0, state: s }).is_err());
            assert!(actions(&log).is_empty());
        } else {
            d.apply(&Setting::Lighting { zone: 0, state: s }).unwrap();
            assert_eq!(
                d.read_state().unwrap().lighting[&0],
                protocol::quantized_light(m, s)
            );
            d.apply(&Setting::Lighting {
                zone: 0,
                state: ZoneState {
                    effect: LightingEffect::Off,
                    brightness: 100,
                },
            })
            .unwrap();
            assert_eq!(
                d.read_state().unwrap().lighting[&0].effect,
                LightingEffect::Off
            );
        }
    }
}
#[test]
fn failed_read_does_not_use_cached_values_for_a_later_write() {
    for m in models() {
        let (mut d, log, mem) = open(m);
        mem.lock().unwrap().fail_reads = 1;
        log.lock().unwrap().clear();
        assert!(d.apply(&Setting::ReportRate(500)).is_err());
        assert!(actions(&log).iter().all(|p| match m.brand.as_str() {
            "asus" => p[0] == 0x12,
            "gskill" => p[0] == 12 && p[4] == 1,
            "marsgaming" => p[1] == 3,
            _ => p[0] == 4,
        }));
    }
}
#[test]
fn original_opaque_profile_bytes_survive_dpi_rate_changes() {
    for m in models().iter().filter(|m| m.brand != "asus") {
        let (mut d, _, mem) = open(m);
        let before = mem.lock().unwrap().clone();
        d.apply(&Setting::Dpi(DpiState {
            stages: vec![DpiStage::uniform(1200), DpiStage::uniform(2400)],
            active: 1,
        }))
        .unwrap();
        d.apply(&Setting::ReportRate(500)).unwrap();
        let after = mem.lock().unwrap().clone();
        if m.brand == "gskill" {
            assert_eq!(&before.config[18..], &after.config[18..]);
        } else {
            assert_eq!(before.keys, after.keys);
            if m.brand == "roccat" {
                assert_eq!(&before.config[20..], &after.config[20..]);
            }
            if m.brand == "etekcity" {
                assert_eq!(&before.config[27..], &after.config[27..]);
            }
        }
    }
}
#[test]
fn public_wire_layout_checksums_and_macro_offsets_are_stable() {
    let roc = models().iter().find(|m| m.brand == "roccat").unwrap();
    let mut p = vec![7, 59, 0, 1, 0, 0, 2, 0, 0];
    p.resize(59, 0);
    protocol::checksum(&mut p);
    assert_eq!(&p[57..], &[69, 0]);
    let (mut d, log, _) = open(roc);
    let mut def = macros::shortcut(4, Modifiers(0), false);
    def.id = "packet".into();
    d.set_macro_definitions(&[def.clone()]).unwrap();
    d.apply(&Setting::Button {
        id: 4,
        action: ButtonAction::Macro(def.id),
    })
    .unwrap();
    let packets = actions(&log);
    let p = packets
        .iter()
        .find(|p| p.len() == 2082 && p[0] == 8)
        .unwrap();
    assert_eq!(&p[..6], &[8, 0x22, 8, 0, 3, 1]);
    assert_eq!(&p[80..88], &[4, 1, 20, 0, 4, 2, 50, 0]);
}
#[test]
fn externally_changed_own_macro_becomes_locked() {
    for m in models().iter().filter(|m| m.macros) {
        let (mut d, _, mem) = open(m);
        let mut def = macros::shortcut(4, Modifiers(0), m.brand == "etekcity");
        def.id = "owned".into();
        d.set_macro_definitions(&[def.clone()]).unwrap();
        d.apply(&Setting::Button {
            id: 4,
            action: ButtonAction::Macro(def.id),
        })
        .unwrap();
        {
            let mut memory = mem.lock().unwrap();
            let p = memory.slots.get_mut(&3).unwrap();
            if m.brand == "gskill" {
                p[266] = 5;
                protocol::gskill_checksum(p);
            } else if m.brand == "roccat" {
                p[80] = 5;
                protocol::checksum(p);
            } else {
                p[30] = 5;
            }
        }
        assert!(d
            .apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Disabled
            })
            .is_err());
        let state = d.read_state().unwrap();
        assert!(!state.buttons.contains_key(&4));
        assert!(!d.capabilities().buttons[3].remappable);
        assert!(d
            .apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Disabled
            })
            .is_err());
    }
}
#[test]
fn complete_five_brand_profiles_export_native_dtos_and_execute() {
    let mut fixtures = Vec::new();
    for m in models() {
        let (mut d, _, _) = open(m);
        let base = serde_json::json!({"descriptor":d.descriptor(),"capabilities":d.capabilities(),"state":d.read_state().unwrap(),"readback":"device_partial","hardware_tested":false,"native_macros":[]});
        let mut defs = vec![];
        let mut settings = vec![
            Setting::Dpi(DpiState {
                stages: vec![DpiStage::uniform(1200), DpiStage::uniform(2400)],
                active: 0,
            }),
            Setting::ReportRate(500),
        ];
        if m.macros {
            let mut def = macros::shortcut(4, Modifiers(1), m.brand == "etekcity");
            def.id = "fixture-five".into();
            def.name = format!("{} test chord", m.brand);
            defs.push(def);
            settings.push(Setting::Button {
                id: 4,
                action: ButtonAction::Macro("fixture-five".into()),
            });
        } else {
            settings.push(Setting::Button {
                id: 4,
                action: ButtonAction::Key {
                    usage: 4,
                    mods: Modifiers(0),
                },
            });
        }
        if !m.lights.is_empty() {
            settings.push(Setting::Lighting {
                zone: 0,
                state: ZoneState {
                    effect: LightingEffect::Static {
                        color: Rgb::new(40, 100, 180),
                    },
                    brightness: 100,
                },
            });
        }
        let plan = crate::multibrand::Plan {
            vendor: m.vendor,
            product: m.product,
            source: None,
            macros: defs.clone(),
            settings,
        };
        crate::multibrand::validate(&plan).unwrap();
        let r = crate::multibrand::simulate(&plan).unwrap();
        assert!(r["software_macros"].as_array().unwrap().is_empty());
        assert!(r["packet_count"].as_u64().unwrap() > 0);
        d.set_macro_definitions(&defs).unwrap();
        for s in &plan.settings {
            d.apply(s).unwrap();
        }
        let state = d.read_state().unwrap();
        let configured = serde_json::json!({"descriptor":d.descriptor(),"capabilities":d.capabilities(),"state":state,"readback":"device_partial","hardware_tested":false,"native_macros":d.macro_definitions()});
        fixtures.push(serde_json::json!({"base":base,"configured":configured}));
    }
    if let Some(dir) = std::env::var_os("OPEN_HUB_FIVE_BRAND_FIXTURE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("native-views.json"),
            serde_json::to_vec_pretty(&fixtures).unwrap(),
        )
        .unwrap();
    }
}
#[test]
#[ignore = "requires explicit browser-generated OPEN_HUB_FIVE_BRAND_UI_PROFILES"]
fn five_brand_ui_saved_profiles_reload_and_execute() {
    let path = std::env::var_os("OPEN_HUB_FIVE_BRAND_UI_PROFILES").expect("UI profiles path");
    let profiles: Vec<crate::profiles::Profile> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(profiles.len(), 10);
    for profile in profiles {
        crate::profile_transfer::validate(&profile).unwrap();
        let loaded: crate::profiles::Profile =
            serde_json::from_slice(&serde_json::to_vec(&profile).unwrap()).unwrap();
        let plan = loaded.multi.unwrap();
        assert!(supported(plan.vendor, plan.product));
        let r = crate::multibrand::simulate(&plan).unwrap();
        assert!(r["packet_count"].as_u64().unwrap() > 0);
        assert_eq!(r["hardware_tested"], false);
    }
}

#[test]
fn asus_equal_and_keypad_plus_have_distinct_nonstandard_wire_codes() {
    let m = &models()[0];
    assert_eq!(
        encode_action(
            m,
            4,
            &ButtonAction::Key {
                usage: 0x2e,
                mods: Modifiers(0)
            }
        )
        .unwrap(),
        vec![0x36, 0]
    );
    assert_eq!(
        encode_action(
            m,
            4,
            &ButtonAction::Key {
                usage: 0x57,
                mods: Modifiers(0)
            }
        )
        .unwrap(),
        vec![0x2e, 0]
    );
    assert_eq!(
        decode_action(m, &[0x36, 0]),
        Some(ButtonAction::Key {
            usage: 0x2e,
            mods: Modifiers(0)
        })
    );
    assert_eq!(
        decode_action(m, &[0x2e, 0]),
        Some(ButtonAction::Key {
            usage: 0x57,
            mods: Modifiers(0)
        })
    );
    assert!(decode_action(m, &[0x57, 0]).is_none());
}

#[test]
fn interrupted_feature_write_restores_original_hardware_settings() {
    for m in models().iter().filter(|m| m.brand != "asus") {
        let (mut d, _, mem) = open(m);
        let original = d.read_state().unwrap();
        mem.lock().unwrap().fail_read_after_write = true;
        assert!(d.apply(&Setting::ReportRate(500)).is_err());
        assert_eq!(d.read_state().unwrap(), original);
    }
}
