use super::*;
use gearwise_core::model::Modifiers;
fn action(count: u8, steps: Vec<MacroStep>) -> Macro {
    Macro {
        id: "counted-native".into(),
        name: "finite native".into(),
        mode: MacroMode::Repeat(count),
        steps,
    }
}
fn bank(size: u16) -> Bank {
    let info = Info {
        memory_model: 1,
        profile_format: 3,
        macro_format: 1,
        profile_count: 1,
        rom_profile_count: 0,
        button_count: 5,
        sector_count: 16,
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
    let mut pages = (0..16u16)
        .map(|s| (s, vec![255; size as usize]))
        .collect::<BTreeMap<_, _>>();
    let mut dir = pages[&0].clone();
    dir[..4].copy_from_slice(&[0, 1, 1, 0]);
    pages.insert(0, crc(dir));
    let mut p = pages[&1].clone();
    p[0x20..0x24].copy_from_slice(&[128, 1, 0, 1]);
    pages.insert(1, crc(p));
    let inv = inventory(&info, &pages).unwrap();
    Bank {
        device_key: "unit:test".into(),
        info,
        pages,
        inventory: inv,
    }
}
#[test]
fn finite_program_expands_exact_complete_bodies_with_only_one_terminal() {
    let body = vec![
        MacroStep::KeyTap {
            usage: 6,
            mods: Modifiers(17),
        },
        MacroStep::Delay(77),
        MacroStep::Media(226),
    ];
    let expected = vec![
        0x43, 17, 6, 0x44, 17, 6, 0x40, 0, 77, 0x45, 0, 226, 0x46, 0, 226,
    ];
    for count in [2, 3, 5, 20] {
        let bytes = compile(&action(count, body.clone()), 256).unwrap();
        assert_eq!(bytes, [expected.repeat(count as usize), vec![255]].concat());
    }
}
#[test]
fn native_schema_count_limit_is_not_another_backends_reserved_counter_byte() {
    for count in 2..=255 {
        let m = action(count, vec![MacroStep::Delay(1)]);
        assert_eq!(
            compile(&m, 256).unwrap(),
            [vec![0x40, 0, 1].repeat(count as usize), vec![255]].concat()
        );
    }
    for count in [0, 1] {
        assert!(compile(&action(count, vec![MacroStep::Delay(1)]), 256).is_err());
    }
    for count in [253, 254, 255] {
        let m = action(count, vec![MacroStep::Delay(1)]);
        assert!(crate::host_macros::validate(&m).is_err());
        assert!(compile(&m, 256).is_ok());
    }
}
#[test]
fn count_does_not_bypass_body_balance_modifier_conflicts_or_supported_event_rules() {
    for steps in [
        vec![MacroStep::KeyDown(4)],
        vec![MacroStep::Text("a".into())],
        vec![MacroStep::WaitForRelease],
        vec![
            MacroStep::KeyDown(224),
            MacroStep::KeyTap {
                usage: 4,
                mods: Modifiers(1),
            },
            MacroStep::KeyUp(224),
        ],
        vec![MacroStep::Delay(0)],
    ] {
        assert!(compile(&action(2, steps), 256).is_err());
    }
}
#[test]
fn expansion_instruction_page_and_capacity_limits_refuse_without_truncation() {
    let body = vec![
        MacroStep::KeyTap {
            usage: 4,
            mods: Modifiers(0)
        };
        256
    ];
    assert!(compile(&action(2, body), 256)
        .unwrap_err()
        .to_string()
        .contains("not truncated"));
    let b = bank(256);
    let m = action(
        2,
        vec![
            MacroStep::KeyTap {
                usage: 4,
                mods: Modifiers(0)
            };
            255
        ],
    );
    assert!(compile(&m, 256).is_ok());
    assert!(plan(&b, &m)
        .unwrap_err()
        .to_string()
        .contains("reader budget"));
    let mut b = bank(256);
    b.inventory.erased_unreferenced.clear();
    assert!(plan(&b, &action(2, vec![MacroStep::Delay(1)])).is_err());
}
#[test]
fn high_count_and_cross_page_programs_preserve_each_iterations_release() {
    for size in [255, 256] {
        let b = bank(size);
        let m = action(
            100,
            vec![
                MacroStep::KeyDown(228),
                MacroStep::KeyDown(4),
                MacroStep::KeyUp(4),
                MacroStep::KeyUp(228),
            ],
        );
        let p = plan(&b, &m).unwrap();
        assert!(p.pages.len() > 1);
        let trace = inspect(
            Address {
                sector: p.macro_sector,
                offset: 0,
            },
            &b.info,
            |s| Ok(p.pages[&s].clone()),
        )
        .unwrap();
        assert!(trace.complete);
        let bytes = trace
            .steps
            .iter()
            .filter(|s| s.bytes[0] != 0x60 && s.bytes[0] != 255)
            .flat_map(|s| s.bytes.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            bytes,
            vec![0x43, 16, 0, 0x43, 0, 4, 0x44, 0, 4, 0x44, 16, 0].repeat(100)
        );
        assert!(trace.steps.iter().all(|s| !matches!(
            s.action,
            crate::onboard_macros::Action::RepeatWhilePressed
                | crate::onboard_macros::Action::RepeatUntilCanceled
        )));
    }
}
