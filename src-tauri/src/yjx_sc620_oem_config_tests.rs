use super::*;
fn fact() -> serde_json::Value {
    serde_json::from_str(include_str!("../tests/fixtures/yjx-sc620/oem-config.json")).unwrap()
}
fn fields() -> BTreeMap<String, String> {
    serde_json::from_value(fact()["fields"].clone()).unwrap()
}
fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}
fn image() -> ConfigImage {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/yjx-sc620/configuration.json"
    ))
    .unwrap();
    ConfigImage::from_response(2, &hex(f["before_response_hex"].as_str().unwrap())).unwrap()
}
fn connection() -> rusqlite::Connection {
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE t_configdata(config_id INTEGER PRIMARY KEY AUTOINCREMENT,profile INTEGER,key TEXT,value TEXT);").unwrap();
    for (k, v) in fields() {
        c.execute(
            "INSERT INTO t_configdata(profile,key,value) VALUES(7,?1,?2)",
            [k, v],
        )
        .unwrap();
    }
    c
}
#[test]
fn all_native_report_rates_and_debounce_values_change_only_the_selected_bytes() {
    let original = image();
    for rate in [125, 250, 500, 1000] {
        for ms in 0..=20 {
            let next = original
                .patch(&ConfigPatch {
                    report_rate_hz: Some(rate),
                    debounce_ms: Some(ms),
                    ..ConfigPatch::default()
                })
                .unwrap();
            assert_eq!(next.report_rate_hz(), Some(rate));
            assert_eq!(next.debounce_ms(), Some(ms));
            for i in 0..46 {
                if i != 2 && i != 43 {
                    assert_eq!(next.bytes()[i], original.bytes()[i]);
                }
            }
            assert_eq!(next.page(), 2);
        }
    }
    assert_eq!(original, image());
}
#[test]
fn invalid_native_rates_and_debounce_refuse_without_clamping_or_modifying_image() {
    let original = image();
    for rate in [0, 124, 2000, 8000, 65535] {
        assert!(original
            .patch(&ConfigPatch {
                report_rate_hz: Some(rate),
                ..ConfigPatch::default()
            })
            .is_err());
    }
    for ms in [21, 255] {
        assert!(original
            .patch(&ConfigPatch {
                debounce_ms: Some(ms),
                ..ConfigPatch::default()
            })
            .is_err());
    }
    assert_eq!(original, image());
}
#[test]
fn unknown_read_values_are_not_replaced_with_known_defaults() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/yjx-sc620/configuration.json"
    ))
    .unwrap();
    let mut response = hex(f["before_response_hex"].as_str().unwrap());
    response[10] = 99;
    response[51] = 255;
    let old = ConfigImage::from_response(2, &response).unwrap();
    assert_eq!(old.report_rate_hz(), None);
    assert_eq!(old.debounce_ms(), None);
    let next = old
        .patch(&ConfigPatch {
            active: Some(0),
            ..ConfigPatch::default()
        })
        .unwrap();
    assert_eq!(next.bytes()[2], 99);
    assert_eq!(next.bytes()[43], 255);
}
#[test]
fn independent_sqlite_read_and_known_config_compile_match_complete_golden_report() {
    let root = std::env::temp_dir().join(format!(
        "openhub-sc620-config-{}",
        crate::profiles::new_id()
    ));
    std::fs::create_dir(&root).unwrap();
    let p = root.join("source.db");
    let b = include_bytes!("../tests/fixtures/yjx-sc620/oem-config.sqlite3");
    std::fs::write(&p, b).unwrap();
    let source = read(&p, 7).unwrap();
    assert_eq!(source.fields, fields());
    assert!(source.unconverted.is_empty());
    let compiled = source.prepare(&image()).unwrap();
    assert_eq!(
        compiled.compile_output().to_vec(),
        hex(fact()["expected_windows_report_hex"].as_str().unwrap())
    );
    assert_eq!(compiled.report_rate_hz(), Some(250));
    assert_eq!(compiled.debounce_ms(), Some(17));
    assert_eq!(compiled.page(), 2);
    assert_eq!(source.profile_id, 7);
    assert_eq!(read(&p, 42).unwrap().patch.report_rate_hz, Some(1000));
    assert_eq!(std::fs::read(&p).unwrap(), b);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_file(p).unwrap();
    std::fs::remove_dir(root).unwrap();
}
#[test]
fn unconverted_source_fields_remain_visible_and_block_whole_source_preparation() {
    let mut f = fields();
    f.insert("sensor_flag".into(), "28".into());
    f.insert("sleep_light".into(), "99".into());
    let src = convert(7, f.clone()).unwrap();
    assert_eq!(src.fields, f);
    assert_eq!(src.unconverted.len(), 2);
    let old = image();
    assert!(src.prepare(&old).is_err());
    assert_eq!(old, image());
}
#[test]
fn missing_unknown_duplicate_or_wrong_schema_does_not_create_a_partial_config() {
    let c = connection();
    c.execute("DELETE FROM t_configdata WHERE key='dpi_group5_rgb'", [])
        .unwrap();
    assert!(parse(&c, 7).is_err());
    let c = connection();
    c.execute_batch("INSERT INTO t_configdata(profile,key,value) VALUES(7,'report_rate','2');")
        .unwrap();
    assert!(parse(&c, 7).is_err());
    let c = connection();
    c.execute_batch("ALTER TABLE t_configdata RENAME TO actual;CREATE VIEW t_configdata AS SELECT * FROM actual;").unwrap();
    assert!(parse(&c, 7).is_err());
    let c = connection();
    c.execute_batch("ALTER TABLE t_configdata ADD COLUMN ignored INTEGER;")
        .unwrap();
    assert!(parse(&c, 7).is_err());
}
#[test]
fn decimal_fields_and_model_bounds_are_strict_and_preserve_unknown_colors() {
    for (key, value) in [
        ("report_rate", "0"),
        ("report_rate", "5"),
        ("button_respondtime", "21"),
        ("button_respondtime", "-1"),
        ("dpi_index", "6"),
        ("dpi_group0_value", "49"),
        ("dpi_group0_value", "26001"),
        ("dpi_group0_value", "50.0"),
        ("dpi_group0_rgb", "16777216"),
        ("light_mode", "7"),
        ("light_mode", " 5"),
        ("light_mode", "+5"),
        ("light_mode", ""),
    ] {
        let mut f = fields();
        f.insert(key.into(), value.into());
        assert!(convert(7, f).is_err(), "{key}={value}");
    }
    let src = convert(7, fields()).unwrap();
    let colors = src.patch.colors.unwrap();
    for i in 0..6 {
        let c = src.fields[&format!("dpi_group{i}_rgb")]
            .parse::<u32>()
            .unwrap();
        assert_eq!(colors[i], [c as u8, (c >> 8) as u8, (c >> 16) as u8]);
    }
}
#[test]
fn file_profile_selection_does_not_select_a_hardware_page_or_mix_profile_rows() {
    let c = connection();
    for (k, v) in fields() {
        c.execute(
            "INSERT INTO t_configdata(profile,key,value) VALUES(42,?1,?2)",
            [k, v],
        )
        .unwrap();
    }
    c.execute(
        "UPDATE t_configdata SET value='4' WHERE profile=42 AND key='report_rate'",
        [],
    )
    .unwrap();
    let a = parse(&c, 7).unwrap();
    let b = parse(&c, 42).unwrap();
    assert_eq!(a.patch.report_rate_hz, Some(250));
    assert_eq!(b.patch.report_rate_hz, Some(1000));
    assert_eq!(a.prepare(&image()).unwrap().page(), 2);
    assert_eq!(b.prepare(&image()).unwrap().page(), 2);
    for profile in [-1, 0, 999] {
        assert!(parse(&c, profile).is_err());
    }
}
#[test]
fn preview_tampering_cannot_bypass_unconverted_fields_or_change_settings() {
    let mut source = convert(7, fields()).unwrap();
    source.patch.debounce_ms = Some(0);
    assert!(source.prepare(&image()).is_err());
    let mut f = fields();
    f.insert("e-sports_flag".into(), "2".into());
    let mut source = convert(7, f).unwrap();
    source.unconverted.clear();
    assert!(source.prepare(&image()).is_err());
    let source = convert(7, fields()).unwrap();
    let copy: Source = serde_json::from_value(serde_json::to_value(&source).unwrap()).unwrap();
    assert_eq!(copy, source);
    assert!(copy.prepare(&image()).is_ok());
}
#[test]
fn oversize_wrong_type_or_null_values_are_rejected_before_returning_a_preview() {
    for sql in [
        "UPDATE t_configdata SET value=NULL WHERE key='light_mode';",
        "UPDATE t_configdata SET value=x'ff' WHERE key='light_mode';",
        "UPDATE t_configdata SET value=printf('%0600d',5) WHERE key='light_mode';",
    ] {
        let c = connection();
        c.execute_batch(sql).unwrap();
        assert!(parse(&c, 7).is_err());
    }
    let mut f = fields();
    for i in 0..50 {
        f.insert(format!("extra{i}"), "0".into());
    }
    assert!(convert(7, f).is_err());
}
