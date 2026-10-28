use super::*;
use gearwise_core::model::Modifiers;
fn table(mask: u8, usage: u8) -> [u8; 32] {
    let mut p = [0xabu8; 32];
    let rows = if mask == 0 {
        vec![[0x81, usage, 0], [0x41, usage, 0]]
    } else {
        vec![
            [0x80, mask, 0],
            [0x81, usage, 0],
            [0x41, usage, 0],
            [0x40, mask, 0],
        ]
    };
    p[0] = rows.len() as u8;
    for (i, r) in rows.into_iter().enumerate() {
        p[1 + i * 3..4 + i * 3].copy_from_slice(&r);
    }
    let end = 1 + usize::from(p[0]) * 3;
    p[end] = 0x55u8.wrapping_sub(sum(&p[..end]));
    p
}
#[test]
fn all_modifier_masks_and_keyboard_usages_decode_with_opaque_tail() {
    assert_eq!(
        &table(129, 6)[..14],
        &[4, 128, 129, 0, 129, 6, 0, 65, 6, 0, 64, 129, 0, 193]
    );
    for mask in 0..=255 {
        for usage in 4..=0xa4 {
            let encoded = shortcuts::encode(usage, Modifiers(mask)).unwrap();
            let mut body = table(mask, usage);
            body[..encoded.len()].copy_from_slice(&encoded);
            assert_eq!(
                shortcuts::decode(&body).unwrap(),
                ButtonAction::Key {
                    usage,
                    mods: Modifiers(mask)
                }
            );
            assert_eq!(
                shortcuts::decode(&table(mask, usage)).unwrap(),
                ButtonAction::Key {
                    usage,
                    mods: Modifiers(mask)
                }
            );
        }
    }
}
#[test]
fn incomplete_or_ambiguous_shortcuts_remain_unknown() {
    let good = table(3, 6);
    for index in 0..14 {
        let mut bad = good;
        bad[index] ^= 1;
        assert!(shortcuts::decode(&bad).is_err());
    }
    for events in [
        vec![[1, 0, 4, 0]],
        vec![[0, 0, 1, 0], [1, 0, 4, 0], [1, 1, 4, 0]],
        vec![[1, 0, 4, 0], [1, 0, 5, 0], [1, 1, 5, 0], [1, 1, 4, 0]],
        vec![[1, 0, 4, 0], [1, 1, 5, 0]],
        vec![[1, 0, 4, 1], [1, 1, 4, 1]],
        vec![[2, 0, 0xcd, 0], [2, 1, 0xcd, 0]],
        vec![[1, 0, 4, 0], [1, 2, 4, 0]],
    ] {
        assert!(shortcuts::chord(&events).is_err());
    }
}
#[test]
fn split_left_and_right_modifiers_preserve_all_eight_bits() {
    assert_eq!(
        shortcuts::chord(&[
            [0, 0, 0x11, 0],
            [0, 0, 0x88, 0],
            [1, 0, 6, 0],
            [1, 1, 6, 0],
            [0, 1, 0x11, 0],
            [0, 1, 0x88, 0]
        ])
        .unwrap(),
        ButtonAction::Key {
            usage: 6,
            mods: Modifiers(0x99)
        }
    );
}
#[test]
fn existing_oem_chords_read_and_rewrite_only_the_active_shortcut_prefix() {
    for m in models().iter().filter(|m| m.native_macros) {
        let (mut d, log) = memory_device(m.product).unwrap();
        let original = table(0x81, 6);
        for (n, row) in original.chunks(10).enumerate() {
            d.write_verified(288 + n as u16 * 10, row).unwrap();
        }
        d.write_verified(100, &[5, 0, 0, 80]).unwrap();
        log.lock().unwrap().clear();
        let state = d.read_state().unwrap();
        assert_eq!(
            state.buttons[&2],
            ButtonAction::Key {
                usage: 6,
                mods: Modifiers(0x81)
            }
        );
        assert!(!log.lock().unwrap().iter().any(
            |r| matches!(r,gearwise_core::transport::MockRecord::Write(p) if p.get(1)==Some(&7))
        ));
        d.apply(&Setting::Button {
            id: 2,
            action: ButtonAction::Key {
                usage: 5,
                mods: Modifiers(3),
            },
        })
        .unwrap();
        let actual = d.read_blob(288, 32).unwrap();
        let payload = shortcuts::encode(5, Modifiers(3)).unwrap();
        assert_eq!(&actual[..payload.len()], payload.as_slice());
        assert_eq!(&actual[payload.len()..], &original[payload.len()..]);
        let state = d.read_state().unwrap();
        assert_eq!(
            state.buttons[&2],
            ButtonAction::Key {
                usage: 5,
                mods: Modifiers(3)
            }
        );
        assert!(d.macro_definitions().is_empty());
    }
}

