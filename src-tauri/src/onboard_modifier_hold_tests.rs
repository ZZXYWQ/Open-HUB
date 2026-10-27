use super::*;
use gearwise_core::{
    model::Modifiers,
    profile::{Macro, MacroMode, MacroStep},
};
fn definition(steps: Vec<MacroStep>) -> Macro {
    Macro {
        id: "modifier-hold".into(),
        name: "modifier hold".into(),
        mode: MacroMode::Once,
        steps,
    }
}
#[test]
fn eight_independent_modifiers_have_exact_delta_mask_and_zero_key_operands() {
    for bit in 0..8 {
        let key = 0xe0 + bit;
        let bytes = compile(
            &definition(vec![
                MacroStep::KeyDown(key),
                MacroStep::KeyDown(4),
                MacroStep::Delay(17),
                MacroStep::KeyUp(4),
                MacroStep::KeyUp(key),
            ]),
            256,
        )
        .unwrap();
        assert_eq!(
            bytes,
            vec![
                0x43,
                1 << bit,
                0,
                0x43,
                0,
                4,
                0x40,
                0,
                17,
                0x44,
                0,
                4,
                0x44,
                1 << bit,
                0,
                0xff
            ]
        );
    }
}
#[test]
fn nested_sides_preserve_individual_press_release_order() {
    let mut steps = (0xe0..=0xe7).map(MacroStep::KeyDown).collect::<Vec<_>>();
    steps.extend([MacroStep::KeyDown(6), MacroStep::KeyUp(6)]);
    steps.extend((0xe0..=0xe7).rev().map(MacroStep::KeyUp));
    let bytes = compile(&definition(steps), 256).unwrap();
    let mut expected = Vec::new();
    for bit in 0..8 {
        expected.extend([0x43, 1 << bit, 0]);
    }
    expected.extend([0x43, 0, 6, 0x44, 0, 6]);
    for bit in (0..8).rev() {
        expected.extend([0x44, 1 << bit, 0]);
    }
    expected.push(255);
    assert_eq!(bytes, expected);
}
#[test]
fn atomic_shortcut_cannot_release_a_separately_held_modifier() {
    for bit in 0..8 {
        for mask in [1 << bit, 255] {
            let m = definition(vec![
                MacroStep::KeyDown(0xe0 + bit),
                MacroStep::KeyTap {
                    usage: 4,
                    mods: Modifiers(mask),
                },
                MacroStep::KeyUp(0xe0 + bit),
            ]);
            assert!(compile(&m, 256)
                .unwrap_err()
                .to_string()
                .contains("overlaps"));
        }
        let m = definition(vec![
            MacroStep::KeyDown(0xe0 + bit),
            MacroStep::KeyTap {
                usage: 4,
                mods: Modifiers(1 << ((bit + 1) % 8)),
            },
            MacroStep::KeyUp(0xe0 + bit),
        ]);
        assert!(compile(&m, 256).is_ok());
    }
}
#[test]
fn unbalanced_duplicate_or_invalid_modifier_uses_never_compile() {
    for steps in [
        vec![MacroStep::KeyDown(224)],
        vec![MacroStep::KeyUp(224)],
        vec![MacroStep::KeyDown(224), MacroStep::KeyUp(225)],
        vec![
            MacroStep::KeyDown(224),
            MacroStep::KeyDown(224),
            MacroStep::KeyUp(224),
        ],
        vec![MacroStep::KeyDown(232), MacroStep::KeyUp(232)],
    ] {
        assert!(compile(&definition(steps), 256).is_err());
    }
}
#[test]
fn held_modifiers_cross_explicit_page_links_without_an_inserted_release() {
    for size in [255, 256] {
        let info = Info {
            memory_model: 1,
            profile_format: 3,
            macro_format: 1,
            profile_count: 1,
            rom_profile_count: 0,
            button_count: 5,
            sector_count: 6,
            sector_size: size,
            mechanical_layout: 10,
            various_info: 1,
            bindings_writable: true,
        };
        let crc = |mut p: Vec<u8>| {
            let n = p.len();
            let c = crate::onboard::crc_ccitt(&p[..n - 2]);
            p[n - 2..].copy_from_slice(&c.to_be_bytes());
            p
        };
        let mut pages = (0..6u16)
            .map(|s| (s, vec![255; size as usize]))
            .collect::<BTreeMap<_, _>>();
        let mut dir = pages[&0].clone();
        dir[..4].copy_from_slice(&[0, 1, 1, 0]);
        pages.insert(0, crc(dir));
        let mut profile = pages[&1].clone();
        profile[0x20..0x24].copy_from_slice(&[128, 1, 0, 1]);
        pages.insert(1, crc(profile));
        let inventory = inventory(&info, &pages).unwrap();
        let bank = Bank {
            device_key: "unit:test".into(),
            info,
            pages,
            inventory,
        };
        let mut steps = vec![MacroStep::KeyDown(225)];
        steps.extend(vec![
            MacroStep::KeyTap {
                usage: 4,
                mods: Modifiers(0)
            };
            43
        ]);
        steps.push(MacroStep::KeyUp(225));
        let p = plan(&bank, &definition(steps)).unwrap();
        assert_eq!(p.pages.len(), 2);
        let trace = inspect(
            Address {
                sector: p.macro_sector,
                offset: 0,
            },
            &bank.info,
            |s| Ok(p.pages[&s].clone()),
        )
        .unwrap();
        assert!(trace.complete);
        assert_eq!(trace.steps.len(), 90);
        assert_eq!(trace.steps.first().unwrap().bytes, [0x43, 2, 0]);
        assert_eq!(trace.steps[88].bytes, [0x44, 2, 0]);
        assert!(trace.steps.iter().any(|s| s.bytes[0] == 0x60));
    }
}
