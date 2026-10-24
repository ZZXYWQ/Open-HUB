//! Static table of the SteelSeries mice this crate drives.
//!
//! Product ids, command bytes, value encodings and factory defaults come from
//! rivalcfg's per-model profiles (hardware-tested by its users). Where a
//! second project covers the same mouse it was compared: libratbag for the
//! DPI, button-table and colour-stop layouts of the Rival 100/300/310/600 and
//! Sensei 310 families, and OpenRGB for the Rival 3 and Aerox colour packets,
//! the 2.4 GHz command flag and the control collection of the Aerox models.
//! Mice that rivalcfg does not document are not listed.
//!
//! The projects disagree in three places, and rivalcfg's values are used:
//! libratbag encodes the 125 and 250 Hz codes of the Rival 310/600 and
//! Sensei 310 as 8 and 4 (rivalcfg: 4 and 3), the Rival 300 DPI code as
//! `dpi / 50 - 1` (rivalcfg: `dpi / 50`), and sends the Rival 310/Sensei 310
//! colour stops as an output report (rivalcfg: feature report).
//!
//! Spec constants are grouped by protocol family; a model is a product id
//! plus the spec it shares with its siblings (special editions, and the wired
//! and dongle product ids of one wireless mouse).

use gearwise_core::model::{
    ButtonAction, Connection, DpiFunction, EffectKind, LightingEffect, MouseButton, Rgb,
    VID_STEELSERIES,
};
use gearwise_core::transport::HidInterfaceInfo;

use crate::cpi::{self, Cpi};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// How a command is delivered to the mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Report {
    Output,
    Feature,
}

/// Which top-level HID collection of the control interface carries commands.
#[derive(Clone, Copy, Debug)]
pub enum Collection {
    /// Confirmed vendor collection (usage page, usage).
    Exact(u16, u16),
    /// Some vendor-defined collection (usage page 0xFF00 and up).
    Vendor,
}

/// Where a mouse takes its commands.
#[derive(Clone, Copy, Debug)]
pub struct Control {
    /// USB interface number (rivalcfg's "endpoint").
    pub interface: i32,
    pub collection: Collection,
}

impl Control {
    /// Does `iface` carry the commands?
    ///
    /// The Linux backend lists one entry per hidraw node, so the interface
    /// number alone picks it. Windows lists every top-level collection of an
    /// interface; the mouse and keyboard ones are owned by the OS and cannot
    /// be written, so only the vendor collection is claimed there
    /// (`per_collection`). Entries without usage information match on the
    /// interface number alone.
    pub fn matches(&self, iface: &HidInterfaceInfo, per_collection: bool) -> bool {
        if iface.interface_number != self.interface {
            return false;
        }
        if !per_collection || (iface.usage_page == 0 && iface.usage == 0) {
            return true;
        }
        match self.collection {
            Collection::Exact(page, usage) => (iface.usage_page, iface.usage) == (page, usage),
            Collection::Vendor => iface.usage_page >= 0xff00,
        }
    }
}

/// How packets are framed for a model.
#[derive(Clone, Copy, Debug)]
pub struct Wire {
    /// Zero-pad every packet to at least this many bytes (0: send as is).
    pub pad: usize,
    /// 2.4 GHz dongle: command bytes carry a flag and every packet is
    /// acknowledged with a reply report.
    pub dongle: bool,
}

/// How the number of stages is announced in a block DPI command.
#[derive(Clone, Copy, Debug)]
pub enum Count {
    /// The number itself.
    Number,
    /// One bit per stage (`0b11111` for five).
    Flags,
}

/// How DPI stages reach the firmware.
#[derive(Clone, Copy, Debug)]
pub enum DpiProto {
    /// One packet per stage (two stages): `head, stage number, code, tail`.
    /// The active stage cannot be chosen from the host.
    PerStage {
        head: &'static [u8],
        tail: &'static [u8],
    },
    /// One packet holds all stages: `cmd, count, selected, codes...`.
    Block {
        cmd: &'static [u8],
        /// Wire value of the first stage as the selected one.
        first: u8,
        count: Count,
        /// Codes are 16-bit little endian instead of one byte.
        wide: bool,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct DpiSpec {
    pub cpi: Cpi,
    /// How many stages the mouse holds.
    pub stages: u8,
    /// Factory stages.
    pub defaults: &'static [u16],
    pub proto: DpiProto,
}

#[derive(Clone, Copy, Debug)]
pub struct RateSpec {
    /// `cmd, code`.
    pub cmd: &'static [u8],
    /// (Hz, code), ascending.
    pub codes: &'static [(u16, u8)],
}

/// One lit area.
#[derive(Clone, Copy, Debug)]
pub struct Zone {
    pub name: &'static str,
    /// LED number on the wire.
    pub led: u8,
    /// Factory lighting.
    pub default: LightingEffect,
}

/// Layout of the "colour cycle" packet used by Rival 310/600 and Sensei: a
/// header followed by a list of colour stops that the firmware loops over.
#[derive(Clone, Copy, Debug)]
pub struct Cycle {
    pub cmd: [u8; 2],
    /// Header length after the command.
    pub header: usize,
    /// Offsets (in the header) that hold the LED number.
    pub leds: &'static [usize],
    /// Offset of the 16-bit loop duration in milliseconds.
    pub duration: usize,
    /// Offset of the "play once" flag.
    pub once: usize,
    /// Offset of the colour stop count.
    pub count: usize,
}

/// Lighting command family.
#[derive(Clone, Copy, Debug)]
pub enum LightProto {
    /// `05 00 R G B` plus effect `07 00 E` (Rival 100/110/300S).
    Backlight,
    /// `08 LED R G B` plus effect `07 LED E` (Rival 300).
    LogoWheel,
    /// `05 00 LED R G B 64` (Rival 3).
    FourZone,
    /// `21 MASK` and one RGB slot per zone (Aerox 3/5 wired).
    Mask,
    /// `21 01 LED R G B` (Aerox/Prime Wireless).
    PerLed,
    /// `62 01 R G B` and a fixed tail (Prime, Prime+).
    Prime,
    /// `21 00 R G B` (Prime Mini).
    PrimeMini,
    /// Colour-stop list, sent as a feature report.
    Cycle(Cycle),
}

#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub proto: LightProto,
    pub zones: &'static [Zone],
}

const FX_STATIC: &[EffectKind] = &[EffectKind::Off, EffectKind::Static];
const FX_BREATHING: &[EffectKind] = &[EffectKind::Off, EffectKind::Static, EffectKind::Breathing];
const FX_CYCLE: &[EffectKind] = &[
    EffectKind::Off,
    EffectKind::Static,
    EffectKind::Breathing,
    EffectKind::ColorCycle,
];

impl Light {
    /// Effects every zone of this mouse can show.
    pub fn effects(&self) -> &'static [EffectKind] {
        match self.proto {
            LightProto::Backlight | LightProto::LogoWheel => FX_BREATHING,
            LightProto::Cycle(_) => FX_CYCLE,
            _ => FX_STATIC,
        }
    }
}

