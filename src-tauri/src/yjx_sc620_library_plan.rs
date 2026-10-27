//! Static library transaction compiler. No HID, identity or restore executor.
use super::{
    bindings::{Image as Page, MacroBinding},
    invalid,
    macros::Image as Library,
};
use gearwise_core::Result;
#[derive(Clone, Debug)]
pub struct Change {
    pub page: u8,
    pub record: u8,
    pub expected: [u8; 4],
    pub binding: MacroBinding,
}
/// Caller-supplied complete snapshots; these are not obtained from a device here.
/// Untargeted references across all six read pages keep their exact slot content.
pub fn compile(
    before: &[Page],
    old: &Library,
    next: &Library,
    changes: &[Change],
) -> Result<Vec<[u8; 65]>> {
    if before.len() != 6
        || before
            .iter()
            .enumerate()
            .any(|(i, p)| p.page() as usize != i)
    {
        return Err(invalid("need all six ordered binding read pages"));
    }
    if changes.is_empty() || changes.len() > 55 {
        return Err(invalid("select 1..55 macro bindings"));
    }
    let mut selected = std::collections::BTreeSet::new();
    let mut pages = before.to_vec();
    for c in changes {
        if c.page >= 5 || !selected.insert((c.page, c.record)) {
            return Err(invalid("duplicate or unverified write page"));
        }
        pages[c.page as usize] =
            pages[c.page as usize].with_macro(c.record, c.expected, &c.binding, next)?;
    }
    let changed = old.bytes() != next.bytes();
    for p in before {
        for record in 0..11 {
            if selected.contains(&(p.page(), record)) {
                continue;
            }
            let raw = p.record(record)?;
            if raw[0] == 0x70 {
                if let Some(binding) = MacroBinding::decode(raw) {
                    let original = old.raw_slot(binding.slot as usize)?;
                    if original != next.raw_slot(binding.slot as usize)? {
                        return Err(invalid(
                            "library edit changes an untargeted macro reference",
                        ));
                    }
                } else if changed {
                    return Err(invalid(
                        "opaque original macro binding blocks global library replacement",
                    ));
                }
            }
        }
    }
    let mut packets = if changed {
        next.compile_reports()
    } else {
        Vec::new()
    };
    for i in 0..5 {
        if before[i] != pages[i] {
            packets.push(pages[i].compile_output()?);
        }
    }
    Ok(packets)
}
#[cfg(test)]
#[path = "yjx_sc620_library_tests.rs"]
mod tests;
