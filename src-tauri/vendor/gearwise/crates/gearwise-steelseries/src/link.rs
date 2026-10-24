//! Moving command packets to the device.
//!
//! SteelSeries mice use unnumbered reports: every packet goes out with report
//! id 0 in front. Most commands are output reports; a few models take the
//! lighting and button tables as feature reports.

use std::thread;
use std::time::Duration;

use gearwise_core::Result;
use gearwise_core::transport::HidTransport;

use crate::models::{Report, Wire};

/// Bit set in the command byte when talking through a 2.4 GHz dongle.
const DONGLE_FLAG: u8 = 0x40;
/// Dongles acknowledge every command with a 64-byte report that must be read.
const ACK_TIMEOUT_MS: i32 = 200;
/// Reply reports are at most this long.
const REPLY_LEN: usize = 64;
/// Input reports inspected while waiting for the answer to a query.
const REPLY_TRIES: usize = 4;

/// One command: the bytes after the report id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    pub report: Report,
    pub bytes: Vec<u8>,
}

impl Packet {
    pub fn output(bytes: Vec<u8>) -> Self {
        Self {
            report: Report::Output,
            bytes,
        }
    }
}

/// An open control interface.
pub struct Link {
    io: Box<dyn HidTransport>,
    wire: Wire,
    /// Pause after each packet; firmware drops commands that arrive too fast.
    gap: Duration,
}

impl Link {
    pub fn new(io: Box<dyn HidTransport>, wire: Wire, gap: Duration) -> Self {
        Self { io, wire, gap }
    }

    /// Send a command; dongles are then asked for their acknowledgement.
    pub fn send(&mut self, packet: &Packet) -> Result<()> {
        self.write(packet)?;
        if self.wire.dongle {
            self.read(ACK_TIMEOUT_MS)?;
        }
        Ok(())
    }

    /// Send a command and return the first reply `accept` recognises. `None`
    /// when the device stays silent or only sends something else.
    pub fn query(
        &mut self,
        packet: &Packet,
        accept: impl Fn(&[u8]) -> bool,
    ) -> Result<Option<Vec<u8>>> {
        self.write(packet)?;
        for _ in 0..REPLY_TRIES {
            let reply = self.read(ACK_TIMEOUT_MS)?;
            if reply.is_empty() {
                break;
            }
            if accept(&reply) {
                return Ok(Some(reply));
            }
        }
        Ok(None)
    }

    fn write(&mut self, packet: &Packet) -> Result<()> {
        let mut buf = Vec::with_capacity(1 + self.wire.pad.max(packet.bytes.len()));
        buf.push(0);
        buf.extend_from_slice(&packet.bytes);
        if buf.len() <= self.wire.pad {
            buf.resize(1 + self.wire.pad, 0);
        }
        if self.wire.dongle {
            buf[1] |= DONGLE_FLAG;
        }
        match packet.report {
            Report::Output => {
                self.io.write(&buf)?;
            }
            Report::Feature => self.io.send_feature_report(&buf)?,
        }
        if !self.gap.is_zero() {
            thread::sleep(self.gap);
        }
        Ok(())
    }