/// Factory behaviour of a button.
#[derive(Clone, Copy, Debug)]
pub enum Preset {
    Click(MouseButton),
    DpiCycle,
    Disabled,
    /// A keyboard key (HID usage).
    Key(u8),
}

impl Preset {
    pub fn action(self) -> ButtonAction {
        match self {
            Preset::Click(b) => ButtonAction::Mouse(b),
            Preset::DpiCycle => ButtonAction::Dpi(DpiFunction::Cycle),
            Preset::Disabled => ButtonAction::Disabled,
            Preset::Key(usage) => ButtonAction::Key {
                usage,
                mods: Default::default(),
            },
        }
    }
}

/// A remappable button.
#[derive(Clone, Copy, Debug)]
pub struct Btn {
    /// rivalcfg's button number, also the code that makes another button
    /// behave like this one.
    pub id: u16,
    pub name: &'static str,
    pub default: Preset,
    pub hotspot: (f32, f32),
}

/// One slot of the button table.
#[derive(Clone, Copy, Debug)]
pub enum Field {
    Button(Btn),
    /// Not exposed (wheel rotation, unused buttons); always sent as is.
    Fixed(&'static [u8]),
}

#[derive(Clone, Copy, Debug)]
pub struct ButtonSpec {
    /// Command bytes before the table.
    pub cmd: &'static [u8],
    pub report: Report,
    /// Bytes per slot.
    pub width: usize,
    /// Slot code introducing a keyboard key.
    pub keyboard: u8,
    /// Slot code introducing a consumer-control (media) key.
    pub consumer: u8,
    pub fields: &'static [Field],
}

#[derive(Clone, Copy, Debug)]
pub enum BatteryProto {
    /// `92` answers `92 STATUS`; bit 7 of the status is "charging", the rest
    /// is the charge in steps of 5 % plus one.
    Status,
    /// `AA 01` answers `LEVEL ? CHARGING`.
    Level,
}

/// Everything the driver needs to know about a family of products.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub control: Control,
    pub wire: Wire,
    pub dpi: DpiSpec,
    pub rate: RateSpec,
    pub light: Option<Light>,
    pub buttons: Option<ButtonSpec>,
    pub battery: Option<BatteryProto>,
    /// Command that stores the settings in the mouse.
    pub save: &'static [u8],
}

/// One product id.
#[derive(Clone, Copy, Debug)]
pub struct Model {
    pub pid: u16,
    pub name: &'static str,
    pub connection: Connection,
    pub spec: &'static Spec,
}

/// The model for a product id.
pub fn find(pid: u16) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.pid == pid)
}

/// The model whose control interface `iface` is, if any. See
/// [`Control::matches`] for `per_collection`.
pub fn for_interface(iface: &HidInterfaceInfo, per_collection: bool) -> Option<&'static Model> {
    if iface.vendor_id != VID_STEELSERIES {
        return None;
    }
    find(iface.product_id).filter(|m| m.spec.control.matches(iface, per_collection))
}

// ---------------------------------------------------------------------------
// Shared building blocks
// ---------------------------------------------------------------------------

/// Aerox, Rival 3 Wireless: the vendor collection on interface 3 (OpenRGB).
const AEROX_CONTROL: Control = Control {
    interface: 3,
    collection: Collection::Exact(0xffc0, 1),
};
/// Mice that command through interface 3 without a confirmed usage.
const IFACE_3: Control = Control {
    interface: 3,
    collection: Collection::Vendor,
};
/// Mice that command through interface 0.
const IFACE_0: Control = Control {
    interface: 0,
    collection: Collection::Vendor,
};

