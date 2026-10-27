use super::*;
fn record(action: Action) -> Record {
    Record {
        delay_raw: 0,
        action,
    }
}
fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|v| u8::from_str_radix(v, 16).unwrap())
        .collect()
}
#[test]
fn independent_complete_image_and_all_reports_match_static_oem_vectors() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/yjx-sc620/macros.json")).unwrap();
    let defs = vec![
        vec![
            Record {
                delay_raw: 0x1234,
                action: Action::KeyDown(4),
            },
            Record {
                delay_raw: 0x5678,
                action: Action::KeyUp(4),
            },
        ],
        vec![],
        vec![
            Record {
                delay_raw: 1,
                action: Action::Wheel(1),
            },
            Record {
                delay_raw: 65535,
                action: Action::Wheel(-1),
            },
        ],
    ];
    let image = Image::compile(&defs).unwrap();
    assert_eq!(image.bytes(), hex(f["memory_hex"].as_str().unwrap()));
    let reports = image.compile_reports();
    assert_eq!(reports.len(), 3);
    for (r, s) in reports.iter().zip(f["reports_hex"].as_array().unwrap()) {
        assert_eq!(r.to_vec(), hex(s.as_str().unwrap()));
    }
    for (i, def) in defs.iter().enumerate() {
        assert_eq!(&image.records(i).unwrap(), def);
    }
    assert_eq!(image.bytes().len(), 84); // No extra sentinel after either nonempty macro.
}
#[test]
fn supported_keyboard_mouse_and_wheel_records_roundtrip() {
    let mut events = Vec::new();
    for k in 4..=0xa4 {
        events.extend([record(Action::KeyDown(k)), record(Action::KeyUp(k))]);
    }
    for n in 0..5 {
        events.extend([
            record(Action::MouseDown(1 << n)),
            record(Action::MouseUp(1 << n)),
        ]);
    }
    events.extend([record(Action::Wheel(1)), record(Action::Wheel(-1))]);
    let image = Image::compile(&[events.clone()]).unwrap();
    assert_eq!(image.records(0).unwrap(), events);
}
#[test]
fn capacity_and_little_endian_chunk_offsets_are_exact() {
    let image = Image::compile(&[vec![record(Action::KeyDown(4)); 1040]]).unwrap();
    assert_eq!(image.bytes().len(), CAPACITY);
    let reports = image.compile_reports();
    assert_eq!(reports.len(), CAPACITY.div_ceil(56) + 1);
    let mut decoded = Vec::new();
    for (i, r) in reports[..reports.len() - 1].iter().enumerate() {
        assert_eq!(u16::from_le_bytes([r[6], r[7]]) as usize, i * 56);
        assert_eq!(r[4], r[5..].iter().fold(0u8, |a, b| a.wrapping_add(*b)));
        decoded.extend_from_slice(&r[9..9 + r[5] as usize]);
        assert!(r[9 + r[5] as usize..].iter().all(|b| *b == 0));
    }
    assert_eq!(decoded, image.bytes());
    assert!(Image::compile(&[vec![record(Action::KeyDown(4)); 1041]]).is_err());
}
#[test]
fn slot_limits_empty_library_and_explicit_empty_macro_are_distinct() {
    let empty = Image::compile(&[]).unwrap();
    assert_eq!(empty.bytes(), &[0; 64]);
    assert_eq!(empty.slots(), 0);
    assert!(empty.records(0).is_err());
    let all = Image::compile(&vec![vec![]; 32]).unwrap();
    assert_eq!(all.slots(), 32);
    for i in 0..32 {
        assert!(all.records(i).unwrap().is_empty());
        assert_eq!(
            u16::from_le_bytes([all.bytes()[i * 2], all.bytes()[i * 2 + 1]]) as usize,
            64 + i * 4
        );
    }
    assert!(Image::compile(&vec![vec![]; 33]).is_err());
}
#[test]
fn opaque_events_remain_exact_and_are_never_substituted_with_defaults() {
    let mut bytes = vec![0; 64];
    bytes[0] = 64;
    bytes.extend([0, 0, 0xc7, 0x92]);
    let image = Image::from_bytes(bytes.clone()).unwrap();
    assert!(image.records(0).is_err());
    assert_eq!(image.bytes(), bytes);
    let packets = image.compile_reports();
    assert_eq!(&packets[1][17..21], &[0, 0, 0xc7, 0x92]);
    assert_eq!(image.bytes(), bytes);
}
#[test]
fn malformed_offsets_and_end_markers_fail_without_truncation() {
    let valid = Image::compile(&[
        vec![record(Action::KeyDown(4)), record(Action::KeyUp(4))],
        vec![record(Action::Wheel(1))],
    ])
    .unwrap()
    .bytes()
    .to_vec();
    for len in [0, 63, 65, 4228] {
        assert!(Image::from_bytes(vec![0; len]).is_err());
    }
    for (at, value) in [
        (0, 60),
        (0, 65),
        (0, 68),
        (2, 64),
        (2, 0),
        (4, 76),
        (66, 0xc2),
        (70, 2),
        (74, 0x44),
    ] {
        let mut bad = valid.clone();
        bad[at] = value;
        assert!(Image::from_bytes(bad).is_err(), "mutation {at}={value}");
    }
}
#[test]
fn invalid_known_actions_are_rejected_before_any_reports_exist() {
    for a in [
        Action::KeyDown(0),
        Action::KeyUp(0xe0),
        Action::MouseDown(32),
        Action::MouseUp(3),
        Action::Wheel(0),
        Action::Wheel(2),
    ] {
        assert!(Image::compile(&[vec![record(a)]]).is_err());
    }
}
#[test]
fn json_keeps_raw_delay_and_refuses_unknown_fields_or_overflow() {
    let r = Record {
        delay_raw: 65535,
        action: Action::MouseUp(16),
    };
    let j = serde_json::to_value(&r).unwrap();
    assert_eq!(serde_json::from_value::<Record>(j.clone()).unwrap(), r);
    for j in [
        serde_json::json!({"delay_raw":65536,"action":{"KeyDown":4}}),
        serde_json::json!({"delay_raw":1,"action":{"KeyDown":4},"ignored":true}),
    ] {
        assert!(serde_json::from_value::<Record>(j).is_err());
    }
}

#[test]
fn unresolved_modifier_codes_stay_opaque_instead_of_using_binding_bitmasks() {
    for kind in [1, 0x41] {
        for value in [0, 1, 2, 4, 8, 16, 32, 64, 128, 0xe0, 0xe7] {
            let mut raw = vec![0; 64];
            raw[0] = 64;
            raw.extend([0, 0, kind | 0x80, value]);
            let image = Image::from_bytes(raw.clone()).unwrap();
            assert!(image.records(0).is_err());
            assert_eq!(image.bytes(), raw);
        }
    }
}
