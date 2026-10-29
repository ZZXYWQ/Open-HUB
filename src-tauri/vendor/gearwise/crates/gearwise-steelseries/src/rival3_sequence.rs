//! Bounded OEM color-sequence data and deterministic sequencer state.
//! This models reviewed GG122 native state transitions. It opens no HID handle,
//! owns no thread; the application supplies connection ownership and lifetime.
use gearwise_core::{Error, Result};
use serde::{Deserialize,Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sequence {
    pub repeat: u8,
    #[serde(rename = "data")]
    pub frames: Vec<[u8; 3]>,
}
impl Sequence {
    pub fn decode(value: &serde_json::Value) -> Result<Self> {
        let s: Self = serde_json::from_value(value.clone())
            .map_err(|e| Error::invalid(format!("Rival3 sequence: {e}")))?;
        if s.frames.len() > 4096 {
            return Err(Error::invalid("Rival3 sequence exceeds4096 frames"));
        }
        Ok(s)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tick {
    pub color: Option<[u8; 3]>,
    pub reached_end: bool,
    pub running: bool,
}
#[derive(Clone, Debug)]
pub struct Cursor {
    sequence: Sequence,
    step_ms: u32,
    index: u32,
    completed: u8,
    running: bool,
}
impl Cursor {
    pub fn new(sequence: Sequence, step_ms: u32) -> Result<Self> {
        if sequence.frames.is_empty()
            || sequence.frames.len() > 4096
            || step_ms == 0
            || step_ms > 60000
        {
            return Err(Error::invalid(
                "Rival3 sequence needs1..4096 frames and nonzero bounded step",
            ));
        }
        Ok(Self {
            sequence,
            step_ms,
            index: 0,
            completed: 0,
            running: true,
        })
    }
    pub fn stop(&mut self) {
        self.running = false;
    }
    /// Resume retains the index and completed-cycle counter; start resets them.
    pub fn resume(&mut self) {
        self.running = true;
    }
    pub fn start(&mut self) {
        self.index = 0;
        self.completed = 0;
        self.running = true;
    }
    pub fn running(&self) -> bool {
        self.running
    }
    /// The scheduler supplies its shared elapsed milliseconds and queued ticks.
    /// This function does not assume a timer origin, sleep interval or HID speed.
    /// It models a published scheduler snapshot; native notification/publication
    /// races and physical transfer timing are not claimed as reproduced.
    pub fn tick(&mut self, global_ms: u64, pending: u32) -> Tick {
        if !self.running || pending == 0 {
            return Tick {
                color: None,
                reached_end: false,
                running: self.running,
            };
        }
        let color = self.sequence.frames.get(self.index as usize).copied();
        let mut end = false;
        let len = self.sequence.frames.len() as u32;
        if self.sequence.repeat == 255 {
            let next = ((global_ms / u64::from(self.step_ms)) % u64::from(len)) as u32;
            end = next < self.index;
            self.index = next;
        } else {
            self.index = self.index.wrapping_add(pending);
            if self.index >= len {
                self.completed = self.completed.wrapping_add(1);
                end = true;
                if self.completed < self.sequence.repeat {
                    self.index = 0;
                } else {
                    self.running = false;
                }
            }
        }
        // GG122 advances/stops before the actual SendCommand gate. Therefore the
        // final finite-cycle packet is suppressed, even though its color was built.
        Tick {
            color: if self.running { color } else { None },
            reached_end: end,
            running: self.running,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn every_finite_repeat_byte_completes_on_exact_boundary_and_suppresses_only_terminal_frame(){
        for repeat in 0..255u8{for length in [1,2,3,17]{
            let sequence=Sequence{repeat,frames:(0..length).map(|i|[i as u8,10,20]).collect()};
            let mut c=Cursor::new(sequence,33).unwrap();let total=usize::from(repeat.max(1))*length;
            for tick in 1..=total{let out=c.tick(tick as u64*33,1);assert_eq!(out.running,tick<total,"repeat{repeat} length{length} tick{tick}");assert_eq!(out.color,if tick==total{None}else{Some([((tick-1)%length)as u8,10,20])});}
            assert!(!c.running());assert!(c.tick((total+1)as u64*33,1).color.is_none());
        }}
    }
    fn s(repeat: u8) -> Sequence {
        Sequence {
            repeat,
            frames: vec![[1, 2, 3], [4, 5, 6], [7, 8, 9]],
        }
    }
    #[test]
    fn infinite_uses_old_frame_then_shared_phase_and_never_bursts_catchup() {
        let mut c = Cursor::new(s(255), 33).unwrap();
        assert_eq!(
            c.tick(33, 1),
            Tick {
                color: Some([1, 2, 3]),
                reached_end: false,
                running: true
            }
        );
        assert_eq!(c.tick(66, 1).color, Some([4, 5, 6]));
        assert_eq!(
            c.tick(99, 1),
            Tick {
                color: Some([7, 8, 9]),
                reached_end: true,
                running: true
            }
        );
        assert_eq!(c.tick(132, 1).color, Some([1, 2, 3]));
        assert_eq!(
            c.tick(297, 5),
            Tick {
                color: Some([4, 5, 6]),
                reached_end: true,
                running: true
            }
        );
        assert_eq!(c.tick(330, 1).color, Some([1, 2, 3]));
    }
    #[test]
    fn finite_zero_one_and_two_keep_native_last_packet_suppression() {
        for repeat in [0, 1, 2] {
            let mut c = Cursor::new(s(repeat), 33).unwrap();
            let mut colors = Vec::new();
            let mut ends = 0;
            for i in 1..=8 {
                let tick = c.tick(i * 33, 1);
                if let Some(v) = tick.color {
                    colors.push(v);
                }
                ends += usize::from(tick.reached_end);
            }
            let expected = if repeat == 2 {
                vec![[1, 2, 3], [4, 5, 6], [7, 8, 9], [1, 2, 3], [4, 5, 6]]
            } else {
                vec![[1, 2, 3], [4, 5, 6]]
            };
            assert_eq!(colors, expected);
            assert_eq!(ends, if repeat == 2 { 2 } else { 1 });
            assert!(!c.running());
        }
        let mut c = Cursor::new(
            Sequence {
                repeat: 1,
                frames: vec![[1, 2, 3]],
            },
            33,
        )
        .unwrap();
        assert_eq!(
            c.tick(33, 1),
            Tick {
                color: None,
                reached_end: true,
                running: false
            }
        );
    }
    #[test]
    fn stop_resume_and_restart_are_distinct_and_queued_ticks_are_one_transition() {
        let mut c = Cursor::new(s(2), 33).unwrap();
        assert_eq!(c.tick(33, 1).color, Some([1, 2, 3]));
        c.stop();
        assert_eq!(
            c.tick(66, 20),
            Tick {
                color: None,
                reached_end: false,
                running: false
            }
        );
        c.resume();
        assert_eq!(c.tick(99, 1).color, Some([4, 5, 6]));
        c.start();
        assert_eq!(c.tick(132, 1).color, Some([1, 2, 3]));
        assert_eq!(
            c.tick(165, 10),
            Tick {
                color: Some([4, 5, 6]),
                reached_end: true,
                running: true
            }
        );
        assert_eq!(
            c.tick(198, 0),
            Tick {
                color: None,
                reached_end: false,
                running: true
            }
        );
        assert_eq!(c.tick(231, 1).color, Some([1, 2, 3]));
    }
    #[test]
    fn bounded_data_rejects_invalid_rgb_shape_unknown_fields_repeat_and_clock() {
        for data in [
            json!({"repeat":256,"data":[]}),
            json!({"repeat":255,"data":[[1,2]]}),
            json!({"repeat":255,"data":[[1,2,256]]}),
            json!({"repeat":255,"data":[],"extra":1}),
            json!({"repeat":255,"data":vec![[1,2,3];4097]}),
        ] {
            assert!(Sequence::decode(&data).is_err());
        }
        assert!(Cursor::new(s(255), 0).is_err());
        assert!(Cursor::new(s(255), 60001).is_err());
        assert!(Cursor::new(
            Sequence {
                repeat: 0,
                frames: vec![]
            },
            33
        )
        .is_err());
        assert!(Sequence::decode(&json!({"repeat":0,"data":[]})).is_ok());
    }
}
