//! Static table of the Razer devices this crate drives.
//!
//! Product ids, transaction ids, polling families, DPI ceilings and lighting
//! LEDs come from the openrazer driver and daemon; HID interface numbers and
//! usages (needed to pick the one control node per device when using hidapi)
//! and keyboard matrices come from OpenRGB. Entries whose data was not
//! confirmed by at least one of them are not listed.

use gearwise_core::model::{Connection, DeviceKind, EffectKind, VID_RAZER};
use gearwise_core::transport::HidInterfaceInfo;

use crate::commands::{PollProto, Store, WaveCodes, led};
use crate::keymap::KeyMatrix;
use crate::link::Settle;

/// Which HID interface carries the 90-byte feature reports.
///
/// A physical device exposes several interfaces (boot keyboard, mouse,
/// consumer keys, vendor); only one answers Razer commands, and on Windows
/// hidapi lists each top-level collection separately.
#[derive(Clone, Copy, Debug)]
pub struct Control {
    /// USB interface number.
    pub interface: i32,
    /// (usage page, usage) pairs the node may report. Backends that cannot
    /// report usages (0, 0) are matched on the interface number alone.
    pub usages: &'static [(u16, u16)],
}

impl Control {
    fn matches(&self, i: &HidInterfaceInfo) -> bool {
        // A node without a USB interface number (-1) never matches: there is
        // no safe way to tell the interfaces of one device apart.
        if i.interface_number != self.interface {
            return false;
        }
        let unknown_usage = i.usage_page == 0 && i.usage == 0;
        unknown_usage || self.usages.contains(&(i.usage_page, i.usage))
    }
}

/// DPI hardware limits of a mouse.
#[derive(Clone, Copy, Debug)]
pub struct DpiSpec {
    /// Highest DPI the sensor supports.
    pub max: u16,
    /// Whether the device has an on-board stage table (up to five stages).
    pub stages: bool,
}

/// Polling-rate support.
#[derive(Clone, Copy, Debug)]
pub struct Polling {
    pub proto: PollProto,
    /// Selectable rates in Hz, ascending; empty if the rate is not settable.
    pub rates: &'static [u16],
}

/// A lighting zone, addressed by the LED id used on the wire.
#[derive(Clone, Copy, Debug)]
pub struct Zone {
    pub led: u8,
    pub name: &'static str,
    pub effects: &'static [EffectKind],
}

/// Everything the driver needs to know about one product id.
#[derive(Clone, Copy, Debug)]
pub struct Model {
    pub pid: u16,
    pub name: &'static str,
    pub kind: DeviceKind,
    pub connection: Connection,
    /// Transaction id stamped on every request. Firmware echoes it;
    /// openrazer itself sends a few commands to the same device with another
    /// id, so it is not enforced, but the id it uses for the device's main
    /// commands is kept.
    pub transaction_id: u8,
    pub control: Control,
    pub settle: Settle,
    pub dpi: Option<DpiSpec>,
    pub polling: Polling,
    pub battery: bool,
    pub zones: &'static [Zone],
    pub wave: WaveCodes,
    pub matrix: Option<KeyMatrix>,
    /// Game-mode support and how the device wants the flag stored.
    pub game_mode: Option<Store>,
}

/// The model for a product id.
pub fn find(pid: u16) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.pid == pid)
}

/// The model whose control interface `iface` is, if any. Exactly one
/// interface of a physical device qualifies.
pub fn for_interface(iface: &HidInterfaceInfo) -> Option<&'static Model> {
    if iface.vendor_id != VID_RAZER {
        return None;
    }
    find(iface.product_id).filter(|m| m.control.matches(iface))
}

// ---------------------------------------------------------------------------
// Shared building blocks
// ---------------------------------------------------------------------------

use EffectKind::*;

/// Logo/scroll LEDs of older mice: everything but wave.
const FX_CLASSIC: &[EffectKind] = &[Off, Static, Breathing, ColorCycle, Reactive];
/// Basilisk V3 Pro / Cobra Pro class: wave but no breathing or reactive.
const FX_WAVE: &[EffectKind] = &[Off, Static, ColorCycle, Wave];
/// Basilisk V3: like `FX_WAVE` without an "off" effect (use brightness 0).
const FX_WAVE_NO_OFF: &[EffectKind] = &[Static, ColorCycle, Wave];
const FX_KEYBOARD: &[EffectKind] = &[Off, Static, Breathing, ColorCycle, Wave, Reactive];
const FX_KEYBOARD_NO_REACTIVE: &[EffectKind] = &[Off, Static, Breathing, ColorCycle, Wave];

