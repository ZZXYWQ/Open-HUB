//! Static device table: which Corsair PIDs we drive, with which protocol and
//! what they can do.
//!
//! PIDs, packet sizes and LED counts come from the ckb-next, OpenRGB and
//! OpenLinkHub device lists. Nearly every entry is confirmed by at least two
//! of them; the exceptions (M65 RGB Ultra Wireless 0x1bd6, M75 Wireless
//! 0x2b08) rest on OpenLinkHub alone.

use gearwise_core::keys::{self, KeyboardLayout};
use gearwise_core::model::DeviceKind;

/// HID usage page of the Bragi control interface.
pub(crate) const BRAGI_PAGE: u16 = 0xff42;
/// Usage of the Bragi control collection; usage 2 on the same page is the
/// event channel, which must not be claimed.
pub(crate) const BRAGI_USAGE: u16 = 0x0001;
/// HID usage page of the legacy NXP control collection.
pub(crate) const NXP_PAGE: u16 = 0xffc2;
/// USB interface number of the control collection (all supported models).
pub(crate) const CONTROL_INTERFACE: i32 = 1;

/// A group of LEDs lit as one lighting zone.
pub(crate) struct Zone {
    pub id: u8,
    pub name: &'static str,
    /// Indices into the device's LED buffer.
    pub leds: &'static [u16],
    /// Lit white before the host has chosen a colour (off for indicators).
    pub default_on: bool,
}

const fn zone(id: u8, name: &'static str, leds: &'static [u16], default_on: bool) -> Zone {
    Zone {
        id,
        name,
        leds,
        default_on,
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Layout {
    Full,
    Tkl,
}

impl Layout {
    pub(crate) fn build(self) -> KeyboardLayout {
        match self {
            Layout::Full => keys::ansi_full(),
            Layout::Tkl => keys::ansi_tkl(),
        }
    }
}

/// Sensor limits. Resolution is adjustable in steps of [`DPI_STEP`].
pub(crate) struct DpiSpec {
    pub min: u16,
    pub max: u16,
    /// Separate X/Y properties (0x21/0x22) rather than the linked one (0x20).
    pub separate_xy: bool,
}

pub(crate) const DPI_STEP: u16 = 10;
/// Stages offered to the user. Bragi keeps the table on the host; NXP maps
/// them to hardware stages 1..=5 (stage 0 is the sniper button's).
pub(crate) const DPI_STAGES: u8 = 5;

pub(crate) struct BragiSpec {
    /// Output report payload size: 64 for most, 128 or 1024 for some keyboards.
    pub packet: usize,
    /// Entries in the lighting buffer.
    pub leds: u16,
    /// Zoned lighting (empty on per-key keyboards).
    pub zones: &'static [Zone],
    /// Per-key keyboard layout; LED index is derived from the HID usage.
    pub keys: Option<Layout>,
    pub dpi: Option<DpiSpec>,
    pub battery: bool,
    pub angle_snap: bool,
    /// (label, property value) for property 0x06.
    pub lift: &'static [(&'static str, u8)],
}

pub(crate) struct NxpSpec {
    /// Per-key keyboard: layout and key slots per colour plane (144 or 168).
    pub keys: Option<(Layout, usize)>,
    /// Mouse: maximum DPI.
    pub dpi_max: Option<u16>,
    /// Polling rate can be changed (and is re-enumerated by the device).
    pub adj_rate: bool,
    /// Keyboards that need special-function and key-input setup before
    /// software lighting takes effect (K70 MK.2 family, K95 Platinum).
    pub extended: bool,
}

impl NxpSpec {
    /// Sensor limits of a mouse.
    pub(crate) fn dpi(&self) -> Option<DpiSpec> {
        self.dpi_max.map(|max| DpiSpec {
            min: 100,
            max,
            separate_xy: true,
        })
    }
}

pub(crate) enum Proto {
    Bragi(BragiSpec),
    /// Slipstream receiver: no device of its own, wireless devices are
    /// reached through it with a sub-device index in the packet header.
    Receiver,
    Nxp(NxpSpec),
}

pub(crate) struct Model {
    pub pid: u16,
    pub name: &'static str,
    pub kind: DeviceKind,
    pub proto: Proto,
}

impl Model {
    /// (usage page, usage or 0 for any) of the control collection.
    pub(crate) fn control_endpoint(&self) -> (u16, u16) {
        match self.proto {
            Proto::Nxp(_) => (NXP_PAGE, 0),
            _ => (BRAGI_PAGE, BRAGI_USAGE),
        }
    }
}

