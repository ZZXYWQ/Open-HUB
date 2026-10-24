//! HID to Windows key tables: set-1 scancodes for keyboard keys and virtual
//! keys for media keys. Plain data, so it is unit-tested on every platform.

/// How `SendInput` should name a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WinKey {
    /// Hardware scancode (`KEYEVENTF_SCANCODE`); `extended` is the E0 prefix.
    Scan { code: u16, extended: bool },
    /// Virtual-key code, for keys whose scancode cannot be injected
    /// reliably (Pause has an E1 prefix; NumLock shares scancode 0x45).
    Vk(u16),
}

const fn s(code: u16) -> WinKey {
    WinKey::Scan {
        code,
        extended: false,
    }
}

const fn e(code: u16) -> WinKey {
    WinKey::Scan {
        code,
        extended: true,
    }
}

const VK_PAUSE: u16 = 0x13;
const VK_NUMLOCK: u16 = 0x90;

/// Keyboard usage (page 0x07) to key, sorted by usage. Includes the two
/// ISO-layout keys (0x32, 0x64) beyond `gearwise_core::keys::KEYS`.
#[rustfmt::skip]
const KEYS: &[(u8, WinKey)] = &[
    (0x04, s(0x1e)), (0x05, s(0x30)), (0x06, s(0x2e)), (0x07, s(0x20)), (0x08, s(0x12)),
    (0x09, s(0x21)), (0x0a, s(0x22)), (0x0b, s(0x23)), (0x0c, s(0x17)), (0x0d, s(0x24)),
    (0x0e, s(0x25)), (0x0f, s(0x26)), (0x10, s(0x32)), (0x11, s(0x31)), (0x12, s(0x18)),
    (0x13, s(0x19)), (0x14, s(0x10)), (0x15, s(0x13)), (0x16, s(0x1f)), (0x17, s(0x14)),
    (0x18, s(0x16)), (0x19, s(0x2f)), (0x1a, s(0x11)), (0x1b, s(0x2d)), (0x1c, s(0x15)),
    (0x1d, s(0x2c)),
    // 1-9, 0
    (0x1e, s(0x02)), (0x1f, s(0x03)), (0x20, s(0x04)), (0x21, s(0x05)), (0x22, s(0x06)),
    (0x23, s(0x07)), (0x24, s(0x08)), (0x25, s(0x09)), (0x26, s(0x0a)), (0x27, s(0x0b)),
    // Enter, Esc, Backspace, Tab, Space
    (0x28, s(0x1c)), (0x29, s(0x01)), (0x2a, s(0x0e)), (0x2b, s(0x0f)), (0x2c, s(0x39)),
    // - = [ ] \ (ISO #) ; ' ` , . /
    (0x2d, s(0x0c)), (0x2e, s(0x0d)), (0x2f, s(0x1a)), (0x30, s(0x1b)), (0x31, s(0x2b)),
    (0x32, s(0x2b)), (0x33, s(0x27)), (0x34, s(0x28)), (0x35, s(0x29)), (0x36, s(0x33)),
    (0x37, s(0x34)), (0x38, s(0x35)),
    // Caps Lock, F1-F12
    (0x39, s(0x3a)),
    (0x3a, s(0x3b)), (0x3b, s(0x3c)), (0x3c, s(0x3d)), (0x3d, s(0x3e)), (0x3e, s(0x3f)),
    (0x3f, s(0x40)), (0x40, s(0x41)), (0x41, s(0x42)), (0x42, s(0x43)), (0x43, s(0x44)),
    (0x44, s(0x57)), (0x45, s(0x58)),
    // Print Screen, Scroll Lock, Pause
    (0x46, e(0x37)), (0x47, s(0x46)), (0x48, WinKey::Vk(VK_PAUSE)),
    // Insert, Home, Page Up, Delete, End, Page Down, Right, Left, Down, Up
    (0x49, e(0x52)), (0x4a, e(0x47)), (0x4b, e(0x49)), (0x4c, e(0x53)), (0x4d, e(0x4f)),
    (0x4e, e(0x51)), (0x4f, e(0x4d)), (0x50, e(0x4b)), (0x51, e(0x50)), (0x52, e(0x48)),
    // Num Lock, keypad / * - + Enter
    (0x53, WinKey::Vk(VK_NUMLOCK)), (0x54, e(0x35)), (0x55, s(0x37)), (0x56, s(0x4a)),
    (0x57, s(0x4e)), (0x58, e(0x1c)),
    // Keypad 1-9, 0, .
    (0x59, s(0x4f)), (0x5a, s(0x50)), (0x5b, s(0x51)), (0x5c, s(0x4b)), (0x5d, s(0x4c)),
    (0x5e, s(0x4d)), (0x5f, s(0x47)), (0x60, s(0x48)), (0x61, s(0x49)), (0x62, s(0x52)),
    (0x63, s(0x53)),
    // ISO extra key, Menu
    (0x64, s(0x56)), (0x65, e(0x5d)),
    // F13-F24
    (0x68, s(0x64)), (0x69, s(0x65)), (0x6a, s(0x66)), (0x6b, s(0x67)), (0x6c, s(0x68)),
    (0x6d, s(0x69)), (0x6e, s(0x6a)), (0x6f, s(0x6b)), (0x70, s(0x6c)), (0x71, s(0x6d)),
    (0x72, s(0x6e)), (0x73, s(0x76)),
    // Left Ctrl/Shift/Alt/Win, Right Ctrl/Shift/Alt/Win
    (0xe0, s(0x1d)), (0xe1, s(0x2a)), (0xe2, s(0x38)), (0xe3, e(0x5b)),
    (0xe4, e(0x1d)), (0xe5, s(0x36)), (0xe6, e(0x38)), (0xe7, e(0x5c)),
];

