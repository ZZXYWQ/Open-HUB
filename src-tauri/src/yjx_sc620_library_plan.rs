//! Static library transaction compiler. No HID, identity or restore executor.
use super::{
    bindings::{Action, Change as BasicChange, Image as Page, MacroBinding},
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Basic(Action),
    Macro(MacroBinding),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub page: u8,
    pub record: u8,
    pub expected: [u8; 4],
    pub target: Target,
}
/// Preserve the existing macro-only API and its nonempty-selection contract.
pub fn compile(
    before: &[Page],
    old: &Library,
    next: &Library,
    changes: &[Change],
) -> Result<Vec<[u8; 65]>> {
    if changes.is_empty() {
        return Err(invalid("select at least one macro binding"));
    }
    let edits = changes
        .iter()
        .map(|c| Edit {
            page: c.page,
            record: c.record,
            expected: c.expected,
            target: Target::Macro(c.binding.clone()),
        })
        .collect::<Vec<_>>();
    compile_edits(before, old, next, &edits)
}
/// All original snapshots are caller-provided; no HID is accessed here.
/// Basic and macro changes share one selected-record and reference check.
pub fn compile_edits(
    before: &[Page],
    old: &Library,
    next: &Library,
    changes: &[Edit],
) -> Result<Vec<[u8; 65]>> {
    if before.len() != 6
        || before
            .iter()
            .enumerate()
            .any(|(i, p)| p.page() as usize != i)
    {
        return Err(invalid("need all six ordered binding read pages"));
    }
    if changes.len() > 55 || (changes.is_empty() && old.bytes() != next.bytes()) {
        return Err(invalid("select 1..55 macro bindings"));
    }
    let mut selected = std::collections::BTreeSet::new();
    let mut pages = before.to_vec();
    for c in changes {
        if c.page >= 5 || !selected.insert((c.page, c.record)) {
            return Err(invalid("duplicate or unverified write page"));
        }
        pages[c.page as usize] = match &c.target {
            Target::Macro(binding) => {
                pages[c.page as usize].with_macro(c.record, c.expected, binding, next)?
            }
            Target::Basic(action) => pages[c.page as usize].patch(&[BasicChange {
                record: c.record,
                expected: c.expected,
                action: action.clone(),
            }])?,
        };
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
