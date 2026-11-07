//! Bound the native macro consumer queue, not physical input or global ownership.
use crate::{
    prime_macro::event::{Event, Page},
    prime_macro_block::Block,
    prime_macro_storage::Image,
};
use gearwise_core::{Error, Result};
use serde::Serialize;
#[derive(Debug, Serialize)]
pub struct Admission {
    pub per_control: Vec<(u8, u8)>,
    pub macro_peak_slots: u8,
    pub reserved_other_slots: u8,
    pub context_limit: u8,
}
/// Caller must bound every non-macro contributor separately. All referenced
/// programs are inspected, including unchanged aliases; opaque data is refused.
/// Three contexts can run simultaneously; a fourth source is not guaranteed.
pub fn analyze(image: &Image, reserved_other_slots: u8) -> Result<Admission> {
    if reserved_other_slots > 4 {
        return Err(Error::invalid(
            "Prime external consumer reservation exceeds4",
        ));
    }
    let mut per_control = vec![];
    for reference in image.references() {
        let block = Block::decode(image.memory(), reference.bytes.start)?;
        if !(1..=4).contains(&block.metadata.playback) || block.metadata.direction > 1 {
            return Err(Error::Unsupported(
                "Prime unreviewed referenced playback/direction",
            ));
        }
        block.check_input_balance()?;
        let mut held = std::collections::BTreeSet::new();
        let mut peak = 0;
        for event in block.events {
            if let Event::Key {
                usage,
                page: Page::Consumer,
                down,
            } = event
            {
                if down {
                    held.insert(usage);
                } else {
                    held.remove(&usage);
                }
                peak = peak.max(held.len());
            }
        }
        if peak > 4 {
            return Err(Error::invalid("Prime context consumer capacity exceeded"));
        }
        per_control.push((reference.control, peak as u8));
    }
    // Different controls aliasing the same bytecode still create independent
    // contexts. Even duplicate consumer usages consume separate merged slots.
    let mut peaks: Vec<_> = per_control.iter().map(|(_, p)| *p).collect();
    peaks.sort_unstable_by(|a, b| b.cmp(a));
    let macro_peak_slots = peaks.into_iter().take(3).sum::<u8>();
    if macro_peak_slots + reserved_other_slots > 4 {
        return Err(Error::invalid(
            "Prime concurrent macros can drop consumer usages",
        ));
    }
    Ok(Admission {
        per_control,
        macro_peak_slots,
        reserved_other_slots,
        context_limit: 3,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{prime_macro_block::binding, prime_macro_once};
    fn image(peaks: &[u8], alias: bool) -> Image {
        let mut m = vec![0x4a; 5120];
        let mut p = [0x51, 1, 0, 0, 0].repeat(6);
        p[25..].copy_from_slice(&[0x30, 0, 0, 0, 0]);
        let mut w = vec![0x31, 0, 0, 0, 0, 0x32, 0, 0, 0, 0];
        for (i, peak) in peaks.iter().enumerate() {
            let at = if alias { 64 } else { 64 + i * 128 };
            let mut e = vec![];
            for u in 1..=*peak {
                e.push(Event::Key {
                    usage: u,
                    page: Page::Consumer,
                    down: true,
                });
            }
            e.push(Event::Delay(20));
            for u in 1..=*peak {
                e.push(Event::Key {
                    usage: u,
                    page: Page::Consumer,
                    down: false,
                });
            }
            let raw = prime_macro_once::compile(&e).unwrap().encode().unwrap();
            m[at..at + raw.len()].copy_from_slice(&raw);
            let record = binding(at).unwrap();
            if i < 4 {
                p[(i + 1) * 5..(i + 2) * 5].copy_from_slice(&record);
            } else {
                w[(i - 4) * 5..(i - 3) * 5].copy_from_slice(&record);
            }
        }
        Image::new("open-a", &m, &p, &w).unwrap()
    }
    #[test]
    fn separate_control_aliases_and_other_producers_are_not_deduplicated() {
        let a = image(&[2, 2, 2], true);
        assert!(analyze(&a, 0).is_err());
        let a = image(&[1, 1, 1, 1, 1, 1], true);
        assert_eq!(analyze(&a, 1).unwrap().macro_peak_slots, 3);
        assert!(analyze(&a, 2).is_err());
        assert!(analyze(&image(&[], false), 5).is_err());
    }
    #[test]
    fn only_three_largest_context_peaks_determine_the_macro_bound() {
        let a = image(&[0, 1, 1, 1, 1, 1], false);
        let result = analyze(&a, 1).unwrap();
        assert_eq!(result.context_limit, 3);
        assert_eq!(result.per_control.len(), 6);
        assert!(analyze(&image(&[3, 2], false), 0).is_err());
        assert_eq!(
            analyze(&image(&[2, 1, 1], false), 0)
                .unwrap()
                .macro_peak_slots,
            4
        );
    }
    #[test]
    fn foreign_referenced_modes_events_and_directions_refuse_admission() {
        for (byte, value) in [(65, 5), (65, 6), (68, 2), (72, 0xfd)] {
            let a = image(&[1], false);
            let mut m = a.memory().to_vec();
            m[byte] = value;
            let a = Image::new(a.scope(), &m, a.physical(), a.wheel()).unwrap();
            assert!(analyze(&a, 0).is_err());
        }
    }
}