/// Consumer usage (page 0x0C) to virtual key.
#[rustfmt::skip]
const MEDIA: &[(u16, u16)] = &[
    (0x0cd, 0xb3), // VK_MEDIA_PLAY_PAUSE
    (0x0b5, 0xb0), // VK_MEDIA_NEXT_TRACK
    (0x0b6, 0xb1), // VK_MEDIA_PREV_TRACK
    (0x0b7, 0xb2), // VK_MEDIA_STOP
    (0x0e2, 0xad), // VK_VOLUME_MUTE
    (0x0e9, 0xaf), // VK_VOLUME_UP
    (0x0ea, 0xae), // VK_VOLUME_DOWN
    (0x192, 0xb7), // VK_LAUNCH_APP2 (calculator)
    (0x223, 0xac), // VK_BROWSER_HOME
    (0x18a, 0xb4), // VK_LAUNCH_MAIL
    (0x221, 0xaa), // VK_BROWSER_SEARCH
    (0x22a, 0xab), // VK_BROWSER_FAVORITES
    (0x224, 0xa6), // VK_BROWSER_BACK
    (0x225, 0xa7), // VK_BROWSER_FORWARD
    (0x227, 0xa8), // VK_BROWSER_REFRESH
];

pub(crate) fn key(usage: u8) -> Option<WinKey> {
    KEYS.binary_search_by_key(&usage, |&(u, _)| u)
        .ok()
        .map(|i| KEYS[i].1)
}

pub(crate) fn media(usage: u16) -> Option<u16> {
    MEDIA.iter().find(|&&(u, _)| u == usage).map(|&(_, vk)| vk)
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
                "{name} (0x{usage:02x}) has no scancode"
            );
        }
    }

    #[test]
    fn scancodes_are_distinct_apart_from_the_iso_hash_key() {
        let mut seen = std::collections::HashMap::new();
        for &(usage, k) in KEYS {
            if let Some(prev) = seen.insert(k, usage) {
                assert!(
                    usage == 0x32 && prev == 0x31,
                    "0x{prev:02x} and 0x{usage:02x} share {k:?}"
                );
            }
        }
    }

    #[test]
    fn spot_checks_match_the_set_1_scancode_table() {
        assert_eq!(key(0x04), Some(s(0x1e))); // A
        assert_eq!(key(0x28), Some(s(0x1c))); // Enter
        assert_eq!(key(0x58), Some(e(0x1c))); // keypad Enter is Enter with E0
        assert_eq!(key(0x4a), Some(e(0x47))); // Home is keypad 7 with E0
        assert_eq!(key(0x5f), Some(s(0x47))); // keypad 7
        assert_eq!(key(0xe5), Some(s(0x36))); // Right Shift has no E0
        assert_eq!(key(0xe4), Some(e(0x1d))); // Right Ctrl
        assert_eq!(key(0x73), Some(s(0x76))); // F24
        assert_eq!(key(0x48), Some(WinKey::Vk(0x13))); // Pause
        assert_eq!(key(0x00), None);
    }

    #[test]
    fn every_pickable_media_usage_is_mapped() {
        for &(usage, name) in &gearwise_core::model::media::ALL {
            assert!(
                media(usage).is_some(),
                "{name} (0x{usage:03x}) has no virtual key"
            );
        }
        assert_eq!(media(gearwise_core::model::media::VOLUME_UP), Some(0xaf));
        assert_eq!(media(0xffff), None);
    }
}
