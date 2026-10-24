//! Razer commands: request builders and response parsers.
//!
//! Every command is a (class, id) pair; ids with bit 7 set read a value.
//! Byte layouts follow the openrazer kernel driver (`razerchromacommon.c`)
//! and its captures; only the commands this crate uses are covered.

use gearwise_core::model::{Battery, DpiStage, DpiState, LightingEffect, Rgb, WaveDirection};

use crate::report::{MAX_ARGS, Report};

/// Storage selector (first argument of most commands): apply without
/// persisting.
pub const NOSTORE: u8 = 0x00;
/// Storage selector: persist the setting in the device.
pub const VARSTORE: u8 = 0x01;

/// LED ids used by the lighting commands. They double as zone ids in the
/// capabilities exposed to the app.
pub mod led {
    pub const SCROLL_WHEEL: u8 = 0x01;
    pub const LOGO: u8 = 0x04;
    pub const BACKLIGHT: u8 = 0x05;
    /// The game-mode indicator; toggling it also toggles the mode.
    pub const GAME: u8 = 0x08;
}

/// Most DPI stages the stage table can hold.
pub const MAX_STAGES: usize = 5;

/// Which polling-rate command family a device speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollProto {
    /// Class 0, id 0x05/0x85: 125, 500 and 1000 Hz only.
    Legacy,
    /// Class 0, id 0x40/0xc0: 125 Hz to 8 kHz. Mice send the command twice
    /// (selector 0 then 1) as Synapse does; keyboards once.
    Hyper { follow_up: bool },
}

/// Whether a setting is written to the device's persistent memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Store {
    No,
    Var,
}

impl Store {
    fn byte(self) -> u8 {
        match self {
            Store::No => NOSTORE,
            Store::Var => VARSTORE,
        }
    }
}

/// How a device encodes wave directions (values differ between families).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveCodes {
    /// Code for [`WaveDirection::LeftToRight`], called "right" by openrazer.
    pub right: u8,
    /// Code for [`WaveDirection::RightToLeft`].
    pub left: u8,
}

impl WaveCodes {
    /// Most devices: right = 1, left = 2.
    pub const STANDARD: Self = Self { right: 1, left: 2 };
    /// Older keyboards (Huntsman family): right = 1, left = 0.
    pub const ZERO_BASED: Self = Self { right: 1, left: 0 };
    /// Ornata V3: the standard values swapped.
    pub const FLIPPED: Self = Self { right: 2, left: 1 };
}

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/// Class 0x00, id 0x82: 22-byte ASCII serial.
pub fn serial() -> Report {
    Report::new(0x00, 0x82, 0x16, &[])
}

/// Class 0x00, id 0x81: firmware major, minor in the first two bytes.
pub fn firmware() -> Report {
    Report::new(0x00, 0x81, 0x02, &[])
}

pub fn parse_serial(r: &Report) -> Option<String> {
    let s: String = r.args[..22]
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect();
    let s = s.trim();
    (!s.is_empty() && s.chars().all(|c| c.is_ascii_graphic())).then(|| s.to_string())
}

pub fn parse_firmware(r: &Report) -> String {
    format!("v{}.{}", r.args[0], r.args[1])
}

// ---------------------------------------------------------------------------
// Polling rate
// ---------------------------------------------------------------------------

const LEGACY_RATES: [(u16, u8); 3] = [(1000, 0x01), (500, 0x02), (125, 0x08)];
const HYPER_RATES: [(u16, u8); 7] = [
    (8000, 0x01),
    (4000, 0x02),
    (2000, 0x04),
    (1000, 0x08),
    (500, 0x10),
    (250, 0x20),
    (125, 0x40),
];

fn rate_table(proto: PollProto) -> &'static [(u16, u8)] {
    match proto {
        PollProto::Legacy => &LEGACY_RATES,
        PollProto::Hyper { .. } => &HYPER_RATES,
    }
}

/// Requests that set the polling rate, in the order they must be sent;
/// `None` if the rate has no code in this family.
pub fn set_polling(proto: PollProto, hz: u16) -> Option<Vec<Report>> {
    let code = rate_table(proto).iter().find(|(r, _)| *r == hz)?.1;
    Some(match proto {
        PollProto::Legacy => vec![Report::new(0x00, 0x05, 0x01, &[code])],
        PollProto::Hyper { follow_up } => {
            let selectors: &[u8] = if follow_up { &[0x00, 0x01] } else { &[0x00] };
            selectors
                .iter()
                .map(|&sel| Report::new(0x00, 0x40, 0x02, &[sel, code]))
                .collect()
        }
    })
}