const LOGO: Zone = Zone {
    led: led::LOGO,
    name: "Logo",
    effects: FX_CLASSIC,
};
const SCROLL: Zone = Zone {
    led: led::SCROLL_WHEEL,
    name: "Scroll wheel",
    effects: FX_CLASSIC,
};
const WAVE_LOGO: Zone = Zone {
    led: led::LOGO,
    name: "Logo",
    effects: FX_WAVE,
};
const WAVE_SCROLL: Zone = Zone {
    led: led::SCROLL_WHEEL,
    name: "Scroll wheel",
    effects: FX_WAVE,
};
const KEYS: Zone = Zone {
    led: led::BACKLIGHT,
    name: "Keys",
    effects: FX_KEYBOARD,
};

const ZONES_LOGO: &[Zone] = &[LOGO];
const ZONES_LOGO_SCROLL: &[Zone] = &[LOGO, SCROLL];
const ZONES_WAVE_LOGO_SCROLL: &[Zone] = &[WAVE_LOGO, WAVE_SCROLL];
const ZONES_BASILISK_V3: &[Zone] = &[
    Zone {
        led: led::LOGO,
        name: "Logo",
        effects: FX_WAVE_NO_OFF,
    },
    Zone {
        led: led::SCROLL_WHEEL,
        name: "Scroll wheel",
        effects: FX_WAVE_NO_OFF,
    },
];
const ZONES_KEYS: &[Zone] = &[KEYS];
const ZONES_ORNATA: &[Zone] = &[Zone {
    led: led::BACKLIGHT,
    name: "Underglow",
    effects: FX_KEYBOARD_NO_REACTIVE,
}];

const POLL_LEGACY: Polling = Polling {
    proto: PollProto::Legacy,
    rates: &[125, 500, 1000],
};
const POLL_8K_MOUSE: Polling = Polling {
    proto: PollProto::Hyper { follow_up: true },
    rates: &[125, 500, 1000, 2000, 4000, 8000],
};
const POLL_8K_KEYBOARD: Polling = Polling {
    proto: PollProto::Hyper { follow_up: false },
    rates: &[125, 250, 500, 1000, 2000, 4000, 8000],
};
const POLL_NONE: Polling = Polling {
    proto: PollProto::Legacy,
    rates: &[],
};

const fn staged(max: u16) -> Option<DpiSpec> {
    Some(DpiSpec { max, stages: true })
}

const fn single(max: u16) -> Option<DpiSpec> {
    Some(DpiSpec { max, stages: false })
}

/// Mice put their control reports on the mouse interface 0 (generic desktop
/// "mouse" usage).
const MOUSE_CONTROL: Control = Control {
    interface: 0,
    usages: &[(0x01, 0x02)],
};
/// The Basilisk V3 moved them to the consumer-control interface 3.
const BASILISK_V3_CONTROL: Control = Control {
    interface: 3,
    usages: &[(0x0c, 0x01)],
};
/// Most keyboards use interface 2 (mouse usage)...
const KEYBOARD_CONTROL: Control = Control {
    interface: 2,
    usages: &[(0x01, 0x02)],
};
/// ...newer ones interface 3 (consumer control).
const KEYBOARD_CONTROL_3: Control = Control {
    interface: 3,
    usages: &[(0x0c, 0x01)],
};
/// BlackWidow V4 firmware before 1.5 reports a different usage on interface 3.
const KEYBOARD_CONTROL_3_ANY_FW: Control = Control {
    interface: 3,
    usages: &[(0x01, 0x00), (0x0c, 0x01)],
};

/// Defaults for mice; entries override what differs.
const MOUSE: Model = Model {
    pid: 0,
    name: "",
    kind: DeviceKind::Mouse,
    connection: Connection::Wired,
    transaction_id: 0x1f,
    control: MOUSE_CONTROL,
    settle: Settle::Fast,
    dpi: None,
    polling: POLL_LEGACY,
    battery: false,
    zones: &[],
    wave: WaveCodes::STANDARD,
    matrix: None,
    game_mode: None,
};

