//! HID to Linux evdev code tables (values from `linux/input-event-codes.h`).

use gearwise_core::model::MouseButton;

const KEY_A: u16 = 30;
const KEY_B: u16 = 48;
const KEY_C: u16 = 46;
const KEY_D: u16 = 32;
const KEY_E: u16 = 18;
const KEY_F: u16 = 33;
const KEY_G: u16 = 34;
const KEY_H: u16 = 35;
const KEY_I: u16 = 23;
const KEY_J: u16 = 36;
const KEY_K: u16 = 37;
const KEY_L: u16 = 38;
const KEY_M: u16 = 50;
const KEY_N: u16 = 49;
const KEY_O: u16 = 24;
const KEY_P: u16 = 25;
const KEY_Q: u16 = 16;
const KEY_R: u16 = 19;
const KEY_S: u16 = 31;
const KEY_T: u16 = 20;
const KEY_U: u16 = 22;
const KEY_V: u16 = 47;
const KEY_W: u16 = 17;
const KEY_X: u16 = 45;
const KEY_Y: u16 = 21;
const KEY_Z: u16 = 44;
const KEY_1: u16 = 2;
const KEY_2: u16 = 3;
const KEY_3: u16 = 4;
const KEY_4: u16 = 5;
const KEY_5: u16 = 6;
const KEY_6: u16 = 7;
const KEY_7: u16 = 8;
const KEY_8: u16 = 9;
const KEY_9: u16 = 10;
const KEY_0: u16 = 11;
const KEY_ENTER: u16 = 28;
const KEY_ESC: u16 = 1;
const KEY_BACKSPACE: u16 = 14;
const KEY_TAB: u16 = 15;
const KEY_SPACE: u16 = 57;
const KEY_MINUS: u16 = 12;
const KEY_EQUAL: u16 = 13;
const KEY_LEFTBRACE: u16 = 26;
const KEY_RIGHTBRACE: u16 = 27;
const KEY_BACKSLASH: u16 = 43;
const KEY_SEMICOLON: u16 = 39;
const KEY_APOSTROPHE: u16 = 40;
const KEY_GRAVE: u16 = 41;
const KEY_COMMA: u16 = 51;
const KEY_DOT: u16 = 52;
const KEY_SLASH: u16 = 53;
const KEY_CAPSLOCK: u16 = 58;
const KEY_F1: u16 = 59;
const KEY_F2: u16 = 60;
const KEY_F3: u16 = 61;
const KEY_F4: u16 = 62;
const KEY_F5: u16 = 63;
const KEY_F6: u16 = 64;
const KEY_F7: u16 = 65;
const KEY_F8: u16 = 66;
const KEY_F9: u16 = 67;
const KEY_F10: u16 = 68;
const KEY_F11: u16 = 87;
const KEY_F12: u16 = 88;
const KEY_SYSRQ: u16 = 99;
const KEY_SCROLLLOCK: u16 = 70;
const KEY_PAUSE: u16 = 119;
const KEY_INSERT: u16 = 110;
const KEY_HOME: u16 = 102;
const KEY_PAGEUP: u16 = 104;
const KEY_DELETE: u16 = 111;
const KEY_END: u16 = 107;
const KEY_PAGEDOWN: u16 = 109;
const KEY_RIGHT: u16 = 106;
const KEY_LEFT: u16 = 105;
const KEY_DOWN: u16 = 108;
const KEY_UP: u16 = 103;
const KEY_NUMLOCK: u16 = 69;
const KEY_KPSLASH: u16 = 98;
const KEY_KPASTERISK: u16 = 55;
const KEY_KPMINUS: u16 = 74;
const KEY_KPPLUS: u16 = 78;
const KEY_KPENTER: u16 = 96;
const KEY_KP1: u16 = 79;
const KEY_KP2: u16 = 80;
const KEY_KP3: u16 = 81;
const KEY_KP4: u16 = 75;
const KEY_KP5: u16 = 76;
const KEY_KP6: u16 = 77;
const KEY_KP7: u16 = 71;
const KEY_KP8: u16 = 72;
const KEY_KP9: u16 = 73;
const KEY_KP0: u16 = 82;
const KEY_KPDOT: u16 = 83;
const KEY_102ND: u16 = 86;
const KEY_COMPOSE: u16 = 127;
const KEY_F13: u16 = 183;
const KEY_F14: u16 = 184;
const KEY_F15: u16 = 185;
const KEY_F16: u16 = 186;
const KEY_F17: u16 = 187;
const KEY_F18: u16 = 188;
const KEY_F19: u16 = 189;
const KEY_F20: u16 = 190;
const KEY_F21: u16 = 191;
const KEY_F22: u16 = 192;
const KEY_F23: u16 = 193;
const KEY_F24: u16 = 194;
const KEY_LEFTCTRL: u16 = 29;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_LEFTALT: u16 = 56;
const KEY_LEFTMETA: u16 = 125;
const KEY_RIGHTCTRL: u16 = 97;
const KEY_RIGHTSHIFT: u16 = 54;
const KEY_RIGHTALT: u16 = 100;
const KEY_RIGHTMETA: u16 = 126;
const KEY_PLAYPAUSE: u16 = 164;
const KEY_NEXTSONG: u16 = 163;
const KEY_PREVIOUSSONG: u16 = 165;
const KEY_STOPCD: u16 = 166;
const KEY_MUTE: u16 = 113;
const KEY_VOLUMEUP: u16 = 115;
const KEY_VOLUMEDOWN: u16 = 114;
const KEY_CALC: u16 = 140;
const KEY_HOMEPAGE: u16 = 172;
const KEY_MAIL: u16 = 155;
const KEY_SEARCH: u16 = 217;
const KEY_BOOKMARKS: u16 = 156;
const KEY_BACK: u16 = 158;
const KEY_FORWARD: u16 = 159;
const KEY_REFRESH: u16 = 173;
const BTN_LEFT: u16 = 272;
const BTN_RIGHT: u16 = 273;
const BTN_MIDDLE: u16 = 274;
const BTN_SIDE: u16 = 275;
const BTN_EXTRA: u16 = 276;