pub fn get_polling(proto: PollProto) -> Report {
    match proto {
        PollProto::Legacy => Report::new(0x00, 0x85, 0x01, &[]),
        PollProto::Hyper { .. } => Report::new(0x00, 0xc0, 0x01, &[]),
    }
}

/// The code sits in the first argument for the legacy family and in the
/// second for the newer one.
pub fn parse_polling(proto: PollProto, r: &Report) -> Option<u16> {
    let code = match proto {
        PollProto::Legacy => r.args[0],
        PollProto::Hyper { .. } => r.args[1],
    };
    rate_table(proto)
        .iter()
        .find(|(_, c)| *c == code)
        .map(|(hz, _)| *hz)
}

// ---------------------------------------------------------------------------
// DPI
// ---------------------------------------------------------------------------

/// Class 0x04, id 0x05: set X/Y resolution (big endian).
pub fn set_dpi(s: DpiStage) -> Report {
    let (x, y) = (s.x.to_be_bytes(), s.y.to_be_bytes());
    Report::new(0x04, 0x05, 0x07, &[VARSTORE, x[0], x[1], y[0], y[1], 0, 0])
}

pub fn get_dpi() -> Report {
    Report::new(0x04, 0x85, 0x07, &[NOSTORE])
}

fn be(hi: u8, lo: u8) -> u16 {
    u16::from_be_bytes([hi, lo])
}

pub fn parse_dpi(r: &Report) -> DpiStage {
    DpiStage {
        x: be(r.args[1], r.args[2]),
        y: be(r.args[3], r.args[4]),
    }
}

/// Class 0x04, id 0x06: write the whole stage table.
///
/// Arguments: storage, active stage (1-based), stage count, then per stage
/// seven bytes: index, X (2), Y (2), two reserved. The packet size is fixed
/// at room for five stages.
pub fn set_dpi_stages(state: &DpiState) -> Report {
    let mut a = [0u8; 3 + 7 * MAX_STAGES];
    a[0] = VARSTORE;
    a[1] = state.active + 1;
    a[2] = state.stages.len() as u8;
    for (i, s) in state.stages.iter().enumerate().take(MAX_STAGES) {
        let o = 3 + 7 * i;
        a[o] = i as u8;
        a[o + 1..o + 3].copy_from_slice(&s.x.to_be_bytes());
        a[o + 3..o + 5].copy_from_slice(&s.y.to_be_bytes());
    }
    Report::new(0x04, 0x06, a.len() as u8, &a)
}

pub fn get_dpi_stages() -> Report {
    Report::new(0x04, 0x86, 0x26, &[VARSTORE])
}

/// Parse the stage table; `None` when the device reports no stages.
pub fn parse_dpi_stages(r: &Report) -> Option<DpiState> {
    let (active, count) = (r.args[1] as usize, r.args[2] as usize);
    if count == 0 || count > MAX_STAGES {
        return None;
    }
    let stages = (0..count)
        .map(|i| {
            let o = 3 + 7 * i;
            DpiStage {
                x: be(r.args[o + 1], r.args[o + 2]),
                y: be(r.args[o + 3], r.args[o + 4]),
            }
        })
        .collect();
    Some(DpiState {
        stages,
        active: active.saturating_sub(1).min(count - 1) as u8,
    })
}

// ---------------------------------------------------------------------------
// Battery
// ---------------------------------------------------------------------------

/// Class 0x07, id 0x80: charge level 0-255 in the second argument.
pub fn battery_level() -> Report {
    Report::new(0x07, 0x80, 0x02, &[])
}

/// Class 0x07, id 0x84: 1 in the second argument while charging.
pub fn charging() -> Report {
    Report::new(0x07, 0x84, 0x02, &[])
}

pub fn parse_battery(level: &Report, charging: &Report) -> Battery {
    Battery {
        percent: Some(byte_to_percent(level.args[1])),
        charging: charging.args[1] != 0,
    }
}

