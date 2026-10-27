use super::*;
use crate::yjx_sc620::{bindings::Image as Page, library_plan, oem_library::Event};
fn source(events: Vec<Event>) -> Macro {
    Macro {
        id: 7,
        name: "Macro".into(),
        playback: Playback::Once,
        play_type: 0,
        play_times: 1,
        delay_type: 0,
        delay_time: 0,
        events,
    }
}
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
fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}
fn fixture() -> Vec<Macro> {
    let root = std::env::temp_dir().join(format!(
        "openhub-sc620-compile-{}",
        crate::profiles::new_id()
    ));
    std::fs::create_dir(&root).unwrap();
    let file = root.join("source.db");
    std::fs::write(
        &file,
        include_bytes!("../tests/fixtures/yjx-sc620/oem-macro-library.sqlite3"),
    )
    .unwrap();
    let got = super::super::oem_library::read(&file).unwrap();
    std::fs::remove_file(file).unwrap();
    std::fs::remove_dir(root).unwrap();
    got
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
fn static_oem_key_cases_encode_every_supported_press_and_release() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/yjx-sc620/oem-event-keys.json"
    ))
    .unwrap();
    assert_eq!(f["keys"].as_array().unwrap().len(), 109);
    for row in f["keys"].as_array().unwrap() {
        let vk = row["vk"].as_i64().unwrap();
        let usage = row["usage"].as_u64().unwrap() as u8;
        let m = source(vec![e(2, vk, 0), e(3, vk, 1)]);
        let got = records(&m).unwrap();
        assert_eq!(
            got,
            vec![
                Record {
                    delay_raw: 10,
                    action: Action::KeyDown(usage)
                },
                Record {
                    delay_raw: 0,
                    action: Action::KeyUp(usage)
                }
            ]
        );
        let image = Image::compile(&[got]).unwrap();
        assert_eq!(
            &image.bytes()[64..],
            &[10, 0, 0x42, usage, 0, 0, 0x82, usage]
        );
    }
}
#[test]
fn python_sqlite_to_native_library_and_full_plan_matches_independent_reports() {
    let all = fixture();
    assert!(prepare(&all).is_err());
    let selected = all.into_iter().filter(|m| m.id != 11).collect::<Vec<_>>();
    let prepared = prepare(&selected).unwrap();
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/yjx-sc620/oem-event-compile.json"
    ))
    .unwrap();
    assert_eq!(
        prepared.image.bytes(),
        hex(f["image_hex"].as_str().unwrap())
    );
    assert_eq!(
        prepared
            .slots
            .iter()
            .map(|s| (s.oem_id, s.binding.slot))
            .collect::<Vec<_>>(),
        [(7, 0), (42, 1)]
    );
    let changes = vec![
        library_plan::Change {
            page: 0,
            record: 3,
            expected: [0x20, 1, 0, 0],
            binding: prepared.slots[0].binding.clone(),
        },
        library_plan::Change {
            page: 4,
            record: 4,
            expected: [0x20, 1, 0, 0],
            binding: prepared.slots[1].binding.clone(),
        },
    ];
    let before = (0..6).map(page).collect::<Vec<_>>();
    let packets = library_plan::compile(
        &before,
        &Image::compile(&[]).unwrap(),
        &prepared.image,
        &changes,
    )
    .unwrap();
    assert_eq!(packets.len(), 5);
    for (p, r) in packets.iter().zip(f["reports_hex"].as_array().unwrap()) {
        assert_eq!(p.to_vec(), hex(r.as_str().unwrap()));
    }
    assert_eq!(before, (0..6).map(page).collect::<Vec<_>>());
}
#[test]
fn delay_rows_accumulate_into_previous_record_and_trailing_zero_stays_zero() {
    for (a, b, expected) in [(20, 30, 50), (0, 0, 10), (1, 2, 3)] {
        let m = source(vec![e(2, 65, 0), e(1, a, 1), e(1, b, 2), e(3, 65, 3)]);
        let got = records(&m).unwrap();
        assert_eq!(got[0].delay_raw, expected);
        assert_eq!(got[1].delay_raw, 0);
    }
    for delay in [0, 1, 65535] {
        let m = source(vec![e(2, 65, 0), e(1, delay, 1)]);
        let got = records(&m).unwrap();
        assert_eq!(got[0].delay_raw, delay as u16);
        let image = Image::compile(&[got]).unwrap();
        assert_eq!(image.bytes()[66], 0xc2);
    }
}
#[test]
fn leading_delays_and_16bit_overflow_are_refused_without_mutating_source() {
    for events in [
        vec![e(1, 50, 0), e(2, 65, 1)],
        vec![e(2, 65, 0), e(1, 65536, 1)],
        vec![e(2, 65, 0), e(1, 65535, 1), e(1, 1, 2)],
        vec![e(2, 65, 0), e(1, -1, 1)],
    ] {
        let m = source(events);
        let before = m.clone();
        assert!(records(&m).is_err());
        assert_eq!(m, before);
    }
}
#[test]
fn scroll_description_overrides_match_exact_oem_case_sensitive_substrings() {
    for kind in [4, 5] {
        for (desc, wheel) in [
            ("Scroll Up", 1),
            ("before Scroll Down after", -1),
            ("Scroll Up Scroll Down", 1),
        ] {
            let mut event = e(kind, 16, 0);
            event.description = desc.into();
            assert_eq!(
                records(&source(vec![event])).unwrap()[0].action,
                Action::Wheel(wheel)
            );
        }
        let mut event = e(kind, 1, 0);
        event.description = "scroll up".into();
        assert_eq!(
            records(&source(vec![event])).unwrap()[0].action,
            if kind == 4 {
                Action::MouseDown(1)
            } else {
                Action::MouseUp(1)
            }
        );
    }
}
#[test]
fn mouse_and_explicit_wheel_events_preserve_button_sides() {
    for bit in [1, 2, 4, 8, 16] {
        let got = records(&source(vec![e(4, bit, 0), e(5, bit, 1)])).unwrap();
        assert_eq!(got[0].action, Action::MouseDown(bit as u8));
        assert_eq!(got[1].action, Action::MouseUp(bit as u8));
    }
    for (kind, dir) in [(6, 1), (7, -1)] {
        assert_eq!(
            records(&source(vec![e(kind, 0, 0)])).unwrap()[0].action,
            Action::Wheel(dir)
        );
    }
}
#[test]
fn unsupported_modifiers_and_unknown_events_never_produce_partial_library() {
    for (kind, value) in [(2, 0), (2, 0xa2), (3, 0xe2), (4, 3), (5, 32), (99, 5)] {
        let m = source(vec![e(2, 65, 0), e(kind, value, 1)]);
        let original = m.clone();
        assert!(prepare(&[m]).is_err());
        assert_eq!(original.events.len(), 2);
    }
}
#[test]
fn empty_macro_boundary_field_is_preserved_and_selected_order_has_explicit_id_mapping() {
    let mut a = source(vec![]);
    a.id = 42;
    let b = source(vec![e(2, 65, 0)]);
    let p = prepare(&[a, b]).unwrap();
    assert_eq!(
        p.slots
            .iter()
            .map(|s| (s.oem_id, s.binding.slot))
            .collect::<Vec<_>>(),
        [(42, 0), (7, 1)]
    );
    assert_eq!(&p.image.bytes()[64..68], &[1, 0, 0x80, 0]);
    assert!(p.image.records(0).is_err()); // Delay-bearing sentinel remains opaque rather than dropped.
}
#[test]
fn stale_playback_duplicate_ids_order_and_capacity_fail_before_emitting_packets() {
    let mut m = source(vec![e(2, 65, 0)]);
    m.playback = Playback::UntilAnyClick;
    assert!(records(&m).is_err());
    let m = source(vec![e(2, 65, 0)]);
    assert!(prepare(&[m.clone(), m.clone()]).is_err());
    let mut bad = m.clone();
    bad.events.push(e(3, 65, 0));
    assert!(records(&bad).is_err());
    let huge = source((0..1041).map(|i| e(2, 65, i)).collect());
    assert!(prepare(&[huge]).is_err());
    let too_many = (1..=33)
        .map(|id| Macro { id, ..m.clone() })
        .collect::<Vec<_>>();
    assert!(prepare(&too_many).is_err());
}

#[test]
fn global_oem_cursor_ors_one_into_prior_terminal_delay_only_before_nonempty_macros() {
    let a = source(vec![e(2, 65, 0), e(1, 50, 1)]);
    let mut b = source(vec![e(2, 66, 0)]);
    b.id = 42;
    let p = prepare(&[a.clone(), b.clone()]).unwrap();
    assert_eq!(&p.image.bytes()[64..68], &[51, 0, 0xc2, 4]);
    b.events.clear();
    let p = prepare(&[a, b]).unwrap();
    assert_eq!(&p.image.bytes()[64..68], &[50, 0, 0xc2, 4]);
    assert_eq!(&p.image.bytes()[68..72], &[0, 0, 0x80, 0]);
}