const MOUSE_WIRELESS: Model = Model {
    connection: Connection::Wireless,
    battery: true,
    ..MOUSE
};

/// Defaults for keyboards.
const KEYBOARD: Model = Model {
    kind: DeviceKind::Keyboard,
    transaction_id: 0x3f,
    control: KEYBOARD_CONTROL,
    polling: POLL_NONE,
    zones: ZONES_KEYS,
    game_mode: Some(Store::Var),
    ..MOUSE
};

// Matrices. The key rows are the physical rows; boards with underglow rows
// below (V4, Huntsman Elite) are driven for their key rows only.
const MATRIX_FULL: KeyMatrix = KeyMatrix {
    rows: 6,
    cols: 22,
    col_offset: 1,
    tkl: false,
    overrides: &[],
};
/// BlackWidow V3: Space and Right Alt sit one column left of the shifted grid.
const MATRIX_BLACKWIDOW_V3: KeyMatrix = KeyMatrix {
    overrides: &[(0x2c, 5, 6), (0xe6, 5, 10)],
    ..MATRIX_FULL
};
const MATRIX_BLACKWIDOW_V3_TKL: KeyMatrix = KeyMatrix {
    cols: 18,
    tkl: true,
    ..MATRIX_FULL
};
/// V3 Pro and Huntsman V2 start at column 0 (media keys follow the nav block).
const MATRIX_UNSHIFTED: KeyMatrix = KeyMatrix {
    col_offset: 0,
    ..MATRIX_FULL
};
/// BlackWidow V4 / V4 Pro: a spare column and a macro/dial column on the left.
const MATRIX_BLACKWIDOW_V4: KeyMatrix = KeyMatrix {
    cols: 23,
    col_offset: 2,
    ..MATRIX_FULL
};

// ---------------------------------------------------------------------------
// The table
// ---------------------------------------------------------------------------

