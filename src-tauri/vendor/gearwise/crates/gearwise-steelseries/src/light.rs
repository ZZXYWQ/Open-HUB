//! Lighting commands.
//!
//! Most of these mice only take a colour per LED: Off is black and
//! brightness scales the colour on the host. The Rival 100 and 300 families
//! add a firmware "breathing" effect with three speeds, and the colour-stop
//! models (Rival 310/600, Sensei) loop through a short list of colours, which
//! gives them breathing and a colour cycle too.

use gearwise_core::model::{LightingEffect, Rgb, ZoneState};
use gearwise_core::{Error, Result};

use crate::link::Packet;
use crate::models::{Cycle, Light, LightProto, Report, Zone};

/// Loop time recorded for steady colours sent as a one-stop cycle.
const STEADY_MS: u16 = 1000;
/// Least time the firmware needs per colour stop (libratbag).
const MS_PER_STOP: u16 = 330;
/// The three firmware breathing speeds, as (loop time, effect code).
const BREATHING: [(u16, u8); 3] = [(3000, 4), (5000, 3), (7000, 2)];
/// Effect code for a steady colour.
const STEADY: u8 = 1;

/// Validate `want` for `zone` and encode it. Returns the state the zone will
/// be in (periods snapped to what the firmware can do) and the packets.
pub fn plan(light: &Light, zone: u8, want: &ZoneState) -> Result<(ZoneState, Vec<Packet>)> {
    let z = light
        .zones
        .get(usize::from(zone))
        .ok_or_else(|| Error::invalid(format!("no lighting zone {zone}")))?;
    if want.brightness > 100 {
        return Err(Error::invalid("brightness is 0-100"));
    }
    if !light.effects().contains(&want.effect.kind()) {
        return Err(Error::Unsupported("lighting effect"));
    }
    let mut applied = *want;

    let packets = match light.proto {
        LightProto::Cycle(layout) => {
            let (effect, packet) = cycle(&layout, z, want);
            applied.effect = effect;
            vec![packet]
        }
        proto => {
            let rgb = steady_color(want);
            let mut packets = vec![Packet::output(color_bytes(proto, z, rgb))];
            if let Some((effect_bytes, effect)) = firmware_effect(proto, z, &want.effect) {
                packets.push(Packet::output(effect_bytes));
                applied.effect = effect;
            }
            packets
        }
    };
    Ok((applied, packets))
}

/// The colour a one-colour-per-LED mouse should show.
fn steady_color(want: &ZoneState) -> Rgb {
    match want.effect {
        LightingEffect::Static { color } | LightingEffect::Breathing { color, .. } => {
            color.scaled(want.brightness)
        }
        _ => Rgb::BLACK,
    }
}

fn color_bytes(proto: LightProto, zone: &Zone, c: Rgb) -> Vec<u8> {
    let rgb = [c.r, c.g, c.b];
    match proto {
        LightProto::Backlight => [&[0x05, 0x00][..], &rgb].concat(),
        LightProto::LogoWheel => [&[0x08, zone.led][..], &rgb].concat(),
        // The trailing 0x64 is a fixed 100 % brightness.
        LightProto::FourZone => [&[0x05, 0x00, zone.led][..], &rgb, &[0x64]].concat(),
        // One RGB slot per zone follows the mask; only the target's is filled.
        LightProto::Mask => {
            let blank = vec![0; 3 * usize::from(zone.led)];
            [&[0x21, 1 << zone.led][..], &blank, &rgb].concat()
        }
        LightProto::PerLed => [&[0x21, 0x01, zone.led][..], &rgb].concat(),
        LightProto::Prime => [&[0x62, 0x01][..], &rgb, &[0; 15], &[0xff]].concat(),
        LightProto::PrimeMini => [&[0x21, 0x00][..], &rgb].concat(),
        LightProto::Cycle(_) => unreachable!("cycle mice use `cycle`"),
    }
}

