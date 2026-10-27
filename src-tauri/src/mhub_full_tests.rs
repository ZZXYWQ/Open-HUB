use super::*;
use serde_json::json;

fn source() -> Value {
    serde_json::from_str(include_str!("../tests/fixtures/mhub-full-template.json")).unwrap()
}
fn library() -> Value {
    serde_json::from_str(include_str!("../tests/fixtures/mhub-macro-library.json")).unwrap()
}
fn plan() -> Plan {
    parse(&source(), &library()).unwrap()
}
fn baseline() -> Snapshot {
    serde_json::from_str(include_str!("../tests/fixtures/mchose.json")).unwrap()
}
fn boards(snapshot: &Snapshot) -> [Vec<Vec<u8>>; 3] {
    std::array::from_fn(|_| snapshot.state.buttons.clone())
}
fn simple(slot: u8, name: &str, code: u8) -> MouseMacro {
    native::macro_record(
        slot,
        name,
        2,
        &[
            MacroEvent {
                kind: "key".into(),
                code,
                down: true,
                delay_ms: 0,
            },
            MacroEvent {
                kind: "key".into(),
                code,
                down: false,
                delay_ms: 0,
            },
        ],
        None,
    )
    .unwrap()
}

#[test]
fn official_flat_template_and_explicit_macro_library_match_independent_vectors() {
    let p = plan();
    assert_eq!((p.vendor, p.model, p.board), (0x3837, 0x4026, 0));
    assert_eq!(p.dpi_x, [400, 900, 1800, 3600, 7200, 14400]);
    assert_eq!(
        p.dpi_y.as_ref().unwrap(),
        &[450, 950, 1850, 3650, 7250, 14450]
    );
    assert_eq!(
        (p.usb_rate, p.wireless_rate, p.rotation, p.sensor),
        (3, 4, -8, 82)
    );
    assert_eq!(
        (p.count, p.active, p.sleep, p.never_sleep, p.lod),
        (4, 1, 5, true, 3)
    );
    assert_eq!(p.buttons[3], [0x13, 3, 4]);
    assert_eq!(p.buttons[4], [0x21, 10, 0]); // hex000a is slot10, not a decimal conversion.
    assert_eq!(p.macros.len(), 1);
    assert_eq!(p.macros[0].slot, 10);
    let mut golden = vec![32, 0, 0, 0, 2, 0, 0, 0];
    golden.extend_from_slice(b"Ctrl A test");
    golden.resize(64, 0);
    golden.extend_from_slice(&[
        0, 0, 81, 224, 0, 0, 81, 4, 37, 0, 63, 0, 0, 0, 17, 4, 0, 0, 17, 224, 0, 0, 64, 1, 0, 0, 0,
        1, 0, 0, 133, 1,
    ]);
    assert_eq!(p.macros[0].raw, golden);
}

#[test]
fn named_macro_references_require_unique_explicit_bodies_not_same_name_on_target() {
    assert!(parse(&source(), &json!([]))
        .unwrap_err()
        .contains("缺少正文"));
    let lib = library();
    assert!(parse(&source(), &json!([lib[0], lib[0]]))
        .unwrap_err()
        .contains("不唯一"));
    let mut s = source();
    s[PRODUCT]["macroInfo"] = json!({});
    assert!(parse(&s, &lib).is_err());
    s = source();
    s[PRODUCT]["macroInfo"]["0000"] = json!("unused");
    assert!(parse(&s, &lib).is_err());
    s = source();
    s[PRODUCT]["macroInfo"] = json!({"000a":"Ctrl A test","000A":"Ctrl A test"});
    assert!(parse(&s, &lib).is_err());
}

