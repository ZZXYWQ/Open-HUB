//! Offline admission for the evidenced native press-triggered once subset.
//! Delays remain raw scheduler counts: no millisecond conversion, allocation,
//! device upload, merged-context capacity or persistence is approved here.
use crate::prime_macro::{
    Metadata,
    event::{Event, Page},
};
use crate::prime_macro_block::Block;
use gearwise_core::{Error, Result};

/// Build a balanced one-shot block, refusing consumer usage loss in the
/// firmware's four-byte per-context queue. The complete upload planner must
/// still account for the shared four-byte queue across concurrent contexts.
pub fn compile(events: &[Event]) -> Result<Block> {
    if events.len() > (crate::prime_macro_transfer::COVERED_BYTES - 8) / 4 {
        return Err(Error::invalid(
            "Prime once block exceeds reviewed transfer prefix",
        ));
    }
    let count = u16::try_from(events.len())
        .map_err(|_| Error::invalid("Prime native event count exceeds16 bits"))?;
    let block = Block {
        metadata: Metadata {
            playback: 1,
            event_count: count,
            direction: 1,
            repeats: 1,
            repeat_delay: 0,
        },
        events: events.to_vec(),
    };
    block.check_input_balance()?;
    let mut consumer = std::collections::BTreeSet::new();
    for event in events {
        if let Event::Key {
            usage,
            page: Page::Consumer,
            down,
        } = *event
        {
            if down {
                consumer.insert(usage);
            } else {
                consumer.remove(&usage);
            }
            if consumer.len() > 4 {
                return Err(Error::invalid(
                    "Prime native context holds at most4 consumer usages",
                ));
            }
        }
    }
    Ok(block)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_once_header_keeps_canonical_events_and_raw_delay_counts() {
        let events = [
            Event::Key {
                usage: 4,
                page: Page::Keyboard,
                down: true,
            },
            Event::Delay(0x1234),
            Event::Key {
                usage: 4,
                page: Page::Keyboard,
                down: false,
            },
        ];
        let block = compile(&events).unwrap();
        assert_eq!(block.events, events);
        assert_eq!(
            block.encode().unwrap(),
            [
                0x77, 1, 3, 0, 1, 1, 0, 0, 2, 4, 1, 1, 4, 0, 0x34, 0x12, 2, 4, 1, 0
            ]
        );
    }
    #[test]
    fn consumer_queue_capacity_is_peak_held_not_total_distinct_usages() {
        let key = |usage, down| Event::Key {
            usage,
            page: Page::Consumer,
            down,
        };
        let mut events: Vec<_> = (1..=4).map(|u| key(u, true)).collect();
        events.extend((1..=4).map(|u| key(u, false)));
        assert!(compile(&events).is_ok());
        let mut five: Vec<_> = (1..=5).map(|u| key(u, true)).collect();
        five.extend((1..=5).map(|u| key(u, false)));
        assert!(compile(&five).is_err());
        let serial: Vec<_> = (1..=120)
            .flat_map(|u| [key(u, true), key(u, false)])
            .collect();
        assert!(compile(&serial).is_ok());
        events.insert(
            0,
            Event::Key {
                usage: 1,
                page: Page::Keyboard,
                down: true,
            },
        );
        events.push(Event::Key {
            usage: 1,
            page: Page::Keyboard,
            down: false,
        });
        assert!(compile(&events).is_ok());
    }
    #[test]
    fn empty_unbalanced_invalid_and_oversize_bodies_never_compile() {
        assert!(compile(&[]).is_err());
        assert!(
            compile(&[Event::Mouse {
                button: 1,
                down: true
            }])
            .is_err()
        );
        assert!(
            compile(&[
                Event::Mouse {
                    button: 9,
                    down: true
                },
                Event::Mouse {
                    button: 9,
                    down: false
                }
            ])
            .is_err()
        );
        assert!(compile(&vec![Event::Delay(0); 254]).is_ok());
        assert!(compile(&vec![Event::Delay(0); 255]).is_err());
        assert!(compile(&vec![Event::Delay(0); 65536]).is_err());
    }
}
