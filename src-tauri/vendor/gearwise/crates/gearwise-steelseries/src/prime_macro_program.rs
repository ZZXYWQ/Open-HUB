//! Native playback policies evidenced on the pinned Prime firmware. Offline;
//! delay values are scheduler counts, not verified milliseconds.
use crate::{prime_macro::event::Event, prime_macro_block::Block, prime_macro_once};
use gearwise_core::{Error, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    #[default]
    Press,
    Release,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Playback {
    #[default]
    Once,
    Counted {
        count: u8,
        interval_ticks: u16,
    },
    WhileHeld,
    Toggle,
}
pub fn compile(events: &[Event], playback: Playback, trigger: Trigger) -> Result<Block> {
    let mut block = prime_macro_once::compile(events)?;
    block.metadata.direction = if trigger == Trigger::Press { 1 } else { 0 };
    match playback {
        Playback::Once => {}
        Playback::Counted {
            count,
            interval_ticks,
        } => {
            if count == 0 {
                return Err(Error::invalid("Prime zero repeat count wraps to256"));
            }
            block.metadata.playback = 2;
            block.metadata.repeats = count;
            block.metadata.repeat_delay = interval_ticks;
        }
        Playback::WhileHeld => {
            if trigger != Trigger::Press {
                return Err(Error::invalid("Prime held playback needs press direction"));
            }
            block.metadata.playback = 3;
        }
        Playback::Toggle => block.metadata.playback = 4,
    }
    Ok(block)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::prime_macro::event::Page;
    fn key(down: bool) -> Event {
        Event::Key {
            usage: 4,
            page: Page::Keyboard,
            down,
        }
    }
    #[test]
    fn invalid_counts_directions_unbalanced_inputs_and_capacity_are_rejected() {
        let balanced = [key(true), key(false)];
        assert!(
            compile(
                &balanced,
                Playback::Counted {
                    count: 0,
                    interval_ticks: 0
                },
                Trigger::Press
            )
            .is_err()
        );
        assert!(compile(&balanced, Playback::WhileHeld, Trigger::Release).is_err());
        for mode in [
            Playback::Once,
            Playback::Counted {
                count: 255,
                interval_ticks: 65535,
            },
            Playback::WhileHeld,
            Playback::Toggle,
        ] {
            assert!(compile(&[], mode, Trigger::Press).is_err());
            assert!(compile(&[key(true)], mode, Trigger::Press).is_err());
            let mut events: Vec<_> = (1..=5)
                .map(|usage| Event::Key {
                    usage,
                    page: Page::Consumer,
                    down: true,
                })
                .collect();
            events.extend((1..=5).map(|usage| Event::Key {
                usage,
                page: Page::Consumer,
                down: false,
            }));
            assert!(compile(&events, mode, Trigger::Press).is_err());
        }
    }
}