const WIRED: Wire = Wire {
    pad: 0,
    dongle: false,
};
const PADDED_32: Wire = Wire {
    pad: 32,
    dongle: false,
};
const DONGLE: Wire = Wire {
    pad: 0,
    dongle: true,
};

/// 125/250/500/1000 Hz as 4/3/2/1.
const RATES_DESCENDING: &[(u16, u8)] = &[(125, 4), (250, 3), (500, 2), (1000, 1)];
/// 125/250/500/1000 Hz as 3/2/1/0 (newer wireless-era firmware).
const RATES_ZERO_BASED: &[(u16, u8)] = &[(125, 3), (250, 2), (500, 1), (1000, 0)];

const SAVE_09_00: &[u8] = &[0x09, 0x00];
const SAVE_59_00: &[u8] = &[0x59, 0x00];
const SAVE_11_00: &[u8] = &[0x11, 0x00];

const fn solid(r: u8, g: u8, b: u8) -> LightingEffect {
    LightingEffect::Static {
        color: Rgb::new(r, g, b),
    }
}

const RED_GREEN_BLUE: &[Zone] = &[
    Zone {
        name: "Front",
        led: 0,
        default: solid(255, 0, 0),
    },
    Zone {
        name: "Middle",
        led: 1,
        default: solid(0, 255, 0),
    },
    Zone {
        name: "Rear",
        led: 2,
        default: solid(0, 0, 255),
    },
];

const fn cycling(period_ms: u16) -> LightingEffect {
    LightingEffect::ColorCycle { period_ms }
}

// Button hotspots on the artwork (normalised, top-down).
const LEFT: (f32, f32) = (0.30, 0.20);
const RIGHT: (f32, f32) = (0.70, 0.20);
const MIDDLE: (f32, f32) = (0.50, 0.17);
const BACK: (f32, f32) = (0.10, 0.50);
const FORWARD: (f32, f32) = (0.11, 0.40);
const DPI: (f32, f32) = (0.50, 0.30);
const RIGHT_REAR: (f32, f32) = (0.90, 0.50);
const RIGHT_FRONT: (f32, f32) = (0.89, 0.40);

const fn button(id: u16, name: &'static str, default: Preset, hotspot: (f32, f32)) -> Field {
    Field::Button(Btn {
        id,
        name,
        default,
        hotspot,
    })
}

const B1: Field = button(1, "Left click", Preset::Click(MouseButton::Left), LEFT);
const B2: Field = button(2, "Right click", Preset::Click(MouseButton::Right), RIGHT);
const B3: Field = button(
    3,
    "Middle click",
    Preset::Click(MouseButton::Middle),
    MIDDLE,
);
const B4: Field = button(4, "Back", Preset::Click(MouseButton::Back), BACK);
const B5: Field = button(5, "Forward", Preset::Click(MouseButton::Forward), FORWARD);
const B6_DPI: Field = button(6, "DPI button", Preset::DpiCycle, DPI);
/// Wheel rotation slots, always left at their factory value.
const WHEEL_UP: Field = Field::Fixed(&[0x31]);
const WHEEL_DOWN: Field = Field::Fixed(&[0x32]);
const UNUSED: Field = Field::Fixed(&[0x00]);

const SIX_BUTTONS: &[Field] = &[B1, B2, B3, B4, B5, B6_DPI];
const SIX_BUTTONS_AND_WHEEL: &[Field] = &[B1, B2, B3, B4, B5, B6_DPI, WHEEL_UP, WHEEL_DOWN];

/// Five-byte slots: `code, argument...`; keyboard 0x51, media 0x61.
const fn slots5(cmd: &'static [u8], report: Report, fields: &'static [Field]) -> ButtonSpec {
    ButtonSpec {
        cmd,
        report,
        width: 5,
        keyboard: 0x51,
        consumer: 0x61,
        fields,
    }
}

// ---------------------------------------------------------------------------
// Rival 3 family (interface 3)
// ---------------------------------------------------------------------------

const RIVAL_3: Spec = Spec {
    control: IFACE_3,
    wire: WIRED,
    dpi: DpiSpec {
        cpi: cpi::CORE,
        stages: 5,
        defaults: &[800, 1600],
        proto: DpiProto::Block {
            cmd: &[0x0b, 0x00],
            first: 1,
            count: Count::Number,
            wide: false,
        },
    },
    rate: RateSpec {
        cmd: &[0x04, 0x00],
        codes: RATES_DESCENDING,
    },
    light: Some(Light {
        proto: LightProto::FourZone,
        zones: &[
            Zone {
                name: "Front",
                led: 1,
                default: solid(255, 0, 0),
            },
            Zone {
                name: "Middle",
                led: 2,
                default: solid(0, 255, 0),
            },
            Zone {
                name: "Rear",
                led: 3,
                default: solid(0, 0, 255),
            },
            Zone {
                name: "Logo",
                led: 4,
                default: solid(128, 0, 128),
            },
        ],
    }),
    buttons: Some(ButtonSpec {
        cmd: &[0x07, 0x00],
        report: Report::Output,
        // The original Rival 3 has two-byte slots.
        width: 2,
        keyboard: 0x33,
        consumer: 0x34,
        fields: SIX_BUTTONS_AND_WHEEL,
    }),
    battery: None,
    save: SAVE_09_00,
};