#[test]
fn binding_endianness_chords_media_presets_and_incapable_dpi_are_distinct() {
    for (kind, value, expected) in [
        ("16", "0106", vec![0x16, 1, 6]),
        ("14", "00e9", vec![0x14, 0xe9, 0]),
        ("01", "000002", vec![1, 2, 0, 0]),
        ("12", "0003", vec![0x12, 3, 0]),
        ("11", "0029", vec![0x11, 41, 0]),
        ("33", "0002", vec![0x33, 2, 0]),
    ] {
        let mut s = source();
        s[PRODUCT]["forwardKeyType"] = json!(kind);
        s[PRODUCT]["forwardKeyVal"] = json!(value);
        assert_eq!(parse(&s, &library()).unwrap().buttons[3], expected);
    }
    let mut s = source();
    s[PRODUCT].as_object_mut().unwrap().remove("dpiKeyType");
    s[PRODUCT].as_object_mut().unwrap().remove("dpiKeyVal");
    assert_eq!(parse(&s, &library()).unwrap().buttons[5], [255; 3]);
    s[PRODUCT]["isSupportDpiKey"] = json!(true);
    assert!(parse(&s, &library()).is_err());
    for kind in ["22", "23", "24", "90"] {
        let mut s = source();
        s[PRODUCT]["forwardKeyType"] = json!(kind);
        assert!(parse(&s, &library()).is_err());
    }
}

#[test]
fn original_rate_indexes_signed_rotation_and_numeric_strings_do_not_wrap() {
    for (index, code) in [0, 2, 3, 4, 5, 6].into_iter().enumerate() {
        let mut s = source();
        s[PRODUCT]["usbRateIndex"] = json!(index.to_string());
        assert_eq!(parse(&s, &library()).unwrap().usb_rate, code);
    }
    for (raw, angle) in [(30, 120), (226, -120), (255, -4)] {
        let mut s = source();
        s[PRODUCT]["rotateVal"] = json!(raw);
        assert_eq!(parse(&s, &library()).unwrap().rotation, angle);
    }
    for (field, value) in [
        ("usbRateIndex", json!(6)),
        ("rotateVal", json!(128)),
        ("dpi0", json!("800junk")),
        ("dpi0", json!(800.5)),
        ("dpi0", json!(false)),
        ("sleep", json!(-1)),
        ("sleep", json!(61)),
    ] {
        let mut s = source();
        s[PRODUCT][field] = value;
        assert!(parse(&s, &library()).is_err(), "{field}");
    }
}

#[test]
fn y_axis_may_be_absent_only_when_inactive_and_never_partially_filled() {
    let mut s = source();
    s[PRODUCT]["dpiHasY"] = json!(0);
    for i in 0..6 {
        s[PRODUCT]
            .as_object_mut()
            .unwrap()
            .remove(&format!("dpiY{i}"));
    }
    assert!(parse(&s, &library()).unwrap().dpi_y.is_none());
    s[PRODUCT]["dpiHasY"] = json!(1);
    assert!(parse(&s, &library()).is_err());
    s[PRODUCT]["dpiHasY"] = json!(0);
    s[PRODUCT]["dpiY0"] = json!(800);
    assert!(parse(&s, &library()).is_err());
}

#[test]
fn incomplete_wrong_model_unknown_functional_fields_and_missing_left_button_refuse() {
    for field in [
        "name",
        "sleepMode",
        "dpi3",
        "retreatKeyVal",
        "lodIndex",
        "sensor",
    ] {
        let mut s = source();
        s[PRODUCT].as_object_mut().unwrap().remove(field);
        assert!(parse(&s, &library()).is_err(), "{field}");
    }
    let mut s = source();
    s["MCHOSE K5 Pro"] = s[PRODUCT].clone();
    assert!(parse(&s, &library()).is_err());
    s = source();
    s[PRODUCT]["adaptivePowerSwitch"] = json!(1);
    assert!(parse(&s, &library()).unwrap_err().contains("尚未转换"));
    s = source();
    s[PRODUCT]["leftKeyVal"] = json!("0002");
    assert!(parse(&s, &library()).unwrap_err().contains("普通左键"));
    s = source();
    s[PRODUCT]["deviceName"] = json!("MCHOSE A5 V3 Ultra+");
    assert!(parse(&s, &library()).is_err());
    s = source();
    s[PRODUCT]["usbDpiIndex"] = json!(4);
    assert!(parse(&s, &library()).is_err());
}

