use super::*;
use serde_json::json;
fn encode(bytes: &[u8]) -> Vec<u8> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = vec![];
    for q in bytes.chunks(3) {
        let b = *q.get(1).unwrap_or(&0);
        let c = *q.get(2).unwrap_or(&0);
        out.extend_from_slice(&[
            alphabet[(q[0] >> 2) as usize],
            alphabet[((q[0] & 3) << 4 | b >> 4) as usize],
            if q.len() > 1 {
                alphabet[((b & 15) << 2 | c >> 6) as usize]
            } else {
                b'='
            },
            if q.len() > 2 {
                alphabet[(c & 63) as usize]
            } else {
                b'='
            },
        ]);
    }
    out
}
fn fixture(m: &atk::Model) -> Value {
    json!({"keyActions":[[[{"changeKey":[6,1,3]}]]],"deviceConfig":{"lightConfig":{"mode":255},"macroList":[{"unknown":"retained only in source"}],"mouseConfig":{"cidMid":{"cid":2,"mid":m.mids[0],"uniqueId":[1,2,3,4]},"dpiTotal":2,"dpiLevel":1,"reportRate":2,"dpiList":(0..8).map(|i|json!({"dpi":{"xDpi":400+50*i,"yDpi":800+50*i,"dpiEx":255,"usesLogical":true},"color":[1,2,3]})).collect::<Vec<_>>(),"keyDebounceTime":4,"moveSync":0},"device_name":"未受信文件名","app_path":"C:/do-not-import.exe"},"source":{"path":"do-not-authorize"}})
}
fn prepare(v: &Value, m: &atk::Model) -> Result<Prepared> {
    prepare_bytes(
        &encode(&serde_json::to_vec(v).unwrap()),
        m.vendor,
        m.product,
    )
}
fn models() -> impl Iterator<Item = &'static atk::Model> {
    atk::models().iter().filter(|m| m.native_macros)
}
#[test]
fn every_exact_model_file_converts_saves_and_executes_without_other_settings() {
    let mut cases = vec![];
    for m in models() {
        let f = fixture(m);
        let result = prepare(&f, m).unwrap();
        assert!(result.note.contains("按键、宏、灯效"));
        let p = result.profile;
        let plan = p.multi.as_ref().unwrap();
        assert_eq!(plan.settings.len(), 2);
        assert!(plan.macros.is_empty());
        assert!(plan.source.is_none());
        assert!(p.app_path.is_none());
        assert!(!p.targets(0x046d, &[m.product]));
        let compiled = crate::multibrand::compile(plan).unwrap();
        assert!(!compiled.software_input);
        let (mut d, _) = atk::simulated(m.product).unwrap();
        let before = d.read_state().unwrap();
        for s in &compiled.settings {
            d.apply(s).unwrap();
        }
        let after = d.read_state().unwrap();
        assert_eq!(
            after.dpi.as_ref().unwrap().stages,
            vec![DpiStage { x: 400, y: 800 }, DpiStage { x: 450, y: 850 }]
        );
        assert_eq!(after.dpi.as_ref().unwrap().active, 1);
        assert_eq!(after.report_rate, Some(500));
        assert_eq!(after.buttons, before.buttons);
        assert_eq!(after.lighting, before.lighting);
        assert_eq!(after.dpi_colors, before.dpi_colors);
        assert_eq!(after.dpi_indicator, before.dpi_indicator);
        assert_eq!(after.tuning, before.tuning);
        let mut store = crate::profiles::ProfileStore::default();
        crate::profile_transfer::merge(&mut store, vec![p.clone()]).unwrap();
        assert_eq!(store.profiles.len(), 2);
        let saved = serde_json::to_vec(&p).unwrap();
        crate::profile_transfer::validate(&serde_json::from_slice(&saved).unwrap()).unwrap();
        cases.push(json!({"vendor":m.vendor,"product":m.product,"prepared":{"profile":p,"note":result.note},"encoded":String::from_utf8(encode(&serde_json::to_vec(&f).unwrap())).unwrap()}));
    }
    assert_eq!(cases.len(), atk::native_test_count());
    if let Some(path) = std::env::var_os("OPEN_HUB_COMPX_FILE_FIXTURES") {
        std::fs::write(path, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
    }
}
#[test]
fn wrong_identity_unsupported_model_and_invalid_dpi_never_become_plans() {
    let m = models().find(|m| m.product == 0x113e).unwrap();
    for (pointer, value) in [
        ("/deviceConfig/mouseConfig/cidMid/cid", json!(1)),
        ("/deviceConfig/mouseConfig/cidMid/mid", json!(39)),
        ("/deviceConfig/mouseConfig/dpiTotal", json!(0)),
        ("/deviceConfig/mouseConfig/dpiTotal", json!(9)),
        ("/deviceConfig/mouseConfig/dpiLevel", json!(2)),
        ("/deviceConfig/mouseConfig/reportRate", json!(6)),
        ("/deviceConfig/mouseConfig/reportRate", json!(2.5)),
        (
            "/deviceConfig/mouseConfig/dpiList/0/dpi/usesLogical",
            json!(false),
        ),
        ("/deviceConfig/mouseConfig/dpiList/0/dpi/xDpi", json!(0)),
        ("/deviceConfig/mouseConfig/dpiList/0/dpi/xDpi", json!(425)),
        ("/deviceConfig/mouseConfig/dpiList/0/dpi/yDpi", json!(30050)),
        ("/deviceConfig/mouseConfig/dpiList", json!([])),
    ] {
        let mut f = fixture(m);
        *f.pointer_mut(pointer).unwrap() = value;
        assert!(prepare(&f, m).is_err(), "{pointer}");
    }
    let f = encode(&serde_json::to_vec(&fixture(m)).unwrap());
    for (v, p) in [(0x046d, 0x113e), (0x3554, 0xf507), (0x373b, 0xffff)] {
        assert!(prepare_bytes(&f, v, p).is_err());
    }
}
#[test]
fn strict_envelope_rejects_bad_padding_duplicate_keys_and_non_utf8() {
    for bad in [
        b"".as_slice(),
        b"A",
        b"====",
        b"AA=A",
        b"AB==",
        b"AAB=",
        b"AA==AA==",
        b"AA-_",
    ] {
        assert!(decode(bad).is_err(), "{bad:?}");
    }
    let m = models().next().unwrap();
    for raw in [
        b"{\"deviceConfig\":{},\"deviceConfig\":{}}".as_slice(),
        b"{\"deep\":{\"x\":1,\"x\":2}}",
        b"{} trailing",
        b"[]",
        b"\xff",
    ] {
        assert!(prepare_bytes(&encode(raw), m.vendor, m.product).is_err());
    }
    let v = vec![b'A'; MAX + 4];
    assert!(decode(&v).is_err());
}
#[test]
fn explicit_file_rejects_wrong_extension_and_reads_without_device_access() {
    let m = models().next().unwrap();
    let root = std::env::temp_dir().join(format!("compx-file-{}", crate::profiles::new_id()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("配置.BIN");
    std::fs::write(&path, encode(&serde_json::to_vec(&fixture(m)).unwrap())).unwrap();
    assert!(prepare_file(&path, m.vendor, m.product, false, false, false).is_ok());
    assert!(prepare_file(
        &root.join("bad.json"),
        m.vendor,
        m.product,
        false,
        false,
        false
    )
    .is_err());
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(root).unwrap();
}
#[test]
#[ignore = "requires saved browser file-import profiles"]
fn browser_file_import_saves_execute_the_native_plan() {
    let bytes =
        std::fs::read(std::env::var_os("OPEN_HUB_COMPX_FILE_UI_PROFILES").unwrap()).unwrap();
    let profiles: Vec<Profile> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(profiles.len(), atk::native_test_count());
    for p in profiles {
        crate::profile_transfer::validate(&p).unwrap();
        let plan = p.multi.unwrap();
        let c = crate::multibrand::compile(&plan).unwrap();
        assert_eq!(c.settings.len(), 2);
        let (mut d, _) = atk::simulated(plan.product).unwrap();
        for s in c.settings {
            d.apply(&s).unwrap();
        }
        let state = d.read_state().unwrap();
        assert_eq!(state.dpi.unwrap().stages[0], DpiStage { x: 400, y: 800 });
        assert_eq!(state.report_rate, Some(500));
    }
}

fn button_fixture(m: &atk::Model, changes: [[u8; 3]; 5]) -> Value {
    let mut f = fixture(m);
    let defs = [
        [1, 1, 0],
        [1, 2, 0],
        [1, 4, 0],
        [1, 8, 0],
        [1, 16, 0],
        [2, 1, 0],
    ];
    let mut keys=(0..6).map(|i|json!({"defaultKey":defs[i],"defaultLabel":"untrusted label","changeKey":if i==0{defs[0]}else{changes[i-1]}})).collect::<Vec<_>>();
    keys.reverse();
    f["keyActions"] = json!([[keys]]);
    f
}
#[test]
fn full_known_button_tables_use_physical_default_keys_and_native_driver() {
    let choices = [
        [0, 0, 0],
        [1, 1, 0],
        [1, 2, 0],
        [1, 4, 0],
        [1, 8, 0],
        [1, 16, 0],
        [2, 1, 0],
        [2, 2, 0],
        [2, 3, 0],
        [3, 1, 0],
        [3, 2, 0],
        [9, 255, 0],
        [9, 161, 0],
        [9, 162, 0],
        [9, 163, 0],
        [9, 164, 0],
        [11, 1, 0],
        [11, 2, 0],
    ];
    let mut cases = vec![];
    for m in models() {
        for offset in 0..choices.len() {
            let changes = std::array::from_fn(|i| {
                if m.buttons == 5 && i == 4 {
                    [2, 1, 0]
                } else {
                    choices[(offset + i) % choices.len()]
                }
            });
            let f = button_fixture(m, changes);
            let prepared = prepare_with_buttons(
                &encode(&serde_json::to_vec(&f).unwrap()),
                m.vendor,
                m.product,
                true,
            )
            .unwrap();
            let plan = prepared.profile.multi.as_ref().unwrap();
            let c = crate::multibrand::compile(plan).unwrap();
            assert_eq!(c.settings.len(), usize::from(m.buttons) + 1);
            assert!(!c.software_input);
            let (mut d, _) = atk::simulated(m.product).unwrap();
            let before = d.read_state().unwrap();
            for s in &c.settings {
                d.apply(s).unwrap();
            }
            let after = d.read_state().unwrap();
            assert_eq!(after.buttons[&1], before.buttons[&1]);
            for s in &c.settings {
                if let Setting::Button { id, action } = s {
                    assert_eq!(&after.buttons[id], action);
                }
            }
            assert_eq!(after.lighting, before.lighting);
            assert_eq!(after.dpi_colors, before.dpi_colors);
            assert_eq!(after.tuning, before.tuning);
            crate::profile_transfer::validate(&prepared.profile).unwrap();
            if offset == 9 {
                cases.push(json!({"vendor":m.vendor,"product":m.product,"prepared":prepared}));
            }
        }
    }
    if let Some(path) = std::env::var_os("OPEN_HUB_COMPX_BUTTON_FILE_FIXTURES") {
        std::fs::write(path, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
    }
}
#[test]
fn unresolved_macro_shortcut_and_ambiguous_physical_keys_reject_entire_button_conversion() {
    let m = models().next().unwrap();
    let base = button_fixture(m, [[1, 2, 0], [1, 4, 0], [1, 8, 0], [1, 16, 0], [2, 1, 0]]);
    for field in ["changeKey", "defaultKey"] {
        for bad in [
            json!([6, 0, 3]),
            json!([5, 0, 0]),
            json!([255, 1, 0]),
            json!([1, 3, 0]),
            json!([1, 2, 1]),
            json!([1, 2]),
            json!([1, 2, 0, 0]),
            json!([1, -1, 0]),
        ] {
            let mut f = base.clone();
            f["keyActions"][0][0][0][field] = bad;
            assert!(prepare_with_buttons(
                &encode(&serde_json::to_vec(&f).unwrap()),
                m.vendor,
                m.product,
                true
            )
            .is_err());
            assert!(prepare(&f, m).is_ok());
        }
    }
    assert_eq!(buttons(&base, 5).unwrap().len(), 4);
    let mut extra = base.clone();
    extra["keyActions"][0][0][0]["changeKey"] = json!([2, 2, 0]);
    assert!(buttons(&extra, 5).is_err());
    let mut f = base.clone();
    f["keyActions"][0][0][0]["defaultKey"] = f["keyActions"][0][0][1]["defaultKey"].clone();
    assert!(buttons(&f, 6).is_err());
    let mut f = base.clone();
    f["keyActions"][0][0][5]["changeKey"] = json!([0, 0, 0]);
    assert!(buttons(&f, 6).is_err());
    for table in [json!([]), json!([[[]]]), json!([[], []])] {
        let mut f = base.clone();
        f["keyActions"] = table;
        assert!(buttons(&f, 6).is_err());
    }
}
#[test]
#[ignore = "requires actual browser button file-import saves"]
fn browser_button_file_imports_execute_all_saved_bindings() {
    let bytes =
        std::fs::read(std::env::var_os("OPEN_HUB_COMPX_BUTTON_FILE_UI_PROFILES").unwrap()).unwrap();
    let profiles: Vec<Profile> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(profiles.len(), atk::native_test_count());
    for p in profiles {
        crate::profile_transfer::validate(&p).unwrap();
        let plan = p.multi.unwrap();
        let c = crate::multibrand::compile(&plan).unwrap();
        assert_eq!(
            c.settings.len(),
            usize::from(atk::model(plan.vendor, plan.product).unwrap().buttons) + 1
        );
        let (mut d, _) = atk::simulated(plan.product).unwrap();
        let before = d.read_state().unwrap();
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        let state = d.read_state().unwrap();
        assert_eq!(state.buttons[&1], before.buttons[&1]);
        for s in c.settings {
            if let Setting::Button { id, action } = s {
                assert_eq!(state.buttons[&id], action);
            }
        }
    }
}

#[test]
fn original_shortcut_file_chords_convert_and_execute_on_every_connection() {
    let mut cases = vec![];
    for m in models() {
        let mut f = button_fixture(m, [[1, 2, 0], [1, 4, 0], [1, 8, 0], [1, 16, 0], [2, 1, 0]]);
        f["keyActions"][0][0][4]["changeKey"] = json!([5, 0, 0]);
        f["deviceConfig"]["mouseConfig"]["shortcutAction"] =
            json!({"right":[["000",0,129,0],["001",0,6,0],["001",1,6,0],["000",1,129,0]]});
        let prepared = prepare_with_buttons(
            &encode(&serde_json::to_vec(&f).unwrap()),
            m.vendor,
            m.product,
            true,
        )
        .unwrap();
        let plan = prepared.profile.multi.as_ref().unwrap();
        let wanted = gearwise_core::model::ButtonAction::Key {
            usage: 6,
            mods: gearwise_core::model::Modifiers(129),
        };
        assert!(plan.settings.contains(&Setting::Button {
            id: 2,
            action: wanted
        }));
        let c = crate::multibrand::compile(plan).unwrap();
        assert!(!c.software_input);
        let (mut d, _) = atk::simulated(m.product).unwrap();
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        let actual = d.read_state().unwrap();
        assert_eq!(
            actual.buttons[&2],
            gearwise_core::model::ButtonAction::Key {
                usage: 6,
                mods: gearwise_core::model::Modifiers(129)
            }
        );
        assert!(d.macro_definitions().is_empty());
        cases.push(json!({"vendor":m.vendor,"product":m.product,"prepared":prepared}));
    }
    if let Some(path) = std::env::var_os("OPEN_HUB_COMPX_CHORD_FILE_FIXTURES") {
        std::fs::write(path, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
    }
}
#[test]
fn file_shortcut_missing_body_binary_kind_and_event_balance_are_validated() {
    let value = json!([
        ["000", 0, 3, 0],
        ["001", 0, 6, 0],
        ["001", 1, 6, 0],
        ["000", 1, 3, 0]
    ]);
    assert!(shortcut_chord(&value).is_ok());
    for bad in [
        json!(null),
        json!([]),
        json!([["000", 0, 3, 0], ["001", 0, 6, 0]]),
        json!([["1000", 0, 6, 0], ["001", 1, 6, 0]]),
        json!([["001", 0, 6], ["001", 1, 6, 0]]),
        json!([[1, 0, 6, 0], [1, 1, 5, 0]]),
    ] {
        assert!(shortcut_chord(&bad).is_err());
    }
}

#[test]
#[ignore = "requires actual browser chord import/editor output"]
fn browser_chord_import_and_editor_saves_preserve_both_modifier_sides() {
    let bytes =
        std::fs::read(std::env::var_os("OPEN_HUB_COMPX_CHORD_FILE_UI_PROFILES").unwrap()).unwrap();
    let profiles: Vec<Profile> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(profiles.len(), atk::native_test_count() * 2);
    for (index, p) in profiles.into_iter().enumerate() {
        crate::profile_transfer::validate(&p).unwrap();
        let plan = p.multi.unwrap();
        let usage = if index % 2 == 0 { 6 } else { 7 };
        assert!(plan.settings.contains(&Setting::Button {
            id: 2,
            action: gearwise_core::model::ButtonAction::Key {
                usage,
                mods: gearwise_core::model::Modifiers(129)
            }
        }));
        let c = crate::multibrand::compile(&plan).unwrap();
        assert!(!c.software_input);
        let (mut d, _) = atk::simulated(plan.product).unwrap();
        for s in c.settings {
            d.apply(&s).unwrap();
        }
        let state = d.read_state().unwrap();
        assert_eq!(
            state.buttons[&2],
            gearwise_core::model::ButtonAction::Key {
                usage,
                mods: gearwise_core::model::Modifiers(129)
            }
        );
        assert!(d.macro_definitions().is_empty());
    }
}

fn extras_fixture(m: &atk::Model) -> Value {
    let mut f = button_fixture(m, [[1, 2, 0], [1, 4, 0], [1, 8, 0], [1, 16, 0], [2, 1, 0]]);
    let mouse = &mut f["deviceConfig"]["mouseConfig"];
    mouse["keyDebounceTime"] = json!(4);
    mouse["moveSync"] = json!(0);
    mouse["lineCorrection"] = json!(1);
    mouse["rippleCorrection"] = json!(1);
    mouse["sleepTime"] = json!(120);
    mouse["sensorSleepEnabled"] = json!(0);
    mouse["sensorSleepTime"] = json!(300);
    mouse["silentHeight"] = json!(2);
    mouse["sensorModel"] = json!(255);
    for (i, item) in mouse["dpiList"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        item["color"] = json!({"r":i*20,"g":255-i*20,"b":i+5});
    }
    f["deviceConfig"]["lightConfig"] = json!({"enabled":1,"mode":8,"brightness":2,"speed":0});
    f
}
fn selected(f: &Value, m: &atk::Model, tuning: bool, lighting: bool) -> Result<Prepared> {
    prepare_selected(
        &encode(&serde_json::to_vec(f).unwrap()),
        m.vendor,
        m.product,
        true,
        tuning,
        lighting,
    )
}
#[test]
fn selected_performance_and_light_files_save_compile_execute_and_preserve_unselected_fields() {
    let mut cases = vec![];
    for m in models() {
        let f = extras_fixture(m);
        let light = atk::palette_stages(m.vendor, m.product) > 0;
        let prepared = selected(&f, m, true, light).unwrap();
        let mut store = crate::profiles::ProfileStore::default();
        crate::profile_transfer::merge(&mut store, vec![prepared.profile.clone()]).unwrap();
        let plan = prepared.profile.multi.as_ref().unwrap();
        let c = crate::multibrand::compile(plan).unwrap();
        assert!(!c.software_input);
        let (mut d, _) = atk::simulated(m.product).unwrap();
        let before = d.read_state().unwrap();
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        let after = d.read_state().unwrap();
        let t = after.tuning.unwrap();
        assert_eq!(t.debounce_ms, Some(4));
        assert_eq!(t.motion_sync, Some(false));
        assert_eq!(t.ripple_control, Some(true));
        assert_eq!(t.led_idle_seconds, Some(120));
        assert_eq!(t.sensor_sleep_enabled, Some(false));
        assert_eq!(t.sensor_sleep_seconds, Some(300));
        if atk::tuning_capabilities(m.vendor, m.product)
            .unwrap()
            .angle_snapping
        {
            assert_eq!(t.angle_snapping, Some(true));
        } else {
            assert_eq!(t.angle_snapping, before.tuning.unwrap().angle_snapping);
        }
        if light {
            let i = after.dpi_indicator.unwrap();
            assert!(i.enabled);
            assert_eq!(i.mode, gearwise_core::model::DpiIndicatorMode::Breathing);
            assert_eq!(i.brightness, gearwise_core::model::IndicatorLevel::High);
            assert_eq!(i.speed, gearwise_core::model::IndicatorLevel::Low);
            assert_eq!(after.dpi_colors.len(), 8);
            assert_eq!(
                after.dpi_colors[&7],
                gearwise_core::model::Rgb {
                    r: 140,
                    g: 115,
                    b: 12
                }
            );
        } else {
            assert_eq!(after.dpi_indicator, before.dpi_indicator);
            assert_eq!(after.dpi_colors, before.dpi_colors);
        }
        assert!(prepared.note.contains("已支持的性能与省电字段"));
        assert_eq!(after.buttons[&1], before.buttons[&1]);
        cases.push(json!({"vendor":m.vendor,"product":m.product,"includeLighting":light,"prepared":prepared}));
    }
    if let Some(path) = std::env::var_os("OPEN_HUB_COMPX_EXTRAS_FILE_FIXTURES") {
        std::fs::write(path, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
    }
}
#[test]
fn exact_oem_modes_brightness_and_speed_enums_preserve_all_palette_slots() {
    for m in models().filter(|m| atk::palette_stages(m.vendor, m.product) > 0) {
        for (mode, enabled, expected) in [
            (0, 0, gearwise_core::model::DpiIndicatorMode::Unspecified),
            (8, 1, gearwise_core::model::DpiIndicatorMode::Breathing),
            (9, 1, gearwise_core::model::DpiIndicatorMode::Static),
        ] {
            let mut f = extras_fixture(m);
            f["deviceConfig"]["lightConfig"]["mode"] = json!(mode);
            f["deviceConfig"]["lightConfig"]["enabled"] = json!(enabled);
            let p = selected(&f, m, false, true).unwrap();
            let (mut d, _) = atk::simulated(m.product).unwrap();
            for s in &p.profile.multi.unwrap().settings {
                d.apply(s).unwrap();
            }
            let state = d.read_state().unwrap();
            assert_eq!(state.dpi_indicator.unwrap().mode, expected);
            assert_eq!(
                state.dpi_colors[&7],
                gearwise_core::model::Rgb {
                    r: 140,
                    g: 115,
                    b: 12
                }
            );
        }
        for bright in 0..=2 {
            for speed in 0..=2 {
                let mut f = extras_fixture(m);
                f["deviceConfig"]["lightConfig"]["brightness"] = json!(bright);
                f["deviceConfig"]["lightConfig"]["speed"] = json!(speed);
                assert!(selected(&f, m, false, true).is_ok());
            }
        }
    }
}
#[test]
fn invalid_selected_fields_refuse_whole_import_but_unselected_categories_remain_explicitly_excluded(
) {
    let m = models().find(|m| m.product == 0x113e).unwrap();
    for (pointer, bad, tuning, lighting) in [
        ("/deviceConfig/mouseConfig/moveSync", json!(2), true, false),
        (
            "/deviceConfig/mouseConfig/sleepTime",
            json!(12),
            true,
            false,
        ),
        (
            "/deviceConfig/mouseConfig/sensorSleepTime",
            json!(30.5),
            true,
            false,
        ),
        (
            "/deviceConfig/mouseConfig/sensorSleepEnabled",
            json!(true),
            true,
            false,
        ),
        (
            "/deviceConfig/mouseConfig/silentHeight",
            json!(3),
            true,
            false,
        ),
        ("/deviceConfig/lightConfig/mode", json!(1), false, true),
        ("/deviceConfig/lightConfig/enabled", json!(0), false, true),
        (
            "/deviceConfig/lightConfig/brightness",
            json!(255),
            false,
            true,
        ),
        ("/deviceConfig/lightConfig/speed", json!(3), false, true),
        (
            "/deviceConfig/mouseConfig/dpiList/7/color/r",
            json!(256),
            false,
            true,
        ),
        (
            "/deviceConfig/mouseConfig/dpiList/7/color",
            json!([1, 2, 3]),
            false,
            true,
        ),
    ] {
        let mut f = extras_fixture(m);
        *f.pointer_mut(pointer).unwrap() = bad;
        assert!(selected(&f, m, tuning, lighting).is_err(), "{pointer}");
        assert!(selected(&f, m, false, false).is_ok());
    }
    let mut f = extras_fixture(m);
    f["deviceConfig"]["mouseConfig"]
        .as_object_mut()
        .unwrap()
        .remove("sensorSleepTime");
    assert!(selected(&f, m, true, false).is_err());
    for m in models().filter(|m| atk::palette_stages(m.vendor, m.product) == 0) {
        assert!(selected(&extras_fixture(m), m, false, true).is_err());
    }
    let mut f = extras_fixture(m);
    f["deviceConfig"]["mouseConfig"]["dpiList"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(selected(&f, m, false, true).is_err());
}
#[test]
fn unsupported_performance_fields_are_not_fabricated_and_exact_lod_model_is_enforced() {
    let m = models().find(|m| m.product == 0x103f).unwrap();
    let mut f = extras_fixture(m);
    f["deviceConfig"]["mouseConfig"]["lineCorrection"] = json!(255);
    f["deviceConfig"]["mouseConfig"]["silentHeight"] = json!(255);
    let p = selected(&f, m, true, false).unwrap();
    let plan = p.profile.multi.unwrap();
    let t = plan
        .settings
        .iter()
        .find_map(|s| {
            if let Setting::Tuning(v) = s {
                Some(v)
            } else {
                None
            }
        })
        .unwrap();
    assert!(t.angle_snapping.is_none());
    assert!(t.lift_off_microns.is_none());
    let m = models().find(|m| m.product == 0x1017).unwrap();
    let mut f = extras_fixture(m);
    f["deviceConfig"]["mouseConfig"]["silentHeight"] = json!(3);
    assert!(selected(&f, m, true, false)
        .unwrap()
        .profile
        .multi
        .unwrap()
        .settings
        .iter()
        .any(|s| matches!(s,Setting::Tuning(t)if t.lift_off_microns==Some(700))));
}
#[test]
#[ignore = "requires current browser selected-extra saves"]
fn browser_selected_extras_saved_profiles_import_and_execute_exact_categories() {
    let bytes =
        std::fs::read(std::env::var_os("OPEN_HUB_COMPX_EXTRAS_FILE_UI_PROFILES").unwrap()).unwrap();
    let profiles: Vec<Profile> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(profiles.len(), atk::native_test_count());
    for p in profiles {
        crate::profile_transfer::validate(&p).unwrap();
        let plan = p.multi.unwrap();
        let c = crate::multibrand::compile(&plan).unwrap();
        let (mut d, _) = atk::simulated(plan.product).unwrap();
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        let state = d.read_state().unwrap();
        let t = state.tuning.unwrap();
        assert_eq!(t.motion_sync, Some(false));
        assert_eq!(t.led_idle_seconds, Some(120));
        assert_eq!(t.sensor_sleep_seconds, Some(300));
        if atk::palette_stages(plan.vendor, plan.product) > 0 {
            assert_eq!(
                state.dpi_colors[&7],
                gearwise_core::model::Rgb {
                    r: 140,
                    g: 115,
                    b: 12
                }
            );
            assert_eq!(
                state.dpi_indicator.unwrap().mode,
                gearwise_core::model::DpiIndicatorMode::Breathing
            );
        }
    }
}
