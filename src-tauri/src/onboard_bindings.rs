//! Native HID++ 0x8100 button records. No global input mapping or macro
//! allocation. Patch only explicitly selected records after checking their
//! original bytes; all unknown bindings, LEDs and other layers stay intact.
use crate::onboard::{crc_ccitt, sector_crc_ok, Info};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Layer {
    Normal,
    GShift,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Special {
    WheelLeft,
    WheelRight,
    DpiUp,
    DpiDown,
    DpiCycle,
    DpiDefault,
    DpiShift,
    ProfileUp,
    ProfileDown,
    ProfileCycle,
    GShift,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Action {
    Disabled,
    Mouse(u8),
    Key { usage: u8, modifiers: u8 },
    Consumer(u16),
    Special(Special),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    /// One-based physical record index reported by this mouse, not a global key.
    pub button: u8,
    pub layer: Layer,
    pub expected: [u8; 4],
    pub action: Action,
}
fn fail(message: &str) -> String {
    format!("Logitech on-board bindings: {message}")
}
pub fn encode(action: &Action, info: &Info) -> Result<[u8; 4], String> {
    Ok(match action {
        Action::Disabled => [0xff; 4],
        Action::Mouse(button) if (1..=16).contains(button) => {
            let mask = (1u16 << (*button - 1)).to_be_bytes();
            [0x80, 1, mask[0], mask[1]]
        }
        Action::Key { usage, modifiers }
            if (4..=0xa4).contains(usage) || (*usage == 0 && *modifiers != 0) =>
        {
            [0x80, 2, *modifiers, *usage]
        }
        Action::Consumer(usage) if (1..=0x3ff).contains(usage) => {
            let bytes = usage.to_be_bytes();
            [0x80, 3, bytes[0], bytes[1]]
        }
        Action::Special(value) => {
            let code = match value {
                Special::WheelLeft => 1,
                Special::WheelRight => 2,
                Special::DpiUp => 3,
                Special::DpiDown => 4,
                Special::DpiCycle => 5,
                Special::DpiDefault => 6,
                Special::DpiShift => {
                    if (info.mechanical_layout >> 2) & 3 != 2 {
                        return Err(fail("this model does not report native DPI shift"));
                    }
                    7
                }
                Special::ProfileUp => 8,
                Special::ProfileDown => 9,
                Special::ProfileCycle => 10,
                Special::GShift => {
                    if info.mechanical_layout & 3 != 2 {
                        return Err(fail("this model does not report G-Shift"));
                    }
                    11
                }
            };
            [0x90, code, 0, 0]
        }
        _ => return Err(fail("unsupported output action")),
    })
}
pub fn validate_target(device: &crate::device::Device) -> Result<(), String> {
    if device.vendor_id != 0x046d || !device.kind.eq_ignore_ascii_case("mouse") {
        return Err(fail("native mouse bindings require a Logitech mouse"));
    }
    if device.index != crate::hidpp::DIRECT {
        return Err(fail("receiver binding writes require verified physical identity pinning; use direct USB for this experimental feature"));
    }
    Ok(())
}
pub fn validate_info(info: &Info) -> Result<(), String> {
    if info.memory_model != 1 || !(1..=5).contains(&info.profile_format) || info.macro_format != 1 {
        return Err(fail("unverified on-board profile format"));
    }
    if !(1..=16).contains(&info.button_count)
        || !(210..=4096).contains(&info.sector_size)
        || info.profile_count == 0
        || usize::from(info.profile_count) * 4 + 2 > usize::from(info.sector_size)
    {
        return Err(fail("invalid button count or profile geometry"));
    }
    Ok(())
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub expected_device: String,
    pub slot: u8,
    pub sector: u16,
    pub changes: Vec<Change>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub slot: u8,
    pub sector: u16,
    pub bytes_changed: usize,
    pub unchanged: bool,
    pub active_reloaded: bool,
    pub reload_note: Option<String>,
}
pub fn patch(original: &[u8], info: &Info, changes: &[Change]) -> Result<Vec<u8>, String> {
    validate_info(info)?;
    if changes.is_empty() || changes.len() > 32 {
        return Err(fail("select 1..32 independent bindings"));
    }
    if original.len() != usize::from(info.sector_size) {
        return Err(fail("profile length differs from descriptor"));
    }
    if !sector_crc_ok(original) {
        return Err(fail("original profile checksum failed"));
    }
    let mut selected = BTreeSet::new();
    let mut replacements = Vec::new();
    for change in changes {
        if change.button == 0
            || change.button > info.button_count
            || !selected.insert((change.layer, change.button))
        {
            return Err(fail("duplicate or unknown physical binding"));
        }
        if change.layer == Layer::GShift && info.mechanical_layout & 3 != 2 {
            return Err(fail("G-Shift layer is not reported by this mouse"));
        }
        let offset = match change.layer {
            Layer::Normal => 0x20,
            Layer::GShift => 0x60,
        } + usize::from(change.button - 1) * 4;
        if original[offset..offset + 4] != change.expected {
            return Err(fail(
                "binding changed since it was read; refresh before editing",
            ));
        }
        replacements.push((offset, encode(&change.action, info)?));
    }
    let mut out = original.to_vec();
    for (offset, value) in replacements {
        out[offset..offset + 4].copy_from_slice(&value);
    }
    // Keep a normal-layer left button. G-Shift cannot be the only path to click.
    if !(0..usize::from(info.button_count)).any(|i| {
        let at = 0x20 + 4 * i;
        out[at] == 0x80
            && out[at + 1] == 1
            && u16::from_be_bytes([out[at + 2], out[at + 3]]) & 1 != 0
    }) {
        return Err(fail(
            "at least one normal-layer physical button must retain left click",
        ));
    }
    let n = out.len();
    let crc = crc_ccitt(&out[..n - 2]);
    out[n - 2..].copy_from_slice(&crc.to_be_bytes());
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn info() -> Info {
        Info {
            memory_model: 1,
            profile_format: 3,
            macro_format: 1,
            profile_count: 5,
            rom_profile_count: 5,
            button_count: 11,
            sector_count: 8,
            sector_size: 256,
            mechanical_layout: 10,
            various_info: 1,
            bindings_writable: true,
        }
    }
    fn sector() -> Vec<u8> {
        let mut bytes = (0..256).map(|n| (n % 251) as u8).collect::<Vec<_>>();
        bytes[0x20..0x24].copy_from_slice(&[0x80, 1, 0, 1]);
        let crc = crc_ccitt(&bytes[..254]);
        bytes[254..].copy_from_slice(&crc.to_be_bytes());
        bytes
    }
    fn change(bytes: &[u8], button: u8, layer: Layer, action: Action) -> Change {
        let at = match layer {
            Layer::Normal => 0x20,
            Layer::GShift => 0x60,
        } + usize::from(button - 1) * 4;
        Change {
            button,
            layer,
            expected: bytes[at..at + 4].try_into().unwrap(),
            action,
        }
    }
    #[test]
    fn native_records_have_independent_exact_golden_encoding() {
        let i = info();
        for (a, raw) in [
            (Action::Mouse(1), [0x80, 1, 0, 1]),
            (Action::Mouse(5), [0x80, 1, 0, 16]),
            (Action::Mouse(16), [0x80, 1, 128, 0]),
            (
                Action::Key {
                    usage: 4,
                    modifiers: 255,
                },
                [0x80, 2, 255, 4],
            ),
            (
                Action::Key {
                    usage: 0,
                    modifiers: 16,
                },
                [0x80, 2, 16, 0],
            ),
            (Action::Consumer(0x192), [0x80, 3, 1, 0x92]),
            (Action::Special(Special::GShift), [0x90, 11, 0, 0]),
            (Action::Special(Special::DpiShift), [0x90, 7, 0, 0]),
            (Action::Disabled, [255; 4]),
        ] {
            assert_eq!(encode(&a, &i).unwrap(), raw);
        }
    }
    #[test]
    fn normal_and_gshift_patch_preserves_every_other_byte_and_unknown_macro_data() {
        let original = sector();
        let i = info();
        let changes = [
            change(
                &original,
                4,
                Layer::Normal,
                Action::Key {
                    usage: 6,
                    modifiers: 5,
                },
            ),
            change(&original, 4, Layer::GShift, Action::Consumer(0xcd)),
        ];
        let patched = patch(&original, &i, &changes).unwrap();
        assert!(sector_crc_ok(&patched));
        for n in 0..254 {
            if !((0x2c..0x30).contains(&n) || (0x6c..0x70).contains(&n)) {
                assert_eq!(patched[n], original[n], "untouched byte {n}");
            }
        }
        assert_eq!(&patched[0x2c..0x30], &[0x80, 2, 5, 6]);
        assert_eq!(&patched[0x6c..0x70], &[0x80, 3, 0, 0xcd]);
    }
    #[test]
    fn stale_records_unknown_layouts_duplicates_and_dangerous_left_removal_reject() {
        let original = sector();
        let i = info();
        let c = change(&original, 4, Layer::Normal, Action::Disabled);
        let mut stale = c.clone();
        stale.expected[3] ^= 1;
        assert!(patch(&original, &i, &[stale]).is_err());
        assert!(patch(&original, &i, &[c.clone(), c.clone()]).is_err());
        let mut unknown = info();
        unknown.profile_format = 6;
        assert!(patch(&original, &unknown, &[c.clone()]).is_err());
        let mut no_shift = info();
        no_shift.mechanical_layout = 0;
        assert!(patch(
            &original,
            &no_shift,
            &[change(&original, 4, Layer::GShift, Action::Disabled)]
        )
        .is_err());
        assert!(encode(&Action::Special(Special::DpiShift), &no_shift).is_err());
        assert!(patch(
            &original,
            &i,
            &[change(&original, 1, Layer::Normal, Action::Disabled)]
        )
        .is_err());
        let changes = [
            change(&original, 1, Layer::Normal, Action::Disabled),
            change(&original, 4, Layer::Normal, Action::Mouse(1)),
        ];
        assert!(patch(&original, &i, &changes).is_ok());
        let mut corrupt = original.clone();
        corrupt[12] ^= 1;
        assert!(patch(&corrupt, &i, &[c]).is_err());
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;
    #[test]
    fn extra_native_action_parameters_are_rejected_instead_of_silently_lost() {
        let v = serde_json::json!({"Key":{"usage":4,"modifiers":1,"repeat":3}});
        assert!(serde_json::from_value::<Action>(v).is_err());
        let v = serde_json::json!({"VendorProfile":{"schema":"attack-shark-x11/1","data":{},"raw_command":[1,2,3]}});
        assert!(serde_json::from_value::<gearwise_core::Setting>(v).is_err());
        let v = serde_json::json!({"Times":{"count":1,"toggle":true}});
        assert!(serde_json::from_value::<crate::x11::Playback>(v).is_err());
    }
}