#[test]
fn all170_proved_keyboard_aliases_and_four_modes_use_production_macro_encoder() {
    let tables: Value = serde_json::from_str(include_str!("mhub-key-usages.json")).unwrap();
    assert_eq!(tables["key"].as_object().unwrap().len(), 170);
    for (key, usage) in tables["key"].as_object().unwrap() {
        for mode in [0, 1, 2, 3] {
            let mut lib = library();
            lib[0]["type"] = json!(mode);
            lib[0]["info"]["keyList"] = json!([{"type":"key","press":"down","val":key},{"type":"key","press":"up","val":key}]);
            let p = parse(&source(), &lib).unwrap();
            assert_eq!(p.macros[0].raw[4], mode);
            assert_eq!(p.macros[0].raw[67], usage.as_u64().unwrap() as u8);
            assert_eq!(p.macros[0].raw[71], usage.as_u64().unwrap() as u8);
        }
    }
}

#[test]
fn unsupported_or_malformed_macro_content_is_never_replaced_with_empty_events() {
    for event in [
        json!({"type":"launch","press":"down","val":"app.exe"}),
        json!({"type":"time","press":null,"val":65536}),
        json!({"type":"key","val":"a"}),
        json!({"type":"key","press":"up","val":"a"}),
        json!({"type":"time","press":"down","val":20}),
        json!({"type":"key","press":"down","val":"not-proved-key"}),
        json!({"type":"time","val":10,"repeat":3}),
    ] {
        let mut lib = library();
        lib[0]["info"]["keyList"] = json!([event]);
        assert!(parse(&source(), &lib).is_err());
    }
    let mut lib = library();
    lib[0]["info"]["keyList"] = json!([]);
    assert!(parse(&source(), &lib).is_err());
    lib = library();
    lib[0]["type"] = json!(4);
    assert!(parse(&source(), &lib).is_err());
}

#[test]
fn compilation_retains_reserved_bytes_other_groups_firmware_and_unassigned_macros() {
    let p = plan();
    let mut before = baseline();
    before.state.raw_settings[4] = 0xa0;
    before.macros.push(simple(3, "Unassigned user macro", 8));
    let keys = boards(&before);
    let result = compile(&p, &before, &keys).unwrap();
    assert_eq!(result.slot_map[&10], 0);
    assert_eq!(result.macro_writes.len(), 1);
    assert_eq!(result.macro_writes[0].slot, 0);
    let after = &result.snapshot.state;
    assert_eq!(
        &after.raw_settings[..9],
        &[0, 0x31, 0x41, 5, 0xa1, 82, 254, 12, 8]
    );
    assert_eq!(&after.raw_settings[9..], &before.state.raw_settings[9..]);
    assert_eq!(
        after.raw_x,
        [0, 0, 4, 1, 1, 0x90, 1, 0x84, 3, 8, 7, 0x10, 14, 0x20, 28, 0x40, 56]
    );
    assert_eq!(
        after.raw_y,
        [0, 1, 4, 1, 1, 0xc2, 1, 0xb6, 3, 0x3a, 7, 0x42, 14, 0x52, 28, 0x72, 56]
    );
    assert_eq!(after.buttons[4], [0x21, 0, 0]);
    assert_eq!(&after.names[1..], &before.state.names[1..]);
    assert_eq!(after.firmware, before.state.firmware);
    assert_eq!(after.receiver_firmware, before.state.receiver_firmware);
    assert_eq!(
        result.snapshot.macros.iter().find(|m| m.slot == 3),
        before.macros.first()
    );
    native::validate_snapshot(&result.snapshot).unwrap();
}