/// Keyboard usage (page 0x07) to evdev `KEY_*`, sorted by usage. Includes
/// the two ISO-layout keys (0x32, 0x64) beyond `gearwise_core::keys::KEYS`.
#[rustfmt::skip]
const KEYS: &[(u8, u16)] = &[
    (0x04, KEY_A),
    (0x05, KEY_B),
    (0x06, KEY_C),
    (0x07, KEY_D),
    (0x08, KEY_E),
    (0x09, KEY_F),
    (0x0a, KEY_G),
    (0x0b, KEY_H),
    (0x0c, KEY_I),
    (0x0d, KEY_J),
    (0x0e, KEY_K),
    (0x0f, KEY_L),
    (0x10, KEY_M),
    (0x11, KEY_N),
    (0x12, KEY_O),
    (0x13, KEY_P),
    (0x14, KEY_Q),
    (0x15, KEY_R),
    (0x16, KEY_S),
    (0x17, KEY_T),
    (0x18, KEY_U),
    (0x19, KEY_V),
    (0x1a, KEY_W),
    (0x1b, KEY_X),
    (0x1c, KEY_Y),
    (0x1d, KEY_Z),
    (0x1e, KEY_1),
    (0x1f, KEY_2),
    (0x20, KEY_3),
    (0x21, KEY_4),
    (0x22, KEY_5),
    (0x23, KEY_6),
    (0x24, KEY_7),
    (0x25, KEY_8),
    (0x26, KEY_9),
    (0x27, KEY_0),
    (0x28, KEY_ENTER),
    (0x29, KEY_ESC),
    (0x2a, KEY_BACKSPACE),
    (0x2b, KEY_TAB),
    (0x2c, KEY_SPACE),
    (0x2d, KEY_MINUS),
    (0x2e, KEY_EQUAL),
    (0x2f, KEY_LEFTBRACE),
    (0x30, KEY_RIGHTBRACE),
    (0x31, KEY_BACKSLASH),
    (0x32, KEY_BACKSLASH),
    (0x33, KEY_SEMICOLON),
    (0x34, KEY_APOSTROPHE),
    (0x35, KEY_GRAVE),
    (0x36, KEY_COMMA),
    (0x37, KEY_DOT),
    (0x38, KEY_SLASH),
    (0x39, KEY_CAPSLOCK),
    (0x3a, KEY_F1),
    (0x3b, KEY_F2),
    (0x3c, KEY_F3),
    (0x3d, KEY_F4),
    (0x3e, KEY_F5),
    (0x3f, KEY_F6),
    (0x40, KEY_F7),
    (0x41, KEY_F8),
    (0x42, KEY_F9),
    (0x43, KEY_F10),
    (0x44, KEY_F11),
    (0x45, KEY_F12),
    (0x46, KEY_SYSRQ),
    (0x47, KEY_SCROLLLOCK),
    (0x48, KEY_PAUSE),
    (0x49, KEY_INSERT),
    (0x4a, KEY_HOME),
    (0x4b, KEY_PAGEUP),
    (0x4c, KEY_DELETE),
    (0x4d, KEY_END),
    (0x4e, KEY_PAGEDOWN),
    (0x4f, KEY_RIGHT),
    (0x50, KEY_LEFT),
    (0x51, KEY_DOWN),
    (0x52, KEY_UP),
    (0x53, KEY_NUMLOCK),
    (0x54, KEY_KPSLASH),
    (0x55, KEY_KPASTERISK),
    (0x56, KEY_KPMINUS),
    (0x57, KEY_KPPLUS),
    (0x58, KEY_KPENTER),
    (0x59, KEY_KP1),
    (0x5a, KEY_KP2),
    (0x5b, KEY_KP3),
    (0x5c, KEY_KP4),
    (0x5d, KEY_KP5),
    (0x5e, KEY_KP6),
    (0x5f, KEY_KP7),
    (0x60, KEY_KP8),
    (0x61, KEY_KP9),
    (0x62, KEY_KP0),
    (0x63, KEY_KPDOT),
    (0x64, KEY_102ND),
    (0x65, KEY_COMPOSE),
    (0x68, KEY_F13),
    (0x69, KEY_F14),
    (0x6a, KEY_F15),
    (0x6b, KEY_F16),
    (0x6c, KEY_F17),
    (0x6d, KEY_F18),
    (0x6e, KEY_F19),
    (0x6f, KEY_F20),
    (0x70, KEY_F21),
    (0x71, KEY_F22),
    (0x72, KEY_F23),
    (0x73, KEY_F24),
    (0xe0, KEY_LEFTCTRL),
    (0xe1, KEY_LEFTSHIFT),
    (0xe2, KEY_LEFTALT),
    (0xe3, KEY_LEFTMETA),
    (0xe4, KEY_RIGHTCTRL),
    (0xe5, KEY_RIGHTSHIFT),
    (0xe6, KEY_RIGHTALT),
    (0xe7, KEY_RIGHTMETA),
];