pub(crate) fn find(pid: u16) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.pid == pid)
}

// ---------------------------------------------------------------------------
// Zone layouts
// ---------------------------------------------------------------------------

/// Keys zone of a per-key keyboard: the base colour for keys without their own.
pub(crate) const KEYS_ZONE: u8 = 0;
const KEYS: &[Zone] = &[];

static M65_ULTRA_ZONES: [Zone; 3] = [
    zone(0, "Logo", &[0], true),
    zone(1, "Scroll wheel", &[1], true),
    zone(2, "DPI indicator", &[2], true),
];
static M65_ULTRA_W_ZONES: [Zone; 2] = [
    zone(0, "Logo", &[0], true),
    zone(1, "DPI indicator", &[1], true),
];
static DARK_CORE_ZONES: [Zone; 4] = [
    zone(0, "Scroll wheel", &[0], true),
    zone(1, "Logo", &[6], true),
    zone(2, "Side accents", &[1, 2, 3, 4, 5, 7], true),
    zone(3, "DPI indicators", &[8, 9, 10, 11], true),
];
static KATAR_ZONES: [Zone; 1] = [zone(0, "Scroll wheel", &[0], true)];
static M55_ZONES: [Zone; 2] = [
    zone(0, "Logo", &[1], true),
    zone(1, "DPI indicator", &[0], true),
];
static IRONCLAW_ZONES: [Zone; 4] = [
    zone(0, "Logo", &[0], true),
    zone(1, "Scroll wheel", &[1], true),
    zone(2, "Front", &[2], true),
    zone(3, "DPI indicators", &[3, 4, 5], true),
];
static HARPOON_ZONES: [Zone; 2] = [
    zone(0, "Logo", &[1], true),
    zone(1, "DPI indicator", &[0], true),
];
static M75_ZONES: [Zone; 2] = [zone(0, "Logo", &[0], true), zone(1, "Bottom", &[1], true)];
/// LED 0 is the Win-lock indicator, LEDs 1..=5 are the five key zones.
static K55_PRO_ZONES: [Zone; 6] = [
    zone(0, "Win-lock indicator", &[0], false),
    zone(1, "Zone 1", &[1], true),
    zone(2, "Zone 2", &[2], true),
    zone(3, "Zone 3", &[3], true),
    zone(4, "Zone 4", &[4], true),
    zone(5, "Zone 5", &[5], true),
];

const LIFT_M65: &[(&str, u8)] = &[("Ultra low", 7), ("Low", 2), ("Medium", 3), ("High", 4)];
const LIFT_M75: &[(&str, u8)] = &[("Low", 2), ("Medium", 3), ("High", 4)];

// ---------------------------------------------------------------------------
// Table
// ---------------------------------------------------------------------------

const fn dpi(min: u16, max: u16, separate_xy: bool) -> Option<DpiSpec> {
    Some(DpiSpec {
        min,
        max,
        separate_xy,
    })
}

const fn mouse(pid: u16, name: &'static str, spec: BragiSpec) -> Model {
    Model {
        pid,
        name,
        kind: DeviceKind::Mouse,
        proto: Proto::Bragi(spec),
    }
}

const fn mouse_spec(
    leds: u16,
    zones: &'static [Zone],
    dpi: Option<DpiSpec>,
    battery: bool,
    angle_snap: bool,
    lift: &'static [(&'static str, u8)],
) -> BragiSpec {
    BragiSpec {
        packet: 64,
        leds,
        zones,
        keys: None,
        dpi,
        battery,
        angle_snap,
        lift,
    }
}

const fn keyboard(pid: u16, name: &'static str, packet: usize, leds: u16, layout: Layout) -> Model {
    let spec = BragiSpec {
        packet,
        leds,
        zones: KEYS,
        keys: Some(layout),
        dpi: None,
        battery: false,
        angle_snap: false,
        lift: &[],
    };
    Model {
        pid,
        name,
        kind: DeviceKind::Keyboard,
        proto: Proto::Bragi(spec),
    }
}

const fn receiver(pid: u16, name: &'static str) -> Model {
    Model {
        pid,
        name,
        kind: DeviceKind::Receiver,
        proto: Proto::Receiver,
    }
}

const fn nxp_keyboard(
    pid: u16,
    name: &'static str,
    layout: Layout,
    slots: usize,
    adj_rate: bool,
    extended: bool,
) -> Model {
    let spec = NxpSpec {
        keys: Some((layout, slots)),
        dpi_max: None,
        adj_rate,
        extended,
    };
    Model {
        pid,
        name,
        kind: DeviceKind::Keyboard,
        proto: Proto::Nxp(spec),
    }
}

