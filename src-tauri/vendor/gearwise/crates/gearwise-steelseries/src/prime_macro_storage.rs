//! Offline Prime macro image ownership and conditional restoration planning.
//! Images/connection stamps are supplied by the caller: this module does not
//! establish fresh USB readback, identity, idle state, persistence or live I/O.
use crate::{
    link::Packet,
    prime_macro::event::Event,
    prime_macro_block::binding_offset,
    prime_macro_once,
    prime_macro_transfer::{self, CHUNK_BYTES, COVERED_BYTES, DECLARED_BYTES},
};
use gearwise_core::{Error, Result};
use std::ops::Range;

/// Firmware's observed1280-word range, not an expanded host allocation budget.
pub const OBSERVED_BYTES: usize = 5120;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    scope: String,
    memory: Vec<u8>,
    physical: Vec<u8>,
    wheel: Vec<u8>,
    references: Vec<Reference>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub control: u8,
    pub canonical_binding: bool,
    pub bytes: Range<usize>,
}
impl Image {
    /// `scope` must later come from the actual current device/connection. A
    /// matching supplied string alone is not proof of hardware identity.
    pub fn new(scope: &str, memory: &[u8], physical: &[u8], wheel: &[u8]) -> Result<Self> {
        if scope.is_empty()
            || scope.len() > 256
            || memory.len() != OBSERVED_BYTES
            || physical.len() != 30
            || wheel.len() != 10
        {
            return Err(Error::invalid("Prime complete image/table/scope shape"));
        }
        if physical[25..] != [0x30, 0, 0, 0, 0] {
            return Err(Error::invalid("Prime fixed bottom CPI record changed"));
        }
        let mut references: Vec<Reference> = Vec::new();
        for (index, record) in physical
            .chunks_exact(5)
            .chain(wheel.chunks_exact(5))
            .enumerate()
        {
            if record[0] != 0x71 {
                continue;
            }
            let canonical_binding = binding_offset(record).is_ok();
            // Original binding wrappers read +3/+4 directly; preserve foreign
            // reserved bytes without treating them as editable canonical data.
            let offset = usize::from(u16::from_le_bytes([record[3], record[4]])) * 4;
            let header = memory
                .get(
                    offset
                        ..offset
                            .checked_add(8)
                            .ok_or(Error::invalid("Prime offset overflow"))?,
                )
                .ok_or(Error::invalid(
                    "Prime referenced macro header outside image",
                ))?;
            if header[0] != 0x77 {
                return Err(Error::invalid("Prime referenced macro marker unknown"));
            }
            // Preserve raw modes/directions/events, including unadmitted mode6.
            // Firmware reads the same header length regardless of playback mode.
            let count = usize::from(u16::from_le_bytes([header[2], header[3]]));
            let end = offset + 8 + count * 4;
            if end >= memory.len() {
                return Err(Error::invalid(
                    "Prime referenced macro extent outside image",
                ));
            }
            let range = offset..end;
            if references
                .iter()
                .any(|r| r.bytes.start != offset && overlap(&r.bytes, &range))
            {
                return Err(Error::invalid(
                    "Prime macro references overlap at different offsets",
                ));
            }
            references.push(Reference {
                control: (index + 1) as u8,
                canonical_binding,
                bytes: range,
            });
        }
        Ok(Self {
            scope: scope.into(),
            memory: memory.into(),
            physical: physical.into(),
            wheel: wheel.into(),
            references,
        })
    }
    pub fn memory(&self) -> &[u8] {
        &self.memory
    }
    pub fn physical(&self) -> &[u8] {
        &self.physical
    }
    pub fn wheel(&self) -> &[u8] {
        &self.wheel
    }
    pub fn references(&self) -> &[Reference] {
        &self.references
    }
}
fn overlap(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

pub struct Edit {
    before: Image,
    after: Image,
    changed: Range<usize>,
    original_extent: Range<usize>,
    packets: Vec<Packet>,
}
pub struct Restore {
    pub packets: Vec<Packet>,
    /// False means the fresh supplied image already matches the original.
    pub needed: bool,
}
impl Edit {
    pub fn before(&self) -> &Image {
        &self.before
    }
    pub fn after(&self) -> &Image {
        &self.after
    }
    pub fn changed(&self) -> Range<usize> {
        self.changed.clone()
    }
    pub fn original_extent(&self) -> Range<usize> {
        self.original_extent.clone()
    }
    pub fn packets(&self) -> &[Packet] {
        &self.packets
    }

    /// Offline conditional restoration decision after at most `attempted`
    /// prefix commands. Caller still needs current identity, exclusive access,
    /// idle-state proof and a real readback/restore transaction before I/O.
    /// Unknown, torn or third-party states are refused rather than guessed.
    pub fn restore(&self, current: &Image, attempted: usize) -> Result<Restore> {
        self.restore_with_attempts(current, attempted, 0)
    }
    /// Account for a previously attempted restoration prefix as well. Attempt
    /// counters must be recorded before sending, for this exact plan/connection.
    pub fn restore_with_attempts(
        &self,
        current: &Image,
        attempted: usize,
        restore_attempted: usize,
    ) -> Result<Restore> {
        if attempted > self.packets.len()
            || restore_attempted > self.packets.len()
            || (attempted == 0 && restore_attempted != 0)
            || current.scope != self.before.scope
            || current.physical != self.before.physical
            || current.wheel != self.before.wheel
        {
            return Err(Error::invalid(
                "Prime restoration scope/table/attempt mismatch",
            ));
        }
        if current.memory == self.before.memory {
            return Ok(Restore {
                packets: Vec::new(),
                needed: false,
            });
        }
        let mut recognized = false;
        for applied in 0..=attempted {
            let mut candidate = self.before.memory.clone();
            candidate[..applied * CHUNK_BYTES]
                .copy_from_slice(&self.after.memory[..applied * CHUNK_BYTES]);
            for restored in 0..=restore_attempted {
                let mut intermediate = candidate.clone();
                intermediate[..restored * CHUNK_BYTES]
                    .copy_from_slice(&self.before.memory[..restored * CHUNK_BYTES]);
                if current.memory == intermediate {
                    recognized = true;
                    break;
                }
            }
            if recognized {
                break;
            }
        }
        if !recognized {
            return Err(Error::invalid(
                "Prime restoration cannot own observed changes",
            ));
        }
        let packets = prime_macro_transfer::plan(
            &current.memory[..DECLARED_BYTES],
            &self.before.memory[..DECLARED_BYTES],
            &[],
        )?
        .packets;
        Ok(Restore {
            packets,
            needed: true,
        })
    }
}
/// Replace one exclusive existing macro without allocating unknown bytes,
/// moving other blocks, changing mappings or clearing the shortened remainder.
/// Aliases require future copy-on-write allocation; zero-filled bytes alone
/// do not establish free space. Only the current reviewed prefix is changed.
pub fn replace_once_in_place(before: &Image, control: u8, events: &[Event]) -> Result<Edit> {
    if !matches!(control, 2..=5 | 7 | 8) {
        return Err(Error::invalid("Prime protected/unknown macro control"));
    }
    let reference = before
        .references
        .iter()
        .find(|r| r.control == control)
        .ok_or(Error::invalid(
            "Prime control has no existing native macro allocation",
        ))?;
    if before
        .references
        .iter()
        .filter(|r| r.bytes.start == reference.bytes.start)
        .count()
        != 1
    {
        return Err(Error::invalid(
            "Prime target macro is shared by another control",
        ));
    }
    if !reference.canonical_binding {
        return Err(Error::invalid(
            "Prime target binding has unadmitted reserved fields",
        ));
    }
    let bytes = prime_macro_once::compile(events)?.encode()?;
    let end = reference.bytes.start + bytes.len();
    if end > reference.bytes.end || end > COVERED_BYTES {
        return Err(Error::invalid(
            "Prime replacement exceeds owned extent or transfer prefix",
        ));
    }
    let changed = reference.bytes.start..end;
    let mut desired = before.memory.clone();
    desired[changed.clone()].copy_from_slice(&bytes);
    let after = Image::new(&before.scope, &desired, &before.physical, &before.wheel)?;
    // This complete reference census proves every unselected block and all
    // other RAM bytes unchanged, including references beyond the host image.
    let unchanged = desired == before.memory;
    let packets = if unchanged {
        Vec::new()
    } else {
        prime_macro_transfer::plan(
            &before.memory[..DECLARED_BYTES],
            &desired[..DECLARED_BYTES],
            &[changed.start],
        )?
        .packets
    };
    let changed = if unchanged {
        changed.start..changed.start
    } else {
        changed
    };
    Ok(Edit {
        before: before.clone(),
        after,
        changed,
        original_extent: reference.bytes.clone(),
        packets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{prime_macro::event::Page, prime_macro_block::binding};
    fn events(usage: u8) -> Vec<Event> {
        vec![
            Event::Key {
                usage,
                page: Page::Keyboard,
                down: true,
            },
            Event::Key {
                usage,
                page: Page::Keyboard,
                down: false,
            },
        ]
    }
    fn fixture(offset: usize) -> Image {
        let mut memory = vec![0x4a; OBSERVED_BYTES];
        let mut physical = vec![0x51, 1, 0, 0, 0].repeat(6);
        physical[25..].copy_from_slice(&[0x30, 0, 0, 0, 0]);
        let wheel = vec![0x31, 0, 0, 0, 0, 0x32, 0, 0, 0, 0];
        for (index, at) in [offset, 600, 1200, 2048].into_iter().enumerate() {
            let mut e = events(4);
            e.insert(1, Event::Delay(7));
            let mut raw = prime_macro_once::compile(&e).unwrap().encode().unwrap();
            if index == 3 {
                raw[1] = 6;
                raw[8] = 0xfd;
            } // preserve unadmitted foreign mode/events
            memory[at..at + raw.len()].copy_from_slice(&raw);
            physical[(index + 1) * 5..(index + 2) * 5].copy_from_slice(&binding(at).unwrap());
        }
        Image::new("mock-device-a/connection-1", &memory, &physical, &wheel).unwrap()
    }
    fn observed(edit: &Edit, prefix: usize) -> Image {
        let mut memory = edit.before.memory.clone();
        memory[..prefix * CHUNK_BYTES].copy_from_slice(&edit.after.memory[..prefix * CHUNK_BYTES]);
        Image::new(
            &edit.before.scope,
            &memory,
            &edit.before.physical,
            &edit.before.wheel,
        )
        .unwrap()
    }
    #[test]
    fn census_and_replacement_preserve_all_foreign_raw_bytes_and_shortened_remainder() {
        let image = fixture(64);
        assert_eq!(image.references.len(), 4);
        let edit = replace_once_in_place(&image, 2, &events(5)).unwrap();
        assert_eq!(edit.changed(), 64..80);
        assert_eq!(edit.original_extent(), 64..84);
        assert_eq!(&edit.after.memory[..64], &image.memory[..64]);
        assert_eq!(&edit.after.memory[80..], &image.memory[80..]);
        assert_eq!(edit.after.physical, image.physical);
        assert_eq!(edit.after.wheel, image.wheel);
        assert_eq!(edit.after.references[3].bytes, 2048..2068);
        assert_eq!(edit.after.memory[2049], 6);
        assert_eq!(edit.after.memory[2056], 0xfd);
        assert_eq!(edit.packets.len(), 2);
        assert_eq!(&edit.packets[0].bytes[5..517], &edit.after.memory[..512]);
        assert_eq!(
            &edit.packets[1].bytes[5..517],
            &edit.after.memory[512..1024]
        );
    }
    #[test]
    fn wheel_target_uses_its_independent_root_and_shared_alias_is_refused() {
        let image = fixture(64);
        let mut physical = image.physical.clone();
        let mut wheel = image.wheel.clone();
        physical[5..10].copy_from_slice(&[0x51, 1, 0, 0, 0]);
        wheel[..5].copy_from_slice(&binding(64).unwrap());
        let image = Image::new("same", &image.memory, &physical, &wheel).unwrap();
        assert!(replace_once_in_place(&image, 7, &events(5)).is_ok());
        wheel[5..10].copy_from_slice(&binding(64).unwrap());
        let shared = Image::new("same", &image.memory, &physical, &wheel).unwrap();
        assert!(replace_once_in_place(&shared, 7, &events(5)).is_err());
        assert!(replace_once_in_place(&shared, 8, &events(5)).is_err());
    }
    #[test]
    fn malformed_census_and_overlapping_roots_are_refused_before_a_plan() {
        let image = fixture(64);
        assert!(Image::new("", &image.memory, &image.physical, &image.wheel).is_err());
        assert!(Image::new("x", &image.memory[..1280], &image.physical, &image.wheel).is_err());
        assert!(Image::new("x", &image.memory, &image.physical[..29], &image.wheel).is_err());
        let mut physical = image.physical.clone();
        physical[25] = 0x71;
        assert!(Image::new("x", &image.memory, &physical, &image.wheel).is_err());
        physical = image.physical.clone();
        physical[6] = 1;
        let foreign_reserved = Image::new("x", &image.memory, &physical, &image.wheel).unwrap();
        assert!(!foreign_reserved.references[0].canonical_binding);
        assert!(replace_once_in_place(&foreign_reserved, 2, &events(5)).is_err());
        let allowed = replace_once_in_place(&foreign_reserved, 3, &events(5)).unwrap();
        assert_eq!(&allowed.after.memory[64..84], &image.memory[64..84]);
        assert_eq!(allowed.after.physical, physical);
        let mut memory = image.memory.clone();
        memory[64] = 0;
        assert!(Image::new("x", &memory, &image.physical, &image.wheel).is_err());
        memory = image.memory.clone();
        memory[66..68].copy_from_slice(&65535u16.to_le_bytes());
        assert!(Image::new("x", &memory, &image.physical, &image.wheel).is_err());
        memory = image.memory.clone();
        let overlapping = prime_macro_once::compile(&events(9))
            .unwrap()
            .encode()
            .unwrap();
        memory[72..88].copy_from_slice(&overlapping);
        physical = image.physical.clone();
        physical[10..15].copy_from_slice(&binding(72).unwrap());
        assert!(Image::new("x", &memory, &physical, &image.wheel).is_err());
    }
    #[test]
    fn protected_unallocated_growth_uncovered_and_unbalanced_edits_are_refused() {
        let image = fixture(64);
        for control in [0, 1, 6, 9, 255] {
            assert!(replace_once_in_place(&image, control, &events(5)).is_err());
        }
        assert!(replace_once_in_place(&image, 7, &events(5)).is_err());
        assert!(
            replace_once_in_place(
                &image,
                2,
                &[Event::Mouse {
                    button: 1,
                    down: true
                }]
            )
            .is_err()
        );
        let mut longer = events(5);
        longer.insert(1, Event::Delay(1));
        longer.insert(1, Event::Delay(2));
        assert!(replace_once_in_place(&image, 2, &longer).is_err());
        assert!(replace_once_in_place(&image, 4, &events(5)).is_err());
        assert!(replace_once_in_place(&image, 5, &events(5)).is_err());
    }
    #[test]
    fn prefix_failure_recovery_recognizes_exact_original_and_attempted_states() {
        let edit = replace_once_in_place(&fixture(504), 2, &events(5)).unwrap();
        for attempted in 0..=2 {
            assert!(!edit.restore(edit.before(), attempted).unwrap().needed);
            for prefix in 1..=attempted {
                let current = observed(&edit, prefix);
                let restore = edit.restore(&current, attempted).unwrap();
                assert!(restore.needed);
                assert_eq!(restore.packets.len(), 2);
                let mut result = current.memory.clone();
                for packet in restore.packets {
                    let off =
                        usize::from(u16::from_le_bytes([packet.bytes[1], packet.bytes[2]])) * 4;
                    result[off..off + 512].copy_from_slice(&packet.bytes[5..517]);
                }
                assert_eq!(result, edit.before.memory);
            }
        }
        assert!(edit.restore(&observed(&edit, 1), 0).is_err());
        assert!(edit.restore(&observed(&edit, 2), 1).is_err());
        assert!(edit.restore(edit.after(), 3).is_err());
    }
    #[test]
    fn recovery_refuses_other_connections_changed_tables_and_any_unknown_memory() {
        let edit = replace_once_in_place(&fixture(504), 2, &events(5)).unwrap();
        let mut other = edit.after.clone();
        other.scope = "mock-device-b/connection-1".into();
        assert!(edit.restore(&other, 2).is_err());
        other.scope = "mock-device-a/connection-2".into();
        assert!(edit.restore(&other, 2).is_err());
        other = edit.after.clone();
        other.physical[1] ^= 1;
        assert!(edit.restore(&other, 2).is_err());
        other = edit.after.clone();
        other.wheel[1] ^= 1;
        assert!(edit.restore(&other, 2).is_err());
        for offset in [0, 511, 700, 1024, 1280, 4000, 5119] {
            other = edit.after.clone();
            other.memory[offset] ^= 1;
            assert!(edit.restore(&other, 2).is_err());
        }
        other = edit.after.clone();
        other.memory[edit.changed.start + 1] = 0x99;
        assert!(edit.restore(&other, 2).is_err());
    }
    #[test]
    fn identical_edit_does_not_emit_context_resetting_commands() {
        let image = fixture(64);
        let mut same = events(4);
        same.insert(1, Event::Delay(7));
        let edit = replace_once_in_place(&image, 2, &same).unwrap();
        assert!(edit.packets().is_empty());
        assert!(edit.changed().is_empty());
        assert_eq!(edit.after(), edit.before());
        assert!(!edit.restore(edit.before(), 0).unwrap().needed);
        assert!(edit.restore(edit.before(), 1).is_err());
    }
    #[test]
    fn interrupted_restore_requires_its_attempt_history_and_can_recover_conditionally() {
        let edit = replace_once_in_place(&fixture(504), 2, &events(5)).unwrap();
        let mut partial = observed(&edit, 2);
        partial.memory[..512].copy_from_slice(&edit.before.memory[..512]);
        assert!(edit.restore(&partial, 2).is_err());
        let result = edit.restore_with_attempts(&partial, 2, 1).unwrap();
        assert!(result.needed);
        assert_eq!(result.packets.len(), 2);
        for forward in 0..=2 {
            for backward in 0..=2 {
                let mut current = observed(&edit, forward);
                current.memory[..backward * 512]
                    .copy_from_slice(&edit.before.memory[..backward * 512]);
                if forward == 0 && backward > 0 {
                    assert!(
                        edit.restore_with_attempts(&current, forward, backward)
                            .is_err()
                    );
                } else {
                    assert!(
                        edit.restore_with_attempts(&current, forward, backward)
                            .is_ok()
                    );
                }
            }
        }
        assert!(edit.restore_with_attempts(&partial, 2, 3).is_err());
        partial.memory[4000] ^= 1;
        assert!(edit.restore_with_attempts(&partial, 2, 1).is_err());
    }
}
