//! HID keyboard usages (page 0x07): names and a full-size ANSI layout.
//!
//! Per-key lighting identifies keys by their HID usage everywhere above the
//! driver layer; each driver maps usages to its own LED indices.

use serde::{Deserialize, Serialize};

/// (usage, name) for every key offered in key pickers.
pub const KEYS: &[(u8, &str)] = &[
    (0x04, "A"),
    (0x05, "B"),
    (0x06, "C"),
    (0x07, "D"),
    (0x08, "E"),
    (0x09, "F"),
    (0x0a, "G"),
    (0x0b, "H"),
    (0x0c, "I"),
    (0x0d, "J"),
    (0x0e, "K"),
    (0x0f, "L"),
    (0x10, "M"),
    (0x11, "N"),
    (0x12, "O"),
    (0x13, "P"),
    (0x14, "Q"),
    (0x15, "R"),
    (0x16, "S"),
    (0x17, "T"),
    (0x18, "U"),
    (0x19, "V"),
    (0x1a, "W"),
    (0x1b, "X"),
    (0x1c, "Y"),
    (0x1d, "Z"),
    (0x1e, "1"),
    (0x1f, "2"),
    (0x20, "3"),
    (0x21, "4"),
    (0x22, "5"),
    (0x23, "6"),
    (0x24, "7"),
    (0x25, "8"),
    (0x26, "9"),
    (0x27, "0"),
    (0x28, "Enter"),
    (0x29, "Esc"),
    (0x2a, "Backspace"),
    (0x2b, "Tab"),
    (0x2c, "Space"),
    (0x2d, "-"),
    (0x2e, "="),
    (0x2f, "["),
    (0x30, "]"),
    (0x31, "\\"),
    (0x33, ";"),
    (0x34, "'"),
    (0x35, "`"),
    (0x36, ","),
    (0x37, "."),
    (0x38, "/"),
    (0x39, "Caps Lock"),
    (0x3a, "F1"),
    (0x3b, "F2"),
    (0x3c, "F3"),
    (0x3d, "F4"),
    (0x3e, "F5"),
    (0x3f, "F6"),
    (0x40, "F7"),
    (0x41, "F8"),
    (0x42, "F9"),
    (0x43, "F10"),
    (0x44, "F11"),
    (0x45, "F12"),
    (0x46, "Print Screen"),
    (0x47, "Scroll Lock"),
    (0x48, "Pause"),
    (0x49, "Insert"),
    (0x4a, "Home"),
    (0x4b, "Page Up"),
    (0x4c, "Delete"),
    (0x4d, "End"),
    (0x4e, "Page Down"),
    (0x4f, "Right"),
    (0x50, "Left"),
    (0x51, "Down"),
    (0x52, "Up"),
    (0x53, "Num Lock"),
    (0x54, "KP /"),
    (0x55, "KP *"),
    (0x56, "KP -"),
    (0x57, "KP +"),
    (0x58, "KP Enter"),
    (0x59, "KP 1"),
    (0x5a, "KP 2"),
    (0x5b, "KP 3"),
    (0x5c, "KP 4"),
    (0x5d, "KP 5"),
    (0x5e, "KP 6"),
    (0x5f, "KP 7"),
    (0x60, "KP 8"),
    (0x61, "KP 9"),
    (0x62, "KP 0"),
    (0x63, "KP ."),
    (0x65, "Menu"),
    (0x68, "F13"),
    (0x69, "F14"),
    (0x6a, "F15"),
    (0x6b, "F16"),
    (0x6c, "F17"),
    (0x6d, "F18"),
    (0x6e, "F19"),
    (0x6f, "F20"),
    (0x70, "F21"),
    (0x71, "F22"),
    (0x72, "F23"),
    (0x73, "F24"),
    (0xe0, "Left Ctrl"),
    (0xe1, "Left Shift"),
    (0xe2, "Left Alt"),
    (0xe3, "Left Super"),
    (0xe4, "Right Ctrl"),
    (0xe5, "Right Shift"),
    (0xe6, "Right Alt"),
    (0xe7, "Right Super"),
];

pub fn name(usage: u8) -> Option<&'static str> {
    KEYS.iter().find(|(u, _)| *u == usage).map(|(_, n)| *n)
}

/// One key cap on a layout, in key units (1.0 = one standard key).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyCell {
    /// HID usage, or a driver-defined id >= 0x100 for keys without one
    /// (G-keys, logo LEDs, media keys).
    pub id: u16,
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyboardLayout {
    pub name: String,
    pub keys: Vec<KeyCell>,
}