/// Consumer usage (page 0x0C) to evdev `KEY_*`.
#[rustfmt::skip]
const MEDIA: &[(u16, u16)] = &[
    (0x0cd, KEY_PLAYPAUSE),
    (0x0b5, KEY_NEXTSONG),
    (0x0b6, KEY_PREVIOUSSONG),
    (0x0b7, KEY_STOPCD),
    (0x0e2, KEY_MUTE),
    (0x0e9, KEY_VOLUMEUP),
    (0x0ea, KEY_VOLUMEDOWN),
    (0x192, KEY_CALC),
    (0x223, KEY_HOMEPAGE),
    (0x18a, KEY_MAIL),
    (0x221, KEY_SEARCH),
    (0x22a, KEY_BOOKMARKS),
    (0x224, KEY_BACK),
    (0x225, KEY_FORWARD),
    (0x227, KEY_REFRESH),
];

pub(crate) fn key(usage: u8) -> Option<u16> {
    KEYS.binary_search_by_key(&usage, |&(u, _)| u)
        .ok()
        .map(|i| KEYS[i].1)
}

pub(crate) fn media(usage: u16) -> Option<u16> {
    MEDIA
        .iter()
        .find(|&&(u, _)| u == usage)
        .map(|&(_, code)| code)
}

/// evdev `BTN_*` for a physical button; `None` for the wheel pseudo-buttons.
/// `BTN_SIDE`/`BTN_EXTRA` are the buttons desktops treat as back/forward.
pub(crate) fn button(button: MouseButton) -> Option<u16> {
    match button {
        MouseButton::Left => Some(BTN_LEFT),
        MouseButton::Right => Some(BTN_RIGHT),
        MouseButton::Middle => Some(BTN_MIDDLE),
        MouseButton::Back => Some(BTN_SIDE),
        MouseButton::Forward => Some(BTN_EXTRA),
        MouseButton::WheelLeft | MouseButton::WheelRight => None,
    }
}

