use super::*;
fn fact() -> serde_json::Value {
    serde_json::from_str(include_str!("../tests/fixtures/yjx-sc620/oem-tuning.json")).unwrap()
}
fn fields() -> BTreeMap<String, String> {
    serde_json::from_value(fact()["fields"].clone()).unwrap()
}
fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}
fn response() -> Vec<u8> {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/yjx-sc620/configuration.json"
    ))
    .unwrap();
    hex(f["before_response_hex"].as_str().unwrap())
}
fn image() -> ConfigImage {
    ConfigImage::from_response(2, &response()).unwrap()
}
#[test]
fn every_known_tuning_combination_keeps_dpi_lighting_and_unselected_bytes() {
    let old = image();
    for lod in [1, 2] {
        for sleep in [1, 3, 5, 10, 20, 30, 60] {
            for reverse in [false, true] {
                for esports in [false, true] {
                    for flags in 0..8 {
                        let next = old
                            .patch(&ConfigPatch {
                                lod_mm: Some(lod),
                                sleep_minutes: Some(sleep),
                                scroll_reverse: Some(reverse),
                                esports: Some(esports),
                                ripple_control: Some(flags & 1 != 0),
                                angle_snap: Some(flags & 2 != 0),
                                motion_sync: Some(flags & 4 != 0),
                                ..ConfigPatch::default()
                            })
                            .unwrap();
                        assert_eq!(next.lod_mm(), Some(lod));
                        assert_eq!(next.sleep_minutes(), Some(sleep));
                        assert_eq!(next.scroll_reverse(), Some(reverse));
                        assert_eq!(next.esports(), Some(esports));
                        assert_eq!(next.ripple_control(), flags & 1 != 0);
                        assert_eq!(next.angle_snap(), flags & 2 != 0);
                        assert_eq!(next.motion_sync(), flags & 4 != 0);
                        assert_eq!(next.bytes()[42] & !0x31, old.bytes()[42] & !0x31);
                        for i in 0..46 {
                            if ![40, 41, 42, 44, 45].contains(&i) {
                                assert_eq!(next.bytes()[i], old.bytes()[i]);
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(old, image());
}
#[test]
fn individual_sensor_changes_preserve_all_other_bits_for_every_original_byte() {
    for raw in 0..=255 {
        let mut r = response();
        r[50] = raw;
        let old = ConfigImage::from_response(2, &r).unwrap();
        for on in [false, true] {
            for (patch, mask) in [
                (
                    ConfigPatch {
                        ripple_control: Some(on),
                        ..ConfigPatch::default()
                    },
                    0x10,
                ),
                (
                    ConfigPatch {
                        angle_snap: Some(on),
                        ..ConfigPatch::default()
                    },
                    1,
                ),
                (
                    ConfigPatch {
                        motion_sync: Some(on),
                        ..ConfigPatch::default()
                    },
                    0x20,
                ),
            ] {
                let next = old.patch(&patch).unwrap();
                assert_eq!(next.bytes()[42], (raw & !mask) | if on { mask } else { 0 });
                assert_eq!(old.bytes()[42], raw);
            }
        }
    }
}
#[test]
fn unknown_original_power_and_lod_values_are_retained_by_other_edits() {
    let mut r = response();
    for offset in [40, 41, 44, 45] {
        r[8 + offset] = 255;
    }
    let old = ConfigImage::from_response(2, &r).unwrap();
    assert_eq!(old.lod_mm(), None);
    assert_eq!(old.sleep_minutes(), None);
    assert_eq!(old.scroll_reverse(), None);
    assert_eq!(old.esports(), None);
    let next = old
        .patch(&ConfigPatch {
            active: Some(0),
            ..ConfigPatch::default()
        })
        .unwrap();
    for offset in [40, 41, 44, 45] {
        assert_eq!(next.bytes()[offset], 255);
    }
}
#[test]
fn invalid_native_lod_or_sleep_does_not_get_normalized() {
    let old = image();
    for lod in [0, 3, 255] {
        assert!(old
            .patch(&ConfigPatch {
                lod_mm: Some(lod),
                ..ConfigPatch::default()
            })
            .is_err());
    }
    for minutes in [0, 2, 4, 31, 255] {
        assert!(old
            .patch(&ConfigPatch {
                sleep_minutes: Some(minutes),
                ..ConfigPatch::default()
            })
            .is_err());
    }
    assert_eq!(old, image());
}
#[test]
fn original22_field_sqlite_read_and_prepare_matches_independent_complete_report() {
    let root = std::env::temp_dir().join(format!(
        "openhub-sc620-tuning-{}",
        crate::profiles::new_id()
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("source.db");
    std::fs::write(
        &path,
        include_bytes!("../tests/fixtures/yjx-sc620/oem-config.sqlite3"),
    )
    .unwrap();
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute("DELETE FROM t_configdata WHERE profile=7", [])
        .unwrap();
    for (k, v) in fields() {
        c.execute(
            "INSERT INTO t_configdata(profile,key,value) VALUES(7,?1,?2)",
            [k, v],
        )
        .unwrap();
    }
    drop(c);
    let before = std::fs::read(&path).unwrap();
    let src = read(&path, 7).unwrap();
    assert_eq!(src.fields.len(), 22);
    assert!(src.unconverted.is_empty());
    let prepared = src.prepare(&image()).unwrap();
    assert_eq!(
        prepared.compile_output().to_vec(),
        hex(fact()["expected_windows_report_hex"].as_str().unwrap())
    );
    assert_eq!(prepared.bytes()[42] & !0x31, 0x84);
    assert_eq!(src.profile_id, 7);
    assert_eq!(prepared.page(), 2);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(root).unwrap();
}
#[test]
fn source_sensor_bits_follow_oem_remap_and_retain_source_only_low_bits() {
    for native in 0..8 {
        for metadata in 0..4 {
            let n = ((native & 1) << 2) | ((native & 2) << 2) | ((native & 4) << 2) | metadata;
            let mut f = fields();
            f.insert("sensor_flag".into(), n.to_string());
            let src = convert(7, f).unwrap();
            assert_eq!(src.fields["sensor_flag"], n.to_string());
            assert!(src.unconverted.is_empty());
            let got = src.prepare(&image()).unwrap();
            let bits = if native & 1 != 0 { 0x10 } else { 0 }
                | if native & 2 != 0 { 1 } else { 0 }
                | if native & 4 != 0 { 0x20 } else { 0 };
            assert_eq!(got.bytes()[42], 0x84 | bits);
        }
    }
}
#[test]
fn unsupported_source_values_remain_unconverted_and_block_whole_source_preparation() {
    for (key, v) in [
        ("liftoff_height", "3"),
        ("scroll_flag", "2"),
        ("e-sports_flag", "0"),
        ("sleep_light", "2"),
        ("sensor_flag", "128"),
        ("dpi_flag", "5"),
    ] {
        let mut f = fields();
        f.insert(key.into(), v.into());
        let src = convert(7, f).unwrap();
        assert_eq!(src.unconverted.get(key).map(String::as_str), Some(v));
        assert!(src.prepare(&image()).is_err());
    }
}
#[test]
fn original_source_modes_are_not_replaced_with_bool_or_combo_index_conventions() {
    for (value, on) in [("1", false), ("2", true)] {
        let mut f = fields();
        f.insert("e-sports_flag".into(), value.into());
        let src = convert(7, f).unwrap();
        assert_eq!(src.patch.esports, Some(on));
        assert_eq!(
            src.prepare(&image()).unwrap().bytes()[45],
            value.parse::<u8>().unwrap()
        );
    }
    for minutes in [1, 3, 5, 10, 20, 30, 60] {
        let mut f = fields();
        f.insert("sleep_light".into(), minutes.to_string());
        assert_eq!(
            convert(7, f).unwrap().prepare(&image()).unwrap().bytes()[44],
            minutes as u8
        );
    }
}
#[test]
fn old_patch_json_keeps_unselected_power_and_sensor_fields() {
    let patch: ConfigPatch = serde_json::from_str(r#"{"active":0}"#).unwrap();
    assert_eq!(patch.lod_mm, None);
    assert_eq!(patch.esports, None);
    assert_eq!(patch.ripple_control, None);
    let old = image();
    let next = old.patch(&patch).unwrap();
    assert_eq!(&next.bytes()[40..], &old.bytes()[40..]);
    assert_eq!(
        serde_json::from_value::<ConfigPatch>(serde_json::to_value(patch).unwrap())
            .unwrap()
            .active,
        Some(0)
    );
}
#[test]
fn preview_changes_still_require_recalculation_with_tuning_fields() {
    let src = convert(7, fields()).unwrap();
    let mut changed = src.clone();
    changed.patch.esports = Some(false);
    assert!(changed.prepare(&image()).is_err());
    let mut changed = src.clone();
    changed.patch.ripple_control = Some(true);
    assert!(changed.prepare(&image()).is_err());
    let mut f = fields();
    f.insert("unknown_power_option".into(), "1".into());
    let src = convert(7, f).unwrap();
    assert!(src.prepare(&image()).is_err());
}