const RIVAL_3_WIRELESS: Spec = Spec {
    control: AEROX_CONTROL,
    wire: WIRED,
    dpi: DpiSpec {
        cpi: cpi::AIR,
        stages: 5,
        defaults: &[400, 800, 1200, 2400, 3200],
        proto: DpiProto::Block {
            cmd: &[0x20],
            first: 1,
            count: Count::Number,
            wide: true,
        },
    },
    rate: RateSpec {
        cmd: &[0x17],
        codes: RATES_ZERO_BASED,
    },
    light: None,
    buttons: Some(slots5(
        &[0x19],
        Report::Output,
        &[B1, B2, B3, B4, B5, B6_DPI],
    )),
    battery: Some(BatteryProto::Level),
    save: &[0x09],
};

// ---------------------------------------------------------------------------
// Rival 100 / 110 / Kana v2 (interface 0, one LED, two fixed DPI stages)
// ---------------------------------------------------------------------------

const BACKLIGHT: Light = Light {
    proto: LightProto::Backlight,
    zones: &[Zone {
        name: "Backlight",
        led: 0,
        default: solid(255, 24, 0),
    }],
};

const PER_STAGE_03: DpiProto = DpiProto::PerStage {
    head: &[0x03],
    tail: &[],
};

const RIVAL_100: Spec = Spec {
    control: IFACE_0,
    wire: WIRED,
    dpi: DpiSpec {
        cpi: cpi::RIVAL_100,
        stages: 2,
        defaults: &[1000, 2000],
        proto: PER_STAGE_03,
    },
    rate: RateSpec {
        cmd: &[0x04, 0x00],
        codes: RATES_DESCENDING,
    },
    light: Some(BACKLIGHT),
    buttons: None,
    battery: None,
    save: SAVE_09_00,
};

/// Rival 110/106 and Rival 300S: the Rival 100 protocol in 32-byte packets
/// with the PMW3326 sensor.
const RIVAL_110: Spec = Spec {
    wire: PADDED_32,
    dpi: DpiSpec {
        cpi: cpi::PMW3326,
        stages: 2,
        defaults: &[800, 1600],
        proto: PER_STAGE_03,
    },
    ..RIVAL_100
};

const KANA_V2: Spec = Spec {
    dpi: DpiSpec {
        cpi: cpi::KANA_V2,
        stages: 2,
        defaults: &[800, 1600],
        proto: PER_STAGE_03,
    },
    light: None,
    ..RIVAL_100
};

// ---------------------------------------------------------------------------
// Rival 300 (interface 0, logo and wheel)
// ---------------------------------------------------------------------------

const RIVAL_300: Spec = Spec {
    control: IFACE_0,
    wire: WIRED,
    dpi: DpiSpec {
        cpi: cpi::LINEAR_6500,
        stages: 2,
        defaults: &[800, 1600],
        proto: PER_STAGE_03,
    },
    rate: RateSpec {
        cmd: &[0x04, 0x00],
        codes: RATES_DESCENDING,
    },
    light: Some(Light {
        proto: LightProto::LogoWheel,
        zones: &[
            Zone {
                name: "Logo",
                led: 1,
                default: solid(255, 24, 0),
            },
            Zone {
                name: "Scroll wheel",
                led: 2,
                default: solid(255, 24, 0),
            },
        ],
    }),
    buttons: Some(slots5(&[0x31, 0x00], Report::Output, SIX_BUTTONS)),
    battery: None,
    save: SAVE_09_00,
};

// ---------------------------------------------------------------------------
// Rival 310 / Sensei 310 / Sensei Ten / Rival 600 (interface 0, colour stops)
// ---------------------------------------------------------------------------

const CYCLE_310: Cycle = Cycle {
    cmd: [0x5b, 0x00],
    header: 26,
    leds: &[0],
    duration: 1,
    once: 17,
    count: 25,
};

const CYCLE_600: Cycle = Cycle {
    cmd: [0x05, 0x00],
    header: 28,
    leds: &[0, 5],
    duration: 6,
    once: 22,
    count: 27,
};

const LOGO_AND_WHEEL: &[Zone] = &[
    Zone {
        name: "Logo",
        led: 0,
        default: cycling(1000),
    },
    Zone {
        name: "Scroll wheel",
        led: 1,
        default: cycling(1000),
    },
];

const RIVAL_310: Spec = Spec {
    control: IFACE_0,
    wire: WIRED,
    dpi: DpiSpec {
        cpi: cpi::LINEAR_12000,
        stages: 2,
        defaults: &[800, 1600],
        proto: DpiProto::PerStage {
            head: &[0x53, 0x00],
            tail: &[0x00, 0x42],
        },
    },
    rate: RateSpec {
        cmd: &[0x54, 0x00],
        codes: RATES_DESCENDING,
    },
    light: Some(Light {
        proto: LightProto::Cycle(CYCLE_310),
        zones: LOGO_AND_WHEEL,
    }),
    buttons: Some(slots5(&[0x31, 0x00], Report::Output, SIX_BUTTONS)),
    battery: None,
    save: SAVE_59_00,
};

/// Two extra buttons on the right edge, bound to Page Down / Page Up.
const SENSEI_BUTTONS: &[Field] = &[
    B1,
    B2,
    B3,
    B4,
    B5,
    button(6, "Right side rear", Preset::Key(0x4e), RIGHT_REAR),
    button(7, "Right side front", Preset::Key(0x4b), RIGHT_FRONT),
    button(8, "DPI button", Preset::DpiCycle, DPI),
];

