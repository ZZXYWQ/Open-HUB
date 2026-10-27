use super::*;
use gearwise_core::{
    profile::{MacroMode, MacroStep},
    transport::MockRecord,
};
fn definition(count: u8) -> Macro {
    Macro {
        id: "finite".into(),
        name: "Finite".into(),
        mode: MacroMode::Repeat(count),
        steps: vec![
            MacroStep::KeyDown(4),
            MacroStep::Delay(50),
            MacroStep::KeyUp(4),
            MacroStep::Delay(1),
        ],
    }
}
#[test]
fn all_finite_binding_bytes_are_distinct_from_three_special_modes() {
    for count in 2..=252 {
        let m = definition(count);
        let raw = native_macro::encode(&m).unwrap();
        assert_eq!(native_macro::mode(m.mode), count);
        let got = native_macro::decode(0, count, &raw).unwrap();
        assert_eq!(got.mode, m.mode);
        assert_eq!(got.steps, m.steps);
        assert_eq!(&raw[31..], &[2, 129, 4, 0, 0, 50, 65, 4, 0, 0, 1, 0x56]);
    }
    for count in [0, 1, 253, 254, 255] {
        assert!(native_macro::encode(&definition(count)).is_err());
    }
}
#[test]
fn nine_transports_counted_macro_readback_and_other_mouse_are_independent() {
    for model in models()
        .iter()
        .filter(|m| m.native_macros && m.native_buttons)
    {
        let (mut d, log) = memory_device(model.product).unwrap();
        let (mut other, _) = memory_device(model.product).unwrap();
        let peer = other.read_state().unwrap();
        for count in [2, 3, 10, 252] {
            let m = definition(count);
            d.set_macro_definitions(&[m.clone()]).unwrap();
            d.apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Macro(m.id),
            })
            .unwrap();
            let state = d.read_state().unwrap();
            let ButtonAction::Macro(id) = &state.buttons[&4] else {
                panic!("counted macro collapsed into a different action")
            };
            let got = d
                .macro_definitions()
                .into_iter()
                .find(|m| &m.id == id)
                .unwrap();
            assert_eq!(got.mode, MacroMode::Repeat(count));
            assert_eq!(got.steps, m.steps);
            assert!(log.lock().unwrap().iter().any(|r|matches!(r,MockRecord::Write(p)if p[1]==7&&p[3..6]==[0,108,4]&&p[6..10]==[6,0,count,0x55u8.wrapping_sub(6).wrapping_sub(count)])));
        }
        assert_eq!(other.read_state().unwrap(), peer);
    }
}
#[test]
fn invalid_finite_modes_never_write_a_macro_or_binding() {
    for count in [0, 1, 253, 254, 255] {
        let (mut d, log) = memory_device(0x1017).unwrap();
        log.lock().unwrap().clear();
        assert!(d.set_macro_definitions(&[definition(count)]).is_err());
        assert!(log.lock().unwrap().is_empty());
    }
}