/// The follow-up effect command and the effect it leaves the zone in, for
/// mice with a firmware breathing mode.
fn firmware_effect(
    proto: LightProto,
    zone: &Zone,
    effect: &LightingEffect,
) -> Option<(Vec<u8>, LightingEffect)> {
    let prefix: &[u8] = match proto {
        LightProto::Backlight => &[0x07, 0x00],
        LightProto::LogoWheel => &[0x07, zone.led],
        _ => return None,
    };
    let (code, applied) = match *effect {
        LightingEffect::Breathing { color, period_ms } => {
            let (ms, code) = BREATHING
                .iter()
                .copied()
                .min_by_key(|(ms, _)| ms.abs_diff(period_ms))
                .expect("breathing speeds are listed");
            (
                code,
                LightingEffect::Breathing {
                    color,
                    period_ms: ms,
                },
            )
        }
        // Static and Off also cancel a previous breathing effect.
        other => (STEADY, other),
    };
    Some(([prefix, &[code]].concat(), applied))
}

/// A colour-stop packet: a header, the first colour, then each stop's colour
/// with its distance (0-255) from the previous stop.
fn cycle(layout: &Cycle, zone: &Zone, want: &ZoneState) -> (LightingEffect, Packet) {
    let scale = |c: Rgb| c.scaled(want.brightness);
    let (stops, once, effect): (Vec<(Rgb, u8)>, bool, LightingEffect) = match want.effect {
        LightingEffect::Static { color } => (vec![(scale(color), 0)], true, want.effect),
        LightingEffect::Breathing { color, period_ms } => (
            vec![(Rgb::BLACK, 0), (scale(color), 127), (Rgb::BLACK, 128)],
            false,
            LightingEffect::Breathing {
                color,
                period_ms: period_ms.max(3 * MS_PER_STOP),
            },
        ),
        LightingEffect::ColorCycle { period_ms } => (
            [
                Rgb::new(255, 0, 0),
                Rgb::new(0, 255, 0),
                Rgb::new(0, 0, 255),
                Rgb::new(255, 0, 0),
            ]
            .map(scale)
            .into_iter()
            .zip([0, 85, 85, 85])
            .collect(),
            false,
            LightingEffect::ColorCycle {
                period_ms: period_ms.max(4 * MS_PER_STOP),
            },
        ),
        // Off: the effect list never offers anything else.
        _ => (vec![(Rgb::BLACK, 0)], true, LightingEffect::Off),
    };
    let duration = effect.period_ms().unwrap_or(STEADY_MS);

    let mut bytes = layout.cmd.to_vec();
    bytes.resize(layout.cmd.len() + layout.header, 0);
    let at = |offset: usize| layout.cmd.len() + offset;
    for &offset in layout.leds {
        bytes[at(offset)] = zone.led;
    }
    bytes[at(layout.duration)..at(layout.duration) + 2].copy_from_slice(&duration.to_le_bytes());
    bytes[at(layout.once)] = u8::from(once);
    bytes[at(layout.count)] = stops.len() as u8;
    let first = stops[0].0;
    bytes.extend_from_slice(&[first.r, first.g, first.b]);
    for (c, distance) in stops {
        bytes.extend_from_slice(&[c.r, c.g, c.b, distance]);
    }
    (
        effect,
        Packet {
            report: Report::Feature,
            bytes,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gearwise_core::model::EffectKind;

    const ZONE: Zone = Zone {
        name: "Logo",
        led: 0,
        default: LightingEffect::Off,
    };

    fn state(effect: LightingEffect, brightness: u8) -> ZoneState {
        ZoneState { effect, brightness }
    }

    fn light(proto: LightProto) -> Light {
        Light {
            proto,
            zones: &[ZONE],
        }
    }

    const CYCLE: Cycle = Cycle {
        cmd: [0x5b, 0x00],
        header: 26,
        leds: &[0],
        duration: 1,
        once: 17,
        count: 25,
    };

    #[test]
    fn brightness_scales_the_color() {
        let l = light(LightProto::PrimeMini);
        let s = state(
            LightingEffect::Static {
                color: Rgb::new(200, 100, 50),
            },
            50,
        );
        let (_, p) = plan(&l, 0, &s).unwrap();
        assert_eq!(p[0].bytes, [0x21, 0x00, 100, 50, 25]);
        let off = state(LightingEffect::Off, 100);
        assert_eq!(plan(&l, 0, &off).unwrap().1[0].bytes, [0x21, 0x00, 0, 0, 0]);
    }

    #[test]
    fn breathing_snaps_to_a_firmware_speed() {
        let l = light(LightProto::Backlight);
        let red = Rgb::new(255, 0, 0);
        let want = state(
            LightingEffect::Breathing {
                color: red,
                period_ms: 4100,
            },
            100,
        );
        let (applied, p) = plan(&l, 0, &want).unwrap();
        assert_eq!(p[0].bytes, [0x05, 0x00, 255, 0, 0]);
        assert_eq!(p[1].bytes, [0x07, 0x00, 3]);
        assert_eq!(
            applied.effect,
            LightingEffect::Breathing {
                color: red,
                period_ms: 5000
            }
        );
        // A steady colour resets the effect to "steady".
        let steady = state(LightingEffect::Static { color: red }, 100);
        assert_eq!(plan(&l, 0, &steady).unwrap().1[1].bytes, [0x07, 0x00, 1]);
    }

    #[test]
    fn cycle_packets_follow_the_stop_list_layout() {
        let l = light(LightProto::Cycle(CYCLE));
        let red = Rgb::new(255, 0, 0);

        let (_, p) = plan(&l, 0, &state(LightingEffect::Static { color: red }, 100)).unwrap();
        assert_eq!(p[0].report, Report::Feature);
        let b = &p[0].bytes;
        assert_eq!(&b[..2], [0x5b, 0x00]);
        assert_eq!(&b[3..5], 1000u16.to_le_bytes(), "duration");
        assert_eq!(b[19], 1, "play once");
        assert_eq!(b[27], 1, "stop count");
        assert_eq!(&b[28..], [255, 0, 0, 255, 0, 0, 0]);

        let (applied, p) = plan(
            &l,
            0,
            &state(
                LightingEffect::Breathing {
                    color: red,
                    period_ms: 4000,
                },
                100,
            ),
        )
        .unwrap();
        let b = &p[0].bytes;
        assert_eq!(&b[3..5], 4000u16.to_le_bytes());
        assert_eq!((b[19], b[27]), (0, 3));
        assert_eq!(
            &b[28..],
            [0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 127, 0, 0, 0, 128]
        );
        assert_eq!(applied.effect.period_ms(), Some(4000));
    }

    #[test]
    fn colour_cycle_walks_the_primaries() {
        let l = light(LightProto::Cycle(CYCLE));
        let (_, p) = plan(
            &l,
            0,
            &state(LightingEffect::ColorCycle { period_ms: 10_000 }, 100),
        )
        .unwrap();
        let b = &p[0].bytes;
        assert_eq!(b[27], 4);
        assert_eq!(
            &b[28..],
            [
                255, 0, 0, 255, 0, 0, 0, 0, 255, 0, 85, 0, 0, 255, 85, 255, 0, 0, 85
            ]
        );
    }

    #[test]
    fn fast_cycles_are_slowed_to_what_the_firmware_manages() {
        let l = light(LightProto::Cycle(CYCLE));
        let (applied, p) = plan(
            &l,
            0,
            &state(LightingEffect::ColorCycle { period_ms: 500 }, 100),
        )
        .unwrap();
        assert_eq!(
            applied.effect,
            LightingEffect::ColorCycle { period_ms: 1320 }
        );
        assert_eq!(&p[0].bytes[3..5], 1320u16.to_le_bytes());
    }

    #[test]
    fn rejects_what_the_zone_cannot_show() {
        let l = light(LightProto::Mask);
        let red = Rgb::new(255, 0, 0);
        let breathing = state(
            LightingEffect::Breathing {
                color: red,
                period_ms: 4000,
            },
            100,
        );
        assert!(matches!(
            plan(&l, 0, &breathing),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            plan(&l, 1, &state(LightingEffect::Off, 100)),
            Err(Error::InvalidValue(_))
        ));
        assert!(matches!(
            plan(&l, 0, &state(LightingEffect::Off, 101)),
            Err(Error::InvalidValue(_))
        ));
        assert!(!l.effects().contains(&EffectKind::Wave));
    }
}