const fn nxp_mouse(pid: u16, name: &'static str, dpi_max: u16) -> Model {
    let spec = NxpSpec {
        keys: None,
        dpi_max: Some(dpi_max),
        adj_rate: true,
        extended: false,
    };
    Model {
        pid,
        name,
        kind: DeviceKind::Mouse,
        proto: Proto::Nxp(spec),
    }
}

#[rustfmt::skip]
pub(crate) static MODELS: &[Model] = &[
    // --- Bragi mice -------------------------------------------------------
    mouse(0x1b9e, "Corsair M65 RGB Ultra", mouse_spec(3, &M65_ULTRA_ZONES, dpi(100, 26_000, true), false, true, LIFT_M65)),
    mouse(0x1bb5, "Corsair M65 RGB Ultra Wireless", mouse_spec(2, &M65_ULTRA_W_ZONES, dpi(100, 26_000, true), true, true, LIFT_M65)),
    mouse(0x1bd6, "Corsair M65 RGB Ultra Wireless", mouse_spec(2, &M65_ULTRA_W_ZONES, dpi(100, 26_000, true), true, true, LIFT_M65)),
    mouse(0x1b80, "Corsair Dark Core RGB Pro", mouse_spec(12, &DARK_CORE_ZONES, dpi(100, 18_000, true), true, true, &[])),
    mouse(0x1b7e, "Corsair Dark Core RGB Pro SE", mouse_spec(12, &DARK_CORE_ZONES, dpi(100, 18_000, true), true, true, &[])),
    mouse(0x1b93, "Corsair Katar Pro", mouse_spec(1, &KATAR_ZONES, dpi(200, 12_400, false), false, false, &[])),
    mouse(0x1bac, "Corsair Katar Pro XT", mouse_spec(1, &KATAR_ZONES, dpi(100, 18_000, true), false, false, &[])),
    mouse(0x1b70, "Corsair M55 RGB Pro", mouse_spec(2, &M55_ZONES, dpi(200, 12_400, false), false, false, &[])),
    mouse(0x1b4c, "Corsair Ironclaw RGB Wireless", mouse_spec(6, &IRONCLAW_ZONES, dpi(200, 18_000, true), true, true, &[])),
    mouse(0x1b5e, "Corsair Harpoon RGB Wireless", mouse_spec(2, &HARPOON_ZONES, dpi(200, 10_000, false), true, true, &[])),
    mouse(0x1bf0, "Corsair M75", mouse_spec(2, &M75_ZONES, dpi(100, 26_000, true), false, true, LIFT_M75)),
    mouse(0x1bf1, "Corsair M75 Wireless", mouse_spec(2, &M75_ZONES, dpi(100, 26_000, true), true, true, LIFT_M75)),
    mouse(0x2b08, "Corsair M75 Wireless", mouse_spec(2, &M75_ZONES, dpi(100, 26_000, true), true, true, LIFT_M75)),
    // --- Bragi keyboards --------------------------------------------------
    Model {
        pid: 0x1ba4,
        name: "Corsair K55 RGB PRO",
        kind: DeviceKind::Keyboard,
        proto: Proto::Bragi(BragiSpec {
            packet: 64,
            leds: 6,
            zones: &K55_PRO_ZONES,
            keys: None,
            dpi: None,
            battery: false,
            angle_snap: false,
            lift: &[],
        }),
    },
    keyboard(0x1bfd, "Corsair K70 Core RGB", 64, 123, Layout::Full),
    keyboard(0x1bff, "Corsair K70 Core RGB", 64, 123, Layout::Full),
    keyboard(0x1b73, "Corsair K70 RGB TKL", 1024, 193, Layout::Tkl),
    keyboard(0x1bb9, "Corsair K70 RGB TKL Champion Series", 1024, 193, Layout::Tkl),
    keyboard(0x1bb3, "Corsair K70 RGB PRO", 1024, 193, Layout::Full),
    keyboard(0x1bd4, "Corsair K70 RGB PRO", 1024, 193, Layout::Full),
    keyboard(0x1b7c, "Corsair K100 RGB Optical", 1024, 193, Layout::Full),
    keyboard(0x1b7d, "Corsair K100 RGB MX", 1024, 193, Layout::Full),
    keyboard(0x1bc5, "Corsair K100 RGB Optical", 1024, 193, Layout::Full),
    keyboard(0x1b89, "Corsair K95 RGB Platinum XT", 64, 156, Layout::Full),
    // --- Slipstream receivers --------------------------------------------
    receiver(0x1ba6, "Corsair Slipstream Wireless Receiver"),
    receiver(0x1b65, "Corsair Slipstream Wireless Receiver"),
    receiver(0x1b66, "Corsair Slipstream Wireless Receiver"),
    receiver(0x1b7f, "Corsair Slipstream Wireless Receiver"),
    receiver(0x1b81, "Corsair Slipstream Wireless Receiver"),
    receiver(0x1bdc, "Corsair Slipstream Wireless Receiver"),
    receiver(0x2b00, "Corsair Slipstream Wireless Receiver"),
    // --- Legacy NXP keyboards --------------------------------------------
    nxp_keyboard(0x1b49, "Corsair K70 RGB MK.2", Layout::Full, 144, true, true),
    nxp_keyboard(0x1b6b, "Corsair K70 RGB MK.2 SE", Layout::Full, 144, true, true),
    nxp_keyboard(0x1b55, "Corsair K70 RGB MK.2 Low Profile", Layout::Full, 144, true, true),
    nxp_keyboard(0x1b2d, "Corsair K95 RGB Platinum", Layout::Full, 168, true, true),
    nxp_keyboard(0x1b33, "Corsair K70 LUX RGB", Layout::Full, 144, false, false),
    nxp_keyboard(0x1b38, "Corsair K70 RGB Rapidfire", Layout::Full, 144, false, false),
    nxp_keyboard(0x1b37, "Corsair K65 LUX RGB", Layout::Tkl, 144, false, false),
    nxp_keyboard(0x1b39, "Corsair K65 RGB Rapidfire", Layout::Tkl, 144, false, false),
    nxp_keyboard(0x1b48, "Corsair Strafe RGB MK.2", Layout::Full, 144, true, false),
    // --- Legacy NXP mice ---------------------------------------------------
    nxp_mouse(0x1b2e, "Corsair M65 PRO RGB", 12_000),
    nxp_mouse(0x1b5a, "Corsair M65 RGB Elite", 18_000),
    nxp_mouse(0x1b75, "Corsair Harpoon RGB PRO", 12_000),
    nxp_mouse(0x1b5d, "Corsair Ironclaw RGB", 18_000),
    nxp_mouse(0x1b5c, "Corsair Nightsword RGB", 18_000),
    nxp_mouse(0x1b3e, "Corsair Scimitar PRO RGB", 16_000),
    nxp_mouse(0x1b8b, "Corsair Scimitar RGB Elite", 18_000),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap;

    #[test]
    fn pids_are_unique() {
        for (i, m) in MODELS.iter().enumerate() {
            assert!(
                MODELS[i + 1..].iter().all(|o| o.pid != m.pid),
                "duplicate pid {:04x}",
                m.pid
            );
        }
    }

    #[test]
    fn zone_leds_fit_the_buffer_and_do_not_overlap() {
        for m in MODELS {
            let Proto::Bragi(b) = &m.proto else { continue };
            let mut seen = vec![false; b.leds as usize];
            for z in b.zones {
                for &led in z.leds {
                    assert!(
                        (led as usize) < seen.len(),
                        "{} zone {} led {led}",
                        m.name,
                        z.name
                    );
                    assert!(!seen[led as usize], "{} led {led} in two zones", m.name);
                    seen[led as usize] = true;
                }
            }
            if !b.zones.is_empty() {
                assert!(seen.iter().all(|s| *s), "{}: unassigned LED", m.name);
            }
            let ids: Vec<u8> = b.zones.iter().map(|z| z.id).collect();
            assert!(
                ids.iter()
                    .enumerate()
                    .all(|(i, id)| ids[..i].iter().all(|o| o != id))
            );
        }
    }

    #[test]
    fn keyboard_layouts_map_inside_their_buffers() {
        for m in MODELS {
            match &m.proto {
                Proto::Bragi(BragiSpec {
                    keys: Some(l),
                    leds,
                    packet,
                    ..
                }) => {
                    assert!(matches!(packet, 64 | 128 | 1024));
                    for k in l.build().keys {
                        let led = keymap::bragi_led(k.id).expect("every layout key has a led");
                        assert!(led < *leds as usize, "{} key {}", m.name, k.label);
                    }
                }
                Proto::Nxp(NxpSpec {
                    keys: Some((l, slots)),
                    ..
                }) => {
                    for k in l.build().keys {
                        let led = keymap::nxp_led(k.id).expect("every layout key has a led");
                        assert!(led < *slots, "{} key {}", m.name, k.label);
                    }
                }
                _ => {}
            }
        }
    }
}
