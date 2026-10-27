use super::*;
fn source() -> Source {
    let p = std::env::temp_dir().join(format!("sc620-edit-{}.db", crate::profiles::new_id()));
    std::fs::write(
        &p,
        include_bytes!("../tests/fixtures/yjx-sc620/oem-profile.sqlite3"),
    )
    .unwrap();
    let source = source_preview::preview(&p, Some(7)).unwrap().source;
    std::fs::remove_file(p).unwrap();
    source
}
fn facts() -> serde_json::Value {
    serde_json::from_str(include_str!("../tests/fixtures/yjx-sc620/source-edit.json")).unwrap()
}
fn request() -> Request {
    serde_json::from_value(facts()["request"].clone()).unwrap()
}
fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}
fn replay(s: &Source) {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/yjx-sc620/configuration.json"
    ))
    .unwrap();
    let config = super::super::ConfigImage::from_response(
        2,
        &hex(f["before_response_hex"].as_str().unwrap()),
    )
    .unwrap();
    let pages = (0..6)
        .map(|n| {
            let mut b = [0; 64];
            b[..3].copy_from_slice(&[0xaa, 8, 0xa5]);
            b[4] = 44;
            b[7] = n;
            for i in 0..11 {
                b[8 + 4 * i..12 + 4 * i].copy_from_slice(&[32, 1, 0, 0]);
            }
            super::super::bindings::Image::from_response(n, &b).unwrap()
        })
        .collect::<Vec<_>>();
    let plan = s
        .prepare(
            &config,
            &pages,
            &super::super::macros::Image::compile(&[]).unwrap(),
        )
        .unwrap();
    let gold = facts();
    assert_eq!(plan.reports.len(), 5);
    for (r, g) in plan
        .reports
        .iter()
        .zip(gold["reports_hex"].as_array().unwrap())
    {
        assert_eq!(r.to_vec(), hex(g.as_str().unwrap()));
    }
}
#[test]
fn complete_settings_buttons_and_macro_mode_edit_match_five_independent_reports() {
    let old = source();
    let next = update(old.clone(), request()).unwrap();
    assert!(next.static_subset_convertible);
    assert!(!next.hardware_apply_available);
    assert_eq!(next.source.settings.fields["sensor_flag"], "20");
    assert_eq!(next.source.settings.fields["report_rate"], "4");
    assert_eq!(next.source.settings.fields["dpi_group1_rgb"], "65280");
    replay(&next.source);
    assert_eq!(old, source());
}
#[test]
fn all_known_basic_choices_compile_back_to_the_exact_requested_native_action() {
    let original: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/yjx-sc620/bindings.json")).unwrap();
    let mut actions = original["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| serde_json::from_value::<Action>(row["action"].clone()).unwrap())
        .collect::<Vec<_>>();
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/yjx-sc620/oem-special-actions.json"
    ))
    .unwrap();
    for c in f["cases"].as_array().unwrap() {
        let a: Action = serde_json::from_value(c["action"].clone()).unwrap();
        if !matches!(a, Action::Disabled(_)) {
            actions.push(a);
        }
    }
    for mouse in [
        Mouse::Left,
        Mouse::Right,
        Mouse::Middle,
        Mouse::Back,
        Mouse::Forward,
    ] {
        actions.push(Action::Mouse(mouse));
    }
    for usage in [0, 4, 29, 0xa4] {
        for modifiers in [0, 1, 2, 4, 8, 16, 32, 64, 128, 255] {
            if usage != 0 || modifiers != 0 {
                actions.push(Action::Key { usage, modifiers });
            }
        }
    }
    for a in actions {
        let next = update(
            source(),
            Request {
                buttons: vec![ButtonEdit {
                    control: 6,
                    choice: Choice::Basic(a.clone()),
                }],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            next.keys[5].action.as_ref().unwrap().encode().unwrap(),
            a.encode().unwrap()
        );
    }
}
#[test]
fn all_eleven_disabled_controls_derive_original_hint_without_user_supplied_bytes() {
    for control in 1..=11 {
        let next = update(
            source(),
            Request {
                buttons: vec![ButtonEdit {
                    control,
                    choice: Choice::Disabled,
                }],
                ..Default::default()
            },
        )
        .unwrap();
        let k = next
            .source
            .keys
            .iter()
            .find(|k| k.key_code == control)
            .unwrap();
        assert_eq!(k.kind, 24);
        assert!(matches!(
            oem_profile::basic(k).unwrap(),
            Action::Disabled(_)
        ));
    }
    assert!(update(
        source(),
        Request {
            buttons: vec![ButtonEdit {
                control: 6,
                choice: Choice::Basic(Action::Disabled(
                    super::super::bindings::DisableHint::Forward
                ))
            }],
            ..Default::default()
        }
    )
    .is_err());
}
#[test]
fn metadata_unknown_values_and_other_rows_are_preserved_after_explicit_edits() {
    let mut old = source();
    old.settings
        .fields
        .insert("unverified".into(), "original".into());
    old.settings = oem_config::convert(7, old.settings.fields.clone()).unwrap();
    let next = update(old.clone(), request()).unwrap();
    assert_eq!(next.source.settings.fields["unverified"], "original");
    assert!(!next.static_subset_convertible);
    for i in 0..11 {
        let a = &old.keys[i];
        let b = &next.source.keys[i];
        assert_eq!(a.layout_value, b.layout_value);
        assert_eq!(a.layout_description, b.layout_description);
        assert_eq!(a.description, b.description);
        if ![4, 5, 6, 11].contains(&a.key_code) {
            assert_eq!(a, b);
        }
    }
    for (a, b) in old.macros.iter().zip(&next.source.macros) {
        assert_eq!(a.events, b.events);
        assert_eq!(a.delay_type, b.delay_type);
        assert_eq!(a.delay_time, b.delay_time);
    }
}
#[test]
fn sensor_metadata_and_unknown_bits_survive_boolean_edits_and_missing_flag_is_not_invented() {
    let mut old = source();
    old.settings
        .fields
        .insert("sensor_flag".into(), "227".into());
    old.settings = oem_config::convert(7, old.settings.fields.clone()).unwrap();
    let next = update(
        old,
        Request {
            settings: ConfigPatch {
                ripple_control: Some(true),
                motion_sync: Some(false),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(next.source.settings.fields["sensor_flag"], "231");
    assert!(!next.static_subset_convertible);
    let mut old = source();
    old.settings.fields.remove("sensor_flag");
    old.settings = oem_config::convert(7, old.settings.fields.clone()).unwrap();
    assert!(update(
        old,
        Request {
            settings: ConfigPatch {
                angle_snap: Some(true),
                ..Default::default()
            },
            ..Default::default()
        }
    )
    .is_err());
}
#[test]
fn invalid_ranges_duplicate_edits_and_missing_references_refuse_atomically() {
    let original = source();
    for patch in [
        ConfigPatch {
            report_rate_hz: Some(8000),
            ..Default::default()
        },
        ConfigPatch {
            active: Some(6),
            ..Default::default()
        },
        ConfigPatch {
            debounce_ms: Some(21),
            ..Default::default()
        },
        ConfigPatch {
            dpi: Some([49; 6]),
            ..Default::default()
        },
        ConfigPatch {
            lod_mm: Some(3),
            ..Default::default()
        },
        ConfigPatch {
            sleep_minutes: Some(2),
            ..Default::default()
        },
    ] {
        assert!(update(
            original.clone(),
            Request {
                settings: patch,
                ..Default::default()
            }
        )
        .is_err());
    }
    for buttons in [
        vec![ButtonEdit {
            control: 12,
            choice: Choice::Disabled,
        }],
        vec![ButtonEdit {
            control: 6,
            choice: Choice::Macro(1),
        }],
        vec![
            ButtonEdit {
                control: 6,
                choice: Choice::Disabled
            };
            2
        ],
    ] {
        assert!(update(
            original.clone(),
            Request {
                buttons,
                ..Default::default()
            }
        )
        .is_err());
    }
    assert_eq!(original, source());
}
#[test]
fn all_playback_modes_keep_event_content_and_cannot_invent_toggle_or_zero_repeat() {
    for p in [
        Playback::Once,
        Playback::Repeat(255),
        Playback::WhileHeld,
        Playback::UntilAnyClick,
    ] {
        let old = source();
        let next = update(
            old.clone(),
            Request {
                macros: vec![MacroEdit {
                    id: 42,
                    playback: p.clone(),
                }],
                ..Default::default()
            },
        )
        .unwrap();
        let m = &next.source.macros[1];
        assert_eq!(m.playback, p);
        assert_eq!(Playback::from_oem(m.play_type, m.play_times).unwrap(), p);
        assert_eq!(m.events, old.macros[1].events);
    }
    for p in [Playback::Repeat(0), Playback::Repeat(1)] {
        assert!(update(
            source(),
            Request {
                macros: vec![MacroEdit { id: 7, playback: p }],
                ..Default::default()
            }
        )
        .is_err());
    }
    assert!(
        serde_json::from_str::<Request>(r#"{"macros":[{"id":7,"playback":"Toggle"}]}"#).is_err()
    );
}
#[test]
fn source_archive_after_edit_reloads_without_affecting_source_or_generic_profiles() {
    let next = update(source(), request()).unwrap();
    let p = std::env::temp_dir().join(format!("sc620-edited-{}.json", crate::profiles::new_id()));
    source_preview::export(&p, next.source.clone()).unwrap();
    let loaded = source_preview::preview(&p, None).unwrap();
    assert_eq!(loaded.source, next.source);
    assert!(crate::profile_transfer::preview("openhub", Some(p.display().to_string())).is_err());
    replay(&loaded.source);
    std::fs::remove_file(p).unwrap();
}
#[test]
#[ignore = "requires explicit source editor UI DTO output"]
fn emit_editor_dtos() {
    let path = std::env::var_os("OPEN_HUB_SC620_EDITOR_DTOS").unwrap();
    let base = source_preview::assess(source(), vec![7]).unwrap();
    let edited = update(base.source.clone(), request()).unwrap();
    std::fs::write(
        path,
        serde_json::to_vec_pretty(
            &serde_json::json!({"base":base,"edited":edited,"request":request()}),
        )
        .unwrap(),
    )
    .unwrap();
}
#[test]
#[ignore = "requires actual built source editor UI request"]
fn replay_actual_editor_request() {
    let path = std::env::var_os("OPEN_HUB_SC620_EDITOR_REQUEST").unwrap();
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let actual_source: Source = serde_json::from_value(value["source"].clone()).unwrap();
    assert_eq!(actual_source, source());
    let request: Request = serde_json::from_value(value["request"].clone()).unwrap();
    let edited = update(actual_source, request).unwrap();
    replay(&edited.source);
}
