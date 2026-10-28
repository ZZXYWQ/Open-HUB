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
    assert!(prepare_file(&path, m.vendor, m.product, false).is_ok());
    assert!(prepare_file(&root.join("bad.json"), m.vendor, m.product, false).is_err());
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