const SENSEI_310: Spec = Spec {
    buttons: Some(slots5(&[0x31, 0x00], Report::Output, SENSEI_BUTTONS)),
    ..RIVAL_310
};

const SENSEI_TEN: Spec = Spec {
    dpi: DpiSpec {
        cpi: cpi::LINEAR_18000,
        stages: 5,
        defaults: &[400, 800, 1200, 2400, 3200],
        proto: DpiProto::Block {
            cmd: &[0x55, 0x00],
            first: 1,
            count: Count::Flags,
            wide: true,
        },
    },
    light: Some(Light {
        proto: LightProto::Cycle(CYCLE_310),
        zones: &[
            Zone {
                name: "Logo",
                led: 0,
                default: cycling(10_000),
            },
            Zone {
                name: "Scroll wheel",
                led: 1,
                default: cycling(10_000),
            },
        ],
    }),
    ..SENSEI_310
};

const RIVAL_600_BUTTONS: &[Field] = &[
    B1,
    B2,
    B3,
    B4,
    B5,
    // Button 6 is disabled out of the box and not documented.
    UNUSED,
    button(7, "DPI button", Preset::DpiCycle, DPI),
];

const fn strip(name: &'static str, led: u8) -> Zone {
    Zone {
        name,
        led,
        default: cycling(1000),
    }
}

const RIVAL_600: Spec = Spec {
    dpi: DpiSpec {
        cpi: cpi::LINEAR_12000,
        stages: 2,
        defaults: &[800, 1600],
        proto: DpiProto::PerStage {
            head: &[0x03, 0x00],
            tail: &[0x00, 0x42],
        },
    },
    rate: RateSpec {
        cmd: &[0x04, 0x00],
        codes: RATES_DESCENDING,
    },
    light: Some(Light {
        proto: LightProto::Cycle(CYCLE_600),
        zones: &[
            strip("Scroll wheel", 0),
            strip("Logo", 1),
            strip("Left strip top", 2),
            strip("Right strip top", 3),
            strip("Left strip middle", 4),
            strip("Right strip middle", 5),
            strip("Left strip bottom", 6),
            strip("Right strip bottom", 7),
        ],
    }),
    buttons: Some(slots5(&[0x31, 0x00], Report::Feature, RIVAL_600_BUTTONS)),
    save: SAVE_09_00,
    ..RIVAL_310
};

// ---------------------------------------------------------------------------
// Prime, Prime+ (interface 0) and Prime Mini (interface 3)
// ---------------------------------------------------------------------------

const PRIME: Spec = Spec {
    control: IFACE_0,
    wire: WIRED,
    dpi: DpiSpec {
        cpi: cpi::LINEAR_18000,
        stages: 5,
        defaults: &[400, 800, 1200, 2400, 3200],
        proto: DpiProto::Block {
            cmd: &[0x61],
            first: 0,
            count: Count::Number,
            wide: true,
        },
    },
    rate: RateSpec {
        cmd: &[0x5d],
        codes: RATES_DESCENDING,
    },
    light: Some(Light {
        proto: LightProto::Prime,
        zones: &[Zone {
            name: "Logo",
            led: 0,
            default: solid(255, 82, 0),
        }],
    }),
    buttons: Some(slots5(&[0x5b], Report::Output, SIX_BUTTONS)),
    battery: None,
    save: &[0x59],
};

const PRIME_MINI: Spec = Spec {
    control: IFACE_3,
    dpi: DpiSpec {
        cpi: cpi::LINEAR_18000,
        stages: 5,
        defaults: &[400, 800, 1200, 2400, 3200],
        proto: DpiProto::Block {
            cmd: &[0x2d],
            first: 0,
            count: Count::Number,
            wide: true,
        },
    },
    rate: RateSpec {
        cmd: &[0x2b],
        codes: RATES_DESCENDING,
    },
    light: Some(Light {
        proto: LightProto::PrimeMini,
        zones: &[Zone {
            name: "Logo",
            led: 0,
            default: solid(255, 0, 0),
        }],
    }),
    buttons: Some(slots5(&[0x2a], Report::Output, SIX_BUTTONS_AND_WHEEL)),
    save: SAVE_11_00,
    ..PRIME
};

// ---------------------------------------------------------------------------
// Aerox and Prime Wireless (interface 3)
// ---------------------------------------------------------------------------

const AEROX_BUTTONS: ButtonSpec = slots5(&[0x2a], Report::Output, SIX_BUTTONS_AND_WHEEL);

const AEROX_3: Spec = Spec {
    control: AEROX_CONTROL,
    wire: WIRED,
    dpi: DpiSpec {
        cpi: cpi::CORE,
        stages: 5,
        defaults: &[800, 1600],
        proto: DpiProto::Block {
            cmd: &[0x2d],
            first: 1,
            count: Count::Number,
            wide: false,
        },
    },
    rate: RateSpec {
        cmd: &[0x2b],
        codes: RATES_DESCENDING,
    },
    light: Some(Light {
        proto: LightProto::Mask,
        zones: RED_GREEN_BLUE,
    }),
    buttons: Some(AEROX_BUTTONS),
    battery: None,
    save: SAVE_11_00,
};

