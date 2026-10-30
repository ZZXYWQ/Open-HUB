//! Pinned Prime1.0.15 output/interrupt read candidates, not write authority.
//! Requires an owned ordered interface and a new unpredictable128-bit session
//! seed. No physical identity, atomic image, quiet inputs or firmware parity
//! is established here. No application/service caller is enabled.
use crate::prime_feature_read::{REPORT_BYTES, Read};
use gearwise_core::{Error, Result, transport::HidTransport};
use std::{thread, time::Duration};
const INPUT: usize = 64;
const ECHO: usize = 32;
const REPORT_LIMIT: usize = 32;
fn notification(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xaa, 0x55, 0xf0]) || bytes.starts_with(&[0xaa, 0x55, 0xff])
}
pub struct Candidate {
    bytes: Vec<u8>,
    fences: u64,
    narrowed: bool,
}
impl Candidate {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn completed_fences(&self) -> u64 {
        self.fences
    }
    pub fn used_narrow_reads(&self) -> bool {
        self.narrowed
    }
    pub fn physical_identity_verified(&self) -> bool {
        false
    }
    pub fn atomic_image_verified(&self) -> bool {
        false
    }
}
pub struct Reader<T: HidTransport> {
    io: T,
    seed: [u8; 16],
    sequence: u128,
    fences: u64,
    gap: Duration,
}
enum Exchange {
    Data(Vec<u8>),
    Narrow,
}
impl<T: HidTransport> Reader<T> {
    /// Caller must supply a fresh unpredictable seed for this owned transport;
    /// reusing it across Reader instances can admit an old matching fence.
    pub fn new(io: T, seed: [u8; 16]) -> Result<Self> {
        if seed == [0; 16] || seed[..2] == [0xaa, 0x55] {
            return Err(Error::invalid(
                "Prime session seed is empty or notification-prefixed",
            ));
        }
        Ok(Self {
            io,
            seed,
            sequence: 0,
            fences: 0,
            gap: Duration::from_millis(30),
        })
    }
    fn write(&mut self, frame: &[u8]) -> Result<()> {
        let n = self.io.write(frame)?;
        if n != frame.len() {
            return Err(Error::protocol("Prime output short write"));
        }
        if !self.gap.is_zero() {
            thread::sleep(self.gap);
        }
        Ok(())
    }
    fn receive(&mut self) -> Result<[u8; INPUT]> {
        let mut frame = [0; INPUT];
        let n = self.io.read_timeout(&mut frame, 200)?;
        if n == 0 {
            return Err(Error::protocol("Prime output read timeout"));
        }
        if n != INPUT {
            return Err(Error::protocol("Prime interrupt reply must be64 bytes"));
        }
        Ok(frame)
    }
    fn exchange(&mut self, read: &Read) -> Result<Exchange> {
        let request = read.output_report()?;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(Error::invalid("Prime nonce sequence exhausted"))?;
        let mut challenge = [0; ECHO];
        challenge[..16].copy_from_slice(&self.seed);
        challenge[16..].copy_from_slice(&self.sequence.to_le_bytes());
        let mut expected = [0; INPUT];
        expected[..ECHO].copy_from_slice(&challenge);
        // Read our staged request payload at14fc+6 through the byte window.
        // Source memory is request scratch, not mouse configuration/storage.
        let mut fence = vec![0; 65];
        fence[1..7].copy_from_slice(&[0x83, 0, 0x2a, 0x09, ECHO as u8, 0]);
        fence[7..7 + ECHO].copy_from_slice(&challenge);
        self.write(&fence)?;
        let mut matched = false;
        for _ in 0..REPORT_LIMIT {
            if self.receive()? == expected {
                matched = true;
                break;
            }
        }
        if !matched {
            return Err(Error::protocol("Prime nonce fence not observed"));
        }
        self.fences = self
            .fences
            .checked_add(1)
            .ok_or(Error::invalid("Prime fence counter exhausted"))?;
        self.write(&request)?;
        for _ in 0..REPORT_LIMIT {
            let frame = self.receive()?;
            if frame == expected {
                return Ok(Exchange::Narrow);
            }
            if notification(&frame) {
                // For a short read both known notifications have a nonzero
                // byte beyond the payload; they cannot be canonical replies.
                if read.is_version() || read.expected_bytes() <= 2 {
                    continue;
                }
                return Ok(Exchange::Narrow);
            }
            if frame[read.expected_bytes()..].iter().any(|b| *b != 0) {
                return Err(Error::protocol("Prime output reply has nonzero padding"));
            }
            if read.is_version() {
                let mut feature = vec![0; REPORT_BYTES];
                feature[1..65].copy_from_slice(&frame);
                if !read.matches_pinned_cached_version(&read.parse_cached(&feature)?) {
                    return Err(Error::protocol(
                        "Prime output firmware version differs from pinned build",
                    ));
                }
            }
            return Ok(Exchange::Data(frame[..read.expected_bytes()].to_vec()));
        }
        Err(Error::protocol(
            "Prime output notification budget exhausted",
        ))
    }
    /// Build an ordered reply candidate. Multiple chunks are not atomic; this
    /// result must not authorize macro writes or substitute for identity/idle
    /// proof. On error no automatic retry or device selection is performed.
    pub fn read(&mut self, read: &Read) -> Result<Candidate> {
        let previous = self.fences;
        let mut bytes = Vec::new();
        let mut narrowed = false;
        for offset in (0..read.expected_bytes()).step_by(INPUT) {
            let length = (read.expected_bytes() - offset).min(INPUT);
            let part = if read.is_version() {
                read.clone()
            } else {
                read.subrange(offset, length)?
            };
            match self.exchange(&part)? {
                Exchange::Data(data) => bytes.extend(data),
                Exchange::Narrow => {
                    if part.is_version() || length <= 2 {
                        return Err(Error::protocol("Prime ambiguous short/version reply"));
                    }
                    narrowed = true;
                    for sub in (0..length).step_by(2) {
                        match self.exchange(&part.subrange(sub, (length - sub).min(2))?)? {
                            Exchange::Data(data) => bytes.extend(data),
                            Exchange::Narrow => {
                                return Err(Error::protocol("Prime ambiguous narrow reply"));
                            }
                        }
                    }
                }
            }
        }
        Ok(Candidate {
            bytes,
            fences: self.fences - previous,
            narrowed,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use gearwise_core::transport::{MockOp, MockRecord, MockReply, MockTransport};
    fn responder(data: Vec<u8>, noisy: bool) -> MockTransport {
        MockTransport::new(move |op| {
            let MockOp::Write(frame) = op else {
                panic!("feature operation was used")
            };
            assert_eq!(frame.len(), 65);
            assert_eq!(frame[0], 0);
            let mut value = vec![0; 64];
            if &frame[1..7] == [0x83, 0, 0x2a, 0x09, 32, 0] {
                value[..32].copy_from_slice(&frame[7..39]);
            } else {
                assert_eq!(frame[1], 0x83);
                let offset = usize::from(u16::from_le_bytes([frame[3], frame[4]])) - 0x129b;
                let count = usize::from(u16::from_le_bytes([frame[5], frame[6]]));
                value[..count].copy_from_slice(&data[offset..offset + count]);
            }
            let mut input_reports = Vec::new();
            if noisy {
                let mut notify = vec![0; 64];
                notify[..3].copy_from_slice(&[0xaa, 0x55, 0xff]);
                input_reports.push(notify);
            }
            input_reports.push(value);
            MockReply {
                input_reports,
                feature: None,
            }
        })
    }
    #[test]
    fn nonce_fenced_read_preserves_data_without_feature_queries() {
        let io = responder((0..64).collect(), false);
        let log = io.log();
        let mut reader = Reader::new(io, [7; 16]).unwrap();
        reader.gap = Duration::ZERO;
        let result = reader.read(&Read::macro_chunk(0, 30).unwrap()).unwrap();
        assert_eq!(result.bytes(), &(0..30).collect::<Vec<_>>());
        assert_eq!(result.completed_fences(), 1);
        assert!(!result.used_narrow_reads());
        assert!(!result.physical_identity_verified());
        assert!(!result.atomic_image_verified());
        let records = log.lock().unwrap();
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|r| matches!(r, MockRecord::Write(_))));
    }
    #[test]
    fn real_data_matching_notification_header_is_reconstructed_by_two_byte_reads() {
        let mut data: Vec<u8> = (0..64).collect();
        data[..3].copy_from_slice(&[0xaa, 0x55, 0xf0]);
        let mut reader = Reader::new(responder(data.clone(), false), [9; 16]).unwrap();
        reader.gap = Duration::ZERO;
        let result = reader.read(&Read::macro_chunk(0, 64).unwrap()).unwrap();
        assert_eq!(result.bytes(), data);
        assert!(result.used_narrow_reads());
        assert_eq!(result.completed_fences(), 33);
    }
    #[test]
    fn notifications_and_abandoned_wide_reply_do_not_become_narrow_read_data() {
        let data: Vec<u8> = (0..31).collect();
        let mut reader = Reader::new(responder(data.clone(), true), [11; 16]).unwrap();
        reader.gap = Duration::ZERO;
        let result = reader.read(&Read::macro_chunk(0, 31).unwrap()).unwrap();
        assert_eq!(result.bytes(), data);
        assert!(result.used_narrow_reads());
        assert_eq!(result.completed_fences(), 17);
    }
    #[test]
    fn timeout_bad_length_padding_and_wrong_version_never_return_candidates() {
        for mode in 0..4 {
            let mut echo_seen = false;
            let io = MockTransport::new(move |op| {
                let MockOp::Write(frame) = op else { panic!() };
                if !echo_seen {
                    echo_seen = true;
                    if mode == 0 {
                        return MockReply::none();
                    }
                    let mut echo = vec![0; 64];
                    echo[..32].copy_from_slice(&frame[7..39]);
                    return MockReply::input(echo);
                }
                match mode {
                    1 => MockReply::input(vec![1; 63]),
                    2 => {
                        let mut b = vec![0; 64];
                        b[63] = 1;
                        MockReply::input(b)
                    }
                    _ => MockReply::input(vec![0; 64]),
                }
            });
            let mut reader = Reader::new(io, [3; 16]).unwrap();
            reader.gap = Duration::ZERO;
            let read = if mode == 3 {
                Read::version()
            } else {
                Read::macro_chunk(0, 30).unwrap()
            };
            assert!(reader.read(&read).is_err());
        }
    }
    #[test]
    fn seed_and_sequence_are_bounded_and_each_query_uses_a_distinct_challenge() {
        assert!(Reader::new(responder(vec![0; 64], false), [0; 16]).is_err());
        let mut bad = [1; 16];
        bad[..2].copy_from_slice(&[0xaa, 0x55]);
        assert!(Reader::new(responder(vec![0; 64], false), bad).is_err());
        let io = responder(vec![0; 64], false);
        let log = io.log();
        let mut reader = Reader::new(io, [2; 16]).unwrap();
        reader.gap = Duration::ZERO;
        reader.read(&Read::macro_chunk(0, 2).unwrap()).unwrap();
        reader.read(&Read::macro_chunk(2, 2).unwrap()).unwrap();
        let records = log.lock().unwrap();
        let (MockRecord::Write(first), MockRecord::Write(second)) = (&records[0], &records[2])
        else {
            panic!()
        };
        assert_ne!(&first[7..39], &second[7..39]);
        drop(records);
        reader.sequence = u128::MAX;
        assert!(reader.read(&Read::macro_chunk(0, 2).unwrap()).is_err());
        assert_eq!(log.lock().unwrap().len(), 4);
    }
}
