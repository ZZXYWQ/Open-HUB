//! Per-key lighting: mapping HID usages onto Razer's LED matrices.
//!
//! Razer keyboards expose their keys as a rows x columns grid; the position
//! of a key depends on the model. All matrices seen so far are the same ANSI
//! grid with a number of extra columns inserted on the left (macro keys,
//! dials, unused slots) and a few keys relocated, so each model is described
//! as that base grid plus an offset and overrides. The base grid and the
//! per-model adjustments follow OpenRGB's hardware-tested Razer layouts.

use std::collections::BTreeMap;

use gearwise_core::keys::{self, KeyboardLayout};
use gearwise_core::model::Rgb;

use crate::commands;
use crate::report::Report;

/// (HID usage, column) of every key in each row of the base grid: a
/// 6 x 21 ANSI full-size board with the numpad at columns 17-20.
const BASE: [&[(u8, u8)]; 6] = [
    // Esc, F1-F12, PrtSc, ScrLk, Pause
    &[
        (0x29, 0),
        (0x3a, 2),
        (0x3b, 3),
        (0x3c, 4),
        (0x3d, 5),
        (0x3e, 6),
        (0x3f, 7),
        (0x40, 8),
        (0x41, 9),
        (0x42, 10),
        (0x43, 11),
        (0x44, 12),
        (0x45, 13),
        (0x46, 14),
        (0x47, 15),
        (0x48, 16),
    ],
    // ` 1-0 - = Backspace, Ins Home PgUp, Num / * -
    &[
        (0x35, 0),
        (0x1e, 1),
        (0x1f, 2),
        (0x20, 3),
        (0x21, 4),
        (0x22, 5),
        (0x23, 6),
        (0x24, 7),
        (0x25, 8),
        (0x26, 9),
        (0x27, 10),
        (0x2d, 11),
        (0x2e, 12),
        (0x2a, 13),
        (0x49, 14),
        (0x4a, 15),
        (0x4b, 16),
        (0x53, 17),
        (0x54, 18),
        (0x55, 19),
        (0x56, 20),
    ],
    // Tab Q-P [ ] \, Del End PgDn, 7 8 9 +
    &[
        (0x2b, 0),
        (0x14, 1),
        (0x1a, 2),
        (0x08, 3),
        (0x15, 4),
        (0x17, 5),
        (0x1c, 6),
        (0x18, 7),
        (0x0c, 8),
        (0x12, 9),
        (0x13, 10),
        (0x2f, 11),
        (0x30, 12),
        (0x31, 13),
        (0x4c, 14),
        (0x4d, 15),
        (0x4e, 16),
        (0x5f, 17),
        (0x60, 18),
        (0x61, 19),
        (0x57, 20),
    ],
    // Caps A-L ; ' Enter, 4 5 6
    &[
        (0x39, 0),
        (0x04, 1),
        (0x16, 2),
        (0x07, 3),
        (0x09, 4),
        (0x0a, 5),
        (0x0b, 6),
        (0x0d, 7),
        (0x0e, 8),
        (0x0f, 9),
        (0x33, 10),
        (0x34, 11),
        (0x28, 13),
        (0x5c, 17),
        (0x5d, 18),
        (0x5e, 19),
    ],
    // LShift Z-M , . / RShift, Up, 1 2 3 Enter
    &[
        (0xe1, 0),
        (0x1d, 2),
        (0x1b, 3),
        (0x06, 4),
        (0x19, 5),
        (0x05, 6),
        (0x11, 7),
        (0x10, 8),
        (0x36, 9),
        (0x37, 10),
        (0x38, 11),
        (0xe5, 13),
        (0x52, 15),
        (0x59, 17),
        (0x5a, 18),
        (0x5b, 19),
        (0x58, 20),
    ],
    // LCtrl LWin LAlt Space RAlt Fn Menu RCtrl, arrows, 0 .
    // The key where the core layout draws "Right Super" is Razer's Fn key.
    &[
        (0xe0, 0),
        (0xe3, 1),
        (0xe2, 2),
        (0x2c, 6),
        (0xe6, 10),
        (0xe7, 11),
        (0x65, 12),
        (0xe4, 13),
        (0x50, 14),
        (0x51, 15),
        (0x4f, 16),
        (0x62, 18),
        (0x63, 19),
    ],
];

/// Last column of the tenkeyless block in the base grid.
const TKL_LAST_COL: u8 = 16;

/// The part of a keyboard's LED matrix this crate drives.
#[derive(Clone, Copy, Debug)]
pub struct KeyMatrix {
    /// Rows written to the device. Boards with underglow rows below the keys
    /// (Huntsman Elite, BlackWidow V4) leave those untouched.
    pub rows: u8,
    /// Columns per row, including the inserted ones.
    pub cols: u8,
    /// Columns inserted on the left of every row relative to the base grid.
    pub col_offset: u8,
    /// Tenkeyless board: no numpad columns.
    pub tkl: bool,
    /// (HID usage, row, column) of keys that sit elsewhere than
    /// `base + col_offset`.
    pub overrides: &'static [(u8, u8, u8)],
}

