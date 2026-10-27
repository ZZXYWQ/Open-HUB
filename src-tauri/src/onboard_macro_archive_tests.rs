use super::*;
use gearwise_core::{
    model::Modifiers,
    profile::{MacroMode, MacroStep},
};
fn definition() -> Macro {
    Macro {
        id: "portable-native".into(),
        name: "可迁移板载宏".into(),
        mode: MacroMode::Repeat(3),
        steps: vec![
            MacroStep::KeyDown(228),
            MacroStep::KeyDown(4),
            MacroStep::Delay(17),
            MacroStep::KeyUp(4),
            MacroStep::KeyUp(228),
        ],
    }
}
struct Files {
    root: std::path::PathBuf,
    path: std::path::PathBuf,
}
impl Files {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("native-macro-file-{}", crate::profiles::new_id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("macro.json");
        Self { root, path }
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
#[test]
fn file_roundtrip_keeps_complete_definition_without_a_physical_target() {
    let f = Files::new();
    let m = definition();
    write(&f.path, m.clone()).unwrap();
    let original = std::fs::read(&f.path).unwrap();
    assert_eq!(read(&f.path).unwrap(), m);
    assert_eq!(original, std::fs::read(&f.path).unwrap());
    let raw: serde_json::Value = serde_json::from_slice(&original).unwrap();
    assert_eq!(raw.as_object().unwrap().len(), 2);
    assert!(raw.get("selection").is_none());
    assert!(
        crate::profile_transfer::preview("openhub", Some(f.path.display().to_string())).is_err()
    );
}
#[test]
fn unknown_metadata_device_routing_and_event_fields_refuse_instead_of_dropping() {
    let base = serde_json::to_value(Archive {
        format: FORMAT.into(),
        definition: definition(),
    })
    .unwrap();
    for case in 0..4 {
        let mut bad = base.clone();
        match case {
            0 => bad["selection"] = serde_json::json!({"expected_device":"unit:foreign"}),
            1 => bad["definition"]["expected_connection"] = "foreign".into(),
            2 => bad["definition"]["steps"][2]["unknown"] = 1.into(),
            _ => bad["format"] = "open-hub-profiles/2".into(),
        };
        assert!(
            decode(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "case{case}"
        );
    }
}
#[test]
fn duplicate_nested_fields_trailing_input_and_bad_encoding_refuse() {
    let raw = serde_json::to_string(&Archive {
        format: FORMAT.into(),
        definition: definition(),
    })
    .unwrap();
    let bad = raw.replace("\"Delay\":17", "\"Delay\":17,\"Delay\":18");
    assert_ne!(bad, raw);
    assert!(decode(bad.as_bytes()).is_err());
    assert!(decode(format!("{raw} true").as_bytes()).is_err());
    assert!(decode(&[255, 254]).is_err());
}
#[test]
fn invalid_or_unbalanced_body_never_creates_an_archive() {
    let f = Files::new();
    for steps in [
        vec![MacroStep::KeyDown(4)],
        vec![MacroStep::Text("x".into())],
        vec![
            MacroStep::KeyDown(224),
            MacroStep::KeyTap {
                usage: 4,
                mods: Modifiers(1),
            },
            MacroStep::KeyUp(224),
        ],
    ] {
        let mut m = definition();
        m.steps = steps;
        assert!(write(&f.path, m).is_err());
        assert!(!f.path.exists());
    }
    for mode in [
        MacroMode::Repeat(0),
        MacroMode::Repeat(1),
        MacroMode::WhileHeld,
        MacroMode::Toggle,
    ] {
        let mut m = definition();
        m.mode = mode;
        assert!(write(&f.path, m).is_err());
        assert!(!f.path.exists());
    }
}
#[test]
fn export_refuses_existing_files_and_import_refuses_missing_or_oversized_sources() {
    let f = Files::new();
    assert!(read(&f.path).is_err());
    assert!(!f.path.exists());
    write(&f.path, definition()).unwrap();
    let before = std::fs::read(&f.path).unwrap();
    assert!(write(&f.path, definition()).is_err());
    assert_eq!(before, std::fs::read(&f.path).unwrap());
    let big = f.root.join("oversized.json");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(8 * 1024 * 1024 + 1)
        .unwrap();
    assert!(read(&big).is_err());
}
#[test]
fn once_and_high_finite_count_keep_native_bytes_after_reload() {
    let f = Files::new();
    for (count, mode) in [(0, MacroMode::Once), (255, MacroMode::Repeat(255))] {
        let mut m = definition();
        m.mode = mode;
        m.steps = vec![MacroStep::Delay(1)];
        let path = f.root.join(format!("{count}.json"));
        write(&path, m.clone()).unwrap();
        let loaded = read(&path).unwrap();
        assert_eq!(loaded, m);
        assert_eq!(
            crate::onboard_macro_plan::compile(&loaded, 256).unwrap(),
            crate::onboard_macro_plan::compile(&m, 256).unwrap()
        );
    }
}
#[test]
#[ignore = "requires explicit portable macro UI DTO output"]
fn emit_portable_macro_dto() {
    let f = Files::new();
    write(&f.path, definition()).unwrap();
    let loaded = read(&f.path).unwrap();
    std::fs::write(
        std::env::var_os("OPEN_HUB_NATIVE_MACRO_FILE_DTO").unwrap(),
        serde_json::to_vec_pretty(&loaded).unwrap(),
    )
    .unwrap();
}
