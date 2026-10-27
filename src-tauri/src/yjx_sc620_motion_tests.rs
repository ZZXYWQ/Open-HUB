use super::*;
use crate::yjx_sc620::{
    bindings::Image as Page,
    library_plan,
    oem_library::{self, Event},
};
fn e(kind: i64, value: i64, index: i64) -> Event {
    Event {
        record_id: index + 1,
        kind,
        value,
        index,
        description: String::new(),
        delay_raw: 0,
    }
}
fn source(events: Vec<Event>) -> Macro {
    Macro {
        id: 7,
        name: "Motion".into(),
        playback: Playback::Repeat(3),
        play_type: 0,
        play_times: 3,
        delay_type: 0,
        delay_time: 0,
        events,
    }
}
fn fact() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../tests/fixtures/yjx-sc620/oem-motion-compile.json"
    ))
    .unwrap()
}
fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|v| u8::from_str_radix(v, 16).unwrap())
        .collect()
}
fn page(n: u8) -> Page {
    let mut raw = [0; 64];
    raw[..3].copy_from_slice(&[0xaa, 8, 0xa5]);
    raw[4] = 44;
    raw[7] = n;
    for i in 0..11 {
        raw[8 + i * 4..12 + i * 4].copy_from_slice(&[0x20, 1, 0, 0]);
    }
    Page::from_response(n, &raw).unwrap()
}
#[test]
fn every_packed_motion_byte_and_axis_flag_combination_is_lossless_in_native_codec() {
    for packed in 0..=255 {
        for flags in [0, 0x4000, 0x8000, 0xc000] {
            for delay in [0, 1, 0x3fff] {
                let record = Record {
                    action: Action::Motion { packed },
                    delay_raw: flags | delay,
                };
                let image = Image::compile(&[vec![record.clone()]]).unwrap();
                assert_eq!(image.records(0).unwrap(), [record]);
                assert_eq!(image.bytes()[66], 0xc6);
                assert_eq!(image.bytes()[67], packed);
                assert_eq!(
                    u16::from_le_bytes([image.bytes()[64], image.bytes()[65]]),
                    flags | delay
                );
            }
        }
    }
}
#[test]
fn four_oem_byte_motion_types_preserve_every_raw_payload() {
    for kind in 8..=11 {
        for value in -128..=255 {
            let got = records(&source(vec![e(kind, value, 0)])).unwrap();
            assert_eq!(
                got[0].action,
                Action::Motion {
                    packed: value as u8
                }
            );
            assert_eq!(got[0].delay_raw, 0);
        }
    }
}
#[test]
fn all_two_byte_oem_xy_fields_keep_nibbles_and_separate_high_bit_flags() {
    for value in 0..=65535 {
        let got = records(&source(vec![e(12, value, 0)])).unwrap();
        let x = value as u8;
        let y = (value >> 8) as u8;
        let expected = ((u16::from(x) << 4) as u8) | (y & 15);
        assert_eq!(got[0].action, Action::Motion { packed: expected });
        let flags = (u16::from(x & 128) << 7) | (u16::from(y & 128) << 8);
        assert_eq!(got[0].delay_raw, flags);
    }
    for v in [-32768, -1] {
        let signed = records(&source(vec![e(12, v, 0)])).unwrap();
        let unsigned = records(&source(vec![e(12, i64::from(v as u16), 0)])).unwrap();
        assert_eq!(signed, unsigned);
    }
}
#[test]
fn motion_delay_never_overwrites_axis_flags_and_colliding_values_are_rejected() {
    for value in [0x0000, 0x0080, 0x8000, 0x8080] {
        for delay in [0, 1, 16383] {
            let got = records(&source(vec![e(12, value, 0), e(1, delay, 1)])).unwrap();
            let expected = if value & 128 != 0 { 0x4000 } else { 0 }
                | if value & 32768 != 0 { 0x8000 } else { 0 };
            assert_eq!(got[0].delay_raw, expected | delay as u16);
        }
        for tail in [vec![e(1, 16384, 1)], vec![e(1, 16384, 1), e(2, 65, 2)]] {
            let mut events = vec![e(12, value, 0)];
            events.extend(tail);
            let m = source(events);
            let saved = m.clone();
            assert!(prepare(&[m]).is_err());
            assert_eq!(saved.events[0].value, value);
        }
    }
    let ordinary = records(&source(vec![e(2, 65, 0), e(1, 65535, 1)])).unwrap();
    assert_eq!(ordinary[0].delay_raw, 65535);
}
#[test]
fn original_sql_motion_rows_flow_through_parser_and_production_full_plan_reports() {
    let f = fact();
    let events: Vec<Event> = serde_json::from_value(f["events"].clone()).unwrap();
    let root = std::env::temp_dir().join(format!(
        "openhub-sc620-motion-{}",
        crate::profiles::new_id()
    ));
    std::fs::create_dir(&root).unwrap();
    let file = root.join("source.db");
    std::fs::write(
        &file,
        include_bytes!("../tests/fixtures/yjx-sc620/oem-macro-library.sqlite3"),
    )
    .unwrap();
    let c = rusqlite::Connection::open(&file).unwrap();
    c.execute_batch("DELETE FROM t_macrorecord;DELETE FROM t_macro_data;INSERT INTO t_macro_data VALUES(7,'Motion',0,3,0,0);").unwrap();
    for e in &events {
        c.execute(
            "INSERT INTO t_macrorecord VALUES(?1,7,?2,?3,?4,?5,?6)",
            rusqlite::params![
                e.record_id,
                e.kind,
                e.index,
                e.description,
                e.value,
                e.delay_raw
            ],
        )
        .unwrap();
    }
    drop(c);
    let before = std::fs::read(&file).unwrap();
    let macros = oem_library::read(&file).unwrap();
    assert_eq!(macros[0].events, events);
    let prepared = prepare(&macros).unwrap();
    assert_eq!(
        prepared.image.bytes(),
        hex(f["image_hex"].as_str().unwrap())
    );
    let pages = (0..6).map(page).collect::<Vec<_>>();
    let change = library_plan::Change {
        page: 2,
        record: 5,
        expected: [0x20, 1, 0, 0],
        binding: prepared.slots[0].binding.clone(),
    };
    let reports = library_plan::compile(
        &pages,
        &Image::compile(&[]).unwrap(),
        &prepared.image,
        &[change],
    )
    .unwrap();
    assert_eq!(reports.len(), 4);
    for (r, gold) in reports.iter().zip(f["reports_hex"].as_array().unwrap()) {
        assert_eq!(r.to_vec(), hex(gold.as_str().unwrap()));
    }
    assert_eq!(std::fs::read(&file).unwrap(), before);
    std::fs::remove_file(file).unwrap();
    std::fs::remove_dir(root).unwrap();
}
#[test]
fn oversized_source_motion_fields_are_never_truncated_to_accepted_values() {
    for (kind, value) in [
        (8, 256),
        (9, -129),
        (10, 65535),
        (11, -1000),
        (12, 65536),
        (12, -32769),
    ] {
        assert!(prepare(&[source(vec![e(kind, value, 0)])]).is_err());
    }
}
#[test]
fn global_macro_boundary_preserves_motion_flags_when_oring_terminal_delay() {
    for value in [0x0080, 0x8000, 0x8080] {
        let a = source(vec![e(12, value, 0)]);
        let mut b = source(vec![e(2, 65, 0)]);
        b.id = 42;
        let p = prepare(&[a.clone(), b]).unwrap();
        let expected = records(&a).unwrap()[0].delay_raw | 1;
        assert_eq!(
            u16::from_le_bytes([p.image.bytes()[64], p.image.bytes()[65]]),
            expected
        );
        assert_eq!(p.image.records(0).unwrap()[0].delay_raw, expected);
    }
}
#[test]
fn motion_json_keeps_packed_wire_data_and_raw_field_without_inventing_axis_directions() {
    let r = Record {
        delay_raw: 0xc123,
        action: Action::Motion { packed: 0x45 },
    };
    let j = serde_json::to_value(&r).unwrap();
    assert_eq!(j["action"], serde_json::json!({"Motion":{"packed":69}}));
    assert_eq!(serde_json::from_value::<Record>(j).unwrap(), r);
    assert!(serde_json::from_str::<Action>(r#"{"Motion":{"packed":256}}"#).is_err());
}