#[test]
fn identical_macro_content_is_reused_without_upload_or_other_board_change() {
    let p = plan();
    let mut before = baseline();
    let mut same = p.macros[0].clone();
    same.slot = 7;
    before.macros.push(same.clone());
    let mut keys = boards(&before);
    keys[1][4] = vec![0x21, 7, 0];
    let result = compile(&p, &before, &keys).unwrap();
    assert_eq!(result.slot_map[&10], 7);
    assert!(result.macro_writes.is_empty());
    assert_eq!(result.snapshot.macros, [same]);
    assert_eq!(keys[1][4], [0x21, 7, 0]);
}

#[test]
fn full_macro_storage_reclaims_only_exclusive_current_board_reference() {
    let p = plan();
    let mut before = baseline();
    before.macros = (0..18).map(|s| simple(s, &format!("old{s}"), 4)).collect();
    before.state.buttons[4] = vec![0x21, 5, 0];
    before.state.raw_buttons = before.state.buttons.concat();
    let mut keys = boards(&before);
    keys[1][4] = vec![0x21, 6, 0];
    keys[2][4] = vec![0x21, 7, 0];
    let result = compile(&p, &before, &keys).unwrap();
    assert_eq!(result.slot_map[&10], 5);
    assert_eq!(result.macro_writes.len(), 1);
    for old in &before.macros {
        if old.slot != 5 {
            assert_eq!(
                result.snapshot.macros.iter().find(|m| m.slot == old.slot),
                Some(old)
            );
        }
    }
    keys[1][4] = vec![0x21, 5, 0];
    assert!(compile(&p, &before, &keys).is_err());
    before.state.buttons[4] = vec![0x11, 41, 0];
    before.state.raw_buttons = before.state.buttons.concat();
    keys[0] = before.state.buttons.clone();
    assert!(compile(&p, &before, &keys).is_err()); // unassigned user macro slots are not scratch space.
}

#[test]
fn unknown_missing_reference_and_stale_board_read_set_do_not_compile() {
    let p = plan();
    let before = baseline();
    let mut keys = boards(&before);
    keys[1][4] = vec![0x21, 18, 0];
    assert!(compile(&p, &before, &keys).is_err());
    keys = boards(&before);
    keys[2][4] = vec![];
    assert!(compile(&p, &before, &keys).is_err());
    keys = boards(&before);
    keys[0][4] = vec![0x11, 4, 0];
    assert!(compile(&p, &before, &keys).is_err());
    let mut wrong = p.clone();
    wrong.board = 1;
    assert!(compile(&wrong, &before, &boards(&before)).is_err());
    wrong = p.clone();
    wrong.sensor ^= 0x40;
    assert!(compile(&wrong, &before, &boards(&before)).is_err());
    wrong = p.clone();
    wrong.buttons[5] = vec![1, 0, 0, 0];
    assert!(compile(&wrong, &before, &boards(&before)).is_err());
}

#[test]
fn inactive_y_stays_in_portable_plan_while_observed_linked_hardware_mirrors_x() {
    let mut p = plan();
    p.independent = false;
    let before = baseline();
    let original_y = p.dpi_y.clone();
    let encoded = serde_json::to_vec(&p).unwrap();
    let parsed: Plan = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(parsed, p);
    let result = compile(&parsed, &before, &boards(&before)).unwrap();
    assert_eq!(result.snapshot.state.dpi_y, p.dpi_x);
    assert_eq!(p.dpi_y, original_y);
    assert_ne!(p.dpi_y.as_ref().unwrap(), &before.state.dpi_y);
}