impl KeyMatrix {
    /// The key layout the UI should draw for this board.
    pub fn layout(&self) -> KeyboardLayout {
        if self.tkl {
            keys::ansi_tkl()
        } else {
            keys::ansi_full()
        }
    }

    /// Matrix position of the key with HID usage `usage`, if the board has it.
    pub fn cell(&self, usage: u8) -> Option<(u8, u8)> {
        if let Some(&(_, r, c)) = self.overrides.iter().find(|(u, _, _)| *u == usage) {
            return Some((r, c));
        }
        BASE.iter().enumerate().find_map(|(row, keys)| {
            let (_, col) = keys.iter().find(|(u, _)| *u == usage)?;
            (!self.tkl || *col <= TKL_LAST_COL).then_some((row as u8, col + self.col_offset))
        })
    }

    /// Row reports (and the final draw command) that make the device show
    /// `colors`, keyed by key id. Keys without a colour are black.
    pub fn frame(&self, colors: &BTreeMap<u16, Rgb>) -> Vec<Report> {
        let mut grid = vec![vec![Rgb::BLACK; self.cols as usize]; self.rows as usize];
        for (&id, &rgb) in colors {
            let cell = u8::try_from(id).ok().and_then(|usage| self.cell(usage));
            if let Some((r, c)) = cell.filter(|&(r, c)| r < self.rows && c < self.cols) {
                grid[r as usize][c as usize] = rgb;
            }
        }
        let mut out: Vec<Report> = grid
            .iter()
            .enumerate()
            .map(|(r, row)| commands::frame_row(r as u8, row))
            .collect();
        out.push(commands::draw_frame());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const FULL: KeyMatrix = KeyMatrix {
        rows: 6,
        cols: 22,
        col_offset: 1,
        tkl: false,
        overrides: &[],
    };

    #[test]
    fn base_grid_covers_the_core_layouts_exactly() {
        let mut usages: Vec<u8> = BASE
            .iter()
            .flat_map(|r| r.iter().map(|(u, _)| *u))
            .collect();
        usages.sort();
        let mut want: Vec<u8> = keys::ansi_full().keys.iter().map(|k| k.id as u8).collect();
        want.sort();
        assert_eq!(usages, want);
    }

    #[test]
    fn base_grid_cells_are_unique_and_in_range() {
        let mut seen = HashSet::new();
        for (r, row) in BASE.iter().enumerate() {
            for &(_, c) in *row {
                assert!(c <= 20);
                assert!(seen.insert((r, c)), "duplicate cell {r},{c}");
            }
        }
    }

    #[test]
    fn matches_the_documented_blackwidow_chroma_grid() {
        // openrazer documents the 6 x 22 grid with a macro column at 0:
        // Esc at (0,1), F1 at (0,3), Return at (3,14), "/" at (4,12), up
        // arrow at (4,16), right arrow at (5,17), numpad 0 and "." at (5,19-20).
        let cell = |u| FULL.cell(u).unwrap();
        assert_eq!(cell(0x29), (0, 1));
        assert_eq!(cell(0x3a), (0, 3));
        assert_eq!(cell(0x28), (3, 14));
        assert_eq!(cell(0x38), (4, 12));
        assert_eq!(cell(0x52), (4, 16));
        assert_eq!(cell(0x4f), (5, 17));
        assert_eq!(cell(0x62), (5, 19));
        assert_eq!(cell(0x63), (5, 20));
        assert_eq!(cell(0x56), (1, 21));
    }

    #[test]
    fn overrides_win_over_the_offset_grid() {
        let m = KeyMatrix {
            overrides: &[(0x2c, 5, 6)],
            ..FULL
        };
        assert_eq!(m.cell(0x2c), Some((5, 6)));
        assert_eq!(FULL.cell(0x2c), Some((5, 7)));
    }

    #[test]
    fn tenkeyless_drops_the_numpad() {
        let tkl = KeyMatrix {
            rows: 6,
            cols: 18,
            col_offset: 1,
            tkl: true,
            overrides: &[],
        };
        assert_eq!(tkl.cell(0x5f), None);
        assert_eq!(tkl.cell(0x4f), Some((5, 17)));
        assert_eq!(tkl.layout().keys.len(), 87);
    }

    #[test]
    fn frame_has_one_row_per_matrix_row_then_draw() {
        let mut colors = BTreeMap::new();
        colors.insert(0x29u16, Rgb::new(9, 8, 7)); // Esc
        colors.insert(0x1ffu16, Rgb::new(1, 1, 1)); // not a HID usage: ignored
        let f = FULL.frame(&colors);
        assert_eq!(f.len(), 7);
        for (i, r) in f[..6].iter().enumerate() {
            assert_eq!((r.class, r.command, r.data_size), (0x0f, 0x03, 0x47));
            assert_eq!((r.args[2], r.args[3], r.args[4]), (i as u8, 0, 21));
        }
        // Esc is at column 1 of row 0.
        assert_eq!(&f[0].args[5..11], &[0, 0, 0, 9, 8, 7]);
        assert!(f[1].args[5..5 + 66].iter().all(|&b| b == 0));
        assert_eq!((f[6].class, f[6].command, f[6].args[2]), (0x0f, 0x02, 0x08));
    }
}
