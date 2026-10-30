//! Offline canonical macro blocks. No allocation, device I/O or upload.
use crate::prime_macro::{
    event::{Event, Page},
    inspect, Metadata,
};
use gearwise_core::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub metadata: Metadata,
    pub events: Vec<Event>,
}
impl Block {
    pub fn decode(memory: &[u8], offset: usize) -> Result<Self> {
        let info = inspect(memory, offset)?;
        let events = memory[info.events]
            .chunks_exact(4)
            .map(Event::decode)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            metadata: info.metadata,
            events,
        })
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        if !(1..=5).contains(&self.metadata.playback)
            || self.events.len() != usize::from(self.metadata.event_count)
        {
            return Err(Error::invalid("Prime block metadata/count mismatch"));
        }
        let mut out = vec![0x77, self.metadata.playback];
        out.extend_from_slice(&self.metadata.event_count.to_le_bytes());
        out.extend_from_slice(&[self.metadata.direction, self.metadata.repeats]);
        out.extend_from_slice(&self.metadata.repeat_delay.to_le_bytes());
        for event in &self.events {
            out.extend_from_slice(&event.encode()?);
        }
        Ok(out)
    }
    /// Input balance only; raw playback/direction fields are not approved here.
    pub fn check_input_balance(&self) -> Result<()> {
        if self.events.is_empty() {
            return Err(Error::invalid("empty Prime macro body"));
        }
        let mut held = std::collections::BTreeSet::new();
        for event in &self.events {
            event.encode()?;
            let item = match *event {
                Event::Key { usage, page, down } => Some((
                    (2, if page == Page::Keyboard { 1 } else { 12 }, usage),
                    down,
                )),
                Event::Mouse { button, down } => Some(((0, 0, button), down)),
                Event::Delay(_) => None,
            };
            if let Some((key, down)) = item {
                if (down && !held.insert(key)) || (!down && !held.remove(&key)) {
                    return Err(Error::invalid("Prime duplicate press or unmatched release"));
                }
            }
        }
        if !held.is_empty() {
            return Err(Error::invalid("Prime macro leaves held inputs"));
        }
        Ok(())
    }
}

/// Raw function71 mapping: two reserved bytes, then a little-endian word offset.
pub fn binding(offset: usize) -> Result<[u8; 5]> {
    if offset % 4 != 0 || offset / 4 > usize::from(u16::MAX) {
        return Err(Error::invalid(
            "Prime macro offset must fit aligned16-bit words",
        ));
    }
    let [lo, hi] = ((offset / 4) as u16).to_le_bytes();
    Ok([0x71, 0, 0, lo, hi])
}
pub fn binding_offset(bytes: &[u8]) -> Result<usize> {
    let b: [u8; 5] = bytes
        .try_into()
        .map_err(|_| Error::invalid("Prime native mapping must be5 bytes"))?;
    if b[..3] != [0x71, 0, 0] {
        return Err(Error::invalid(
            "Prime mapping is not a reviewed macro offset",
        ));
    }
    Ok(usize::from(u16::from_le_bytes([b[3], b[4]])) * 4)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn block() -> Block {
        Block {
            metadata: Metadata {
                playback: 2,
                event_count: 5,
                direction: 1,
                repeats: 7,
                repeat_delay: 0x1234,
            },
            events: vec![
                Event::Key {
                    usage: 4,
                    page: Page::Keyboard,
                    down: true,
                },
                Event::Mouse {
                    button: 2,
                    down: true,
                },
                Event::Delay(23),
                Event::Key {
                    usage: 4,
                    page: Page::Keyboard,
                    down: false,
                },
                Event::Mouse {
                    button: 2,
                    down: false,
                },
            ],
        }
    }
    #[test]
    fn golden_block_roundtrip_preserves_neighbors_and_raw_header() {
        let b = block();
        b.check_input_balance().unwrap();
        let bytes = b.encode().unwrap();
        assert_eq!(
            bytes,
            vec![
                0x77, 2, 5, 0, 1, 7, 0x34, 0x12, 2, 4, 1, 1, 0, 2, 0, 1, 4, 0, 23, 0, 2, 4, 1, 0,
                0, 2, 0, 0
            ]
        );
        let mut memory = vec![0xa5; 64];
        memory[12..40].copy_from_slice(&bytes);
        assert_eq!(Block::decode(&memory, 12).unwrap(), b);
        assert!(memory[..12].iter().chain(&memory[40..]).all(|v| *v == 0xa5));
        let mut raw = b;
        raw.metadata.direction = 255;
        raw.metadata.repeats = 0;
        let mut memory = raw.encode().unwrap();
        memory.push(9);
        assert_eq!(Block::decode(&memory, 0).unwrap(), raw);
    }
    #[test]
    fn mismatch_truncation_unknown_records_and_unbalanced_inputs_are_refused() {
        let b = block();
        let bytes = b.encode().unwrap();
        for n in 0..=bytes.len() {
            assert!(Block::decode(&bytes[..n], 0).is_err());
        }
        let mut memory = bytes;
        memory.push(0);
        memory[8] = 99;
        assert!(Block::decode(&memory, 0).is_err());
        let mut bad = b.clone();
        bad.metadata.event_count += 1;
        assert!(bad.encode().is_err());
        bad = b.clone();
        bad.metadata.playback = 0;
        assert!(bad.encode().is_err());
        for events in [
            vec![],
            vec![b.events[0]],
            vec![b.events[3]],
            vec![b.events[0], b.events[0], b.events[3]],
            vec![b.events[1], b.events[3]],
        ] {
            let mut bad = b.clone();
            bad.events = events;
            assert!(bad.check_input_balance().is_err());
        }
        let mut pages = b;
        pages.events = vec![
            Event::Key {
                usage: 4,
                page: Page::Keyboard,
                down: true,
            },
            Event::Key {
                usage: 4,
                page: Page::Consumer,
                down: true,
            },
            Event::Key {
                usage: 4,
                page: Page::Consumer,
                down: false,
            },
            Event::Key {
                usage: 4,
                page: Page::Keyboard,
                down: false,
            },
        ];
        pages.check_input_balance().unwrap();
    }
    #[test]
    fn every_word_offset_roundtrips_without_rounding() {
        assert_eq!(binding(0x1234 * 4).unwrap(), [0x71, 0, 0, 0x34, 0x12]);
        for words in 0..=65535 {
            let offset = words * 4;
            assert_eq!(binding_offset(&binding(offset).unwrap()).unwrap(), offset);
        }
        for offset in [1, 2, 3, 262144, usize::MAX] {
            assert!(binding(offset).is_err());
        }
        for bytes in [
            vec![],
            vec![0x71, 0, 0, 0],
            vec![0x71, 0, 0, 0, 0, 0],
            vec![0x71, 1, 0, 0, 0],
            vec![0x71, 0, 1, 0, 0],
            vec![0x51, 0, 0, 0, 0],
        ] {
            assert!(binding_offset(&bytes).is_err());
        }
    }
}
