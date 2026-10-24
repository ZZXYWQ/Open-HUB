//! DPI stage commands.

use gearwise_core::model::{DpiStage, DpiState};
use gearwise_core::{Error, Result};

use crate::link::Packet;
use crate::models::{Count, DpiProto, DpiSpec};

/// Validate `want` against the mouse and encode it.
///
/// Each stage is snapped to a value the sensor supports. Returns the stages
/// as the mouse will hold them next to the packets that program them.
pub fn plan(spec: &DpiSpec, want: &DpiState) -> Result<(DpiState, Vec<Packet>)> {
    if want.stages.is_empty() || want.stages.len() > usize::from(spec.stages) {
        return Err(Error::invalid(format!("1-{} DPI stages", spec.stages)));
    }
    if usize::from(want.active) >= want.stages.len() {
        return Err(Error::invalid("active DPI stage out of range"));
    }
    let mut stages = Vec::with_capacity(want.stages.len());
    let mut codes = Vec::with_capacity(want.stages.len());
    for stage in &want.stages {
        if stage.x != stage.y {
            return Err(Error::invalid(
                "this mouse has one DPI per stage, not X and Y",
            ));
        }
        let (dpi, code) = spec.cpi.resolve(stage.x)?;
        stages.push(DpiStage::uniform(dpi));
        codes.push(code);
    }

    let packets = match spec.proto {
        DpiProto::PerStage { head, tail } => {
            if want.active != 0 {
                return Err(Error::Unsupported("choosing the active DPI stage"));
            }
            // The mouse always holds `spec.stages` presets; a shorter list
            // repeats its last stage so the DPI button has nothing else to
            // cycle to.
            (0..spec.stages)
                .map(|n| {
                    let code = codes[usize::from(n).min(codes.len() - 1)];
                    let mut bytes = head.to_vec();
                    bytes.push(n + 1);
                    bytes.push(code.to_le_bytes()[0]);
                    bytes.extend_from_slice(tail);
                    Packet::output(bytes)
                })
                .collect()
        }
        DpiProto::Block {
            cmd,
            first,
            count,
            wide,
        } => {
            let n = codes.len();
            let mut bytes = cmd.to_vec();
            bytes.push(match count {
                Count::Number => n as u8,
                Count::Flags => 0xff >> (8 - n),
            });
            bytes.push(first + want.active);
            for code in codes {
                let le = code.to_le_bytes();
                bytes.extend_from_slice(if wide { &le } else { &le[..1] });
            }
            vec![Packet::output(bytes)]
        }
    };
    Ok((
        DpiState {
            stages,
            active: want.active,
        },
        packets,
    ))
}

/// Make `index` the active stage of `current`.
pub fn select(spec: &DpiSpec, current: &DpiState, index: u8) -> Result<(DpiState, Vec<Packet>)> {
    plan(
        spec,
        &DpiState {
            stages: current.stages.clone(),
            active: index,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpi;

    fn state(stages: &[u16], active: u8) -> DpiState {
        DpiState {
            stages: stages.iter().map(|d| DpiStage::uniform(*d)).collect(),
            active,
        }
    }

    const BLOCK: DpiSpec = DpiSpec {
        cpi: cpi::AIR,
        stages: 5,
        defaults: &[800],
        proto: DpiProto::Block {
            cmd: &[0x2d],
            first: 0,
            count: Count::Number,
            wide: false,
        },
    };

    const PAIR: DpiSpec = DpiSpec {
        cpi: cpi::LINEAR_12000,
        stages: 2,
        defaults: &[800, 1600],
        proto: DpiProto::PerStage {
            head: &[0x53, 0x00],
            tail: &[0x00, 0x42],
        },
    };

    #[test]
    fn block_packet_lists_every_stage() {
        let (applied, packets) = plan(&BLOCK, &state(&[400, 800, 1200], 2)).unwrap();
        assert_eq!(
            packets,
            [Packet::output(vec![0x2d, 3, 2, 0x04, 0x09, 0x0d])]
        );
        assert_eq!(applied, state(&[400, 800, 1200], 2));
    }

    #[test]
    fn flag_counts_set_one_bit_per_stage() {
        let spec = DpiSpec {
            proto: DpiProto::Block {
                cmd: &[0x55, 0x00],
                first: 1,
                count: Count::Flags,
                wide: true,
            },
            cpi: cpi::LINEAR_18000,
            ..BLOCK
        };
        let counts: Vec<u8> = (1..=5)
            .map(|n| plan(&spec, &state(&vec![800; n], 0)).unwrap().1[0].bytes[2])
            .collect();
        assert_eq!(counts, [0b1, 0b11, 0b111, 0b1111, 0b11111]);
    }

    #[test]
    fn per_stage_mice_get_a_packet_per_preset() {
        let (_, packets) = plan(&PAIR, &state(&[800, 12000], 0)).unwrap();
        assert_eq!(
            packets,
            [
                Packet::output(vec![0x53, 0x00, 1, 7, 0x00, 0x42]),
                Packet::output(vec![0x53, 0x00, 2, 119, 0x00, 0x42]),
            ]
        );
    }

    #[test]
    fn a_single_stage_fills_both_presets() {
        let (applied, packets) = plan(&PAIR, &state(&[1600], 0)).unwrap();
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0].bytes[3], packets[1].bytes[3]);
        assert_eq!(applied.stages.len(), 1);
    }

    #[test]
    fn per_stage_mice_cannot_pick_the_active_stage() {
        assert!(matches!(
            plan(&PAIR, &state(&[800, 1600], 1)),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            select(&PAIR, &state(&[800, 1600], 0), 1),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn stages_snap_and_are_recorded_as_snapped() {
        let (applied, _) = plan(&BLOCK, &state(&[849, 851], 0)).unwrap();
        assert_eq!(applied, state(&[800, 900], 0));
    }

    #[test]
    fn rejects_bad_requests() {
        let too_many = state(&[400; 6], 0);
        assert!(matches!(
            plan(&BLOCK, &too_many),
            Err(Error::InvalidValue(_))
        ));
        assert!(matches!(
            plan(&BLOCK, &state(&[], 0)),
            Err(Error::InvalidValue(_))
        ));
        assert!(matches!(
            plan(&BLOCK, &state(&[400, 800], 2)),
            Err(Error::InvalidValue(_))
        ));
        assert!(matches!(
            plan(&BLOCK, &state(&[50], 0)),
            Err(Error::InvalidValue(_))
        ));
        let xy = DpiState {
            stages: vec![DpiStage { x: 800, y: 1600 }],
            active: 0,
        };
        assert!(matches!(plan(&BLOCK, &xy), Err(Error::InvalidValue(_))));
    }
}
