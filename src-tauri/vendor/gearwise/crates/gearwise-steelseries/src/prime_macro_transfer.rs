//! Pinned GG122 Prime macro transfer coverage, offline planning only.
//! This is not a slot allocator, ownership proof or hardware upload API.
//! Pinned firmware resets all macro contexts BEFORE checking write bounds.
//! Validate the complete transaction before I/O; an invalid probe write is
//! not side-effect-free. See docs/PRIME-ONBOARD-MACRO-RESEARCH.md.
use crate::{
    link::Packet,
    models::Report,
    prime_macro_block::{binding, Block},
};
use gearwise_core::{Error, Result};
use std::ops::Range;
pub const DECLARED_BYTES: usize = 1280;
pub const CHUNK_BYTES: usize = 512;
pub const COVERED_BYTES: usize = (DECLARED_BYTES / CHUNK_BYTES) * CHUNK_BYTES;

pub struct CoveragePlan {
    pub packets: Vec<Packet>,
    pub covered: Range<usize>,
    pub preserved_tail: Range<usize>,
}
/// Reproduce the two reviewed Feature5e chunks without claiming they cover the
/// whole declared image. Caller must separately establish snapshot identity,
/// all binding ownership, coverage for every changed/new macro offset, mode semantics
/// and byte-for-byte preservation of all other referenced blocks, including
/// unchanged foreign blocks beyond the transferred prefix,
/// and a failure/readback transaction.
pub fn plan(before: &[u8], desired: &[u8], macro_offsets: &[usize]) -> Result<CoveragePlan> {
    if before.len() != DECLARED_BYTES || desired.len() != DECLARED_BYTES {
        return Err(Error::invalid(
            "Prime coverage planner requires complete1280-byte images",
        ));
    }
    if before[COVERED_BYTES..] != desired[COVERED_BYTES..] {
        return Err(Error::invalid(
            "Prime descriptor does not transfer the final256 bytes",
        ));
    }
    for offset in macro_offsets {
        binding(*offset)?;
        let block = Block::decode(desired, *offset)?;
        block.check_input_balance()?;
        let end = offset
            .checked_add(block.encode()?.len())
            .ok_or(Error::invalid("Prime macro coverage overflow"))?;
        if end > COVERED_BYTES {
            return Err(Error::invalid(
                "Prime referenced macro extends outside transmitted prefix",
            ));
        }
    }
    let mut packets = Vec::new();
    for index in 0..DECLARED_BYTES / CHUNK_BYTES {
        let offset = index * CHUNK_BYTES;
        let mut bytes = vec![0x5e];
        bytes.extend_from_slice(&((offset / 4) as u16).to_le_bytes());
        bytes.extend_from_slice(&((CHUNK_BYTES / 4) as u16).to_le_bytes());
        bytes.extend_from_slice(&desired[offset..offset + CHUNK_BYTES]);
        bytes.resize(642, 0);
        packets.push(Packet {
            report: Report::Feature,
            bytes,
        });
    }
    Ok(CoveragePlan {
        packets,
        covered: 0..COVERED_BYTES,
        preserved_tail: COVERED_BYTES..DECLARED_BYTES,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prime_macro::{
        event::{Event, Page},
        Metadata,
    };
    fn block() -> Vec<u8> {
        Block {
            metadata: Metadata {
                playback: 1,
                event_count: 2,
                direction: 1,
                repeats: 1,
                repeat_delay: 0,
            },
            events: vec![
                Event::Key {
                    usage: 4,
                    page: Page::Keyboard,
                    down: true,
                },
                Event::Key {
                    usage: 4,
                    page: Page::Keyboard,
                    down: false,
                },
            ],
        }
        .encode()
        .unwrap()
    }
    #[test]
    fn exact_two_native_frames_keep_payload_and_zero_padding_without_mutating_images() {
        let before = vec![0x35; DECLARED_BYTES];
        let mut desired = before.clone();
        desired[64..80].copy_from_slice(&block());
        let original = desired.clone();
        let p = plan(&before, &desired, &[64]).unwrap();
        assert_eq!(p.covered, 0..1024);
        assert_eq!(p.preserved_tail, 1024..1280);
        assert_eq!(p.packets.len(), 2);
        for (i, packet) in p.packets.iter().enumerate() {
            assert_eq!(packet.report, Report::Feature);
            assert_eq!(packet.bytes.len(), 642);
            assert_eq!(&packet.bytes[..5], &[0x5e, (i * 128) as u8, 0, 128, 0]);
            assert_eq!(&packet.bytes[5..517], &desired[i * 512..(i + 1) * 512]);
            assert!(packet.bytes[517..].iter().all(|v| *v == 0));
        }
        assert_eq!(desired, original);
        assert!(before.iter().all(|v| *v == 0x35));
    }
    #[test]
    fn no_partial_plan_when_tail_or_reference_coverage_is_invalid() {
        let before = vec![0; DECLARED_BYTES];
        let mut desired = before.clone();
        desired[1024] = 1;
        assert!(plan(&before, &desired, &[]).is_err());
        desired = before.clone();
        desired[1020..1036].copy_from_slice(&block());
        assert!(plan(&before, &desired, &[1020]).is_err());
        // Tail bytes can already contain a valid macro. Preserving its bytes
        // does not make a newly referenced tail macro covered by this transfer.
        let mut tail = before.clone();
        tail[1040..1056].copy_from_slice(&block());
        assert!(plan(&tail, &tail, &[1040]).is_err());
        for n in [0, 1024, 1279, 1281] {
            assert!(plan(&vec![0; n], &desired, &[]).is_err());
            assert!(plan(&before, &vec![0; n], &[]).is_err());
        }
        assert!(plan(&before, &before, &[usize::MAX]).is_err());
    }
    #[test]
    fn last_fully_transmitted_macro_boundary_is_explicit() {
        let before = vec![0; DECLARED_BYTES];
        let mut desired = before.clone();
        desired[1008..1024].copy_from_slice(&block());
        assert!(plan(&before, &desired, &[1008]).is_ok());
        for offset in [1009, 1012] {
            assert!(plan(&before, &desired, &[offset]).is_err());
        }
        desired[1008 + 8] = 99;
        assert!(plan(&before, &desired, &[1008]).is_err());
    }
}
