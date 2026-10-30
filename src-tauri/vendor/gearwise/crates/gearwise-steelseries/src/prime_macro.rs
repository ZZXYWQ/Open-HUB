//! Structural inspection of the reviewed libsteel macro memory header.
//! No event interpretation, device I/O, upload or firmware capability is enabled.
use gearwise_core::{Error, Result};
use std::ops::Range;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Metadata {
    /// Raw native playback code1..5; retain it without inventing host semantics.
    pub playback: u8,
    pub event_count: u16,
    pub direction: u8,
    pub repeats: u8,
    pub repeat_delay: u16,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inspected {
    pub metadata: Metadata,
    pub header: Range<usize>,
    /// Opaque four-byte records, not decoded executable events.
    pub events: Range<usize>,
}
/// Match the pinned OEM parser's strict end boundary: the declared event region
/// must end before the buffer end, rather than exactly at it. The extra byte is
/// not interpreted as a terminator, since the parser only proves its presence.
pub fn inspect(memory: &[u8], offset: usize) -> Result<Inspected> {
    let start = offset
        .checked_add(8)
        .ok_or(Error::invalid("Prime macro header offset overflow"))?;
    let h = memory
        .get(offset..start)
        .ok_or(Error::invalid("Prime macro header is truncated"))?;
    if h[0] != 0x77 || !(1..=5).contains(&h[1]) {
        return Err(Error::invalid("unknown Prime macro marker/playback"));
    }
    let count = u16::from_le_bytes([h[2], h[3]]);
    let end = start
        .checked_add(usize::from(count) * 4)
        .ok_or(Error::invalid("Prime macro event range overflow"))?;
    if end >= memory.len() {
        return Err(Error::invalid(
            "Prime macro event range reaches or exceeds memory end",
        ));
    }
    Ok(Inspected {
        metadata: Metadata {
            playback: h[1],
            event_count: count,
            direction: h[4],
            repeats: h[5],
            repeat_delay: u16::from_le_bytes([h[6], h[7]]),
        },
        header: offset..start,
        events: start..end,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_header_offsets_endianness_and_opaque_records_are_preserved() {
        for playback in 1..=5 {
            let memory = [
                0xaa, 0xbb, 0xcc, 0xdd, 0x77, playback, 2, 0, 1, 7, 0x34, 0x12, 9, 8, 7, 6, 5, 4,
                3, 2, 0xef,
            ];
            let result = inspect(&memory, 4).unwrap();
            assert_eq!(result.header, 4..12);
            assert_eq!(result.events, 12..20);
            assert_eq!(
                result.metadata,
                Metadata {
                    playback,
                    event_count: 2,
                    direction: 1,
                    repeats: 7,
                    repeat_delay: 0x1234
                }
            );
            assert_eq!(&memory[result.events], &[9, 8, 7, 6, 5, 4, 3, 2]);
        }
    }
    #[test]
    fn every_truncation_exact_end_unknown_and_overflow_is_refused() {
        let memory = [0x77, 1, 2, 0, 1, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 0];
        for length in 0..memory.len() {
            assert!(inspect(&memory[..length], 0).is_err(), "{length}");
        }
        for offset in [usize::MAX, usize::MAX - 7, usize::MAX - 8] {
            assert!(inspect(&memory, offset).is_err());
        }
        for code in [0, 6, 255] {
            let mut bad = memory;
            bad[1] = code;
            assert!(inspect(&bad, 0).is_err());
        }
        let mut bad = memory;
        bad[0] = 0x71;
        assert!(inspect(&bad, 0).is_err());
        bad = memory;
        bad[2] = 255;
        bad[3] = 255;
        assert!(inspect(&bad, 0).is_err());
    }
    #[test]
    fn zero_length_and_unknown_direction_bytes_are_structural_data_only() {
        let memory = [0x77, 5, 0, 0, 255, 0, 255, 255, 9];
        let result = inspect(&memory, 0).unwrap();
        assert_eq!(result.events, 8..8);
        assert_eq!(result.metadata.direction, 255);
        assert_eq!(result.metadata.repeat_delay, 65535);
    }
}
