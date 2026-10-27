use super::*;
use crate::yjx_sc620::{
    bindings::{Action, Change as KeyChange, MacroPlayback},
    macros::{Action as Event, Record},
};
fn lib(keys: &[u8]) -> Library {
    Library::compile(
        &keys
            .iter()
            .map(|k| {
                vec![
                    Record {
                        delay_raw: 0,
                        action: Event::KeyDown(*k),
                    },
                    Record {
                        delay_raw: 0,
                        action: Event::KeyUp(*k),
                    },
                ]
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
fn page(n: u8, overrides: &[(u8, [u8; 4])]) -> Page {
    let mut response = [0; 64];
    response[0..3].copy_from_slice(&[0xaa, 8, 0xa5]);
    response[4] = 44;
    response[7] = n;
    for i in 0..11 {
        response[8 + 4 * i..12 + 4 * i].copy_from_slice(&[0x20, 1, 0, 0]);
    }
    for (record, raw) in overrides {
        let at = 8 + 4 * usize::from(*record);
        response[at..at + 4].copy_from_slice(raw);
    }
    Page::from_response(n, &response).unwrap()
}
fn pages() -> Vec<Page> {
    (0..6)
        .map(|p| page(p, if p == 5 { &[(0, [0x70, 0, 1, 3])] } else { &[] }))
        .collect()
}
fn binding(slot: u8, playback: MacroPlayback) -> MacroBinding {
    MacroBinding { slot, playback }
}
fn change(p: u8, r: u8, slot: u8) -> Change {
    Change {
        page: p,
        record: r,
        expected: [0x20, 1, 0, 0],
        binding: binding(slot, MacroPlayback::Code0 { repeat: 7 }),
    }
}
fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}
#[test]
fn native_binding_codes_roundtrip_all_32_slots_and_nonzero_repeat_bytes() {
    for slot in 0..32 {
        for repeat in 1..=255 {
            let b = binding(slot, MacroPlayback::Code0 { repeat });
            assert_eq!(b.encode().unwrap(), [0x70, slot, repeat, 0]);
            assert_eq!(MacroBinding::decode(b.encode().unwrap()), Some(b));
        }
        for (mode, raw) in [(MacroPlayback::Code2, 2), (MacroPlayback::Code3, 3)] {
            let b = binding(slot, mode);
            assert_eq!(b.encode().unwrap(), [0x70, slot, 1, raw]);
            assert_eq!(MacroBinding::decode(b.encode().unwrap()), Some(b));
        }
    }
    for slot in [32, 255] {
        assert!(binding(slot, MacroPlayback::Code2).encode().is_err());
    }
    assert!(binding(0, MacroPlayback::Code0 { repeat: 0 })
        .encode()
        .is_err());
    for raw in [
        [0x70, 0, 0, 0],
        [0x70, 32, 1, 2],
        [0x70, 0, 2, 2],
        [0x70, 0, 2, 3],
        [0x70, 0, 1, 1],
        [0x71, 0, 1, 3],
    ] {
        assert!(MacroBinding::decode(raw).is_none());
    }
}
#[test]
fn full_library_then_marker_then_changed_pages_match_independent_complete_reports() {
    let old = lib(&[4]);
    let next = lib(&[4, 5]);
    let mut b = change(4, 10, 1);
    b.binding.playback = MacroPlayback::Code2;
    let reports = compile(&pages(), &old, &next, &[change(0, 0, 1), b]).unwrap();
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/yjx-sc620/library-plan.json"
    ))
    .unwrap();
    assert_eq!(reports.len(), 5);
    for (r, j) in reports.iter().zip(f["reports_hex"].as_array().unwrap()) {
        assert_eq!(r.to_vec(), hex(j.as_str().unwrap()));
    }
    assert_eq!(reports[2][2], 0x10);
    assert_eq!(reports[3][2], 9);
    assert_eq!(reports[4][8], 4);
    assert!(reports.iter().filter(|r| r[2] == 9).all(|r| r[8] < 5));
}
#[test]
fn sixth_read_page_references_block_global_macro_replacement() {
    let before = pages();
    let old = lib(&[4]);
    let next = lib(&[6, 5]);
    let snapshot = before.clone();
    assert!(compile(&before, &old, &next, &[change(0, 0, 1)]).is_err());
    assert_eq!(before, snapshot);
    assert_eq!(old, lib(&[4]));
}
#[test]
fn shared_slots_require_every_reference_to_be_explicitly_rebound() {
    let mut before = pages();
    before[5] = page(5, &[]);
    before[1] = page(1, &[(4, [0x70, 0, 1, 3]), (8, [0x70, 0, 1, 2])]);
    let old = lib(&[4]);
    let next = lib(&[6]);
    let a = Change {
        expected: [0x70, 0, 1, 3],
        ..change(1, 4, 0)
    };
    let b = Change {
        expected: [0x70, 0, 1, 2],
        ..change(1, 8, 0)
    };
    assert!(compile(&before, &old, &next, &[a.clone()]).is_err());
    let reports = compile(&before, &old, &next, &[a, b]).unwrap();
    assert_eq!(reports.last().unwrap()[8], 1);
}
#[test]
fn opaque_macro_bindings_allow_binding_only_edits_but_block_library_replacement() {
    let mut before = pages();
    before[2] = page(2, &[(8, [0x70, 255, 0, 99])]);
    let old = lib(&[4]);
    assert!(compile(&before, &old, &lib(&[4, 5]), &[change(0, 0, 1)]).is_err());
    let packets = compile(&before, &old, &old, &[change(0, 0, 0)]).unwrap();
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0][2], 9);
}
#[test]
fn stale_duplicate_missing_or_unordered_snapshots_are_rejected() {
    let before = pages();
    let old = lib(&[4]);
    let c = change(0, 0, 0);
    assert!(compile(&before[..5], &old, &old, &[c.clone()]).is_err());
    let mut unordered = before.clone();
    unordered.swap(0, 1);
    assert!(compile(&unordered, &old, &old, &[c.clone()]).is_err());
    assert!(compile(&before, &old, &old, &[]).is_err());
    assert!(compile(&before, &old, &old, &[c.clone(), c.clone()]).is_err());
    assert!(compile(
        &before,
        &old,
        &old,
        &[Change {
            expected: [0; 4],
            ..c
        }]
    )
    .is_err());
    assert_eq!(before, pages());
}
#[test]
fn dangling_old_or_new_slots_are_not_silently_created() {
    let before = pages();
    let old = lib(&[4]);
    assert!(compile(&before, &old, &old, &[change(0, 0, 1)]).is_err());
    let mut dangling = before.clone();
    dangling[2] = page(2, &[(3, [0x70, 2, 1, 3])]);
    assert!(compile(&dangling, &old, &lib(&[4, 5, 6]), &[change(0, 0, 1)]).is_err());
    assert!(compile(&before, &old, &lib(&[]), &[change(0, 0, 0)]).is_err());
}
#[test]
fn sixth_binding_page_is_readable_but_has_no_write_compiler_or_patch() {
    let sixth = page(5, &[]);
    let old = lib(&[4]);
    assert!(sixth.compile_output().is_err());
    assert!(sixth
        .with_macro(0, [0x20, 1, 0, 0], &binding(0, MacroPlayback::Code3), &old)
        .is_err());
    assert!(sixth
        .patch(&[KeyChange {
            record: 0,
            expected: [0x20, 1, 0, 0],
            action: Action::WheelUp
        }])
        .is_err());
    assert!(compile(&pages(), &old, &old, &[change(5, 0, 0)]).is_err());
    for p in 0..5 {
        assert!(page(p, &[]).compile_output().is_ok());
    }
}
#[test]
fn already_identical_binding_and_library_produce_no_reports() {
    let old = lib(&[4]);
    let mut before = pages();
    let c = Change {
        expected: [0x70, 0, 1, 3],
        binding: binding(0, MacroPlayback::Code3),
        ..change(0, 0, 0)
    };
    before[0] = page(0, &[(0, c.expected)]);
    assert!(compile(&before, &old, &old, &[c]).unwrap().is_empty());
}
#[test]
fn macro_binding_json_keeps_wire_codes_and_rejects_extra_or_invented_semantics() {
    let b = binding(31, MacroPlayback::Code0 { repeat: 255 });
    let j = serde_json::to_value(&b).unwrap();
    assert_eq!(serde_json::from_value::<MacroBinding>(j).unwrap(), b);
    for s in [
        r#"{"slot":0,"playback":"Toggle"}"#,
        r#"{"slot":0,"playback":"Code2","raw":true}"#,
        r#"{"slot":0,"playback":{"Code0":{"repeat":256}}}"#,
    ] {
        assert!(serde_json::from_str::<MacroBinding>(s).is_err());
    }
}
