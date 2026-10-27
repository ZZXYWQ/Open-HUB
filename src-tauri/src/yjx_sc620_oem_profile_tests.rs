use super::*;
use crate::yjx_sc620::bindings::{Change as BasicChange, MacroPlayback};
fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}
fn fact() -> serde_json::Value {
    serde_json::from_str(include_str!("../tests/fixtures/yjx-sc620/oem-profile.json")).unwrap()
}
fn image() -> ConfigImage {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/yjx-sc620/configuration.json"
    ))
    .unwrap();
    ConfigImage::from_response(2, &hex(f["before_response_hex"].as_str().unwrap())).unwrap()
}
fn page(n: u8) -> Page {
    let mut b = [0; 64];
    b[..3].copy_from_slice(&[0xaa, 8, 0xa5]);
    b[4] = 44;
    b[7] = n;
    for i in 0..11 {
        b[8 + 4 * i..12 + 4 * i].copy_from_slice(&[0x20, 1, 0, 0]);
    }
    Page::from_response(n, &b).unwrap()
}
fn pages() -> Vec<Page> {
    (0..6).map(page).collect()
}
fn empty() -> Library {
    Library::compile(&[]).unwrap()
}
struct File {
    root: std::path::PathBuf,
    path: std::path::PathBuf,
}
impl File {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("openhub-sc620-joint-{}", crate::profiles::new_id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("source.db");
        std::fs::write(
            &path,
            include_bytes!("../tests/fixtures/yjx-sc620/oem-profile.sqlite3"),
        )
        .unwrap();
        Self { root, path }
    }
    fn source(&self) -> Source {
        read(&self.path, 7).unwrap()
    }
    fn sql(&self, sql: &str) {
        rusqlite::Connection::open(&self.path)
            .unwrap()
            .execute_batch(sql)
            .unwrap();
    }
}
impl Drop for File {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
#[test]
fn sqlite_settings_eleven_buttons_and_sparse_macro_ids_match_five_complete_reports() {
    let f = File::new();
    let bytes = std::fs::read(&f.path).unwrap();
    let source = f.source();
    assert_eq!(source.keys.len(), 11);
    assert_eq!(
        source.macros.iter().map(|m| m.id).collect::<Vec<_>>(),
        [7, 42]
    );
    let before = pages();
    let p = source.prepare(&image(), &before, &empty()).unwrap();
    let facts = fact();
    assert_eq!(p.reports.len(), 5);
    for (report, gold) in p
        .reports
        .iter()
        .zip(facts["reports_hex"].as_array().unwrap())
    {
        assert_eq!(report.to_vec(), hex(gold.as_str().unwrap()));
    }
    assert_eq!(p.edits.len(), 11);
    assert_eq!(p.library.slots(), 2);
    assert_eq!(p.config.page(), 2);
    for (i, edit) in p.edits.iter().enumerate() {
        let encoded = match &edit.target {
            Target::Basic(a) => a.encode().unwrap(),
            Target::Macro(b) => b.encode().unwrap(),
        };
        let slot = if i == 3 {
            4
        } else if i == 4 {
            3
        } else {
            i
        };
        assert_eq!(edit.record as usize, slot);
        assert_eq!(
            encoded.to_vec(),
            hex(facts["native_records_hex"][slot].as_str().unwrap())
        );
    }
    assert_eq!(before, pages());
    assert_eq!(bytes, std::fs::read(&f.path).unwrap());
    assert_eq!(std::fs::read_dir(&f.root).unwrap().count(), 1);
}
#[test]
fn source_profile_ids_are_not_hardware_pages_and_json_roundtrips() {
    let f = File::new();
    let s = read(&f.path, 42).unwrap();
    let p = s.prepare(&image(), &pages(), &empty()).unwrap();
    assert_eq!(p.edits.len(), 1);
    assert_eq!(p.edits[0].page, 2);
    assert_eq!(p.reports.len(), 2);
    let s = f.source();
    let decoded: Source = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(s, decoded);
    assert_eq!(
        decoded
            .prepare(&image(), &pages(), &empty())
            .unwrap()
            .reports,
        s.prepare(&image(), &pages(), &empty()).unwrap().reports
    );
}
#[test]
fn all_verified_basic_media_and_modifier_actions_encode_original_vectors() {
    let s = File::new().source();
    let mut r = s.keys[0].clone();
    for (kind, bytes) in [
        (1, [32, 1, 0, 0]),
        (2, [32, 4, 0, 0]),
        (3, [32, 2, 0, 0]),
        (4, [32, 1, 2, 0]),
        (5, [32, 16, 0, 0]),
        (6, [32, 8, 0, 0]),
        (7, [33, 56, 1, 0]),
        (8, [33, 56, 255, 0]),
        (9, [33, 85, 0, 0]),
    ] {
        r.kind = kind;
        assert_eq!(basic(&r).unwrap().encode().unwrap(), bytes);
    }
    r.kind = 25;
    for (value, usage) in [
        (1, 0x183u16),
        (2, 0xcd),
        (3, 0xb7),
        (4, 0xb6),
        (5, 0xb5),
        (6, 0xe9),
        (7, 0xea),
        (8, 0xe2),
    ] {
        r.value = value;
        assert_eq!(
            basic(&r).unwrap().encode().unwrap(),
            [48, usage as u8, (usage >> 8) as u8, 0]
        );
    }
    r.kind = 22;
    r.value2 = 0;
    for value in 0xe0..=0xe7 {
        r.value = value;
        assert_eq!(
            basic(&r).unwrap().encode().unwrap(),
            [16, 1 << (value - 0xe0), 0, 0]
        );
    }
    r.value2 = 1;
    assert!(basic(&r).is_err());
    r.value = 4;
    r.value2 = 255;
    assert_eq!(basic(&r).unwrap().encode().unwrap(), [16, 255, 4, 0]);
    for (value, mods) in [(256, 0), (-1, 0), (4, 256), (4, -1), (0, 0)] {
        r.value = value;
        r.value2 = mods;
        assert!(basic(&r).is_err());
    }
}
#[test]
fn missing_macro_and_unsupported_actions_layers_and_controls_refuse_whole_plan() {
    let s = File::new().source();
    for (field, value) in [
        ("kind", 10),
        ("kind", 24),
        ("kind", 26),
        ("kind", 27),
        ("layer", 1),
        ("key_code", 12),
        ("key_code", 0),
        ("profile", 42),
    ] {
        let mut v = serde_json::to_value(&s).unwrap();
        v["keys"][0][field] = value.into();
        let bad: Source = serde_json::from_value(v).unwrap();
        assert!(
            bad.prepare(&image(), &pages(), &empty()).is_err(),
            "{field}={value}"
        );
    }
    let mut bad = s.clone();
    bad.keys[3].value = 1;
    assert!(bad.prepare(&image(), &pages(), &empty()).is_err());
    let mut bad = s;
    bad.keys.push(bad.keys[0].clone());
    assert!(bad.prepare(&image(), &pages(), &empty()).is_err());
}
#[test]
fn unknown_unused_macro_is_preserved_in_preview_and_blocks_strict_library_conversion() {
    let f = File::new();
    f.sql("INSERT INTO t_macro_data VALUES(11,'unknown unused',0,1,0,10);INSERT INTO t_macrorecord VALUES(100,11,99,0,'unknown',1,0);");
    let mut s = f.source();
    assert_eq!(s.macros.len(), 3);
    assert_eq!(s.macros[1].events[0].kind, 99);
    assert!(s.prepare(&image(), &pages(), &empty()).is_err());
    s.keys.retain(|k| k.kind != 23);
    let p = s.prepare(&image(), &pages(), &empty()).unwrap();
    assert_eq!(p.library, empty());
    assert!(p.reports.iter().all(|r| r[2] != 0x0d && r[2] != 0x10));
}
#[test]
fn untargeted_sixth_page_and_opaque_macro_references_block_global_replacement() {
    let s = File::new().source();
    let lib = oem_compile::prepare(&s.macros).unwrap().image;
    for raw in [[0x70, 0, 1, 2], [0x70, 255, 0, 99]] {
        let mut ps = pages();
        let mut b = [0; 64];
        b[..3].copy_from_slice(&[0xaa, 8, 0xa5]);
        b[4] = 44;
        b[7] = 5;
        b[8..12].copy_from_slice(&raw);
        ps[5] = Page::from_response(5, &b).unwrap();
        assert!(s.prepare(&image(), &ps, &empty()).is_err());
        // When the existing slot bytes are identical, known references are preserved.
        if raw[1] == 0 {
            assert!(s.prepare(&image(), &ps, &lib).is_ok());
        }
    }
}
#[test]
fn explicitly_replaced_macro_can_become_basic_but_other_references_stay_protected() {
    let s = File::new().source();
    let old = oem_compile::prepare(&s.macros).unwrap().image;
    let mut ps = pages();
    ps[2] = ps[2]
        .with_macro(
            0,
            [32, 1, 0, 0],
            &crate::yjx_sc620::bindings::MacroBinding {
                slot: 0,
                playback: MacroPlayback::Code2,
            },
            &old,
        )
        .unwrap();
    let e = Edit {
        page: 2,
        record: 0,
        expected: [112, 0, 1, 2],
        target: Target::Basic(Action::Mouse(Mouse::Right)),
    };
    let reports = library_plan::compile_edits(&ps, &old, &empty(), &[e]).unwrap();
    assert_eq!(reports.len(), 4);
    assert_eq!(reports[3][9..13], [32, 2, 0, 0]);
}
#[test]
fn repeated_import_is_noop_and_never_reuploads_library_or_config() {
    let s = File::new().source();
    let p = s.prepare(&image(), &pages(), &empty()).unwrap();
    let mut ps = pages();
    for edit in &p.edits {
        ps[2] = match &edit.target {
            Target::Basic(a) => ps[2]
                .patch(&[BasicChange {
                    record: edit.record,
                    expected: edit.expected,
                    action: a.clone(),
                }])
                .unwrap(),
            Target::Macro(b) => ps[2]
                .with_macro(edit.record, edit.expected, b, &p.library)
                .unwrap(),
        };
    }
    assert!(s
        .prepare(&p.config, &ps, &p.library)
        .unwrap()
        .reports
        .is_empty());
    assert!(
        library_plan::compile_edits(&ps, &p.library, &p.library, &[])
            .unwrap()
            .is_empty()
    );
    assert!(library_plan::compile_edits(&ps, &p.library, &empty(), &[]).is_err());
}
#[test]
fn source_sql_types_duplicates_views_and_limits_refuse_without_writes() {
    for sql in ["INSERT INTO t_key_macro_data SELECT 200,profile,fn_layer,key_code,layout_value,layout_desc,macro_type,macro_value,macro_value2,macro_value3,macro_desc FROM t_key_macro_data WHERE key_id=1;","UPDATE t_key_macro_data SET macro_value=NULL WHERE key_id=1;","UPDATE t_key_macro_data SET macro_type='bad' WHERE key_id=1;","UPDATE t_key_macro_data SET layout_desc=printf('%1100s','x') WHERE key_id=1;","ALTER TABLE t_key_macro_data RENAME TO original;CREATE VIEW t_key_macro_data AS SELECT * FROM original;"]{
 let f=File::new();f.sql(sql);let before=std::fs::read(&f.path).unwrap();assert!(read(&f.path,7).is_err(),"{sql}");assert_eq!(before,std::fs::read(&f.path).unwrap());}
}
#[test]
fn unconverted_settings_and_incomplete_snapshots_cannot_produce_partial_plan() {
    let mut s = File::new().source();
    s.settings.fields.insert("dpi_flag".into(), "5".into());
    assert!(s.prepare(&image(), &pages(), &empty()).is_err());
    let s = File::new().source();
    let ps = pages();
    assert!(s.prepare(&image(), &ps[..5], &empty()).is_err());
    let mut bad = ps;
    bad.swap(0, 1);
    assert!(s.prepare(&image(), &bad, &empty()).is_err());
}