    fn read(&mut self, timeout_ms: i32) -> Result<Vec<u8>> {
        let mut buf = [0u8; REPLY_LEN];
        let n = self.io.read_timeout(&mut buf, timeout_ms)?;
        Ok(buf[..n].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use gearwise_core::Error;

    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    enum Op {
        Write(Vec<u8>),
        Feature(Vec<u8>),
        Read,
    }

    /// Records every operation, in order, and replays queued input reports.
    struct Tape {
        ops: Arc<Mutex<Vec<Op>>>,
        input: Vec<Vec<u8>>,
    }

    impl HidTransport for Tape {
        fn write(&mut self, data: &[u8]) -> Result<usize> {
            self.ops.lock().unwrap().push(Op::Write(data.to_vec()));
            Ok(data.len())
        }
        fn read_timeout(&mut self, buf: &mut [u8], _: i32) -> Result<usize> {
            self.ops.lock().unwrap().push(Op::Read);
            if self.input.is_empty() {
                return Ok(0);
            }
            let report = self.input.remove(0);
            buf[..report.len()].copy_from_slice(&report);
            Ok(report.len())
        }
        fn send_feature_report(&mut self, data: &[u8]) -> Result<()> {
            self.ops.lock().unwrap().push(Op::Feature(data.to_vec()));
            Ok(())
        }
        fn get_feature_report(&mut self, _: &mut [u8]) -> Result<usize> {
            Err(Error::Unsupported("feature reads"))
        }
    }

    fn link(wire: Wire, input: Vec<Vec<u8>>) -> (Link, Arc<Mutex<Vec<Op>>>) {
        let ops = Arc::new(Mutex::new(Vec::new()));
        let tape = Tape {
            ops: Arc::clone(&ops),
            input,
        };
        (Link::new(Box::new(tape), wire, Duration::ZERO), ops)
    }

    const PLAIN: Wire = Wire {
        pad: 0,
        dongle: false,
    };

    #[test]
    fn reports_go_out_with_id_zero() {
        let (mut l, ops) = link(PLAIN, vec![]);
        l.send(&Packet::output(vec![0x04, 0x00, 0x01])).unwrap();
        l.send(&Packet {
            report: Report::Feature,
            bytes: vec![0x5b, 0x00],
        })
        .unwrap();
        assert_eq!(
            *ops.lock().unwrap(),
            [
                Op::Write(vec![0, 0x04, 0x00, 0x01]),
                Op::Feature(vec![0, 0x5b, 0x00])
            ]
        );
    }

    #[test]
    fn padding_extends_short_packets_only() {
        let (mut l, ops) = link(
            Wire {
                pad: 6,
                dongle: false,
            },
            vec![],
        );
        l.send(&Packet::output(vec![1, 2])).unwrap();
        l.send(&Packet::output(vec![1; 8])).unwrap();
        assert_eq!(
            *ops.lock().unwrap(),
            [
                Op::Write(vec![0, 1, 2, 0, 0, 0, 0]),
                Op::Write([&[0][..], &[1; 8]].concat())
            ]
        );
    }

    #[test]
    fn dongle_flags_the_command_and_reads_the_acknowledgement() {
        let (mut l, ops) = link(
            Wire {
                pad: 0,
                dongle: true,
            },
            vec![vec![0; 64]],
        );
        l.send(&Packet::output(vec![0x2d, 1, 0, 9])).unwrap();
        assert_eq!(
            *ops.lock().unwrap(),
            [Op::Write(vec![0, 0x6d, 1, 0, 9]), Op::Read]
        );
    }

    #[test]
    fn plain_wires_are_never_read_after_a_command() {
        let (mut l, ops) = link(PLAIN, vec![vec![0; 64]]);
        l.send(&Packet::output(vec![9, 0])).unwrap();
        assert_eq!(*ops.lock().unwrap(), [Op::Write(vec![0, 9, 0])]);
    }

    #[test]
    fn query_skips_unrelated_reports_and_gives_up_when_silent() {
        let (mut l, _) = link(PLAIN, vec![vec![1, 1], vec![2, 2], vec![3, 3]]);
        let got = l.query(&Packet::output(vec![0x92]), |r| r[0] == 2).unwrap();
        assert_eq!(got, Some(vec![2, 2]));

        let (mut l, ops) = link(PLAIN, vec![vec![1, 1]]);
        let got = l.query(&Packet::output(vec![0x92]), |r| r[0] == 2).unwrap();
        assert_eq!(got, None);
        // One stray report, then the timeout ends the wait.
        assert_eq!(ops.lock().unwrap()[1..], [Op::Read, Op::Read]);
    }

    #[test]
    fn query_does_not_consume_the_dongle_acknowledgement() {
        // The answer to a query is the reply itself, not an extra ack.
        let (mut l, ops) = link(
            Wire {
                pad: 0,
                dongle: true,
            },
            vec![vec![0xd2, 0x0b]],
        );
        let got = l.query(&Packet::output(vec![0x92]), |_| true).unwrap();
        assert_eq!(got, Some(vec![0xd2, 0x0b]));
        assert_eq!(*ops.lock().unwrap(), [Op::Write(vec![0, 0xd2]), Op::Read]);
    }
}