// ---------------------------------------------------------------------------
// Lighting (extended matrix, class 0x0f)
// ---------------------------------------------------------------------------

/// 0-100 % to the 0-255 scale the device uses.
pub fn percent_to_byte(p: u8) -> u8 {
    ((p.min(100) as u16 * 255 + 50) / 100) as u8
}

pub fn byte_to_percent(b: u8) -> u8 {
    ((b as u16 * 100 + 127) / 255) as u8
}

/// Class 0x0f, id 0x04: brightness of one LED.
pub fn set_brightness(led: u8, percent: u8) -> Report {
    Report::new(0x0f, 0x04, 0x03, &[VARSTORE, led, percent_to_byte(percent)])
}

pub fn get_brightness(led: u8) -> Report {
    Report::new(0x0f, 0x84, 0x03, &[VARSTORE, led])
}

pub fn parse_brightness(r: &Report) -> u8 {
    byte_to_percent(r.args[2])
}

/// Reactive effects take 1-4, meaning 0.5-2 s of fade.
fn reactive_speed(duration_ms: u16) -> u8 {
    ((duration_ms as u32 + 250) / 500).clamp(1, 4) as u8
}

/// Wave speed byte: lower is faster. 0x28 is the default Synapse sends and
/// 0x10 its "fast wave"; periods are scaled between the two (3 s = default).
fn wave_speed(period_ms: u16) -> u8 {
    (period_ms as u32 * 0x28 / 3000).clamp(0x10, 0x28) as u8
}

/// Class 0x0f, id 0x02: select an effect on one LED.
///
/// Arguments: storage, LED, effect id, then effect parameters. Effect ids:
/// 0 off, 1 static, 2 breathing, 3 spectrum, 4 wave, 5 reactive.
pub fn effect(led: u8, fx: &LightingEffect, wave: WaveCodes) -> Report {
    let s = VARSTORE;
    match *fx {
        LightingEffect::Off => Report::new(0x0f, 0x02, 0x06, &[s, led, 0x00]),
        LightingEffect::Static { color: c } => {
            Report::new(0x0f, 0x02, 0x09, &[s, led, 0x01, 0, 0, 1, c.r, c.g, c.b])
        }
        // One colour: type 1, then the count and the colour.
        LightingEffect::Breathing { color: c, .. } => {
            Report::new(0x0f, 0x02, 0x09, &[s, led, 0x02, 1, 0, 1, c.r, c.g, c.b])
        }
        LightingEffect::ColorCycle { .. } => Report::new(0x0f, 0x02, 0x06, &[s, led, 0x03]),
        LightingEffect::Wave {
            period_ms,
            direction,
        } => {
            let dir = match direction {
                WaveDirection::LeftToRight => wave.right,
                WaveDirection::RightToLeft => wave.left,
            };
            Report::new(
                0x0f,
                0x02,
                0x06,
                &[s, led, 0x04, dir, wave_speed(period_ms), 0],
            )
        }
        LightingEffect::Reactive {
            color: c,
            duration_ms,
        } => Report::new(
            0x0f,
            0x02,
            0x09,
            &[
                s,
                led,
                0x05,
                0,
                reactive_speed(duration_ms),
                1,
                c.r,
                c.g,
                c.b,
            ],
        ),
    }
}

/// Class 0x0f, id 0x03: upload one row of the custom frame.
///
/// Arguments: two zero bytes, row, first column, last column, then RGB
/// triplets. The size byte is 0x47 on every device openrazer drives this way.
pub fn frame_row(row: u8, colors: &[Rgb]) -> Report {
    debug_assert!(!colors.is_empty() && 5 + 3 * colors.len() <= MAX_ARGS);
    let mut a = [0u8; MAX_ARGS];
    a[2] = row;
    a[4] = (colors.len() - 1) as u8;
    for (i, c) in colors.iter().enumerate() {
        a[5 + 3 * i..8 + 3 * i].copy_from_slice(&[c.r, c.g, c.b]);
    }
    Report::new(0x0f, 0x03, 0x47, &a)
}

/// Class 0x0f, id 0x02, effect 8: display the uploaded custom frame.
pub fn draw_frame() -> Report {
    Report::new(0x0f, 0x02, 0x0c, &[NOSTORE, 0x00, 0x08])
}

