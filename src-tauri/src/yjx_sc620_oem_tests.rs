use super::*;
use crate::yjx_sc620::bindings::{MacroBinding, MacroPlayback};
fn connection() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE t_macro_data(macro_id INTEGER PRIMARY KEY AUTOINCREMENT,name TEXT,play_type INTEGER,play_times INTEGER,delay_type INTEGER,delay_time INTEGER);CREATE TABLE t_macrorecord(record_id INTEGER PRIMARY KEY AUTOINCREMENT,macro_id INTEGER,type INTEGER,record_index INTEGER,desc TEXT,value INTEGER,delay_time INTEGER);INSERT INTO t_macro_data VALUES(7,'Macro',0,3,0,0);INSERT INTO t_macrorecord VALUES(1,7,2,0,'Down',65,0);").unwrap();
    c
}
#[test]
fn semantic_mapping_matches_sc620_labels_save_fields_and_exact_native_records() {
    for (source, times, mode, raw) in [
        (0, 1, Playback::Once, [0x70, 0, 1, 0]),
        (0, 3, Playback::Repeat(3), [0x70, 0, 3, 0]),
        (1, 1, Playback::WhileHeld, [0x70, 0, 1, 2]),
        (2, 1, Playback::UntilAnyClick, [0x70, 0, 1, 3]),
    ] {
        let got = Playback::from_oem(source, times).unwrap();
        assert_eq!(got, mode);
        let w = got.wire().unwrap();
        assert_eq!(Playback::from_wire(&w).unwrap(), mode);
        assert_eq!(
            MacroBinding {
                slot: 0,
                playback: w
            }
            .encode()
            .unwrap(),
            raw
        );
    }
    for count in 2..=255 {
        let p = Playback::Repeat(count);
        assert_eq!(Playback::from_wire(&p.wire().unwrap()).unwrap(), p);
    }
    assert_ne!(Playback::UntilAnyClick, Playback::WhileHeld);
    assert!(Playback::Repeat(1).wire().is_err());
    assert!(Playback::Repeat(0).wire().is_err());
    assert!(Playback::from_wire(&MacroPlayback::Code0 { repeat: 0 }).is_err());
    for (kind, times) in [(3, 1), (-1, 1), (0, 0), (0, -1), (0, 256)] {
        assert!(Playback::from_oem(kind, times).is_err());
    }
}
#[test]
fn python_fixture_reads_raw_events_without_replacing_unknowns_or_editing_source() {
    let root =
        std::env::temp_dir().join(format!("openhub-sc620-oem-{}", crate::profiles::new_id()));
    std::fs::create_dir(&root).unwrap();
    let p = root.join("source.db");
    let original = include_bytes!("../tests/fixtures/yjx-sc620/oem-macro-library.sqlite3");
    std::fs::write(&p, original).unwrap();
    let got = read(&p).unwrap();
    let fact: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/yjx-sc620/oem-macro-library.json"
    ))
    .unwrap();
    assert_eq!(got.iter().map(|m| m.id).collect::<Vec<_>>(), [7, 11, 42]);
    for (i, m) in got.iter().enumerate() {
        assert_eq!(
            serde_json::to_value(&m.playback).unwrap(),
            fact["playbacks"][i]
        );
        assert_eq!(m.events.len() as u64, fact["events"][i].as_u64().unwrap());
    }
    assert_eq!(
        got[0].events.iter().map(|e| e.index).collect::<Vec<_>>(),
        [0, 2, 5]
    );
    assert_eq!(got[1].events[0].kind, 99);
    assert_eq!(got[1].events[0].value, -5);
    assert_eq!(got[1].events[0].delay_raw, 123);
    assert_eq!(std::fs::read(&p).unwrap(), original);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_file(p).unwrap();
    std::fs::remove_dir(root).unwrap();
}
#[test]
fn oem_macro_ids_are_preserved_and_never_confused_with_native_slot_indices() {
    let c = connection();
    c.execute_batch("INSERT INTO t_macro_data VALUES(400,'Other',2,1,0,0);")
        .unwrap();
    let m = parse(&c).unwrap();
    assert_eq!(m[0].id, 7);
    assert_eq!(m[1].id, 400);
    assert_eq!(m[1].playback, Playback::UntilAnyClick);
}
#[test]
fn duplicate_indexes_and_orphan_events_reject_instead_of_dropping_rows() {
    for sql in [
        "INSERT INTO t_macrorecord VALUES(2,7,3,0,'Up',65,0);",
        "INSERT INTO t_macrorecord VALUES(2,8,3,1,'Orphan',65,0);",
    ] {
        let c = connection();
        c.execute_batch(sql).unwrap();
        assert!(parse(&c).is_err());
    }
}
#[test]
fn missing_tables_views_and_different_schemas_are_not_guessed() {
    let missing = Connection::open_in_memory().unwrap();
    assert!(parse(&missing).is_err());
    for sql in ["ALTER TABLE t_macro_data RENAME TO actual;CREATE VIEW t_macro_data AS SELECT * FROM actual;","ALTER TABLE t_macrorecord ADD COLUMN ignored TEXT;","ALTER TABLE t_macrorecord RENAME COLUMN value TO payload;"]{let c=connection();c.execute_batch(sql).unwrap();assert!(parse(&c).is_err());}
}
#[test]
fn null_text_real_and_overflow_fields_do_not_get_coerced_or_clamped() {
    for sql in [
        "UPDATE t_macro_data SET play_type=3;",
        "UPDATE t_macro_data SET play_times=0;",
        "UPDATE t_macro_data SET play_times=256;",
        "UPDATE t_macro_data SET delay_time=-1;",
        "UPDATE t_macro_data SET name=NULL;",
        "UPDATE t_macro_data SET name=zeroblob(300);",
        "UPDATE t_macrorecord SET value=1.5;",
        "UPDATE t_macrorecord SET value=2147483648;",
        "UPDATE t_macrorecord SET record_index=-1;",
        "UPDATE t_macrorecord SET delay_time=-1;",
    ] {
        let c = connection();
        c.execute_batch(sql).unwrap();
        assert!(parse(&c).is_err(), "{sql}");
    }
}
#[test]
fn preview_limits_reject_oversized_libraries_before_collecting_rows() {
    let c = connection();
    for i in 8..40 {
        c.execute("INSERT INTO t_macro_data VALUES(?1,'Macro',0,1,0,0)", [i])
            .unwrap();
    }
    assert!(parse(&c).is_err());
    let c = connection();
    c.execute_batch("WITH RECURSIVE n(x) AS(VALUES(2) UNION ALL SELECT x+1 FROM n WHERE x<4097) INSERT INTO t_macrorecord SELECT x,7,2,x,'event',65,0 FROM n;").unwrap();
    assert!(parse(&c).is_err());
}
#[test]
fn held_and_any_click_modes_preserve_original_ignored_repeat_metadata() {
    let c = connection();
    c.execute_batch(
        "UPDATE t_macro_data SET play_type=1,play_times=17,delay_type=99,delay_time=123;",
    )
    .unwrap();
    let m = parse(&c).unwrap();
    assert_eq!(m[0].playback, Playback::WhileHeld);
    assert_eq!(m[0].play_times, 17);
    assert_eq!(m[0].delay_type, 99);
    assert_eq!(m[0].delay_time, 123);
    let encoded = serde_json::to_value(&m).unwrap();
    assert_eq!(serde_json::from_value::<Vec<Macro>>(encoded).unwrap(), m);
}