fn action() -> Setting {
    Setting::Button {
        id: 2,
        action: ButtonAction::Key {
            usage: 6,
            mods: Modifiers(129),
        },
    }
}
#[test]
fn exact_class5_reports_keep_other_control_tables_macro_slots_and_devices_unchanged() {
    for m in models().iter().filter(|m| m.native_macros) {
        let (mut d, log) = memory_device(m.product).unwrap();
        let (mut other, other_log) = memory_device(m.product).unwrap();
        let before = d.read_blob(0, 768 + 12 * 384).unwrap();
        let other_before = other.read_blob(0, before.len()).unwrap();
        log.lock().unwrap().clear();
        other_log.lock().unwrap().clear();
        d.apply(&action()).unwrap();
        let after = d.read_blob(0, before.len()).unwrap();
        assert_eq!(
            d.read_state().unwrap().buttons[&2],
            ButtonAction::Key {
                usage: 6,
                mods: Modifiers(129)
            }
        );
        for at in 0..before.len() {
            if !(288..302).contains(&at) && !(100..104).contains(&at) {
                assert_eq!(after[at], before[at], "{} {at}", m.name);
            }
        }
        assert_eq!(other.read_blob(0, before.len()).unwrap(), other_before);
        assert!(!other_log.lock().unwrap().iter().any(
            |r| matches!(r,gearwise_core::transport::MockRecord::Write(p)if p.get(1)==Some(&7))
        ));
        let records = log.lock().unwrap();
        for golden in [
            vec![8, 7, 0, 1, 32, 10, 4, 128, 129, 0, 129, 6, 0, 65, 6, 0, 72],
            vec![8, 7, 0, 1, 42, 4, 64, 129, 0, 193, 0, 0, 0, 0, 0, 0, 149],
            vec![8, 7, 0, 0, 100, 4, 5, 0, 0, 80, 0, 0, 0, 0, 0, 0, 137],
        ] {
            assert!(records
                .iter()
                .any(|r| matches!(r,gearwise_core::transport::MockRecord::Write(p)if p==&golden)));
        }
        drop(records);
        d.apply(&Setting::Button {
            id: 4,
            action: ButtonAction::Key {
                usage: 7,
                mods: Modifiers(2),
            },
        })
        .unwrap();
        assert_eq!(
            d.read_state().unwrap().buttons[&2],
            ButtonAction::Key {
                usage: 6,
                mods: Modifiers(129)
            }
        );
        assert_eq!(
            d.read_state().unwrap().buttons[&4],
            ButtonAction::Key {
                usage: 7,
                mods: Modifiers(2)
            }
        );
        assert_eq!(d.read_blob(768, 12 * 384).unwrap(), before[768..]);
    }
}
#[test]
fn each_shortcut_set_before_or_after_effect_restores_full_original_state() {
    for m in models().iter().filter(|m| m.native_macros) {
        for nth in 1..=3 {
            for after in [false, true] {
                let (mut d, _) = memory_device(m.product).unwrap();
                let before = d.read_blob(0, 768 + 12 * 384).unwrap();
                let (mut d, _) =
                    super::macro_transaction_tests::instrument(d, vec![(nth, after)], None);
                let error = d.apply(&action()).unwrap_err().to_string();
                assert!(
                    error.contains("original shortcut and binding restored and verified"),
                    "{} {nth} {after} {error}",
                    m.name
                );
                assert_eq!(d.read_blob(0, before.len()).unwrap(), before);
            }
        }
    }
}
#[test]
fn foreign_partial_prefix_tail_and_other_binding_changes_stop_shortcut_restoration() {
    for m in models().iter().filter(|m| m.native_macros) {
        for (address, bytes) in [
            (288, vec![0xa5]),
            (289, vec![0xee]),
            (319, vec![0xa5]),
            (108, vec![1, 2, 0, 82]),
        ] {
            let (d, _) = memory_device(m.product).unwrap();
            let (mut d, count) = super::macro_transaction_tests::instrument(
                d,
                vec![],
                Some((1, address, bytes.clone())),
            );
            let error = d.apply(&action()).unwrap_err().to_string();
            assert!(
                error.contains("state uncertain"),
                "{} {address} {error}",
                m.name
            );
            assert_eq!(count.lock().unwrap().sets, 1);
            assert_eq!(d.read(address, bytes.len()).unwrap(), bytes);
        }
    }
}
#[test]
fn unknown_tables_stale_binding_and_failed_shortcut_restore_do_not_claim_success() {
    for m in models().iter().filter(|m| m.native_macros) {
        let (mut d, _) = memory_device(m.product).unwrap();
        d.write_verified(288, &[0xa5]).unwrap();
        let (mut d, count) = super::macro_transaction_tests::instrument(d, vec![], None);
        assert!(d
            .apply(&action())
            .unwrap_err()
            .to_string()
            .contains("unknown shortcut table retained"));
        assert_eq!(count.lock().unwrap().sets, 0);
        let (mut d, _) = memory_device(m.product).unwrap();
        let old = d.read(100, 4).unwrap();
        d.write_verified(100, &[1, 4, 0, 80]).unwrap();
        let (mut d, count) = super::macro_transaction_tests::instrument(d, vec![], None);
        assert!(d
            .commit_native_shortcut(2, &shortcuts::encode(6, Modifiers(129)).unwrap(), &old)
            .is_err());
        assert_eq!(count.lock().unwrap().sets, 0);
        let (d, _) = memory_device(m.product).unwrap();
        let (mut d, count) =
            super::macro_transaction_tests::instrument(d, vec![(2, true), (3, false)], None);
        let error = d.apply(&action()).unwrap_err().to_string();
        assert!(error.contains("state uncertain"));
        assert!(!error.contains("restored and verified"));
        assert_eq!(count.lock().unwrap().sets, 3);
    }
}
struct BankFlip {
    inner: Box<dyn HidTransport>,
    flipped: bool,
}
impl HidTransport for BankFlip {
    fn write(&mut self, p: &[u8]) -> Result<usize> {
        let n = self.inner.write(p)?;
        if p[1] == 7 {
            self.flipped = true;
        }
        Ok(n)
    }
    fn read_timeout(&mut self, p: &mut [u8], ms: i32) -> Result<usize> {
        let n = self.inner.read_timeout(p, ms)?;
        if self.flipped && n == 17 && p[1] == 0x0e {
            p[..17].copy_from_slice(&frame(0x0e, 0, 1, &[1])?);
        }
        Ok(n)
    }
    fn send_feature_report(&mut self, p: &[u8]) -> Result<()> {
        self.inner.send_feature_report(p)
    }
    fn get_feature_report(&mut self, p: &mut [u8]) -> Result<usize> {
        self.inner.get_feature_report(p)
    }
}
#[test]
fn board_change_after_first_shortcut_set_blocks_all_followup_sets_and_restoration() {
    for m in models().iter().filter(|m| m.native_macros) {
        let (d, _) = memory_device(m.product).unwrap();
        let (mut d, count) = super::macro_transaction_tests::instrument(d, vec![], None);
        d.io = Box::new(BankFlip {
            inner: d.io,
            flipped: false,
        });
        let e = d.apply(&action()).unwrap_err().to_string();
        assert!(e.contains("state uncertain"));
        assert_eq!(count.lock().unwrap().sets, 1);
    }
}