// ---------------------------------------------------------------------------
// Game mode
// ---------------------------------------------------------------------------

/// Class 0x03, id 0x00: switch the game LED, which is the game mode.
pub fn set_game_mode(store: Store, on: bool) -> Report {
    Report::new(0x03, 0x00, 0x03, &[store.byte(), led::GAME, on as u8])
}

pub fn get_game_mode(store: Store) -> Report {
    Report::new(0x03, 0x80, 0x03, &[store.byte(), led::GAME])
}

pub fn parse_game_mode(r: &Report) -> bool {
    r.args[2] != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(mut r: Report, tid: u8) -> Vec<u8> {
        r.transaction_id = tid;
        r.to_wire().to_vec()
    }

    /// Pad `head` with zeros to the 91-byte wire length and set the checksum.
    fn wire(head: &[u8], crc: u8) -> Vec<u8> {
        let mut v = head.to_vec();
        v.resize(91, 0);
        v[89] = crc;
        v
    }

    #[test]
    fn dpi_set_800_by_1600() {
        // 00 | 00 1f 0000 00 07 04 05 | 01 03 20 06 40 00 00
        // crc = 07 ^ 04 ^ 05 ^ 01 ^ 03 ^ 20 ^ 06 ^ 40 = 0x62
        let got = req(set_dpi(DpiStage { x: 800, y: 1600 }), 0x1f);
        let want = wire(
            &[
                0, 0, 0x1f, 0, 0, 0, 0x07, 0x04, 0x05, 0x01, 0x03, 0x20, 0x06, 0x40,
            ],
            0x62,
        );
        assert_eq!(got, want);
    }

    #[test]
    fn dpi_stage_table_with_second_stage_active() {
        let state = DpiState {
            stages: vec![
                DpiStage::uniform(800),
                DpiStage::uniform(1800),
                DpiStage { x: 3200, y: 6400 },
            ],
            active: 1,
        };
        // args: store 01, active 02 (1-based), count 03,
        //   stage 0: 00 0320 0320 0000
        //   stage 1: 01 0708 0708 0000
        //   stage 2: 02 0c80 1900 0000
        // data size 0x26 is fixed.
        // crc: header 26 ^ 04 ^ 06 = 0x24; arguments 01 ^ 02 ^ 03 = 0, stage 0 = 0,
        // stage 1 = 0x01, stage 2 = 02 ^ 0c ^ 80 ^ 19 = 0x97; 0x24 ^ 0x01 ^ 0x97 = 0xb2.
        let head = [
            0, 0, 0x3f, 0, 0, 0, 0x26, 0x04, 0x06, 0x01, 0x02, 0x03, 0x00, 0x03, 0x20, 0x03, 0x20,
            0, 0, 0x01, 0x07, 0x08, 0x07, 0x08, 0, 0, 0x02, 0x0c, 0x80, 0x19, 0x00,
        ];
        assert_eq!(req(set_dpi_stages(&state), 0x3f), wire(&head, 0xb2));
    }

    #[test]
    fn polling_rate_codes() {
        let one = |proto, hz| {
            let v = set_polling(proto, hz).unwrap();
            assert_eq!(v.len(), 1, "{hz} Hz");
            v.into_iter().next().unwrap()
        };
        let r = one(PollProto::Legacy, 1000);
        assert_eq!(
            (r.class, r.command, r.data_size, r.args[0]),
            (0x00, 0x05, 0x01, 0x01)
        );
        assert_eq!(one(PollProto::Legacy, 500).args[0], 0x02);
        assert_eq!(one(PollProto::Legacy, 125).args[0], 0x08);
        assert!(set_polling(PollProto::Legacy, 2000).is_none());

        let r = one(PollProto::Hyper { follow_up: false }, 4000);
        assert_eq!((r.class, r.command, r.data_size), (0x00, 0x40, 0x02));
        assert_eq!(&r.args[..2], &[0x00, 0x02]);
        let both = set_polling(PollProto::Hyper { follow_up: true }, 8000).unwrap();
        assert_eq!(both.len(), 2);
        assert_eq!(
            (&both[0].args[..2], &both[1].args[..2]),
            (&[0x00, 0x01][..], &[0x01, 0x01][..])
        );
    }

    #[test]
    fn polling_request_bytes() {
        // 00 | 00 1f 0000 00 01 00 05 | 02    crc = 01 ^ 00 ^ 05 ^ 02 = 0x06
        let got = req(set_polling(PollProto::Legacy, 500).unwrap().remove(0), 0x1f);
        assert_eq!(
            got,
            wire(&[0, 0, 0x1f, 0, 0, 0, 0x01, 0x00, 0x05, 0x02], 0x06)
        );
    }

    #[test]
    fn static_effect_matches_openrazer_capture() {
        // openrazer documents: 00 3f 0000 00 09 0f 02 010501000001ff0000
        // (store 01, backlight 05, static 01, 00 00, 01, red)
        // crc, byte by byte: 09^0f=06, ^02=04, ^01=05, ^05=00, ^01=01, ^01=00, ^ff=ff -> 0xff
        let fx = LightingEffect::Static {
            color: Rgb::new(255, 0, 0),
        };
        let got = req(effect(led::BACKLIGHT, &fx, WaveCodes::STANDARD), 0x3f);
        let want = wire(
            &[
                0, 0, 0x3f, 0, 0, 0, 0x09, 0x0f, 0x02, 0x01, 0x05, 0x01, 0, 0, 0x01, 0xff, 0, 0,
            ],
            0xff,
        );
        assert_eq!(got, want);
    }

    #[test]
    fn wave_effect_matches_openrazer_capture() {
        // openrazer documents: 00 3f 0000 00 06 0f 02 010504012800
        // crc, byte by byte: 06^0f=09, ^02=0b, ^01=0a, ^05=0f, ^04=0b, ^01=0a, ^28=22 -> 0x22
        let fx = LightingEffect::Wave {
            period_ms: 3000,
            direction: WaveDirection::LeftToRight,
        };
        let got = req(effect(led::BACKLIGHT, &fx, WaveCodes::STANDARD), 0x3f);
        assert_eq!(
            got,
            wire(
                &[
                    0, 0, 0x3f, 0, 0, 0, 0x06, 0x0f, 0x02, 0x01, 0x05, 0x04, 0x01, 0x28, 0x00
                ],
                0x22
            )
        );
    }

    #[test]
    fn brightness_is_scaled_to_a_byte() {
        // 00 | 00 3f 0000 00 03 0f 04 | 01 04 ff   crc = 03^0f^04^01^04^ff = 0xf2
        let got = req(set_brightness(led::LOGO, 100), 0x3f);
        assert_eq!(
            got,
            wire(
                &[0, 0, 0x3f, 0, 0, 0, 0x03, 0x0f, 0x04, 0x01, 0x04, 0xff],
                0xf2
            )
        );
        assert_eq!(percent_to_byte(0), 0);
        assert_eq!(percent_to_byte(50), 128);
        assert_eq!(percent_to_byte(200), 255);
    }

    #[test]
    fn percent_byte_round_trip() {
        for p in 0..=100u8 {
            assert_eq!(byte_to_percent(percent_to_byte(p)), p);
        }
    }

    #[test]
    fn other_effect_encodings() {
        let c = Rgb::new(1, 2, 3);
        let w = WaveCodes::ZERO_BASED;
        let a = |fx: LightingEffect| effect(led::LOGO, &fx, w);
        assert_eq!(
            (
                a(LightingEffect::Off).data_size,
                &a(LightingEffect::Off).args[..3]
            ),
            (6, &[1, 4, 0][..])
        );
        let b = a(LightingEffect::Breathing {
            color: c,
            period_ms: 4000,
        });
        assert_eq!(
            (b.data_size, &b.args[..9]),
            (9, &[1, 4, 2, 1, 0, 1, 1, 2, 3][..])
        );
        let s = a(LightingEffect::ColorCycle { period_ms: 10_000 });
        assert_eq!((s.data_size, &s.args[..3]), (6, &[1, 4, 3][..]));
        let r = a(LightingEffect::Reactive {
            color: c,
            duration_ms: 1000,
        });
        assert_eq!(
            (r.data_size, &r.args[..9]),
            (9, &[1, 4, 5, 0, 2, 1, 1, 2, 3][..])
        );
        let l = a(LightingEffect::Wave {
            period_ms: 1000,
            direction: WaveDirection::RightToLeft,
        });
        assert_eq!(&l.args[..6], &[1, 4, 4, 0, 0x10, 0]);
    }

    #[test]
    fn reactive_and_wave_speeds_are_clamped() {
        assert_eq!(
            [0, 500, 1000, 1500, 2000, 60_000].map(reactive_speed),
            [1, 1, 2, 3, 4, 4]
        );
        assert_eq!(
            [0, 1200, 3000, 10_000].map(wave_speed),
            [0x10, 0x10, 0x28, 0x28]
        );
    }

    #[test]
    fn custom_frame_row_layout() {
        let colors = [Rgb::new(1, 2, 3), Rgb::new(4, 5, 6), Rgb::new(7, 8, 9)];
        let r = frame_row(2, &colors);
        assert_eq!((r.class, r.command, r.data_size), (0x0f, 0x03, 0x47));
        assert_eq!(&r.args[..14], &[0, 0, 2, 0, 2, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        let d = draw_frame();
        assert_eq!(
            (d.class, d.command, d.data_size, &d.args[..3]),
            (0x0f, 0x02, 0x0c, &[0, 0, 8][..])
        );
    }

    #[test]
    fn game_mode_round_trip() {
        let r = set_game_mode(Store::Var, true);
        assert_eq!(
            (r.class, r.command, &r.args[..3]),
            (0x03, 0x00, &[1, 8, 1][..])
        );
        assert_eq!(&get_game_mode(Store::No).args[..2], &[0, 8]);
        let mut resp = get_game_mode(Store::Var);
        resp.args[2] = 1;
        assert!(parse_game_mode(&resp));
    }

    fn response(args: &[u8]) -> Report {
        Report::new(0, 0, args.len() as u8, args)
    }

    #[test]
    fn parses_canned_dpi_response() {
        // storage, X = 0x0320 (800), Y = 0x0640 (1600)
        assert_eq!(
            parse_dpi(&response(&[0, 0x03, 0x20, 0x06, 0x40, 0, 0])),
            DpiStage { x: 800, y: 1600 }
        );
    }

    #[test]
    fn parses_canned_stage_table() {
        // The example documented in openrazer's driver: active 2 of 3 stages.
        let r = response(&[
            1, 2, 3, //
            1, 0x03, 0x20, 0x03, 0x20, 0, 0, //
            2, 0x07, 0x08, 0x07, 0x08, 0, 0, //
            3, 0x0c, 0x80, 0x0c, 0x80, 0, 0,
        ]);
        let st = parse_dpi_stages(&r).unwrap();
        assert_eq!(st.active, 1);
        assert_eq!(
            st.stages,
            vec![
                DpiStage::uniform(800),
                DpiStage::uniform(1800),
                DpiStage::uniform(3200)
            ]
        );
        assert!(parse_dpi_stages(&response(&[1, 1, 0])).is_none());
        assert!(parse_dpi_stages(&response(&[1, 1, 9])).is_none());
    }

    #[test]
    fn parses_canned_battery_and_charging() {
        let level = response(&[0, 0x80]);
        let charging = response(&[0, 1]);
        assert_eq!(
            parse_battery(&level, &charging),
            Battery {
                percent: Some(50),
                charging: true
            }
        );
        assert_eq!(
            parse_battery(&response(&[0, 0xff]), &response(&[0, 0])).percent,
            Some(100)
        );
    }

    #[test]
    fn parses_canned_firmware_and_serial() {
        assert_eq!(parse_firmware(&response(&[1, 12])), "v1.12");
        let mut s = [0u8; 22];
        s[..14].copy_from_slice(b"PM2145H12345AB");
        assert_eq!(
            parse_serial(&response(&s)).as_deref(),
            Some("PM2145H12345AB")
        );
        assert_eq!(parse_serial(&response(&[0u8; 22])), None);
        assert_eq!(parse_serial(&response(&[0xff; 22])), None);
    }

    #[test]
    fn parses_polling_codes() {
        assert_eq!(
            parse_polling(PollProto::Legacy, &response(&[0x08])),
            Some(125)
        );
        assert_eq!(
            parse_polling(PollProto::Hyper { follow_up: true }, &response(&[0, 0x04])),
            Some(2000)
        );
        assert_eq!(parse_polling(PollProto::Legacy, &response(&[0x77])), None);
    }
}
