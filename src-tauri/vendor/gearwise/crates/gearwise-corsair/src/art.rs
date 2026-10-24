//! Physical layouts and zone-to-key maps for zone-lit keyboards, so the UI
//! can draw the real board and show which keys each zone lights.
//!
//! Key ids are HID usages; keys without one use ids from 0x100 (G-keys) and
//! 0x110 (media keys).

use gearwise_core::keys::{self, KeyCell, KeyboardLayout};

/// Macro keys G1–G6.
pub const G_KEYS: u16 = 0x100;
/// Stop, previous, play/pause, next.
pub const MEDIA_KEYS: u16 = 0x110;

pub(crate) struct Physical {
    pub layout: KeyboardLayout,
    /// (zone id, keys it lights).
    pub zones: Vec<(u8, Vec<u16>)>,
}

pub(crate) fn physical(pid: u16) -> Option<Physical> {
    match pid {
        0x1ba4 => Some(k55_rgb_pro()),
        _ => None,
    }
}

fn cell(id: u16, label: &str, x: f32, y: f32) -> KeyCell {
    KeyCell {
        id,
        label: label.into(),
        x,
        y,
        w: 1.0,
        h: 1.0,
    }
}

/// K55 RGB PRO: full-size ANSI board with a G1–G6 macro column on the left,
/// media keys above the numpad and Fn in place of the right Windows key.
/// Zones follow Corsair's product photos: five sweeps left to right.
fn k55_rgb_pro() -> Physical {
    const SHIFT: f32 = 1.5;
    let mut layout = keys::ansi_full();
    for k in &mut layout.keys {
        k.x += SHIFT;
        if k.id == 0xe7 {
            k.label = "Fn".into();
        }
    }
    let rows = [0.0, 1.25, 2.25, 3.25, 4.25, 5.25];
    for (i, y) in rows.iter().enumerate() {
        layout
            .keys
            .push(cell(G_KEYS + i as u16, &format!("G{}", i + 1), 0.0, *y));
    }
    for (i, label) in ["Stop", "Prev", "Play", "Next"].iter().enumerate() {
        layout.keys.push(cell(
            MEDIA_KEYS + i as u16,
            label,
            18.5 + SHIFT + i as f32,
            0.0,
        ));
    }
    layout.name = "K55 RGB PRO".into();

    let zone1: Vec<u16> = (0..6)
        .map(|i| G_KEYS + i)
        .chain([
            0x29, 0x3a, 0x3b, 0x35, 0x1e, 0x1f, 0x20, 0x2b, 0x14, 0x1a, 0x08, 0x39, 0x04, 0x16,
            0xe1, 0x1d, 0x1b, 0xe0, 0xe3, 0xe2,
        ])
        .collect();
    let zone2 = vec![
        0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x41, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x15, 0x17,
        0x1c, 0x18, 0x0c, 0x12, 0x07, 0x09, 0x0a, 0x0b, 0x0d, 0x0e, 0x0f, 0x06, 0x19, 0x05, 0x11,
        0x10, 0x2c,
    ];
    let zone3 = vec![
        0x42, 0x43, 0x44, 0x45, 0x2d, 0x2e, 0x2a, 0x13, 0x2f, 0x30, 0x31, 0x33, 0x34, 0x28, 0x36,
        0x37, 0x38, 0xe5, 0xe6, 0xe7, 0x65, 0xe4,
    ];
    let zone4 = (0x46..=0x52).collect();
    let zone5 = (0x53..=0x63)
        .chain((0..4).map(|i| MEDIA_KEYS + i))
        .collect();
    Physical {
        layout,
        zones: vec![(1, zone1), (2, zone2), (3, zone3), (4, zone4), (5, zone5)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn k55_zones_cover_every_key_exactly_once() {
        let p = physical(0x1ba4).unwrap();
        let all: HashSet<u16> = p.layout.keys.iter().map(|k| k.id).collect();
        assert_eq!(all.len(), p.layout.keys.len(), "unique key ids");
        assert_eq!(all.len(), 104 + 6 + 4);
        let mut seen = HashSet::new();
        for (_, keys) in &p.zones {
            for k in keys {
                assert!(all.contains(k), "zone key 0x{k:x} not on the board");
                assert!(seen.insert(*k), "key 0x{k:x} in two zones");
            }
        }
        assert_eq!(seen, all, "every key belongs to a zone");
    }

    #[test]
    fn k55_has_macro_column_and_fn() {
        let p = physical(0x1ba4).unwrap();
        let g1 = p.layout.keys.iter().find(|k| k.id == G_KEYS).unwrap();
        assert_eq!((g1.x, g1.label.as_str()), (0.0, "G1"));
        assert_eq!(
            p.layout.keys.iter().find(|k| k.id == 0xe7).unwrap().label,
            "Fn"
        );
        assert!(physical(0x1b9e).is_none());
    }
}