#[test]
fn modifier_only_taps_keep_existing_native_macro_path() {
    for m in models().iter().filter(|m| m.native_macros) {
        let (mut d, _) = memory_device(m.product).unwrap();
        d.apply(&Setting::Button {
            id: 2,
            action: ButtonAction::Key {
                usage: 224,
                mods: Modifiers(0),
            },
        })
        .unwrap();
        let state = d.read_state().unwrap();
        assert!(matches!(state.buttons[&2], ButtonAction::Macro(_)));
        assert_eq!(d.read_blob(288, 32).unwrap(), vec![0; 32]);
        assert!(d.macro_definitions().iter().any(|m| m
            .steps
            .contains(&gearwise_core::profile::MacroStep::KeyDown(224))));
    }
}

#[test]
fn malformed_shortcut_transactions_and_foreign_protocols_reject_before_any_set() {
    for product in [0x1115, 0xf507] {
        let (d, _) = memory_device(product).unwrap();
        let (mut d, count) = super::macro_transaction_tests::instrument(d, vec![], None);
        for id in [0, 1, 2, 7] {
            for data in [
                vec![],
                vec![2],
                vec![0; 21],
                vec![2, 0x81, 4, 0, 0x41, 4, 0, 0],
            ] {
                assert!(d.commit_native_shortcut(id, &data, &[1, 2, 0, 82]).is_err());
            }
        }
        if product == 0xf507 {
            assert!(d
                .commit_native_shortcut(
                    2,
                    &shortcuts::encode(4, Modifiers(0)).unwrap(),
                    &[1, 2, 0, 82]
                )
                .is_err());
        }
        assert_eq!(count.lock().unwrap().sets, 0);
    }
}

