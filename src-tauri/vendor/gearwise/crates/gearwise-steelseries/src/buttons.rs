//! Button remapping.
//!
//! The mouse takes the whole button table in one command: one fixed-width
//! slot per button, in a fixed order. A slot holds a code and, for keys, the
//! key. Codes below 0x30 name another mouse button ("act like button N");
//! 0x00 disables the button and 0x30 is the DPI button.

use std::collections::BTreeMap;

use gearwise_core::model::{ButtonAction, DpiFunction, Modifiers, MouseButton, media};
use gearwise_core::{Error, Result};

use crate::link::Packet;
use crate::models::{Btn, ButtonSpec, Field};

const DISABLED: u8 = 0x00;
const DPI_CYCLE: u8 = 0x30;
/// Most modifier keys a slot can carry next to the key.
const MAX_MODIFIERS: usize = 3;

/// The media keys rivalcfg documents; the slot carries the low byte of the
/// HID consumer usage.
const MEDIA: [u16; 6] = [
    media::PLAY_PAUSE,
    media::NEXT,
    media::PREVIOUS,
    media::MUTE,
    media::VOLUME_UP,
    media::VOLUME_DOWN,
];

/// Encode "button `id` now does `action`" together with the mapping already
/// in place. `current` holds every non-default mapping by button id.
pub fn plan(
    spec: &ButtonSpec,
    current: &BTreeMap<u16, ButtonAction>,
    id: u16,
    action: &ButtonAction,
) -> Result<Packet> {
    let known = spec
        .fields
        .iter()
        .any(|f| matches!(f, Field::Button(b) if b.id == id));
    if !known {
        return Err(Error::invalid(format!("no remappable button {id}")));
    }
    let mut actions = current.clone();
    if *action == ButtonAction::Default {
        actions.remove(&id);
    } else {
        actions.insert(id, action.clone());
    }

    let mut bytes = spec.cmd.to_vec();
    for field in spec.fields {
        let slot = match field {
            Field::Button(b) => slot(spec, b, actions.get(&b.id))?,
            Field::Fixed(fixed) => fixed.to_vec(),
        };
        bytes.extend_from_slice(&slot);
        bytes.resize(bytes.len() + spec.width - slot.len(), 0);
    }
    Ok(Packet {
        report: spec.report,
        bytes,
    })
}

fn slot(spec: &ButtonSpec, button: &Btn, action: Option<&ButtonAction>) -> Result<Vec<u8>> {
    match action {
        Some(a) => encode(spec, a),
        None => encode(spec, &button.default.action()),
    }
}

fn encode(spec: &ButtonSpec, action: &ButtonAction) -> Result<Vec<u8>> {
    match action {
        ButtonAction::Default => Err(Error::invalid("a default mapping has no slot value")),
        ButtonAction::Disabled => Ok(vec![DISABLED]),
        ButtonAction::Mouse(b) => Ok(vec![mouse_button(*b)?]),
        ButtonAction::Key { usage, mods } => key(spec, *usage, *mods),
        ButtonAction::Media(usage) => {
            if !MEDIA.contains(usage) {
                return Err(Error::Unsupported("this media key"));
            }
            Ok(vec![spec.consumer, usage.to_le_bytes()[0]])
        }
        ButtonAction::Dpi(DpiFunction::Cycle) => Ok(vec![DPI_CYCLE]),
        ButtonAction::Dpi(_) => Err(Error::Unsupported("this DPI function")),
        ButtonAction::ProfileCycle => Err(Error::Unsupported("profile switching")),
        ButtonAction::Macro(_) => Err(Error::Unsupported("macros on the mouse")),
    }
}

fn mouse_button(b: MouseButton) -> Result<u8> {
    match b {
        MouseButton::Left => Ok(1),
        MouseButton::Right => Ok(2),
        MouseButton::Middle => Ok(3),
        MouseButton::Back => Ok(4),
        MouseButton::Forward => Ok(5),
        MouseButton::WheelLeft | MouseButton::WheelRight => Err(Error::Unsupported("wheel tilt")),
    }
}

/// HID keyboard usages the firmware knows: letters through the keypad, the
/// context menu, F13-F24, two extra keypad keys and the modifier keys.
fn is_key(usage: u8) -> bool {
    matches!(usage, 0x04..=0x65 | 0x68..=0x73 | 0x85 | 0x86 | 0xe0..=0xe7)
}

