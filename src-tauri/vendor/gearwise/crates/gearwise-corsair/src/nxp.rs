//! Legacy ("NXP") protocol client.
//!
//! Fixed 64-byte packets. `0x07` writes a field (no reply), `0x0e` reads one
//! (the first four bytes are echoed back), `0x7f` streams up to 60 data
//! bytes per packet for colour buffers.

use gearwise_core::HidTransport;
use gearwise_core::error::{Error, Result};

pub(crate) const PACKET: usize = 64;

pub(crate) const SET: u8 = 0x07;
pub(crate) const GET: u8 = 0x0e;
const STREAM: u8 = 0x7f;

/// Field ids.
pub(crate) mod field {
    pub const IDENT: u8 = 0x01;
    /// Special-function mode: hardware (1) or software (2) control of the
    /// DPI/sniper buttons and the M-keys.
    pub const SPECIAL: u8 = 0x04;
    /// Lighting control: 2 switches to software lighting.
    pub const LIGHTING: u8 = 0x05;
    pub const POLL_RATE: u8 = 0x0a;
    pub const MOUSE: u8 = 0x13;
    pub const MOUSE_COLOR: u8 = 0x22;
    pub const KEYBOARD_COLOR: u8 = 0x28;
    pub const KEY_INPUT: u8 = 0x40;
}

/// Mouse sub-commands (field 0x13).
pub(crate) mod mouse {
    pub const DPI: u8 = 0x02;
    pub const LIFT: u8 = 0x03;
    pub const SNAP: u8 = 0x04;
    pub const DPI_MASK: u8 = 0x05;
    /// OR-ed with the stage number.
    pub const DPI_PROFILE: u8 = 0xd0;
}

const MAX_SKIPPED: usize = 32;
const REPLY_TIMEOUT_MS: i32 = 250;

/// Result of the firmware identification read (`0e 01`).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Ident {
    pub firmware: String,
    /// Polling interval in ms.
    pub poll_ms: u8,
    /// Mice: bit n set if lighting zone n exists. 0 when not reported.
    pub zones: u8,
}

pub(crate) struct Nxp {
    t: Box<dyn HidTransport>,
}

impl Nxp {
    pub(crate) fn new(t: Box<dyn HidTransport>) -> Self {
        Self { t }
    }

    /// Write one packet (zero padded to 64 bytes).
    pub(crate) fn send(&mut self, bytes: &[u8]) -> Result<()> {
        debug_assert!(bytes.len() <= PACKET);
        let mut out = [0u8; 1 + PACKET];
        out[1..1 + bytes.len()].copy_from_slice(bytes);
        let written=self.t.write(&out)?;if written!=out.len(){return Err(Error::transport(format!("short NXP write: {written}/{}",out.len())));}
        Ok(())
    }

    /// Send a read command and wait for the packet echoing its first four
    /// bytes. Input events and unrelated replies are skipped.
    pub(crate) fn query(&mut self, request: &[u8]) -> Result<[u8; PACKET]> {
        let mut head = [0u8; 4];
        let n = request.len().min(4);
        head[..n].copy_from_slice(&request[..n]);
        self.send(request)?;
        let mut buf = [0u8; PACKET];
        for _ in 0..MAX_SKIPPED {
            buf.fill(0);
            if self.t.read_timeout(&mut buf, REPLY_TIMEOUT_MS)? == 0 {
                return Err(Error::Timeout);
            }
            if buf[..4] == head {
                return Ok(buf);
            }
        }
        Err(Error::Timeout)
    }

    /// One packet of a colour stream: `7f <sequence> <length> 00 <data>`.
    pub(crate) fn stream(&mut self, sequence: u8, data: &[u8]) -> Result<()> {
        debug_assert!(data.len() <= 60);
        let mut p = vec![STREAM, sequence, data.len() as u8, 0];
        p.extend_from_slice(data);
        self.send(&p)
    }

    pub(crate) fn ident(&mut self) -> Result<Ident> {
        let r = self.query(&[GET, field::IDENT, 0, 0])?;
        let fw = u16::from_le_bytes([r[8], r[9]]);
        Ok(Ident {
            firmware: format!("{:x}.{:02x}", fw >> 8, fw & 0xff),
            poll_ms: r[16],
            // Mice append 0xc1 and a zone bitmask to the identification.
            zones: if r[21] == 0xc1 { r[25] } else { 0 },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeNxp;

    #[test]
    fn send_pads_to_a_full_report() {
        let fake = FakeNxp::new();
        let mut nxp = fake.nxp();
        nxp.send(&[SET, field::POLL_RATE, 0, 0, 2]).unwrap();
        let w = fake.writes();
        assert_eq!(w[0].len(), 65);
        assert_eq!(w[0][..6], [0x00, 0x07, 0x0a, 0x00, 0x00, 0x02]);
        assert!(w[0][6..].iter().all(|b| *b == 0));
    }

    #[test]
    fn ident_is_parsed() {
        let fake = FakeNxp::new();
        fake.ident_for_mouse(0x0206, 2, 0x56);
        let id = fake.nxp().ident().unwrap();
        assert_eq!(
            id,
            Ident {
                firmware: "2.06".into(),
                poll_ms: 2,
                zones: 0x56
            }
        );
        assert_eq!(fake.writes()[0][1..5], [0x0e, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn keyboard_ident_reports_no_zones() {
        let fake = FakeNxp::new();
        fake.ident_for_keyboard(0x0130, 1);
        let id = fake.nxp().ident().unwrap();
        assert_eq!(
            id,
            Ident {
                firmware: "1.30".into(),
                poll_ms: 1,
                zones: 0
            }
        );
    }

    #[test]
    fn stream_packets_carry_a_length_header() {
        let fake = FakeNxp::new();
        fake.nxp().stream(2, &[9, 8, 7]).unwrap();
        assert_eq!(fake.writes()[0][1..8], [0x7f, 0x02, 0x03, 0x00, 9, 8, 7]);
    }

    #[test]
    fn unanswered_query_times_out_and_events_are_skipped() {
        let fake = FakeNxp::new();
        assert!(matches!(
            fake.nxp().query(&[GET, 0x13, 0x05, 1]),
            Err(Error::Timeout)
        ));
        fake.reply(&[GET, 0x13, 0x05, 1], &[0x3f]);
        // An HID event report precedes the answer.
        fake.queue_event(vec![0x01, 0x01, 0x00]);
        assert_eq!(fake.nxp().query(&[GET, 0x13, 0x05, 1]).unwrap()[4], 0x3f);
    }
}
