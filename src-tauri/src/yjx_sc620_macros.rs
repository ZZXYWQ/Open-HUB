//! Static SC620 macro memory codec. No live writer or driver registration.
//! Delays are raw record fields, not a claim of measured firmware timing.
use super::{invalid, output, FRAME};
use gearwise_core::Result;
use serde::{Deserialize, Serialize};
pub const CAPACITY: usize = 0x1080;
const HEADER: usize = 64;
const SLOTS: usize = 32;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Action {
    KeyDown(u8),
    KeyUp(u8),
    MouseDown(u8),
    MouseUp(u8),
    Wheel(i8),
    /// Exact OEM packed movement payload; axis polarity is not yet measured.
    Motion {
        packed: u8,
    },
}
impl Action {
    fn encode(&self) -> Result<[u8; 2]> {
        Ok(match *self {
            Self::KeyDown(k) if (4..=0xa4).contains(&k) => [0x42, k],
            Self::KeyUp(k) if (4..=0xa4).contains(&k) => [2, k],
            Self::MouseDown(bit) if bit.is_power_of_two() && bit <= 16 => [0x43, bit],
            Self::MouseUp(bit) if bit.is_power_of_two() && bit <= 16 => [3, bit],
            Self::Wheel(n) if n == 1 || n == -1 => [0x44, n as u8],
            Self::Motion { packed } => [0x46, packed],
            _ => return Err(invalid("unverified macro event")),
        })
    }
    fn decode(kind: u8, value: u8) -> Result<Self> {
        let a = match kind {
            0x42 => Self::KeyDown(value),
            2 => Self::KeyUp(value),
            0x43 => Self::MouseDown(value),
            3 => Self::MouseUp(value),
            0x44 => Self::Wheel(value as i8),
            0x46 => Self::Motion { packed: value },
            _ => return Err(invalid("opaque macro action; original bytes retained")),
        };
        a.encode()?;
        Ok(a)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    /// Exact two-byte field before action bytes; motion shares its high bits
    /// with axis flags. No unit conversion.
    pub delay_raw: u16,
    pub action: Action,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    bytes: Vec<u8>,
    ranges: Vec<std::ops::Range<usize>>,
}
impl Image {
    /// Build a new complete library only. Not a merge/recovery/write operation.
    /// Empty macros have an explicit sentinel; nonempty macros do not.
    pub fn compile(macros: &[Vec<Record>]) -> Result<Self> {
        if macros.len() > SLOTS {
            return Err(invalid("more than 32 macro slots"));
        }
        let size = macros
            .iter()
            .try_fold(HEADER, |n, m| n.checked_add(m.len().max(1).checked_mul(4)?))
            .ok_or_else(|| invalid("macro size overflow"))?;
        if size > CAPACITY {
            return Err(invalid("macro image exceeds 0x1080 bytes"));
        }
        let mut bytes = vec![0; HEADER];
        for (slot, records) in macros.iter().enumerate() {
            let at = bytes.len() as u16;
            bytes[slot * 2..slot * 2 + 2].copy_from_slice(&at.to_le_bytes());
            if records.is_empty() {
                bytes.extend_from_slice(&[0, 0, 0x80, 0]);
                continue;
            }
            for (i, r) in records.iter().enumerate() {
                bytes.extend_from_slice(&r.delay_raw.to_le_bytes());
                let [kind, value] = r.action.encode()?;
                bytes.extend_from_slice(&[
                    kind | if i + 1 == records.len() { 0x80 } else { 0 },
                    value,
                ]);
            }
        }
        Self::from_bytes(bytes)
    }
    /// Preserve opaque records byte-for-byte; typed access may still reject them.
    /// Only a packed prefix of populated slots has been observed in OEM output.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() < HEADER || bytes.len() > CAPACITY || bytes.len() % 4 != 0 {
            return Err(invalid("macro image length"));
        }
        let mut offsets = Vec::new();
        let mut ended = false;
        for entry in bytes[..HEADER].chunks_exact(2) {
            let at = u16::from_le_bytes(entry.try_into().unwrap()) as usize;
            if at == 0 {
                ended = true;
                continue;
            }
            if ended
                || at < HEADER
                || at >= bytes.len()
                || at % 4 != 0
                || offsets.last().is_some_and(|prev| *prev >= at)
            {
                return Err(invalid("macro offset table"));
            }
            offsets.push(at);
        }
        if offsets.first().copied().unwrap_or(bytes.len()) != HEADER {
            return Err(invalid("unowned macro bytes"));
        }
        let ranges = (0..offsets.len())
            .map(|i| offsets[i]..offsets.get(i + 1).copied().unwrap_or(bytes.len()))
            .collect::<Vec<_>>();
        for range in &ranges {
            let records = &bytes[range.clone()];
            if records
                .chunks_exact(4)
                .enumerate()
                .any(|(i, r)| (r[2] & 0x80 != 0) != (i + 1 == records.len() / 4))
            {
                return Err(invalid("macro end marker disagrees with slot boundary"));
            }
        }
        Ok(Self { bytes, ranges })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn slots(&self) -> usize {
        self.ranges.len()
    }
    pub fn raw_slot(&self, slot: usize) -> Result<&[u8]> {
        self.ranges
            .get(slot)
            .map(|range| &self.bytes[range.clone()])
            .ok_or_else(|| invalid("macro slot outside image"))
    }
    pub fn records(&self, slot: usize) -> Result<Vec<Record>> {
        let range = self
            .ranges
            .get(slot)
            .ok_or_else(|| invalid("macro slot outside image"))?;
        let data = &self.bytes[range.clone()];
        if data == [0, 0, 0x80, 0] {
            return Ok(vec![]);
        }
        data.chunks_exact(4)
            .map(|r| {
                Ok(Record {
                    delay_raw: u16::from_le_bytes([r[0], r[1]]),
                    action: Action::decode(r[2] & 0x7f, r[3])?,
                })
            })
            .collect()
    }
    /// Pure compilation of 0D/00 writes and the final 10/A5 marker.
    /// Do not dispatch until model identity, readback and recovery are proved.
    pub fn compile_reports(&self) -> Vec<[u8; 65]> {
        let mut reports = Vec::new();
        for (index, chunk) in self.bytes.chunks(56).enumerate() {
            let mut raw = [0; FRAME];
            raw[0] = 0x55;
            raw[1] = 0x0d;
            raw[4] = chunk.len() as u8;
            raw[5..7].copy_from_slice(&((index * 56) as u16).to_le_bytes());
            raw[8..8 + chunk.len()].copy_from_slice(chunk);
            reports.push(output(raw));
        }
        let mut raw = [0; FRAME];
        raw[0] = 0x55;
        raw[1] = 0x10;
        raw[2] = 0xa5;
        reports.push(output(raw));
        reports
    }
}
#[cfg(test)]
#[path = "yjx_sc620_macro_tests.rs"]
mod tests;