#[test]
fn serialized_plan_cannot_claim_another_brand_or_mismatch_macro_bytes() {
    let original = plan();
    let mut bad = original.clone();
    bad.vendor = 0x046d;
    assert!(validate(&bad).is_err());
    bad = original.clone();
    bad.model = 0x4033;
    assert!(validate(&bad).is_err());
    bad = original.clone();
    bad.macros[0].events[0].code = 4;
    assert!(validate(&bad).is_err());
    bad = original.clone();
    bad.macros.push(bad.macros[0].clone());
    assert!(validate(&bad).is_err());
    bad = original;
    bad.buttons[4][1] = 11;
    assert!(validate(&bad).is_err());
}

#[test]
fn raw_export_and_library_json_reject_duplicate_fields_trailing_garbage_and_large_input() {
    let source = include_bytes!("../tests/fixtures/mhub-full-template.json");
    let lib = include_bytes!("../tests/fixtures/mhub-macro-library.json");
    assert_eq!(parse_bytes(source, Some(lib)).unwrap(), plan());
    let duplicate = String::from_utf8(source.to_vec())
        .unwrap()
        .replace("\"sleep\": 5", "\"sleep\": 5, \"sleep\": 6");
    assert!(parse_bytes(duplicate.as_bytes(), Some(lib))
        .unwrap_err()
        .contains("duplicate"));
    let duplicate = String::from_utf8(lib.to_vec()).unwrap().replacen(
        "\"press\": \"down\"",
        "\"press\": \"down\", \"press\": \"up\"",
        1,
    );
    assert!(parse_bytes(source, Some(duplicate.as_bytes()))
        .unwrap_err()
        .contains("duplicate"));
    assert!(parse_bytes(&[source.as_slice(), b"null"].concat(), Some(lib)).is_err());
    assert!(parse_bytes(&[255], None).is_err());
    assert!(parse_bytes(&vec![b' '; 8 * 1024 * 1024 + 1], None).is_err());
}

#[path = "mhub_full_wire_tests.rs"]
mod wire;

#[test]
fn larger_global_macro_library_and_compact_device_metadata_keep_exact_board_scope() {
    let mut source = source();
    source[PRODUCT]["deviceName"] = json!("MCHOSEA7V3Ultra+");
    let mut large = (0..700)
        .map(|n| json!({"name":format!("unreferenced{n}")}))
        .collect::<Vec<_>>();
    large.push(library()[0].clone());
    assert_eq!(
        parse(&source, &Value::Array(large)).unwrap().macros,
        plan().macros
    );
    source[PRODUCT]["isSupportDpiKey"] = json!(true);
    assert!(parse(&source, &library()).unwrap_err().contains("能力标记"));
    source[PRODUCT]["isSupportDpiKey"] = json!("false");
    assert!(parse(&source, &library()).is_err());
}

#[path="mhub_full_profile_tests.rs"]
mod profiles;
#[test]
fn all_pinned_oem_literal_picker_actions_convert_from_complete_source_fields() {
    let fixture:Value=serde_json::from_str(include_str!("../tests/fixtures/mchose-v3-selectable-actions.json")).unwrap();
    assert_eq!(fixture["source_sha256"],"c3e91c2828283cbcb8edb24474b45d64b2526bcbed0a2740919a0deef7ad61bc");
    let actions=fixture["actions"].as_array().unwrap();assert_eq!(actions.len(),169);
    let original=plan();
    for item in actions {let hex=item["hex"].as_str().unwrap();let expected:Vec<u8>=serde_json::from_value(item["bytes"].clone()).unwrap();let mut doc=source();doc[PRODUCT]["rightKeyType"]=json!(&hex[2..4]);doc[PRODUCT]["rightKeyVal"]=json!(&hex[4..]);let converted=parse(&doc,&library()).unwrap();assert_eq!(converted.buttons[1],expected,"{hex}");let mut reference=original.clone();reference.buttons[1]=expected;assert_eq!(converted,reference,"{hex} changed unrelated source fields");crate::profile_transfer::validate(&to_profile(converted).unwrap()).unwrap();}
}
