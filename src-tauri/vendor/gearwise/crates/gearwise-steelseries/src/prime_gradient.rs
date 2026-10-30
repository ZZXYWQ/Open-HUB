//! Native Prime libsteel program layout, GG122 static descriptor review.
//! This codec does not imply that authored/OEM animation UI or persistence is ready.
use crate::{link::Packet, models::Report};
use gearwise_core::{model::Rgb, Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub color: Rgb,
    pub weight: u8,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Motion {
    pub enabled: bool,
    pub increase: u8,
    pub decrease: u8,
    pub minimum: u8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub initial: Rgb,
    /// Native duration is the OEM uint16 speed multiplied by10.
    pub duration: u32,
    pub motion: Motion,
    pub sections: Vec<Section>,
}
impl Program {
    pub fn steady(color: Rgb) -> Self {
        Self {
            initial: color,
            duration: 0,
            motion: Motion::default(),
            sections: vec![Section { color, weight: 255 }],
        }
    }
    pub fn packet(&self) -> Result<Packet> {
        if self.sections.is_empty() || self.sections.len() > 16 {
            return Err(Error::invalid(
                "Prime native program requires1..16 sections",
            ));
        }
        if self.duration > 655350 || self.duration % 10 != 0 {
            return Err(Error::invalid(
                "Prime duration must fit OEM uint16 speed times10",
            ));
        }
        // Zero duration is only the reviewed static program. A zero-weight
        // dynamic segment can occur after OEM position quantization; keep it.
        if self.duration == 0
            && (self.sections.len() != 1
                || self.sections[0].color != self.initial
                || self.sections[0].weight != 255
                || self.motion != Motion::default())
        {
            return Err(Error::invalid(
                "Prime zero duration must be a steady program",
            ));
        }
        if self.duration != 0 && self.sections.iter().all(|s| s.weight == 0) {
            return Err(Error::invalid(
                "Prime dynamic program has no weighted section",
            ));
        }
        let mut bytes = vec![
            0x62,
            self.sections.len() as u8,
            self.initial.r,
            self.initial.g,
            self.initial.b,
        ];
        bytes.extend_from_slice(&self.duration.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // initial tick
        bytes.extend_from_slice(&[
            u8::from(self.motion.enabled),
            self.motion.increase,
            self.motion.decrease,
            self.motion.minimum,
        ]);
        for s in &self.sections {
            bytes.extend_from_slice(&[s.color.r, s.color.g, s.color.b, s.weight]);
        }
        bytes.resize(642, 0);
        Ok(Packet {
            report: Report::Feature,
            bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn steady_matches_previously_reviewed_prime_wire_program() {
        let p = Program::steady(Rgb::new(171, 205, 239)).packet().unwrap();
        let mut expected = vec![0x62, 1, 171, 205, 239];
        expected.extend_from_slice(&[0; 12]);
        expected.extend_from_slice(&[171, 205, 239, 255]);
        expected.resize(642, 0);
        assert_eq!(p.report, Report::Feature);
        assert_eq!(p.bytes, expected);
    }
    #[test]
    fn sixteen_sections_duration_motion_and_padding_have_exact_native_offsets() {
        let program = Program {
            initial: Rgb::new(1, 2, 3),
            duration: 655350,
            motion: Motion {
                enabled: true,
                increase: 4,
                decrease: 5,
                minimum: 6,
            },
            sections: (0..16)
                .map(|i| Section {
                    color: Rgb::new(i, i + 16, i + 32),
                    weight: i + 1,
                })
                .collect(),
        };
        let packet = program.packet().unwrap();
        let b = packet.bytes;
        assert_eq!(b.len(), 642);
        assert_eq!(&b[..5], &[0x62, 16, 1, 2, 3]);
        assert_eq!(&b[5..9], &[0xf6, 0xff, 9, 0]);
        assert_eq!(&b[9..13], &[0; 4]);
        assert_eq!(&b[13..17], &[1, 4, 5, 6]);
        for i in 0..16 {
            assert_eq!(
                &b[17 + i * 4..21 + i * 4],
                &[i as u8, i as u8 + 16, i as u8 + 32, i as u8 + 1]
            );
        }
        assert!(b[81..].iter().all(|v| *v == 0));
    }
    #[test]
    fn malformed_counts_duration_and_empty_motion_programs_are_rejected() {
        let base = Program {
            initial: Rgb::new(1, 2, 3),
            duration: 1000,
            motion: Motion::default(),
            sections: vec![Section {
                color: Rgb::new(4, 5, 6),
                weight: 255,
            }],
        };
        for n in [0, 17] {
            let mut p = base.clone();
            p.sections = vec![base.sections[0].clone(); n];
            assert!(p.packet().is_err());
        }
        for duration in [1, 9, 655351, 655360, u32::MAX] {
            let mut p = base.clone();
            p.duration = duration;
            assert!(p.packet().is_err());
        }
        let mut p = base.clone();
        p.duration = 0;
        assert!(p.packet().is_err());
        p = base.clone();
        p.sections[0].weight = 0;
        assert!(p.packet().is_err());
        p = Program::steady(base.initial);
        p.motion.enabled = true;
        assert!(p.packet().is_err());
        p = base;
        p.sections.push(Section {
            color: Rgb::new(7, 8, 9),
            weight: 0,
        });
        assert!(p.packet().is_ok());
    }
}