/// Three more slots (buttons 7-9) sit between the DPI button and the wheel.
const AEROX_5_BUTTONS: &[Field] = &[
    B1, B2, B3, B4, B5, B6_DPI, UNUSED, UNUSED, UNUSED, WHEEL_UP, WHEEL_DOWN,
];

const AEROX_5: Spec = Spec {
    dpi: DpiSpec {
        cpi: cpi::AIR,
        stages: 5,
        defaults: &[400, 800, 1200, 2400, 3200],
        proto: DpiProto::Block {
            cmd: &[0x2d],
            first: 0,
            count: Count::Number,
            wide: false,
        },
    },
    buttons: Some(slots5(&[0x2a], Report::Output, AEROX_5_BUTTONS)),
    ..AEROX_3
};

/// The wireless Aeroxes and Prime Wireless in cable mode.
const AEROX_3_WIRELESS: Spec = Spec {
    rate: RateSpec {
        cmd: &[0x2b],
        codes: RATES_ZERO_BASED,
    },
    light: Some(Light {
        proto: LightProto::PerLed,
        zones: RED_GREEN_BLUE,
    }),
    buttons: Some(AEROX_BUTTONS),
    battery: Some(BatteryProto::Status),
    ..AEROX_5
};

const AEROX_3_WIRELESS_DONGLE: Spec = Spec {
    wire: DONGLE,
    ..AEROX_3_WIRELESS
};

const AEROX_5_WIRELESS: Spec = Spec {
    buttons: Some(slots5(&[0x2a], Report::Output, AEROX_5_BUTTONS)),
    ..AEROX_3_WIRELESS
};

const AEROX_5_WIRELESS_DONGLE: Spec = Spec {
    wire: DONGLE,
    ..AEROX_5_WIRELESS
};

/// The Aerox 9 has no button table in rivalcfg.
const AEROX_9_WIRELESS: Spec = Spec {
    buttons: None,
    ..AEROX_3_WIRELESS
};

const AEROX_9_WIRELESS_DONGLE: Spec = Spec {
    wire: DONGLE,
    ..AEROX_9_WIRELESS
};

const PRIME_WIRELESS: Spec = Spec {
    light: Some(Light {
        proto: LightProto::PerLed,
        zones: &[Zone {
            name: "Logo",
            led: 0,
            default: solid(255, 0, 0),
        }],
    }),
    ..AEROX_3_WIRELESS
};

const PRIME_WIRELESS_DONGLE: Spec = Spec {
    wire: DONGLE,
    ..PRIME_WIRELESS
};

// ---------------------------------------------------------------------------
// The product table
// ---------------------------------------------------------------------------

const fn model(pid: u16, name: &'static str, connection: Connection, spec: &'static Spec) -> Model {
    Model {
        pid,
        name,
        connection,
        spec,
    }
}

use Connection::{Wired, Wireless};