/// Every supported product id.
pub static MODELS: &[Model] = &[
    // ---- DeathAdder ----
    Model {
        pid: 0x0084,
        name: "Razer DeathAdder V2",
        transaction_id: 0x3f,
        dpi: single(20_000),
        zones: ZONES_LOGO_SCROLL,
        ..MOUSE
    },
    Model {
        pid: 0x007c,
        name: "Razer DeathAdder V2 Pro (Wired)",
        battery: true,
        transaction_id: 0x3f,
        settle: Settle::Slow,
        dpi: staged(20_000),
        zones: ZONES_LOGO,
        ..MOUSE
    },
    Model {
        pid: 0x007d,
        name: "Razer DeathAdder V2 Pro (Wireless)",
        transaction_id: 0x3f,
        settle: Settle::Slow,
        dpi: staged(20_000),
        zones: ZONES_LOGO,
        ..MOUSE_WIRELESS
    },
    Model {
        pid: 0x00b2,
        name: "Razer DeathAdder V3",
        dpi: staged(30_000),
        polling: POLL_8K_MOUSE,
        ..MOUSE
    },
    Model {
        pid: 0x00b6,
        name: "Razer DeathAdder V3 Pro (Wired)",
        battery: true,
        settle: Settle::Medium,
        dpi: staged(35_000),
        ..MOUSE
    },
    Model {
        pid: 0x00b7,
        name: "Razer DeathAdder V3 Pro (Wireless)",
        settle: Settle::Medium,
        dpi: staged(35_000),
        ..MOUSE_WIRELESS
    },
    // ---- Viper ----
    Model {
        pid: 0x007a,
        name: "Razer Viper Ultimate (Wired)",
        battery: true,
        transaction_id: 0x3f,
        settle: Settle::Slow,
        dpi: staged(20_000),
        zones: ZONES_LOGO,
        ..MOUSE
    },
    Model {
        pid: 0x007b,
        name: "Razer Viper Ultimate (Wireless)",
        transaction_id: 0x3f,
        settle: Settle::Slow,
        dpi: staged(20_000),
        zones: ZONES_LOGO,
        ..MOUSE_WIRELESS
    },
    Model {
        pid: 0x00a5,
        name: "Razer Viper V2 Pro (Wired)",
        battery: true,
        settle: Settle::Medium,
        dpi: staged(30_000),
        ..MOUSE
    },
    Model {
        pid: 0x00a6,
        name: "Razer Viper V2 Pro (Wireless)",
        settle: Settle::Medium,
        dpi: staged(30_000),
        ..MOUSE_WIRELESS
    },
    Model {
        pid: 0x00c0,
        name: "Razer Viper V3 Pro (Wired)",
        battery: true,
        settle: Settle::Medium,
        dpi: staged(35_000),
        ..MOUSE
    },
    Model {
        pid: 0x00c1,
        name: "Razer Viper V3 Pro (Wireless)",
        settle: Settle::Slow,
        dpi: staged(35_000),
        polling: POLL_8K_MOUSE,
        ..MOUSE_WIRELESS
    },
    // ---- Basilisk ----
    Model {
        pid: 0x0099,
        name: "Razer Basilisk V3",
        control: BASILISK_V3_CONTROL,
        dpi: staged(26_000),
        zones: ZONES_BASILISK_V3,
        ..MOUSE
    },
    Model {
        pid: 0x00aa,
        name: "Razer Basilisk V3 Pro (Wired)",
        battery: true,
        settle: Settle::Medium,
        dpi: staged(30_000),
        zones: ZONES_WAVE_LOGO_SCROLL,
        ..MOUSE
    },
    Model {
        pid: 0x00ab,
        name: "Razer Basilisk V3 Pro (Wireless)",
        settle: Settle::Medium,
        dpi: staged(30_000),
        zones: ZONES_WAVE_LOGO_SCROLL,
        ..MOUSE_WIRELESS
    },
    // ---- Naga ----
    Model {
        pid: 0x008f,
        name: "Razer Naga Pro (Wired)",
        battery: true,
        dpi: staged(20_000),
        ..MOUSE
    },
    Model {
        pid: 0x0090,
        name: "Razer Naga Pro (Wireless)",
        settle: Settle::Medium,
        dpi: staged(20_000),
        ..MOUSE_WIRELESS
    },
    Model {
        pid: 0x00a7,
        name: "Razer Naga V2 Pro (Wired)",
        battery: true,
        settle: Settle::Medium,
        dpi: staged(30_000),
        ..MOUSE
    },
    Model {
        pid: 0x00a8,
        name: "Razer Naga V2 Pro (Wireless)",
        settle: Settle::Medium,
        dpi: staged(30_000),
        ..MOUSE_WIRELESS
    },
    // ---- Cobra ----
    Model {
        pid: 0x00a3,
        name: "Razer Cobra",
        dpi: staged(8_500),
        zones: ZONES_LOGO,
        ..MOUSE
    },
    Model {
        pid: 0x00af,
        name: "Razer Cobra Pro (Wired)",
        battery: true,
        settle: Settle::Medium,
        dpi: staged(30_000),
        zones: ZONES_WAVE_LOGO_SCROLL,
        ..MOUSE
    },
    Model {
        pid: 0x00b0,
        name: "Razer Cobra Pro (Wireless)",
        settle: Settle::Medium,
        dpi: staged(30_000),
        zones: ZONES_WAVE_LOGO_SCROLL,
        ..MOUSE_WIRELESS
    },
    // ---- BlackWidow ----
    Model {
        pid: 0x024e,
        name: "Razer BlackWidow V3",
        control: KEYBOARD_CONTROL_3,
        matrix: Some(MATRIX_BLACKWIDOW_V3),
        ..KEYBOARD
    },
    Model {
        pid: 0x0a24,
        name: "Razer BlackWidow V3 Tenkeyless",
        matrix: Some(MATRIX_BLACKWIDOW_V3_TKL),
        ..KEYBOARD
    },
    Model {
        pid: 0x025a,
        name: "Razer BlackWidow V3 Pro (Wired)",
        battery: true,
        matrix: Some(MATRIX_UNSHIFTED),
        ..KEYBOARD
    },
    Model {
        pid: 0x0287,
        name: "Razer BlackWidow V4",
        transaction_id: 0x1f,
        control: KEYBOARD_CONTROL_3_ANY_FW,
        polling: POLL_8K_KEYBOARD,
        matrix: Some(MATRIX_BLACKWIDOW_V4),
        ..KEYBOARD
    },
    Model {
        pid: 0x028d,
        name: "Razer BlackWidow V4 Pro",
        transaction_id: 0x1f,
        control: KEYBOARD_CONTROL_3_ANY_FW,
        polling: POLL_8K_KEYBOARD,
        matrix: Some(MATRIX_BLACKWIDOW_V4),
        ..KEYBOARD
    },
    Model {
        pid: 0x0293,
        name: "Razer BlackWidow V4 X",
        transaction_id: 0x1f,
        matrix: Some(MATRIX_FULL),
        ..KEYBOARD
    },
    // ---- Huntsman ----
    Model {
        pid: 0x0227,
        name: "Razer Huntsman",
        wave: WaveCodes::ZERO_BASED,
        matrix: Some(MATRIX_FULL),
        ..KEYBOARD
    },
    Model {
        pid: 0x0226,
        name: "Razer Huntsman Elite",
        wave: WaveCodes::ZERO_BASED,
        matrix: Some(MATRIX_FULL),
        ..KEYBOARD
    },
    Model {
        pid: 0x0257,
        name: "Razer Huntsman Mini",
        wave: WaveCodes::ZERO_BASED,
        ..KEYBOARD
    },
    Model {
        pid: 0x026c,
        name: "Razer Huntsman V2",
        transaction_id: 0x1f,
        control: KEYBOARD_CONTROL_3,
        polling: POLL_8K_KEYBOARD,
        matrix: Some(MATRIX_UNSHIFTED),
        game_mode: Some(Store::No),
        ..KEYBOARD
    },
    // ---- Ornata ----
    Model {
        pid: 0x028f,
        name: "Razer Ornata V3",
        transaction_id: 0x1f,
        zones: ZONES_ORNATA,
        wave: WaveCodes::FLIPPED,
        ..KEYBOARD
    },
    Model {
        pid: 0x02a1,
        name: "Razer Ornata V3 (Rev 2)",
        transaction_id: 0x1f,
        zones: ZONES_ORNATA,
        wave: WaveCodes::FLIPPED,
        ..KEYBOARD
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn iface(pid: u16, interface: i32, usage_page: u16, usage: u16) -> HidInterfaceInfo {
        HidInterfaceInfo {
            path: format!("{pid:04x}/{interface}/{usage_page:x}:{usage:x}"),
            vendor_id: VID_RAZER,
            product_id: pid,
            interface_number: interface,
            usage_page,
            usage,
            ..Default::default()
        }
    }

    #[test]
    fn product_ids_and_names_are_unique() {
        let pids: HashSet<u16> = MODELS.iter().map(|m| m.pid).collect();
        assert_eq!(pids.len(), MODELS.len());
        let names: HashSet<&str> = MODELS.iter().map(|m| m.name).collect();
        assert_eq!(names.len(), MODELS.len());
        assert!(MODELS.iter().all(|m| !m.name.is_empty() && m.pid != 0));
    }

    #[test]
    fn entries_are_internally_consistent() {
        for m in MODELS {
            let ctx = m.name;
            assert!(
                m.polling.rates.windows(2).all(|w| w[0] < w[1]),
                "{ctx}: rates not ascending"
            );
            for z in m.zones {
                assert!(!z.effects.is_empty(), "{ctx}: zone without effects");
                assert!(
                    z.effects.contains(&EffectKind::Static),
                    "{ctx}: zone cannot show a colour"
                );
            }
            let ids: HashSet<u8> = m.zones.iter().map(|z| z.led).collect();
            assert_eq!(ids.len(), m.zones.len(), "{ctx}: duplicate LED");
            match m.kind {
                DeviceKind::Mouse => {
                    let d = m.dpi.unwrap_or_else(|| panic!("{ctx}: mouse without DPI"));
                    assert!(d.max >= 8_500, "{ctx}");
                    assert!(m.matrix.is_none() && m.game_mode.is_none(), "{ctx}");
                }
                DeviceKind::Keyboard => {
                    assert!(m.dpi.is_none() && !m.zones.is_empty(), "{ctx}");
                }
                other => panic!("{ctx}: unexpected kind {other:?}"),
            }
            if m.connection == Connection::Wireless {
                assert!(m.battery, "{ctx}: wireless without battery");
            }
        }
    }

    #[test]
    fn every_matrix_covers_its_layout_without_collisions() {
        let boards: Vec<_> = MODELS
            .iter()
            .filter_map(|m| m.matrix.map(|x| (m.name, x)))
            .collect();
        assert!(boards.len() >= 8);
        for (name, mx) in boards {
            let layout = mx.layout();
            let mut seen = HashSet::new();
            for k in &layout.keys {
                let (r, c) = mx
                    .cell(k.id as u8)
                    .unwrap_or_else(|| panic!("{name}: no cell for 0x{:02x}", k.id));
                assert!(
                    r < mx.rows && c < mx.cols,
                    "{name}: 0x{:02x} at {r},{c} outside {}x{}",
                    k.id,
                    mx.rows,
                    mx.cols
                );
                assert!(seen.insert((r, c)), "{name}: two keys at {r},{c}");
            }
            // 5 + 3 * cols argument bytes must fit one report.
            assert!(5 + 3 * mx.cols as usize <= 80, "{name}");
        }
    }

    #[test]
    fn matrix_widths_match_openrazer_matrix_dims() {
        // Column counts of the devices' full LED matrices (openrazer MATRIX_DIMS).
        for (pid, cols) in [
            (0x024e, 22),
            (0x0a24, 18),
            (0x025a, 22),
            (0x0287, 23),
            (0x028d, 23),
            (0x0293, 22),
            (0x0227, 22),
            (0x0226, 22),
            (0x026c, 22),
        ] {
            assert_eq!(find(pid).unwrap().matrix.unwrap().cols, cols, "{pid:04x}");
        }
    }

    #[test]
    fn blackwidow_v3_has_exactly_one_matching_interface() {
        // A BlackWidow V3 as hidapi lists it: boot keyboard, two generic
        // interfaces and the consumer-control interface 3 that carries the
        // feature reports.
        let all = [
            iface(0x024e, 0, 0x01, 0x06),
            iface(0x024e, 1, 0x01, 0x80),
            iface(0x024e, 2, 0x01, 0x02),
            iface(0x024e, 3, 0x0c, 0x01),
        ];
        let hits: Vec<_> = all.iter().filter(|i| for_interface(i).is_some()).collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].interface_number, 3);
        assert_eq!(for_interface(hits[0]).unwrap().name, "Razer BlackWidow V3");
    }

    #[test]
    fn mouse_control_is_interface_zero_but_not_other_collections() {
        let mouse = iface(0x0084, 0, 0x01, 0x02);
        assert_eq!(for_interface(&mouse).map(|m| m.pid), Some(0x0084));
        // Same interface, different top-level collection (Windows lists them apart).
        assert!(for_interface(&iface(0x0084, 0, 0xff00, 0x01)).is_none());
        assert!(for_interface(&iface(0x0084, 1, 0x01, 0x06)).is_none());
        assert!(for_interface(&iface(0x0084, 2, 0x01, 0x02)).is_none());
        // The Basilisk V3 uses interface 3 instead.
        assert!(for_interface(&iface(0x0099, 0, 0x01, 0x02)).is_none());
        assert!(for_interface(&iface(0x0099, 3, 0x0c, 0x01)).is_some());
    }

    #[test]
    fn backends_without_usage_info_match_on_the_interface_number() {
        assert!(for_interface(&iface(0x0293, 2, 0, 0)).is_some());
        assert!(for_interface(&iface(0x0293, 1, 0, 0)).is_none());
        // Unknown interface number: refuse rather than guess.
        assert!(for_interface(&iface(0x0293, -1, 0x01, 0x02)).is_none());
    }

    #[test]
    fn blackwidow_v4_accepts_both_firmware_usages() {
        assert!(for_interface(&iface(0x0287, 3, 0x01, 0x00)).is_some());
        assert!(for_interface(&iface(0x0287, 3, 0x0c, 0x01)).is_some());
        assert!(for_interface(&iface(0x0287, 3, 0x01, 0x06)).is_none());
    }

    #[test]
    fn other_vendors_and_unknown_products_are_ignored() {
        let mut i = iface(0x0084, 0, 0x01, 0x02);
        i.vendor_id = 0x046d;
        assert!(for_interface(&i).is_none());
        assert!(for_interface(&iface(0x7777, 0, 0x01, 0x02)).is_none());
    }
}
