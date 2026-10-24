//! The 90-byte Razer feature report: layout, checksum and status codes.
//!
//! Report layout (the HID layer prepends report id 0, giving 91 bytes on the
//! wire):
//!
//! | offset | field |
//! |--------|-------|
//! | 0      | status: 0 new, 1 busy, 2 success, 3 failure, 4 timeout, 5 not supported |
//! | 1      | transaction id (device specific, echoed in the reply) |
//! | 2..4   | remaining packets, big endian |
//! | 4      | protocol type, always 0 |
//! | 5      | size of the meaningful argument bytes |
//! | 6      | command class |
//! | 7      | command id; bit 7 set means "read" |
//! | 8..88  | 80 argument bytes |
//! | 88     | checksum: XOR of bytes 2..88 (status and transaction id excluded) |
//! | 89     | reserved, 0 |

use gearwise_core::{Error, Result};

/// Length of the report proper.
pub const REPORT_LEN: usize = 90;
/// Length of the feature-report buffer including the leading report id byte.
pub const WIRE_LEN: usize = REPORT_LEN + 1;
/// Capacity of the argument area.
pub const MAX_ARGS: usize = 80;

/// Offset of the first argument byte in the wire buffer (report id + 8 header bytes).
const ARGS_AT: usize = 9;
/// Offset of the checksum byte in the wire buffer.
const CRC_AT: usize = 89;

/// Decoded `status` byte of a response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The device has not started on the command yet.
    New,
    /// The device is still working on the command.
    Busy,
    Success,
    Failure,
    /// The device (usually a sleeping wireless mouse) did not answer.
    Timeout,
    NotSupported,
    Unknown(u8),
}

impl From<u8> for Status {
    fn from(b: u8) -> Self {
        match b {
            0x00 => Status::New,
            0x01 => Status::Busy,
            0x02 => Status::Success,
            0x03 => Status::Failure,
            0x04 => Status::Timeout,
            0x05 => Status::NotSupported,
            other => Status::Unknown(other),
        }
    }
}

/// One request or response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub status: u8,
    pub transaction_id: u8,
    pub remaining_packets: u16,
    pub data_size: u8,
    pub class: u8,
    pub command: u8,
    pub args: [u8; MAX_ARGS],
}

impl Report {
    /// A request for `class`/`command`; `args` is zero-padded to the full
    /// argument area. The transaction id is stamped by the link.
    pub fn new(class: u8, command: u8, data_size: u8, args: &[u8]) -> Self {
        let mut a = [0u8; MAX_ARGS];
        let n = args.len().min(MAX_ARGS);
        a[..n].copy_from_slice(&args[..n]);
        Self {
            status: 0,
            transaction_id: 0,
            remaining_packets: 0,
            data_size,
            class,
            command,
            args: a,
        }
    }

    pub fn status(&self) -> Status {
        Status::from(self.status)
    }

    /// Serialise into the 91-byte buffer for `send_feature_report`, computing
    /// the checksum.
    pub fn to_wire(&self) -> [u8; WIRE_LEN] {
        let mut w = [0u8; WIRE_LEN];
        w[1] = self.status;
        w[2] = self.transaction_id;
        w[3..5].copy_from_slice(&self.remaining_packets.to_be_bytes());
        // w[5] is the protocol type, always 0.
        w[6] = self.data_size;
        w[7] = self.class;
        w[8] = self.command;
        w[ARGS_AT..ARGS_AT + MAX_ARGS].copy_from_slice(&self.args);
        // Report bytes 2..88 are wire bytes 3..89.
        w[CRC_AT] = w[3..CRC_AT].iter().fold(0, |acc, b| acc ^ b);
        w
    }

    /// Parse a buffer returned by `get_feature_report` (report id first).
    ///
    /// A 90-byte buffer is accepted as a report whose id byte the HID stack
    /// stripped. The checksum of responses is not verified, as in the
    /// reference drivers.
    pub fn from_wire(buf: &[u8]) -> Result<Self> {
        let body = match buf.len() {
            WIRE_LEN.. => &buf[1..WIRE_LEN],
            REPORT_LEN => buf,
            n => {
                return Err(Error::protocol(format!(
                    "short report: {n} of {WIRE_LEN} bytes"
                )));
            }
        };
        let mut args = [0u8; MAX_ARGS];
        args.copy_from_slice(&body[ARGS_AT - 1..ARGS_AT - 1 + MAX_ARGS]);
        Ok(Self {
            status: body[0],
            transaction_id: body[1],
            remaining_packets: u16::from_be_bytes([body[2], body[3]]),
            data_size: body[5],
            class: body[6],
            command: body[7],
            args,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_matches_captured_brightness_packet() {
        // From a capture documented in openrazer: set brightness 0xb7 on the
        // logo LED, transaction id 0x3f:
        //   00 3f 0000 00 03 0f 04 01 04 b7 ...
        // checksum = 03 ^ 0f ^ 04 ^ 01 ^ 04 ^ b7 = 0xba
        let mut r = Report::new(0x0f, 0x04, 0x03, &[0x01, 0x04, 0xb7]);
        r.transaction_id = 0x3f;
        let w = r.to_wire();
        assert_eq!(w.len(), 91);
        assert_eq!(
            &w[..12],
            &[
                0x00, 0x00, 0x3f, 0x00, 0x00, 0x00, 0x03, 0x0f, 0x04, 0x01, 0x04, 0xb7
            ]
        );
        assert_eq!(w[89], 0xba);
        assert_eq!(w[90], 0x00);
        assert!(w[12..89].iter().all(|&b| b == 0));
    }

    #[test]
    fn checksum_ignores_status_and_transaction_id() {
        let mut a = Report::new(0x04, 0x05, 0x07, &[1, 3, 0x20, 3, 0x20]);
        let b = a.clone();
        a.transaction_id = 0xff;
        a.status = 0x02;
        assert_eq!(a.to_wire()[89], b.to_wire()[89]);
    }

    #[test]
    fn wire_round_trip() {
        let mut r = Report::new(0x07, 0x80, 0x02, &[0, 0x80]);
        r.transaction_id = 0x1f;
        r.status = 0x02;
        r.remaining_packets = 0x0102;
        let back = Report::from_wire(&r.to_wire()).unwrap();
        assert_eq!(back, r);
        assert_eq!(back.status(), Status::Success);
    }

    #[test]
    fn short_buffers_are_rejected() {
        assert!(matches!(
            Report::from_wire(&[0u8; 40]),
            Err(Error::Protocol(_))
        ));
    }

    #[test]
    fn a_report_without_its_id_byte_still_parses() {
        let mut r = Report::new(0x04, 0x85, 0x07, &[0, 0x03, 0x20]);
        r.status = 0x02;
        let wire = r.to_wire();
        assert_eq!(Report::from_wire(&wire[1..]).unwrap(), r);
    }

    #[test]
    fn status_codes_decode() {
        let all: Vec<_> = (0u8..=6).map(Status::from).collect();
        assert_eq!(
            all,
            [
                Status::New,
                Status::Busy,
                Status::Success,
                Status::Failure,
                Status::Timeout,
                Status::NotSupported,
                Status::Unknown(6)
            ]
        );
    }
}