/// `keyboard code, modifier keys..., key`. Modifiers are sent as the modifier
/// keys' own usages (libratbag); two-byte slots have no room for them.
fn key(spec: &ButtonSpec, usage: u8, mods: Modifiers) -> Result<Vec<u8>> {
    if !is_key(usage) {
        return Err(Error::invalid(format!("key 0x{usage:02x}")));
    }
    let mod_keys: Vec<u8> = (0..8)
        .filter(|bit| mods.0 & (1 << bit) != 0)
        .map(|bit| 0xe0 + bit)
        .collect();
    if mod_keys.len() > MAX_MODIFIERS {
        return Err(Error::Unsupported("more than three modifier keys"));
    }
    let mut slot = vec![spec.keyboard];
    slot.extend(mod_keys);
    slot.push(usage);
    if slot.len() > spec.width {
        return Err(Error::Unsupported("modifier keys on this mouse"));
    }
    Ok(slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Preset, Report};

    const SPEC: ButtonSpec = ButtonSpec {
        cmd: &[0x2a],
        report: Report::Output,
        width: 5,
        keyboard: 0x51,
        consumer: 0x61,
        fields: &[
            Field::Button(Btn {
                id: 1,
                name: "Left click",
                default: Preset::Click(MouseButton::Left),
                hotspot: (0.3, 0.2),
            }),
            Field::Button(Btn {
                id: 6,
                name: "DPI button",
                default: Preset::DpiCycle,
                hotspot: (0.5, 0.3),
            }),
            Field::Fixed(&[0x31]),
        ],
    };

    const NARROW: ButtonSpec = ButtonSpec {
        width: 2,
        keyboard: 0x33,
        consumer: 0x34,
        ..SPEC
    };

    fn none() -> BTreeMap<u16, ButtonAction> {
        BTreeMap::new()
    }

    fn key_action(usage: u8, mods: u8) -> ButtonAction {
        ButtonAction::Key {
            usage,
            mods: Modifiers(mods),
        }
    }

    #[test]
    fn default_table_uses_factory_values() {
        let p = plan(&SPEC, &none(), 1, &ButtonAction::Default).unwrap();
        assert_eq!(
            p.bytes,
            [0x2a, 1, 0, 0, 0, 0, 0x30, 0, 0, 0, 0, 0x31, 0, 0, 0, 0]
        );
    }

    #[test]
    fn slots_encode_each_kind_of_action() {
        let slot_of = |a: ButtonAction| plan(&SPEC, &none(), 1, &a).unwrap().bytes[1..6].to_vec();
        assert_eq!(slot_of(ButtonAction::Disabled), [0, 0, 0, 0, 0]);
        assert_eq!(
            slot_of(ButtonAction::Mouse(MouseButton::Middle)),
            [3, 0, 0, 0, 0]
        );
        assert_eq!(
            slot_of(ButtonAction::Mouse(MouseButton::Forward)),
            [5, 0, 0, 0, 0]
        );
        assert_eq!(
            slot_of(ButtonAction::Dpi(DpiFunction::Cycle)),
            [0x30, 0, 0, 0, 0]
        );
        assert_eq!(
            slot_of(ButtonAction::Media(media::VOLUME_UP)),
            [0x61, 0xe9, 0, 0, 0]
        );
        assert_eq!(slot_of(key_action(0x04, 0)), [0x51, 0x04, 0, 0, 0]);
        // Left Ctrl + Right Alt + A: modifier usages in bit order, then the key.
        assert_eq!(
            slot_of(key_action(0x04, Modifiers::LCTRL | Modifiers::RALT)),
            [0x51, 0xe0, 0xe6, 0x04, 0]
        );
        assert_eq!(
            slot_of(key_action(
                0x04,
                Modifiers::LCTRL | Modifiers::LSHIFT | Modifiers::LALT
            )),
            [0x51, 0xe0, 0xe1, 0xe2, 0x04]
        );
    }

    #[test]
    fn earlier_mappings_survive_a_change() {
        let mut current = none();
        current.insert(6, ButtonAction::Disabled);
        let p = plan(&SPEC, &current, 1, &ButtonAction::Mouse(MouseButton::Right)).unwrap();
        assert_eq!(&p.bytes[1..11], [2, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        // Resetting one button leaves the other mapping alone.
        let p = plan(&SPEC, &current, 1, &ButtonAction::Default).unwrap();
        assert_eq!(&p.bytes[1..11], [1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn two_byte_slots_take_plain_keys_only() {
        let p = plan(&NARROW, &none(), 1, &key_action(0x2c, 0)).unwrap();
        assert_eq!(p.bytes, [0x2a, 0x33, 0x2c, 0x30, 0, 0x31, 0]);
        let p = plan(&NARROW, &none(), 1, &ButtonAction::Media(media::MUTE)).unwrap();
        assert_eq!(&p.bytes[1..3], [0x34, 0xe2]);
        assert!(matches!(
            plan(&NARROW, &none(), 1, &key_action(0x2c, Modifiers::LCTRL)),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn refuses_what_the_firmware_cannot_do() {
        let try_action = |a: ButtonAction| plan(&SPEC, &none(), 1, &a);
        assert!(matches!(
            try_action(ButtonAction::Macro("m".into())),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            try_action(ButtonAction::ProfileCycle),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            try_action(ButtonAction::Dpi(DpiFunction::Up)),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            try_action(ButtonAction::Mouse(MouseButton::WheelLeft)),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            try_action(ButtonAction::Media(media::STOP)),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            try_action(key_action(0x04, 0x0f)),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            try_action(key_action(0x00, 0)),
            Err(Error::InvalidValue(_))
        ));
        assert!(matches!(
            try_action(key_action(0x66, 0)),
            Err(Error::InvalidValue(_))
        ));
        assert!(matches!(
            plan(&SPEC, &none(), 7, &ButtonAction::Disabled),
            Err(Error::InvalidValue(_))
        ));
    }
}
