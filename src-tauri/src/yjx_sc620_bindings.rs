//! SC620's 11 four-byte records per page. Record indices are NOT advertised
//! as physical button names until their per-model hardware layout is proved.
use super::{invalid, output, Read, BINDING_LEN, FRAME};
use gearwise_core::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mouse {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Media {
    PlayPause,
    Stop,
    Previous,
    Next,
    VolumeUp,
    VolumeDown,
    Mute,
    Player,
}
impl Media {
    fn usage(self) -> u16 {
        match self {
            Self::PlayPause => 0xcd,
            Self::Stop => 0xb7,
            Self::Previous => 0xb6,
            Self::Next => 0xb5,
            Self::VolumeUp => 0xe9,
            Self::VolumeDown => 0xea,
            Self::Mute => 0xe2,
            Self::Player => 0x183,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Action {
    Mouse(Mouse),
    DoubleClick,
    WheelUp,
    WheelDown,
    DpiCycle,
    Key { usage: u8, modifiers: u8 },
    Media(Media),
}
impl Action {
    pub fn encode(&self) -> Result<[u8; 4]> {
        Ok(match *self {
            Self::Mouse(m) => [
                0x20,
                match m {
                    Mouse::Left => 1,
                    Mouse::Right => 2,
                    Mouse::Middle => 4,
                    Mouse::Back => 8,
                    Mouse::Forward => 16,
                },
                0,
                0,
            ],
            Self::DoubleClick => [0x20, 1, 2, 0],
            Self::WheelUp => [0x21, 0x38, 1, 0],
            Self::WheelDown => [0x21, 0x38, 0xff, 0],
            Self::DpiCycle => [0x21, 0x55, 0, 0],
            Self::Key { usage, modifiers }
                if (4..=0xa4).contains(&usage) || (usage == 0 && modifiers != 0) =>
            {
                [0x10, modifiers, usage, 0]
            }
            Self::Media(m) => {
                let [lo, hi] = m.usage().to_le_bytes();
                [0x30, lo, hi, 0]
            }
            _ => return Err(invalid("unsupported key usage")),
        })
    }
    /// Unknown original actions, macros, special firmware modes and modifier
    /// extensions stay opaque. Decoding never turns them into defaults.
    pub fn decode(bytes: [u8; 4]) -> Option<Self> {
        let known = [
            Self::Mouse(Mouse::Left),
            Self::Mouse(Mouse::Right),
            Self::Mouse(Mouse::Middle),
            Self::Mouse(Mouse::Back),
            Self::Mouse(Mouse::Forward),
            Self::DoubleClick,
            Self::WheelUp,
            Self::WheelDown,
            Self::DpiCycle,
            Self::Media(Media::PlayPause),
            Self::Media(Media::Stop),
            Self::Media(Media::Previous),
            Self::Media(Media::Next),
            Self::Media(Media::VolumeUp),
            Self::Media(Media::VolumeDown),
            Self::Media(Media::Mute),
            Self::Media(Media::Player),
        ];
        if let Some(action) = known.into_iter().find(|a| a.encode().ok() == Some(bytes)) {
            return Some(action);
        }
        if bytes[0] == 0x10 && bytes[3] == 0 {
            let action = Self::Key {
                usage: bytes[2],
                modifiers: bytes[1],
            };
            if action.encode().ok() == Some(bytes) {
                return Some(action);
            }
        }
        None
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    /// Zero-based EEPROM record. This is not an arbitrary address.
    pub record: u8,
    pub expected: [u8; 4],
    pub action: Action,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    page: u8,
    bytes: [u8; BINDING_LEN],
}
impl Image {
    pub fn from_response(page: u8, response: &[u8]) -> Result<Self> {
        let data = Read::Bindings { page }.response(response)?;
        let mut bytes = [0; BINDING_LEN];
        bytes.copy_from_slice(data);
        Ok(Self { page, bytes })
    }
    pub fn page(&self) -> u8 {
        self.page
    }
    pub fn record(&self, index: u8) -> Result<[u8; 4]> {
        if index >= 11 {
            return Err(invalid("binding record outside 0..10"));
        }
        let at = usize::from(index) * 4;
        Ok(self.bytes[at..at + 4].try_into().unwrap())
    }
    pub fn patch(&self, changes: &[Change]) -> Result<Self> {
        if changes.is_empty() || changes.len() > 11 {
            return Err(invalid("select 1..11 distinct binding records"));
        }
        let mut selected = BTreeSet::new();
        let mut replacements = Vec::new();
        for change in changes {
            let old = self.record(change.record)?;
            if !selected.insert(change.record) {
                return Err(invalid("duplicate binding record"));
            }
            if old != change.expected {
                return Err(invalid("binding changed since preview"));
            }
            replacements.push((usize::from(change.record) * 4, change.action.encode()?));
        }
        let mut next = self.clone();
        for (at, bytes) in replacements {
            next.bytes[at..at + 4].copy_from_slice(&bytes);
        }
        Ok(next)
    }
    /// Compile the source's 44-byte page rather than writing a smaller default
    /// eight-record table, which would discard unknown records 8..10.
    pub fn compile_output(&self) -> [u8; 65] {
        let mut raw = [0; FRAME];
        raw[0] = 0x55;
        raw[1] = 9;
        raw[2] = 0xa5;
        raw[4] = BINDING_LEN as u8;
        raw[7] = self.page;
        raw[8..8 + BINDING_LEN].copy_from_slice(&self.bytes);
        output(raw)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!("../tests/fixtures/yjx-sc620/bindings.json")).unwrap()
    }
    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|v| u8::from_str_radix(v, 16).unwrap())
            .collect()
    }
    fn before() -> Image {
        Image::from_response(3, &hex(fixture()["response_hex"].as_str().unwrap())).unwrap()
    }
    #[test]
    fn actions_match_oem_store_vectors_and_decode_without_losing_modifier_sides() {
        for v in fixture()["actions"].as_array().unwrap() {
            let a: Action = serde_json::from_value(v["action"].clone()).unwrap();
            let bytes: [u8; 4] = hex(v["hex"].as_str().unwrap()).try_into().unwrap();
            assert_eq!(a.encode().unwrap(), bytes);
            assert_eq!(Action::decode(bytes), Some(a));
        }
        for bit in 0..8 {
            let a = Action::Key {
                usage: 0,
                modifiers: 1 << bit,
            };
            assert_eq!(Action::decode(a.encode().unwrap()), Some(a));
        }
    }
    #[test]
    fn unrecognized_macro_and_special_records_round_trip_unmodified() {
        let f = fixture();
        let old = before();
        let changes: Vec<Change> = serde_json::from_value(f["changes"].clone()).unwrap();
        let after = old.patch(&changes).unwrap();
        assert_eq!(
            after.compile_output().to_vec(),
            hex(f["expected_output_hex"].as_str().unwrap())
        );
        for i in 0..11 {
            if !changes.iter().any(|c| c.record == i) {
                assert_eq!(after.record(i).unwrap(), old.record(i).unwrap());
            }
        }
        for i in [5, 8, 9, 10] {
            assert_eq!(Action::decode(old.record(i).unwrap()), None);
        }
    }
    #[test]
    fn duplicate_stale_unknown_or_invalid_action_does_not_produce_a_partial_patch() {
        let before = before();
        let good = Change {
            record: 4,
            expected: before.record(4).unwrap(),
            action: Action::Key {
                usage: 4,
                modifiers: 0,
            },
        };
        assert!(before.patch(&[]).is_err());
        assert!(before.patch(&[good.clone(), good.clone()]).is_err());
        let another = Change {
            record: 3,
            expected: before.record(3).unwrap(),
            ..good.clone()
        };
        for c in [
            Change {
                record: 11,
                ..another.clone()
            },
            Change {
                expected: [0; 4],
                ..another.clone()
            },
            Change {
                action: Action::Key {
                    usage: 0,
                    modifiers: 0,
                },
                ..another.clone()
            },
            Change {
                action: Action::Key {
                    usage: 0xe0,
                    modifiers: 0,
                },
                ..another.clone()
            },
        ] {
            assert!(before.patch(&[good.clone(), c]).is_err());
        }
        assert_eq!(before, self::before());
        assert!(before.record(255).is_err());
    }
    #[test]
    fn unknown_raw_commands_and_macro_identifiers_cannot_be_deserialized_as_actions() {
        for s in [
            r#"{"Key":{"usage":4,"modifiers":0,"raw_command":13}}"#,
            r#"{"Macro":{"index":1}}"#,
            r#"{"Raw":[112,0,1,2]}"#,
        ] {
            assert!(serde_json::from_str::<Action>(s).is_err());
        }
    }
}
