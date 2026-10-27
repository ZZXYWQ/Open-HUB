use super::*;
struct Fixture {
    root: std::path::PathBuf,
    path: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "openhub-sc620-preview-{}",
            crate::profiles::new_id()
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("source.db");
        std::fs::write(
            &path,
            include_bytes!("../tests/fixtures/yjx-sc620/oem-profile.sqlite3"),
        )
        .unwrap();
        Self { root, path }
    }
    fn sql(&self, sql: &str) {
        rusqlite::Connection::open(&self.path)
            .unwrap()
            .execute_batch(sql)
            .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
#[test]
fn preview_lists_real_file_ids_and_all_source_sections_without_applicable_profile() {
    let f = Fixture::new();
    let bytes = std::fs::read(&f.path).unwrap();
    let p = preview(&f.path, None).unwrap();
    assert_eq!(p.profile_ids, [7, 42]);
    assert_eq!(p.source.settings.profile_id, 7);
    assert_eq!(p.keys.len(), 11);
    assert_eq!(p.macros.len(), 2);
    assert!(p.static_subset_convertible);
    assert!(!p.hardware_apply_available);
    assert_eq!(p.keys[3].macro_slot, Some(1));
    assert_eq!(p.keys[4].macro_slot, Some(0));
    assert_eq!(p.keys[3].record, Some(4));
    assert_eq!(preview(&f.path, Some(42)).unwrap().keys.len(), 1);
    assert!(preview(&f.path, Some(99)).is_err());
    assert_eq!(bytes, std::fs::read(&f.path).unwrap());
}
#[test]
fn unsupported_keys_macros_and_settings_remain_visible_and_block_whole_conversion() {
    let f = Fixture::new();
    f.sql("UPDATE t_configdata SET value='5' WHERE profile=7 AND key='dpi_flag';UPDATE t_key_macro_data SET macro_type=27 WHERE key_id=1;INSERT INTO t_macro_data VALUES(11,'unknown macro',0,1,0,10);INSERT INTO t_macrorecord VALUES(100,11,99,0,'unknown event',123,0);");
    let p = preview(&f.path, None).unwrap();
    assert!(!p.static_subset_convertible);
    assert!(!p.issues.is_empty());
    assert!(p.keys[0].problem.is_some());
    assert!(p.macros[1].problem.is_some());
    assert_eq!(p.source.settings.unconverted["dpi_flag"], "5");
    assert_eq!(p.source.macros[1].events[0].kind, 99);
    let path = f.root.join("archive.json");
    export(&path, p.source.clone()).unwrap();
    let next = preview(&path, None).unwrap();
    assert_eq!(next.source, p.source);
    assert!(!next.static_subset_convertible);
}
#[test]
fn source_archives_roundtrip_every_field_and_refuse_overwriting_original_or_existing_files() {
    let f = Fixture::new();
    let p = preview(&f.path, None).unwrap();
    let path = f.root.join("archive.json");
    export(&path, p.source.clone()).unwrap();
    assert!(crate::profile_transfer::preview("openhub", Some(path.display().to_string())).is_err());
    let roundtrip = preview(&path, None).unwrap();
    assert_eq!(roundtrip.source, p.source);
    assert_eq!(roundtrip.profile_ids, [7]);
    assert!(preview(&path, Some(42)).is_err());
    let bytes = std::fs::read(&path).unwrap();
    assert!(export(&path, p.source.clone()).is_err());
    assert_eq!(bytes, std::fs::read(&path).unwrap());
    let original = std::fs::read(&f.path).unwrap();
    assert!(export(&f.path, p.source).is_err());
    assert_eq!(original, std::fs::read(&f.path).unwrap());
}
#[test]
fn archive_unknown_fields_summary_tampering_event_order_and_limits_cannot_be_silently_ignored() {
    let f = Fixture::new();
    let source = preview(&f.path, None).unwrap().source;
    let base = serde_json::to_value(Pack {
        format: FORMAT.into(),
        source,
    })
    .unwrap();
    for case in 0..8 {
        let mut bad = base.clone();
        match case {
            0 => bad["unknown"] = true.into(),
            1 => bad["source"]["keys"][0]["unknown"] = true.into(),
            2 => bad["source"]["settings"]["patch"]["report_rate_hz"] = 1000.into(),
            3 => bad["source"]["macros"][0]["events"][1]["index"] = 0.into(),
            4 => bad["source"]["macros"][0]["playback"] = "Once".into(),
            6 => bad["source"]["keys"][0]["id"] = 9_007_199_254_740_992i64.into(),
            7 => {
                bad["source"]["macros"][0]["events"][0]["record_id"] =
                    9_007_199_254_740_992i64.into()
            }
            _ => bad["source"]["keys"][0]["description"] = "x".repeat(1025).into(),
        };
        let path = f.root.join(format!("bad{case}.json"));
        std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(preview(&path, None).is_err(), "case{case}");
    }
}
#[test]
fn empty_oversized_profiles_and_schema_errors_do_not_create_missing_files() {
    let f = Fixture::new();
    let missing = f.root.join("missing.db");
    assert!(preview(&missing, None).is_err());
    assert!(!missing.exists());
    f.sql("DELETE FROM t_configdata");
    assert!(preview(&f.path, None).is_err());
    let f = Fixture::new();
    f.sql("WITH RECURSIVE n(x) AS (SELECT 100 UNION ALL SELECT x+1 FROM n WHERE x<164) INSERT INTO t_configdata(profile,key,value) SELECT x,'x','1' FROM n;");
    assert!(preview(&f.path, None).is_err());
}
#[test]
fn archive_source_replays_through_production_static_compiler_to_original_complete_reports() {
    let f = Fixture::new();
    let source = preview(&f.path, None).unwrap().source;
    let path = f.root.join("archive.json");
    export(&path, source).unwrap();
    replay(&preview(&path, None).unwrap().source);
}
fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}
fn replay(source: &Source) {
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
    let p = source
        .prepare(
            &config,
            &pages,
            &super::super::macros::Image::compile(&[]).unwrap(),
        )
        .unwrap();
    let gold: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/yjx-sc620/oem-profile.json")).unwrap();
    assert_eq!(p.reports.len(), 5);
    for (r, g) in p
        .reports
        .iter()
        .zip(gold["reports_hex"].as_array().unwrap())
    {
        assert_eq!(r.to_vec(), hex(g.as_str().unwrap()));
    }
}
#[test]
#[ignore = "requires explicit SC620 UI fixture output path"]
fn emit_actual_preview_dtos_for_built_ui() {
    let path = std::env::var_os("OPEN_HUB_SC620_PREVIEW_DTOS").unwrap();
    let f = Fixture::new();
    let positive = preview(&f.path, None).unwrap();
    let second = preview(&f.path, Some(42)).unwrap();
    f.sql("UPDATE t_key_macro_data SET macro_type=27 WHERE key_id=1;UPDATE t_configdata SET value='5' WHERE profile=7 AND key='dpi_flag';INSERT INTO t_macro_data VALUES(11,'unknown macro',0,1,0,10);INSERT INTO t_macrorecord VALUES(100,11,99,0,'unknown event',123,0);");
    let blocked = preview(&f.path, None).unwrap();
    std::fs::write(
        path,
        serde_json::to_vec_pretty(
            &serde_json::json!({"positive":positive,"second":second,"blocked":blocked}),
        )
        .unwrap(),
    )
    .unwrap();
}
#[test]
#[ignore = "requires actual built UI export request output"]
fn built_ui_export_preserves_complete_source_and_replays_native_reports() {
    let path = std::env::var_os("OPEN_HUB_SC620_UI_EXPORT").unwrap();
    let actual: Source = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let f = Fixture::new();
    assert_eq!(actual, preview(&f.path, None).unwrap().source);
    let dest = f.root.join("actual-ui-source.json");
    export(&dest, actual).unwrap();
    replay(&preview(&dest, None).unwrap().source);
}