/// Every code the virtual device must declare with `UI_SET_KEYBIT`.
pub(crate) fn all_codes() -> impl Iterator<Item = u16> {
    let buttons = MouseButton::ALL.into_iter().filter_map(button);
    KEYS.iter()
        .map(|&(_, c)| c)
        .chain(MEDIA.iter().map(|&(_, c)| c))
        .chain(buttons)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_with_unique_usages() {
        assert!(KEYS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn every_pickable_key_is_mapped() {
        for &(usage, name) in gearwise_core::keys::KEYS {
            assert!(
                key(usage).is_some(),
                "{name} (0x{usage:02x}) has no evdev code"
            );
        }
    }

    #[test]
    fn spot_checks_match_the_kernel_header() {
        assert_eq!(key(0x04), Some(30)); // A
        assert_eq!(key(0x1e), Some(2)); // 1
        assert_eq!(key(0x27), Some(11)); // 0
        assert_eq!(key(0x28), Some(28)); // Enter
        assert_eq!(key(0x46), Some(99)); // Print Screen is KEY_SYSRQ
        assert_eq!(key(0x4f), Some(106)); // Right
        assert_eq!(key(0x65), Some(127)); // Menu is KEY_COMPOSE
        assert_eq!(key(0x68), Some(183)); // F13
        assert_eq!(key(0x73), Some(194)); // F24
        assert_eq!(key(0xe7), Some(126)); // Right Super
        assert_eq!(key(0x00), None);
        assert_eq!(key(0x03), None);
    }

    #[test]
    fn every_pickable_media_usage_is_mapped() {
        for &(usage, name) in &gearwise_core::model::media::ALL {
            assert!(
                media(usage).is_some(),
                "{name} (0x{usage:03x}) has no evdev code"
            );
        }
        assert_eq!(media(gearwise_core::model::media::PLAY_PAUSE), Some(164));
        assert_eq!(media(0xffff), None);
    }

    #[test]
    fn mouse_buttons_map_to_btn_codes() {
        assert_eq!(button(MouseButton::Left), Some(0x110));
        assert_eq!(button(MouseButton::Middle), Some(0x112));
        assert_eq!(button(MouseButton::Back), Some(0x113));
        assert_eq!(button(MouseButton::Forward), Some(0x114));
        assert_eq!(button(MouseButton::WheelLeft), None);
        assert_eq!(button(MouseButton::WheelRight), None);
    }

    #[test]
    fn declared_codes_are_below_key_max() {
        assert!(all_codes().all(|c| (1..0x2ff).contains(&c)));
    }
}