/// Every supported product id.
pub static MODELS: &[Model] = &[
    // Rival 3
    model(0x1824, "SteelSeries Rival 3", Wired, &RIVAL_3),
    model(0x184c, "SteelSeries Rival 3", Wired, &RIVAL_3),
    model(
        0x1830,
        "SteelSeries Rival 3 Wireless",
        Wireless,
        &RIVAL_3_WIRELESS,
    ),
    // Rival 100 / 105 / Kana v2
    model(0x1702, "SteelSeries Rival 100", Wired, &RIVAL_100),
    model(0x170a, "SteelSeries Rival 100 (Dell)", Wired, &RIVAL_100),
    model(
        0x170b,
        "SteelSeries Rival 100 Dota 2 Edition",
        Wired,
        &RIVAL_100,
    ),
    model(
        0x170c,
        "SteelSeries Rival 100 Dota 2 Edition",
        Wired,
        &RIVAL_100,
    ),
    model(0x1814, "SteelSeries Rival 105", Wired, &RIVAL_100),
    model(0x137a, "SteelSeries Kana v2", Wired, &KANA_V2),
    // Rival 110 / 106 / 300S
    model(0x1729, "SteelSeries Rival 110", Wired, &RIVAL_110),
    model(0x1816, "SteelSeries Rival 106", Wired, &RIVAL_110),
    model(0x1810, "SteelSeries Rival 300S", Wired, &RIVAL_110),
    // Rival 300 and the original Rival
    model(0x1384, "SteelSeries Rival", Wired, &RIVAL_300),
    model(
        0x1392,
        "SteelSeries Rival Dota 2 Edition",
        Wired,
        &RIVAL_300,
    ),
    model(0x1710, "SteelSeries Rival 300", Wired, &RIVAL_300),
    model(
        0x1712,
        "SteelSeries Rival 300 Fallout 4 Edition",
        Wired,
        &RIVAL_300,
    ),
    model(
        0x171c,
        "SteelSeries Rival 300 Evil Geniuses Edition",
        Wired,
        &RIVAL_300,
    ),
    model(
        0x1394,
        "SteelSeries Rival 300 CS:GO Fade Edition",
        Wired,
        &RIVAL_300,
    ),
    model(
        0x171a,
        "SteelSeries Rival 300 CS:GO Hyper Beast Edition",
        Wired,
        &RIVAL_300,
    ),
    model(
        0x1716,
        "SteelSeries Rival 300 CS:GO Fade Edition",
        Wired,
        &RIVAL_300,
    ),
    model(
        0x1714,
        "SteelSeries Rival 300 Acer Predator Edition",
        Wired,
        &RIVAL_300,
    ),
    model(
        0x1718,
        "SteelSeries Rival 300 HP OMEN Edition",
        Wired,
        &RIVAL_300,
    ),
    // Rival 310 / Sensei 310 / Sensei Ten / Rival 600
    model(0x1720, "SteelSeries Rival 310", Wired, &RIVAL_310),
    model(
        0x171e,
        "SteelSeries Rival 310 CS:GO Howl Edition",
        Wired,
        &RIVAL_310,
    ),
    model(
        0x1736,
        "SteelSeries Rival 310 PUBG Edition",
        Wired,
        &RIVAL_310,
    ),
    model(0x1722, "SteelSeries Sensei 310", Wired, &SENSEI_310),
    model(0x1832, "SteelSeries Sensei Ten", Wired, &SENSEI_TEN),
    model(
        0x1834,
        "SteelSeries Sensei Ten CS:GO Neon Rider Edition",
        Wired,
        &SENSEI_TEN,
    ),
    model(0x1724, "SteelSeries Rival 600", Wired, &RIVAL_600),
    model(
        0x172e,
        "SteelSeries Rival 600 Dota 2 Edition",
        Wired,
        &RIVAL_600,
    ),
    // Prime, Prime+, Prime Mini
    model(0x182e, "SteelSeries Prime", Wired, &PRIME),
    model(
        0x182a,
        "SteelSeries Prime Rainbow 6 Siege Black Ice Edition",
        Wired,
        &PRIME,
    ),
    model(
        0x1856,
        "SteelSeries Prime CS:GO Neo Noir Edition",
        Wired,
        &PRIME,
    ),
    model(0x182c, "SteelSeries Prime+", Wired, &PRIME),
    model(0x184d, "SteelSeries Prime Mini", Wired, &PRIME_MINI),
    // Aerox 3 and 5 (wired)
    model(0x1836, "SteelSeries Aerox 3", Wired, &AEROX_3),
    model(0x1850, "SteelSeries Aerox 5", Wired, &AEROX_5),
    // Aerox 3 Wireless
    model(
        0x183a,
        "SteelSeries Aerox 3 Wireless",
        Wired,
        &AEROX_3_WIRELESS,
    ),
    model(
        0x187a,
        "SteelSeries Aerox 3 Wireless CS2 Dragon Lore Edition",
        Wired,
        &AEROX_3_WIRELESS,
    ),
    model(
        0x1838,
        "SteelSeries Aerox 3 Wireless",
        Wireless,
        &AEROX_3_WIRELESS_DONGLE,
    ),
    model(
        0x1878,
        "SteelSeries Aerox 3 Wireless CS2 Dragon Lore Edition",
        Wireless,
        &AEROX_3_WIRELESS_DONGLE,
    ),
    // Aerox 5 Wireless
    model(
        0x1854,
        "SteelSeries Aerox 5 Wireless",
        Wired,
        &AEROX_5_WIRELESS,
    ),
    model(
        0x185e,
        "SteelSeries Aerox 5 Wireless Destiny 2 Edition",
        Wired,
        &AEROX_5_WIRELESS,
    ),
    model(
        0x1862,
        "SteelSeries Aerox 5 Wireless Diablo IV Edition",
        Wired,
        &AEROX_5_WIRELESS,
    ),
    model(
        0x1852,
        "SteelSeries Aerox 5 Wireless",
        Wireless,
        &AEROX_5_WIRELESS_DONGLE,
    ),
    model(
        0x185c,
        "SteelSeries Aerox 5 Wireless Destiny 2 Edition",
        Wireless,
        &AEROX_5_WIRELESS_DONGLE,
    ),
    model(
        0x1860,
        "SteelSeries Aerox 5 Wireless Diablo IV Edition",
        Wireless,
        &AEROX_5_WIRELESS_DONGLE,
    ),
    // Aerox 9 Wireless
    model(
        0x185a,
        "SteelSeries Aerox 9 Wireless",
        Wired,
        &AEROX_9_WIRELESS,
    ),
    model(
        0x1876,
        "SteelSeries Aerox 9 Wireless WoW Edition",
        Wired,
        &AEROX_9_WIRELESS,
    ),
    model(
        0x1858,
        "SteelSeries Aerox 9 Wireless",
        Wireless,
        &AEROX_9_WIRELESS_DONGLE,
    ),
    model(
        0x1874,
        "SteelSeries Aerox 9 Wireless WoW Edition",
        Wireless,
        &AEROX_9_WIRELESS_DONGLE,
    ),
    // Prime Wireless and Prime Mini Wireless
    model(0x1842, "SteelSeries Prime Wireless", Wired, &PRIME_WIRELESS),
    model(
        0x184a,
        "SteelSeries Prime Mini Wireless",
        Wired,
        &PRIME_WIRELESS,
    ),
    model(
        0x1840,
        "SteelSeries Prime Wireless",
        Wireless,
        &PRIME_WIRELESS_DONGLE,
    ),
    model(
        0x1848,
        "SteelSeries Prime Mini Wireless",
        Wireless,
        &PRIME_WIRELESS_DONGLE,
    ),
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn product_ids_are_unique() {
        let mut seen = HashSet::new();
        for m in MODELS {
            assert!(seen.insert(m.pid), "duplicate pid {:04x}", m.pid);
        }
    }

    #[test]
    fn factory_dpi_matches_the_sensor() {
        for m in MODELS {
            let d = &m.spec.dpi;
            assert!(!d.defaults.is_empty() && d.defaults.len() <= usize::from(d.stages));
            for &dpi in d.defaults {
                assert_eq!(d.cpi.resolve(dpi).unwrap().0, dpi, "{} {dpi}", m.name);
            }
            // Per-stage mice always hold exactly two presets.
            if matches!(d.proto, DpiProto::PerStage { .. }) {
                assert_eq!(d.stages, 2, "{}", m.name);
            }
            // One-byte codes must fit.
            if let DpiProto::Block { wide: false, .. } = d.proto {
                for dpi in (d.cpi.min..=d.cpi.max).step_by(usize::from(d.cpi.step)) {
                    assert!(d.cpi.resolve(dpi).unwrap().1 <= 0xff, "{}", m.name);
                }
            }
        }
    }

    #[test]
    fn rates_are_ascending_and_include_1000_hz() {
        for m in MODELS {
            let rates = m.spec.rate.codes;
            assert!(rates.windows(2).all(|w| w[0].0 < w[1].0), "{}", m.name);
            assert!(rates.iter().any(|r| r.0 == 1000), "{}", m.name);
        }
    }

    #[test]
    fn factory_lighting_is_something_the_zone_can_show() {
        for m in MODELS {
            let Some(light) = &m.spec.light else { continue };
            let mut leds = HashSet::new();
            for z in light.zones {
                assert!(light.effects().contains(&z.default.kind()), "{}", m.name);
                assert!(leds.insert(z.led), "{} repeats LED {}", m.name, z.led);
            }
        }
    }

    #[test]
    fn button_tables_are_consistent() {
        for m in MODELS {
            let Some(b) = &m.spec.buttons else { continue };
            let mut ids = HashSet::new();
            for f in b.fields {
                match f {
                    Field::Button(btn) => {
                        assert!(ids.insert(btn.id), "{} repeats button {}", m.name, btn.id);
                        assert!((0.0..=1.0).contains(&btn.hotspot.0));
                        assert!((0.0..=1.0).contains(&btn.hotspot.1));
                    }
                    Field::Fixed(bytes) => assert!(bytes.len() <= b.width),
                }
            }
            // Left click must stay reachable.
            assert!(ids.contains(&1), "{}", m.name);
        }
    }

    #[test]
    fn dongle_models_are_the_wireless_ones_with_the_flag() {
        for m in MODELS {
            let wireless = m.connection == Connection::Wireless;
            // Rival 3 Wireless speaks its own commands without the flag.
            let flagged = m.spec.wire.dongle;
            assert_eq!(wireless, flagged || m.pid == 0x1830, "{}", m.name);
        }
    }

    fn node(pid: u16, interface: i32, page: u16, usage: u16) -> HidInterfaceInfo {
        HidInterfaceInfo {
            path: format!("hid-{pid:04x}-{interface}-{page:04x}-{usage}"),
            vendor_id: VID_STEELSERIES,
            product_id: pid,
            interface_number: interface,
            usage_page: page,
            usage,
            ..Default::default()
        }
    }

    #[test]
    fn linux_picks_the_control_interface_by_number() {
        // Aerox 3: one hidraw node per USB interface.
        let nodes = [
            node(0x1836, 0, 0x0001, 0x0002),
            node(0x1836, 1, 0x0001, 0x0006),
            node(0x1836, 2, 0x000c, 0x0001),
            node(0x1836, 3, 0xffc0, 0x0001),
        ];
        let claimed: Vec<bool> = nodes
            .iter()
            .map(|n| for_interface(n, false).is_some())
            .collect();
        assert_eq!(claimed, [false, false, false, true]);
    }

    #[test]
    fn windows_claims_only_the_vendor_collection() {
        // The same interface 3 listed once per collection.
        let nodes = [
            node(0x1836, 3, 0xffc0, 0x0001),
            node(0x1836, 3, 0xffc0, 0x0002),
            node(0x1836, 3, 0x0001, 0x0006),
        ];
        let claimed: Vec<bool> = nodes
            .iter()
            .map(|n| for_interface(n, true).is_some())
            .collect();
        assert_eq!(claimed, [true, false, false]);

        // Rival 310 has no confirmed usage: any vendor-defined page, never the mouse.
        assert!(for_interface(&node(0x1720, 0, 0xff00, 0x0001), true).is_some());
        assert!(for_interface(&node(0x1720, 0, 0x0001, 0x0002), true).is_none());
        assert!(for_interface(&node(0x1720, 0, 0x0001, 0x0002), false).is_some());
    }

    #[test]
    fn entries_without_usage_info_match_on_the_interface() {
        assert!(for_interface(&node(0x1836, 3, 0, 0), true).is_some());
        assert!(for_interface(&node(0x1836, 2, 0, 0), true).is_none());
    }

    #[test]
    fn unknown_interfaces_and_foreign_devices_are_ignored() {
        // No USB interface number (Bluetooth, odd backends).
        assert!(for_interface(&node(0x1836, -1, 0xffc0, 1), false).is_none());
        assert!(for_interface(&node(0x0001, 3, 0xffc0, 1), false).is_none());
        let other = HidInterfaceInfo {
            vendor_id: 0x046d,
            ..node(0x1836, 3, 0xffc0, 1)
        };
        assert!(for_interface(&other, false).is_none());
    }
}