impl KeyboardLayout {
    pub fn width(&self) -> f32 {
        self.keys.iter().map(|k| k.x + k.w).fold(0.0, f32::max)
    }
    pub fn height(&self) -> f32 {
        self.keys.iter().map(|k| k.y + k.h).fold(0.0, f32::max)
    }
    pub fn contains(&self, id: u16) -> bool {
        self.keys.iter().any(|k| k.id == id)
    }
}

struct Row {
    y: f32,
    x: f32,
    keys: Vec<(u8, &'static str, f32)>,
}

impl Row {
    fn new(y: f32, x: f32) -> Self {
        Row {
            y,
            x,
            keys: Vec::new(),
        }
    }
    fn k(mut self, usage: u8, label: &'static str, w: f32) -> Self {
        self.keys.push((usage, label, w));
        self
    }
    fn place(self, out: &mut Vec<KeyCell>) {
        let mut x = self.x;
        for (usage, label, w) in self.keys {
            out.push(KeyCell {
                id: usage as u16,
                label: label.into(),
                x,
                y: self.y,
                w,
                h: 1.0,
            });
            x += w;
        }
    }
}

fn cell(out: &mut Vec<KeyCell>, usage: u8, label: &str, x: f32, y: f32, w: f32, h: f32) {
    out.push(KeyCell {
        id: usage as u16,
        label: label.into(),
        x,
        y,
        w,
        h,
    });
}

/// Standard 104-key ANSI layout (full size with numpad).
pub fn ansi_full() -> KeyboardLayout {
    let mut k = Vec::with_capacity(104);

    // Function row
    cell(&mut k, 0x29, "Esc", 0.0, 0.0, 1.0, 1.0);
    for (i, u) in (0x3a..=0x3du8).enumerate() {
        cell(
            &mut k,
            u,
            ["F1", "F2", "F3", "F4"][i],
            2.0 + i as f32,
            0.0,
            1.0,
            1.0,
        );
    }
    for (i, u) in (0x3e..=0x41u8).enumerate() {
        cell(
            &mut k,
            u,
            ["F5", "F6", "F7", "F8"][i],
            6.5 + i as f32,
            0.0,
            1.0,
            1.0,
        );
    }
    for (i, u) in (0x42..=0x45u8).enumerate() {
        cell(
            &mut k,
            u,
            ["F9", "F10", "F11", "F12"][i],
            11.0 + i as f32,
            0.0,
            1.0,
            1.0,
        );
    }
    cell(&mut k, 0x46, "PrtSc", 15.25, 0.0, 1.0, 1.0);
    cell(&mut k, 0x47, "ScrLk", 16.25, 0.0, 1.0, 1.0);
    cell(&mut k, 0x48, "Pause", 17.25, 0.0, 1.0, 1.0);

    // Number row
    Row::new(1.25, 0.0)
        .k(0x35, "`", 1.0)
        .k(0x1e, "1", 1.0)
        .k(0x1f, "2", 1.0)
        .k(0x20, "3", 1.0)
        .k(0x21, "4", 1.0)
        .k(0x22, "5", 1.0)
        .k(0x23, "6", 1.0)
        .k(0x24, "7", 1.0)
        .k(0x25, "8", 1.0)
        .k(0x26, "9", 1.0)
        .k(0x27, "0", 1.0)
        .k(0x2d, "-", 1.0)
        .k(0x2e, "=", 1.0)
        .k(0x2a, "Backspace", 2.0)
        .place(&mut k);
    Row::new(1.25, 15.25)
        .k(0x49, "Ins", 1.0)
        .k(0x4a, "Home", 1.0)
        .k(0x4b, "PgUp", 1.0)
        .place(&mut k);
    Row::new(1.25, 18.5)
        .k(0x53, "Num", 1.0)
        .k(0x54, "/", 1.0)
        .k(0x55, "*", 1.0)
        .k(0x56, "-", 1.0)
        .place(&mut k);

    // Top letter row
    Row::new(2.25, 0.0)
        .k(0x2b, "Tab", 1.5)
        .k(0x14, "Q", 1.0)
        .k(0x1a, "W", 1.0)
        .k(0x08, "E", 1.0)
        .k(0x15, "R", 1.0)
        .k(0x17, "T", 1.0)
        .k(0x1c, "Y", 1.0)
        .k(0x18, "U", 1.0)
        .k(0x0c, "I", 1.0)
        .k(0x12, "O", 1.0)
        .k(0x13, "P", 1.0)
        .k(0x2f, "[", 1.0)
        .k(0x30, "]", 1.0)
        .k(0x31, "\\", 1.5)
        .place(&mut k);
    Row::new(2.25, 15.25)
        .k(0x4c, "Del", 1.0)
        .k(0x4d, "End", 1.0)
        .k(0x4e, "PgDn", 1.0)
        .place(&mut k);
    Row::new(2.25, 18.5)
        .k(0x5f, "7", 1.0)
        .k(0x60, "8", 1.0)
        .k(0x61, "9", 1.0)
        .place(&mut k);
    cell(&mut k, 0x57, "+", 21.5, 2.25, 1.0, 2.0);

    // Home row
    Row::new(3.25, 0.0)
        .k(0x39, "Caps", 1.75)
        .k(0x04, "A", 1.0)
        .k(0x16, "S", 1.0)
        .k(0x07, "D", 1.0)
        .k(0x09, "F", 1.0)
        .k(0x0a, "G", 1.0)
        .k(0x0b, "H", 1.0)
        .k(0x0d, "J", 1.0)
        .k(0x0e, "K", 1.0)
        .k(0x0f, "L", 1.0)
        .k(0x33, ";", 1.0)
        .k(0x34, "'", 1.0)
        .k(0x28, "Enter", 2.25)
        .place(&mut k);
    Row::new(3.25, 18.5)
        .k(0x5c, "4", 1.0)
        .k(0x5d, "5", 1.0)
        .k(0x5e, "6", 1.0)
        .place(&mut k);

    // Bottom letter row
    Row::new(4.25, 0.0)
        .k(0xe1, "Shift", 2.25)
        .k(0x1d, "Z", 1.0)
        .k(0x1b, "X", 1.0)
        .k(0x06, "C", 1.0)
        .k(0x19, "V", 1.0)
        .k(0x05, "B", 1.0)
        .k(0x11, "N", 1.0)
        .k(0x10, "M", 1.0)
        .k(0x36, ",", 1.0)
        .k(0x37, ".", 1.0)
        .k(0x38, "/", 1.0)
        .k(0xe5, "Shift", 2.75)
        .place(&mut k);
    cell(&mut k, 0x52, "↑", 16.25, 4.25, 1.0, 1.0);
    Row::new(4.25, 18.5)
        .k(0x59, "1", 1.0)
        .k(0x5a, "2", 1.0)
        .k(0x5b, "3", 1.0)
        .place(&mut k);
    cell(&mut k, 0x58, "Ent", 21.5, 4.25, 1.0, 2.0);

    // Space row
    Row::new(5.25, 0.0)
        .k(0xe0, "Ctrl", 1.25)
        .k(0xe3, "Super", 1.25)
        .k(0xe2, "Alt", 1.25)
        .k(0x2c, "", 6.25)
        .k(0xe6, "Alt", 1.25)
        .k(0xe7, "Super", 1.25)
        .k(0x65, "Menu", 1.25)
        .k(0xe4, "Ctrl", 1.25)
        .place(&mut k);
    Row::new(5.25, 15.25)
        .k(0x50, "←", 1.0)
        .k(0x51, "↓", 1.0)
        .k(0x4f, "→", 1.0)
        .place(&mut k);
    cell(&mut k, 0x62, "0", 18.5, 5.25, 2.0, 1.0);
    cell(&mut k, 0x63, ".", 20.5, 5.25, 1.0, 1.0);

    KeyboardLayout {
        name: "ANSI full size".into(),
        keys: k,
    }
}

/// ANSI tenkeyless: the full layout without the numpad.
pub fn ansi_tkl() -> KeyboardLayout {
    let mut l = ansi_full();
    l.keys.retain(|c| c.x < 18.0);
    l.name = "ANSI tenkeyless".into();
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_layout_has_104_unique_keys() {
        let l = ansi_full();
        assert_eq!(l.keys.len(), 104);
        let mut ids: Vec<u16> = l.keys.iter().map(|k| k.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 104);
        assert!((l.width() - 22.5).abs() < 1e-3);
    }

    #[test]
    fn tkl_has_87_keys() {
        assert_eq!(ansi_tkl().keys.len(), 87);
    }

    #[test]
    fn every_layout_key_is_named() {
        for k in ansi_full().keys {
            assert!(name(k.id as u8).is_some(), "no name for 0x{:02x}", k.id);
        }
    }
}
