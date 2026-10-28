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
fn existing_oem_chords_read_and_can_be_replaced_without_touching_shortcut_table() {
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
        assert_eq!(d.read_blob(288, 32).unwrap(), original);
        let state = d.read_state().unwrap();
        let ButtonAction::Macro(id) = &state.buttons[&2] else {
            panic!("converted shortcut is a native macro")
        };
        let def = d.read_macros.iter().find(|m| &m.id == id).unwrap();
        let inputs = def
            .steps
            .iter()
            .filter(|s| !matches!(s, gearwise_core::profile::MacroStep::Delay(_)))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(inputs, native_macro::keyboard(5, Modifiers(3)).steps);
    }
}
