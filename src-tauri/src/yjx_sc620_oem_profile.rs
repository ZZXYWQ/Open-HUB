//! Strict joint OEM settings/buttons/macros compiler. No live device admission.
use super::{
    bindings::{Action, DisableHint, Image as Page, Media, Mouse},
    invalid,
    library_plan::{self, Edit, Target},
    macros::Image as Library,
    oem_compile, oem_config,
    oem_library::{self, db, open_source, schema},
    ConfigImage,
};
use gearwise_core::Result;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRow {
    pub id: i64,
    pub profile: i64,
    pub layer: i64,
    pub key_code: i64,
    pub layout_value: i64,
    pub layout_description: String,
    pub kind: i64,
    pub value: i64,
    pub value2: i64,
    pub value3: i64,
    pub description: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub settings: oem_config::Source,
    pub keys: Vec<KeyRow>,
    pub macros: Vec<oem_library::Macro>,
}
pub struct Prepared {
    pub config: ConfigImage,
    pub library: Library,
    pub edits: Vec<Edit>,
    pub reports: Vec<[u8; 65]>,
}
/// OEM control IDs map to EEPROM records, not proven physical button labels.
fn record(code: i64) -> Result<u8> {
    match code {
        1..=3 | 6..=11 => Ok((code - 1) as u8),
        4 => Ok(4),
        5 => Ok(3),
        _ => Err(invalid("unverified OEM control ID")),
    }
}
fn basic(row: &KeyRow) -> Result<Action> {
    let action = match row.kind {
        1 => Action::Mouse(Mouse::Left),
        2 => Action::Mouse(Mouse::Middle),
        3 => Action::Mouse(Mouse::Right),
        4 => Action::DoubleClick,
        5 => Action::Mouse(Mouse::Forward),
        6 => Action::Mouse(Mouse::Back),
        7 => Action::WheelUp,
        8 => Action::WheelDown,
        9 => Action::DpiCycle,
        11 => Action::Fire,
        15 => Action::DpiIncrease,
        16 => Action::DpiDecrease,
        17 => Action::DpiLoop,
        19 => Action::ReportRateIncrease,
        20 => Action::ReportRateDecrease,
        21 => Action::ReportRateLoop,
        24 => Action::Disabled(match row.key_code {
            1 => DisableHint::Left,
            2 => DisableHint::Right,
            3 => DisableHint::Middle,
            4 => DisableHint::Forward,
            5 => DisableHint::Back,
            7 => DisableHint::WheelUp,
            8 => DisableHint::WheelDown,
            9 => DisableHint::LegacyDpi,
            6 | 10 | 11 => DisableHint::Unspecified,
            _ => return Err(invalid("unverified disabled control ID")),
        }),
        26 => match row.value {
            1 => Action::Media(Media::ScreenBrightnessUp),
            2 => Action::Media(Media::ScreenBrightnessDown),
            3 => Action::Media(Media::Calculator),
            4 => Action::Key {
                usage: 8,
                modifiers: 8,
            },
            5 => Action::Key {
                usage: 7,
                modifiers: 8,
            },
            6 => Action::Media(Media::BrowserHome),
            7 => Action::Media(Media::Mail),
            8 => Action::Key {
                usage: 0x3e,
                modifiers: 0,
            },
            9 => Action::Key {
                usage: 0x2b,
                modifiers: 4,
            },
            10 => Action::Key {
                usage: 0x46,
                modifiers: 8,
            },
            11 => Action::Key {
                usage: 6,
                modifiers: 1,
            },
            12 => Action::Key {
                usage: 0x19,
                modifiers: 1,
            },
            13 => Action::Key {
                usage: 0x1b,
                modifiers: 1,
            },
            _ => return Err(invalid("unknown OEM shortcut ID")),
        },
        22 => {
            let modifiers = u8::try_from(row.value2).map_err(|_| invalid("OEM key modifiers"))?;
            if (0xe0..=0xe7).contains(&row.value) {
                if modifiers != 0 {
                    return Err(invalid("ambiguous OEM modifier-as-key combination"));
                }
                Action::Key {
                    usage: 0,
                    modifiers: 1 << (row.value - 0xe0),
                }
            } else {
                Action::Key {
                    usage: u8::try_from(row.value).map_err(|_| invalid("OEM key usage"))?,
                    modifiers,
                }
            }
        }
        25 => Action::Media(match row.value {
            1 => Media::Player,
            2 => Media::PlayPause,
            3 => Media::Stop,
            4 => Media::Previous,
            5 => Media::Next,
            6 => Media::VolumeUp,
            7 => Media::VolumeDown,
            8 => Media::Mute,
            _ => return Err(invalid("unknown OEM media ID")),
        }),
        _ => return Err(invalid("OEM button action not yet converted")),
    };
    action.encode()?;
    Ok(action)
}
pub fn read(path: &std::path::Path, profile: i64) -> Result<Source> {
    let c = open_source(path)?;
    let tx = db(c.unchecked_transaction())?;
    let settings = oem_config::parse(&tx, profile)?;
    let macros = oem_library::parse(&tx)?;
    let keys = read_keys(&tx, profile)?;
    db(tx.commit())?;
    Ok(Source {
        settings,
        keys,
        macros,
    })
}
fn read_keys(c: &rusqlite::Connection, profile: i64) -> Result<Vec<KeyRow>> {
    schema(
        c,
        "t_key_macro_data",
        &[
            ("key_id", "INTEGER", 1),
            ("profile", "INTEGER", 0),
            ("fn_layer", "INTEGER", 0),
            ("key_code", "INTEGER", 0),
            ("layout_value", "INTEGER", 0),
            ("layout_desc", "TEXT", 0),
            ("macro_type", "INTEGER", 0),
            ("macro_value", "INTEGER", 0),
            ("macro_value2", "INTEGER", 0),
            ("macro_value3", "INTEGER", 0),
            ("macro_desc", "TEXT", 0),
        ],
    )?;
    let mut stmt=db(c.prepare("SELECT key_id,profile,fn_layer,key_code,layout_value,layout_desc,macro_type,macro_value,macro_value2,macro_value3,macro_desc FROM t_key_macro_data WHERE profile=?1 ORDER BY key_code,fn_layer,key_id LIMIT 67"))?;
    let rows = db(stmt.query_map([profile], |r| {
        Ok(KeyRow {
            id: r.get(0)?,
            profile: r.get(1)?,
            layer: r.get(2)?,
            key_code: r.get(3)?,
            layout_value: r.get(4)?,
            layout_description: r.get(5)?,
            kind: r.get(6)?,
            value: r.get(7)?,
            value2: r.get(8)?,
            value3: r.get(9)?,
            description: r.get(10)?,
        })
    }))?;
    let mut keys = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for row in rows {
        let row = db(row)?;
        if keys.len() >= 66
            || row.id <= 0
            || row.profile != profile
            || row.description.len() > 1024
            || row.layout_description.len() > 1024
            || !seen.insert((row.layer, row.key_code))
        {
            return Err(invalid("ambiguous or oversized OEM key table"));
        }
        keys.push(row);
    }
    Ok(keys)
}
impl Source {
    pub fn prepare(
        &self,
        original: &ConfigImage,
        before: &[Page],
        old: &Library,
    ) -> Result<Prepared> {
        if original.page() >= 5 {
            return Err(invalid("unverified SC620 write page"));
        }
        let config = self.settings.prepare(original)?;
        if self.keys.len() > 66 {
            return Err(invalid("oversized source key table"));
        }
        let has_macros = self.keys.iter().any(|row| row.kind == 23);
        // The source library has one order. Never treat a DB macro ID as a slot.
        // This strict path requires every macro in that library to be convertible.
        let prepared = if has_macros {
            Some(oem_compile::prepare(&self.macros)?)
        } else {
            None
        };
        let library = prepared
            .as_ref()
            .map(|p| p.image.clone())
            .unwrap_or_else(|| old.clone());
        let mut edits = Vec::new();
        for row in &self.keys {
            if row.profile != self.settings.profile_id || row.layer != 0 {
                return Err(invalid(
                    "OEM profile/layer does not match verified ordinary layer",
                ));
            }
            let at = record(row.key_code)?;
            let target = if row.kind == 23 {
                let binding = prepared
                    .as_ref()
                    .unwrap()
                    .slots
                    .iter()
                    .find(|slot| slot.oem_id == row.value)
                    .ok_or_else(|| invalid("OEM button refers to a missing macro ID"))?
                    .binding
                    .clone();
                Target::Macro(binding)
            } else {
                Target::Basic(basic(row)?)
            };
            let expected = before
                .get(original.page() as usize)
                .ok_or_else(|| invalid("missing target page"))?
                .record(at)?;
            edits.push(Edit {
                page: original.page(),
                record: at,
                expected,
                target,
            });
        }
        let mut reports = library_plan::compile_edits(before, old, &library, &edits)?;
        if &config != original {
            reports.push(config.compile_output());
        }
        Ok(Prepared {
            config,
            library,
            edits,
            reports,
        })
    }
}
#[cfg(test)]
#[path = "yjx_sc620_oem_profile_tests.rs"]
mod tests;
