//! Offline request/reply codec for pinned Prime1.0.15 feature diagnostics.
//! GET_FEATURE returns an untagged cache: parsed bytes are NEVER fresh-read,
//! hardware identity, idle-state or write authorization evidence on their own.
use crate::prime_macro_storage::OBSERVED_BYTES;
use gearwise_core::{Error, Result};
pub const PAYLOAD_BYTES: usize = 642;
pub const REPORT_BYTES: usize = 643;
const WINDOW_BASE: usize = 0x20000bd8;
const MACRO_BASE: usize = 0x20001e73;
const MAX_CHUNK: usize = 512;
const VERSION: &[u8] = b"1.0.15+4244c08";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Read {
    address: Option<usize>,
    length: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedBytes {
    bytes: Vec<u8>,
}
impl CachedBytes {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Explicitly false: a successful cached GET does not acknowledge its SET.
    pub fn freshness_verified(&self) -> bool {
        false
    }
}
impl Read {
    pub fn version() -> Self {
        Self {
            address: None,
            length: 64,
        }
    }
    pub fn physical() -> Self {
        Self {
            address: Some(0x20001cb6),
            length: 30,
        }
    }
    pub fn wheel() -> Self {
        Self {
            address: Some(0x20001cd4),
            length: 10,
        }
    }
    pub fn contexts() -> Self {
        Self {
            address: Some(0x20001c00),
            length: 180,
        }
    }
    pub fn macro_chunk(offset: usize, length: usize) -> Result<Self> {
        if length == 0
            || length > MAX_CHUNK
            || offset
                .checked_add(length)
                .is_none_or(|end| end > OBSERVED_BYTES)
        {
            return Err(Error::invalid("Prime diagnostic macro read range"));
        }
        Ok(Self {
            address: Some(MACRO_BASE + offset),
            length,
        })
    }
    pub(crate) fn is_version(&self) -> bool {
        self.address.is_none()
    }
    pub fn subrange(&self, offset: usize, length: usize) -> Result<Self> {
        if self.address.is_none()
            || length == 0
            || length > 512
            || offset
                .checked_add(length)
                .is_none_or(|end| end > self.length)
        {
            return Err(Error::invalid("Prime read subrange"));
        }
        Ok(Self {
            address: self.address.map(|a| a + offset),
            length,
        })
    }
    pub fn output_report(&self) -> Result<Vec<u8>> {
        if self.length > 64 {
            return Err(Error::invalid("Prime output read exceeds64 bytes"));
        }
        let mut report = self.report();
        report.truncate(65);
        Ok(report)
    }
    pub fn expected_bytes(&self) -> usize {
        self.length
    }
    /// Complete unnumbered HID feature buffer, including the required ID0.
    /// Only known read commands90/83 are emitted; no raw write/address API.
    pub fn report(&self) -> Vec<u8> {
        let mut bytes = vec![0; REPORT_BYTES];
        if let Some(address) = self.address {
            bytes[1] = 0x83;
            bytes[3..5].copy_from_slice(&((address - WINDOW_BASE) as u16).to_le_bytes());
            bytes[5..7].copy_from_slice(&(self.length as u16).to_le_bytes());
        } else {
            bytes[1] = 0x90;
        }
        bytes
    }
    /// Parse only an observed cached reply, never an acknowledged live image.
    /// HIDAPI's ID0 wrapper is supplied by the modeled transport boundary;
    /// actual Windows transfer/capture validation remains necessary.
    pub fn parse_cached(&self, report: &[u8]) -> Result<CachedBytes> {
        if report.len() != REPORT_BYTES || report[0] != 0 {
            return Err(Error::protocol("Prime cached feature report length/ID"));
        }
        Ok(CachedBytes {
            bytes: report[1..1 + self.length].into(),
        })
    }
    pub fn matches_pinned_cached_version(&self, cached: &CachedBytes) -> bool {
        if self.address.is_some() || cached.bytes.len() != 64 {
            return false;
        }
        let mut expected = vec![0; 64];
        expected[..VERSION.len()].copy_from_slice(VERSION);
        cached.bytes == expected
    }
}
/// A complete observation recipe. It is not an atomic or fresh snapshot.
pub fn image_reads() -> Vec<Read> {
    let mut reads = vec![
        Read::version(),
        Read::physical(),
        Read::wheel(),
        Read::contexts(),
    ];
    for offset in (0..OBSERVED_BYTES).step_by(MAX_CHUNK) {
        reads.push(
            Read::macro_chunk(offset, (OBSERVED_BYTES - offset).min(MAX_CHUNK))
                .expect("bounded chunk"),
        );
    }
    reads
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_read_headers_use_byte_offsets_and_counts_and_never_write_commands() {
        for (read, offset, length) in [
            (Read::physical(), 0x10de, 30),
            (Read::wheel(), 0x10fc, 10),
            (Read::contexts(), 0x1028, 180),
            (Read::macro_chunk(0, 512).unwrap(), 0x129b, 512),
            (Read::macro_chunk(4608, 512).unwrap(), 0x249b, 512),
        ] {
            let bytes = read.report();
            assert_eq!(bytes.len(), 643);
            assert_eq!(&bytes[..3], &[0, 0x83, 0]);
            assert_eq!(u16::from_le_bytes([bytes[3], bytes[4]]), offset);
            assert_eq!(u16::from_le_bytes([bytes[5], bytes[6]]), length);
            assert!(bytes[7..].iter().all(|v| *v == 0));
        }
        let v = Read::version().report();
        assert_eq!(&v[..2], &[0, 0x90]);
        assert!(v[2..].iter().all(|b| *b == 0));
    }
    #[test]
    fn complete_recipe_covers_exact_firmware_observation_range_without_allocating_it() {
        let reads = image_reads();
        assert_eq!(reads.len(), 14);
        for (index, read) in reads[4..].iter().enumerate() {
            assert_eq!(read.address, Some(MACRO_BASE + index * 512));
            assert_eq!(read.length, 512);
        }
        for (offset, length) in [
            (0, 0),
            (0, 513),
            (OBSERVED_BYTES, 1),
            (OBSERVED_BYTES - 1, 2),
            (usize::MAX, 1),
        ] {
            assert!(Read::macro_chunk(offset, length).is_err());
        }
        assert!(Read::macro_chunk(OBSERVED_BYTES - 1, 1).is_ok());
    }
    #[test]
    fn cached_bytes_and_even_correct_version_never_become_fresh_evidence() {
        let read = Read::version();
        let mut report = vec![0; 643];
        report[1..1 + VERSION.len()].copy_from_slice(VERSION);
        let parsed = read.parse_cached(&report).unwrap();
        assert_eq!(parsed.bytes().len(), 64);
        assert!(read.matches_pinned_cached_version(&parsed));
        assert!(!parsed.freshness_verified());
        assert!(!Read::physical().matches_pinned_cached_version(&parsed));
        report[1] ^= 1;
        assert!(!read.matches_pinned_cached_version(&read.parse_cached(&report).unwrap()));
        for len in [0, 1, 64, 642, 644] {
            assert!(read.parse_cached(&vec![0; len]).is_err());
        }
        let mut report = vec![0; 643];
        report[0] = 1;
        assert!(read.parse_cached(&report).is_err());
    }
    #[test]
    fn output_subranges_keep_their_parent_extent_and_complete_id_zero_frame() {
        let part = Read::macro_chunk(4608, 512)
            .unwrap()
            .subrange(510, 2)
            .unwrap();
        let output = part.output_report().unwrap();
        assert_eq!(output.len(), 65);
        assert_eq!(&output[..7], &[0, 0x83, 0, 0x99, 0x26, 2, 0]);
        assert!(output[7..].iter().all(|b| *b == 0));
        assert!(Read::version().subrange(0, 1).is_err());
        assert!(Read::contexts().output_report().is_err());
        for (offset, length) in [(0, 0), (0, 513), (180, 1), (179, 2), (usize::MAX, 1)] {
            assert!(Read::contexts().subrange(offset, length).is_err());
        }
        assert_eq!(Read::physical().output_report().unwrap().len(), 65);
    }
}