fn source_sequence() -> Vec<gearwise_core::model::ShortcutEvent> {
    use gearwise_core::model::{ShortcutEvent as E, ShortcutKind as K};
    vec![
        E {
            kind: K::Modifier,
            down: true,
            value: 1,
        },
        E {
            kind: K::Modifier,
            down: true,
            value: 128,
        },
        E {
            kind: K::Key,
            down: true,
            value: 6,
        },
        E {
            kind: K::Modifier,
            down: false,
            value: 1,
        },
        E {
            kind: K::Modifier,
            down: false,
            value: 128,
        },
        E {
            kind: K::Key,
            down: false,
            value: 6,
        },
    ]
}
#[test]
fn original_release_order_and_modifier_grouping_roundtrip_exact_native_records() {
    let expected = vec![
        6, 128, 1, 0, 128, 128, 0, 129, 6, 0, 64, 1, 0, 64, 128, 0, 65, 6, 0, 255,
    ];
    assert_eq!(
        shortcuts::encode_events(&source_sequence()).unwrap(),
        expected
    );
    let mut raw = vec![171; 32];
    raw[..20].copy_from_slice(&expected);
    assert_eq!(
        shortcuts::decode(&raw).unwrap(),
        ButtonAction::Shortcut(source_sequence())
    );
    for m in models().iter().filter(|m| m.native_macros) {
        let (mut d, _) = memory_device(m.product).unwrap();
        let before = d.read_blob(0, 768 + 12 * 384).unwrap();
        d.apply(&Setting::Button {
            id: 2,
            action: ButtonAction::Shortcut(source_sequence()),
        })
        .unwrap();
        assert_eq!(d.read(288, 10).unwrap(), expected[..10]);
        assert_eq!(d.read_blob(288, 20).unwrap(), expected);
        assert_eq!(
            d.read_state().unwrap().buttons[&2],
            ButtonAction::Shortcut(source_sequence())
        );
        assert_eq!(d.read_blob(768, 12 * 384).unwrap(), before[768..]);
        let info = HidInterfaceInfo {
            vendor_id: m.vendor,
            product_id: m.product,
            usage_page: m.usage_page,
            usage: 2,
            interface_number: 1,
            ..Default::default()
        };
        let mut reopened = Atk::open(d.io, &info).unwrap();
        assert_eq!(
            reopened.read_state().unwrap().buttons[&2],
            ButtonAction::Shortcut(source_sequence())
        );
    }
}
#[test]
fn full_sequences_keep_key_mouse_media_events_and_refuse_held_or_unknown_inputs() {
    use gearwise_core::model::{ShortcutEvent as E, ShortcutKind as K};
    let events = vec![
        E {
            kind: K::Key,
            down: true,
            value: 4,
        },
        E {
            kind: K::Mouse,
            down: true,
            value: 8,
        },
        E {
            kind: K::Key,
            down: false,
            value: 4,
        },
        E {
            kind: K::Mouse,
            down: false,
            value: 8,
        },
        E {
            kind: K::Media,
            down: true,
            value: 205,
        },
        E {
            kind: K::Media,
            down: false,
            value: 205,
        },
    ];
    for m in models().iter().filter(|m| m.native_macros) {
        let (mut d, _) = memory_device(m.product).unwrap();
        d.apply(&Setting::Button {
            id: 2,
            action: ButtonAction::Shortcut(events.clone()),
        })
        .unwrap();
        assert_eq!(
            d.read_state().unwrap().buttons[&2],
            ButtonAction::Shortcut(events.clone())
        );
    }
    let payload = shortcuts::encode_events(&events).unwrap();
    let mut raw = vec![0; 32];
    raw[..payload.len()].copy_from_slice(&payload);
    assert_eq!(
        shortcuts::decode(&raw).unwrap(),
        ButtonAction::Shortcut(events.clone())
    );
    for at in 0..events.len() {
        let mut bad = events.clone();
        bad.remove(at);
        assert!(shortcuts::encode_events(&bad).is_err());
    }
    let mut bad = events.clone();
    bad[1].value = 3;
    assert!(shortcuts::encode_events(&bad).is_err());
    let mut bad = events;
    bad[4].value = 255;
    assert!(shortcuts::encode_events(&bad).is_err());
}
#[test]
fn sequence_transactions_recover_every_write_and_refuse_foreign_state() {
    for m in models().iter().filter(|m| m.native_macros) {
        let action = Setting::Button {
            id: 2,
            action: ButtonAction::Shortcut(source_sequence()),
        };
        for nth in 1..=3 {
            for after in [false, true] {
                let (mut d, _) = memory_device(m.product).unwrap();
                let before = d.read_blob(0, 768 + 12 * 384).unwrap();
                let (mut d, _) =
                    super::macro_transaction_tests::instrument(d, vec![(nth, after)], None);
                assert!(d
                    .apply(&action)
                    .unwrap_err()
                    .to_string()
                    .contains("restored and verified"));
                assert_eq!(d.read_blob(0, before.len()).unwrap(), before);
            }
        }
        let (d, _) = memory_device(m.product).unwrap();
        let (mut d, counter) =
            super::macro_transaction_tests::instrument(d, vec![], Some((1, 319, vec![0xa5])));
        assert!(d
            .apply(&action)
            .unwrap_err()
            .to_string()
            .contains("state uncertain"));
        assert_eq!(counter.lock().unwrap().sets, 1);
    }
}

#[test]
fn every_source_modifier_release_permutation_retains_complete_wire_order() {
    for usage in 4..=0xa4 {
        for downs in [[1, 128], [128, 1]] {
            let releases = [
                [0, 1, 2],
                [0, 2, 1],
                [1, 0, 2],
                [1, 2, 0],
                [2, 0, 1],
                [2, 1, 0],
            ];
            for order in releases {
                let mut rows = vec![[0, 0, downs[0], 0], [0, 0, downs[1], 0], [1, 0, usage, 0]];
                let ups = [[0, 1, 1, 0], [0, 1, 128, 0], [1, 1, usage, 0]];
                for i in order {
                    rows.push(ups[i]);
                }
                let action = shortcuts::from_rows(&rows).unwrap();
                let ButtonAction::Shortcut(events) = action else {
                    panic!("split records collapsed")
                };
                let raw = shortcuts::encode_events(&events).unwrap();
                let expected = rows
                    .iter()
                    .flat_map(|r| [if r[1] == 0 { 128 } else { 64 } | r[0], r[2], 0])
                    .collect::<Vec<_>>();
                assert_eq!(&raw[1..raw.len() - 1], expected);
            }
        }
    }
}
