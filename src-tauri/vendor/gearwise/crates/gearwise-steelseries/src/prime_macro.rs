//! Reviewed libsteel macro header inspection and canonical event record codec.
//! No allocation/binding planner, device I/O, upload or firmware capability is enabled.
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

/// Canonical, statically reviewed event-record subset. No macro allocation,
/// playback execution or device upload is performed by this module.
pub mod event {
    use gearwise_core::{Error, Result};
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Page {
        Keyboard,
        Consumer,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Event {
        Key { usage: u8, page: Page, down: bool },
        Mouse { button: u8, down: bool },
        Delay(u16),
    }
    impl Event {
        pub fn encode(self) -> Result<[u8; 4]> {
            match self {
                Self::Key { usage: 0, .. } => {
                    Err(Error::invalid("Prime native key usage must be nonzero"))
                }
                Self::Key { usage, page, down } => Ok([
                    2,
                    usage,
                    match page {
                        Page::Keyboard => 1,
                        Page::Consumer => 12,
                    },
                    u8::from(down),
                ]),
                Self::Mouse { button, down } if (1..=8).contains(&button) => {
                    Ok([0, 1 << (button - 1), 0, u8::from(down)])
                }
                Self::Mouse { .. } => Err(Error::invalid("Prime native mouse button must be1..8")),
                Self::Delay(value) => {
                    let [lo, hi] = value.to_le_bytes();
                    Ok([4, 0, lo, hi])
                }
            }
        }
        pub fn decode(bytes: &[u8]) -> Result<Self> {
            let b: [u8; 4] = bytes
                .try_into()
                .map_err(|_| Error::invalid("Prime event record must be exactly4 bytes"))?;
            match b {
                [2, usage, page, state] if usage != 0 && [1, 12].contains(&page) && state <= 1 => {
                    Ok(Self::Key {
                        usage,
                        page: if page == 1 {
                            Page::Keyboard
                        } else {
                            Page::Consumer
                        },
                        down: state == 1,
                    })
                }
                [0, mask, 0, state] if mask.is_power_of_two() && state <= 1 => Ok(Self::Mouse {
                    button: mask.trailing_zeros() as u8 + 1,
                    down: state == 1,
                }),
                [4, 0, lo, hi] => Ok(Self::Delay(u16::from_le_bytes([lo, hi]))),
                _ => Err(Error::invalid(
                    "Prime event record outside reviewed canonical subset",
                )),
            }
        }
    }
    /// Mirror the encoder's65535-unit delay splitting, with an explicit caller
    /// budget and native header-count bound checked before allocating records.
    pub fn split_delay(value: u32, max_records: usize) -> Result<Vec<Event>> {
        let count = ((u64::from(value) + 65534) / 65535).max(1);
        if count > max_records.min(u16::MAX as usize) as u64 {
            return Err(Error::invalid("Prime delay exceeds record budget"));
        }
        let mut out = Vec::with_capacity(count as usize);
        let mut remaining = value;
        while remaining > 65535 {
            out.push(Event::Delay(65535));
            remaining -= 65535;
        }
        out.push(Event::Delay(remaining as u16));
        Ok(out)
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn pinned_native_record_vectors_and_all_key_mouse_values_roundtrip() {
            assert_eq!(
                Event::Key {
                    usage: 4,
                    page: Page::Keyboard,
                    down: true
                }
                .encode()
                .unwrap(),
                [2, 4, 1, 1]
            );
            assert_eq!(
                Event::Key {
                    usage: 0xea,
                    page: Page::Consumer,
                    down: false
                }
                .encode()
                .unwrap(),
                [2, 0xea, 12, 0]
            );
            assert_eq!(
                Event::Mouse {
                    button: 8,
                    down: true
                }
                .encode()
                .unwrap(),
                [0, 128, 0, 1]
            );
            assert_eq!(Event::Delay(0x1234).encode().unwrap(), [4, 0, 0x34, 0x12]);
            for usage in 1..=255 {
                for page in [Page::Keyboard, Page::Consumer] {
                    for down in [false, true] {
                        let event = Event::Key { usage, page, down };
                        assert_eq!(Event::decode(&event.encode().unwrap()).unwrap(), event);
                    }
                }
            }
            for button in 1..=8 {
                for down in [false, true] {
                    let event = Event::Mouse { button, down };
                    assert_eq!(Event::decode(&event.encode().unwrap()).unwrap(), event);
                }
            }
        }
        #[test]
        fn every_delay_value_and_long_delay_boundaries_preserve_full_duration() {
            for value in 0..=65535 {
                let event = Event::Delay(value);
                let encoded = event.encode().unwrap();
                assert_eq!(&encoded[2..], &value.to_le_bytes());
                assert_eq!(Event::decode(&encoded).unwrap(), event);
            }
            for (value, want) in [
                (0, vec![0]),
                (65535, vec![65535]),
                (65536, vec![65535, 1]),
                (131070, vec![65535, 65535]),
                (131071, vec![65535, 65535, 1]),
            ] {
                let events = split_delay(value, 3).unwrap();
                assert_eq!(
                    events,
                    want.into_iter().map(Event::Delay).collect::<Vec<_>>()
                );
                assert_eq!(
                    events
                        .iter()
                        .map(|event| if let Event::Delay(v) = event {
                            u32::from(*v)
                        } else {
                            unreachable!()
                        })
                        .sum::<u32>(),
                    value
                );
            }
            assert!(split_delay(0, 0).is_err());
            assert!(split_delay(65536, 1).is_err());
            assert!(split_delay(u32::MAX, usize::MAX).is_err());
        }
        #[test]
        fn malformed_or_unreviewed_records_are_not_normalized() {
            for b in [
                [2, 0, 1, 1],
                [2, 4, 7, 1],
                [2, 4, 1, 2],
                [0, 0, 0, 1],
                [0, 3, 0, 1],
                [0, 1, 1, 1],
                [0, 1, 0, 2],
                [4, 1, 0, 0],
                [1, 0, 0, 1],
                [12, 0, 0, 1],
            ] {
                assert!(Event::decode(&b).is_err(), "{b:?}");
            }
            for length in [0, 1, 2, 3, 5] {
                assert!(Event::decode(&vec![0; length]).is_err());
            }
            assert!(Event::Key {
                usage: 0,
                page: Page::Keyboard,
                down: true
            }
            .encode()
            .is_err());
            for button in [0, 9, 255] {
                assert!(Event::Mouse {
                    button,
                    down: false
                }
                .encode()
                .is_err());
            }
        }
    }
}
