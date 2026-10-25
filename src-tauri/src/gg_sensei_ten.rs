//! Sensei Ten batch-CPI schema: inert GG122 factories 6755/10232.
//! Disabled OEM slots are omitted in the native enabled-stage list; active
//! hardware slot is translated to its index in that list, never guessed by DPI.
use super::{field, number};
use gearwise_core::model::DpiStage;
use serde_json::Value;
pub(super) fn dpi(s: &Value, notes: &mut Vec<String>) -> Result<(Vec<DpiStage>, usize), String> {
    let batch = s
        .get("batch_resolution")
        .ok_or("缺少 Sensei Ten 五档 CPI 记录")?;
    let object = batch.as_object().ok_or("Sensei Ten CPI 记录不是对象")?;
    let keys = [
        "enable_mask",
        "active_level",
        "resolution_1",
        "resolution_2",
        "resolution_3",
        "resolution_4",
        "resolution_5",
    ];
    if object.len() != keys.len() || object.keys().any(|k| !keys.contains(&k.as_str())) {
        return Err("Sensei Ten CPI 记录包含缺失或未核对的字段".into());
    }
    let mask = field(batch, "enable_mask")?;
    let active_slot = field(batch, "active_level")?;
    if !(1..=31).contains(&mask)
        || !(1..=5).contains(&active_slot)
        || mask & (1 << (active_slot - 1)) == 0
    {
        return Err("Sensei Ten CPI 启用掩码或当前档位无效".into());
    }
    let raw = s
        .get("raw_cpi")
        .map(|v| v.as_object().ok_or("Sensei Ten 显示 CPI 不是对象"))
        .transpose()?;
    if raw.is_some_and(|r| {
        r.keys()
            .any(|k| !["level1", "level2", "level3", "level4", "level5"].contains(&k.as_str()))
    }) {
        return Err("Sensei Ten 显示 CPI 含未核对字段".into());
    }
    let mut stages = Vec::new();
    let mut active = None;
    for slot in 1..=5 {
        let code = field(batch, &format!("resolution_{slot}"))?;
        let enabled = mask & (1 << (slot - 1)) != 0;
        if code > 360 || enabled && code == 0 {
            return Err("Sensei Ten CPI 编码超出 50–18000 范围".into());
        }
        let dpi = (code * 50) as u16;
        if let Some(value) = raw.and_then(|r| r.get(&format!("level{slot}"))) {
            let raw = number(value)?;
            if raw != 0 && raw != u32::from(dpi) {
                return Err("Sensei Ten 显示 CPI 与设备编码不一致".into());
            }
        }
        if enabled {
            if slot == active_slot {
                active = Some(stages.len());
            }
            stages.push(DpiStage::uniform(dpi));
        }
    }
    if mask != 31 {
        notes.push(
            "只导入原方案启用的 CPI 档位，按原顺序保留当前档位；未启用槽位的编辑历史不导入。"
                .into(),
        );
    }
    Ok((stages, active.ok_or("Sensei Ten 当前 CPI 档位未启用")?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{gg_profiles, icue_profiles::Prepared, profiles::Profile};
    use gearwise_core::{
        model::{ButtonAction, DpiState, Modifiers},
        transport::MockRecord,
        Setting,
    };
    use rusqlite::Connection;
    use serde_json::json;
    fn reference() -> Value {
        serde_json::from_str::<Value>(include_str!(
            "../../docs/development/gg-sensei-ten-default-reference.json"
        ))
        .unwrap()["settings"]
            .clone()
    }
    fn fixture(source: &Value) -> Connection {
        let c = gg_profiles::tests::fixture();
        c.execute(
            "UPDATE devices SET product_id=272111666,name='sensei_ten'",
            [],
        )
        .unwrap();
        c.execute(
            "UPDATE configurations SET settings=?1",
            [source.to_string()],
        )
        .unwrap();
        c.execute(
            "UPDATE bindings SET trigger_data=?1",
            [json!({"hidCode":8,"page":1,"type":0,"direction":1}).to_string()],
        )
        .unwrap();
        c
    }
    fn prepared(source: &Value) -> Prepared {
        gg_profiles::read(&fixture(source)).unwrap()
    }
    #[test]
    fn official_five_stage_cpi_eight_controls_and_two_zones_reach_real_driver_and_export_native_dto(
    ) {
        let mut candidates = vec![];
        for (name, mask, slot) in [("factory-five", 31, 1), ("sparse-third", 21, 5)] {
            let mut s = reference();
            s["batch_resolution"]["enable_mask"] = json!(mask);
            s["batch_resolution"]["active_level"] = json!(slot);
            let c = fixture(&s);
            c.execute("UPDATE configurations SET id=?1,name=?1", [name])
                .unwrap();
            c.execute("UPDATE bindings SET config_id=?1", [name])
                .unwrap();
            let mut result = gg_profiles::read(&c).unwrap();
            let p = result.candidates[0]
                .profile
                .as_ref()
                .unwrap_or_else(|| panic!("{:?}", result.candidates[0].issues));
            assert_eq!(p.target_model, Some(0x1832));
            assert_eq!(p.active_level, if mask == 31 { 0 } else { 2 });
            let plan = p.multi.as_ref().unwrap();
            assert_eq!(
                plan.settings
                    .iter()
                    .filter(|s| matches!(s, Setting::Button { .. }))
                    .count(),
                7
            );
            assert_eq!(
                plan.settings
                    .iter()
                    .filter(|s| matches!(s, Setting::Lighting { .. }))
                    .count(),
                2
            );
            for (id, usage) in [(6, 78), (7, 75)] {
                assert!(plan.settings.contains(&Setting::Button {
                    id,
                    action: ButtonAction::Key {
                        usage,
                        mods: Modifiers(0)
                    }
                }));
            }
            assert!(plan.settings.iter().any(|s| matches!(
                s,
                Setting::Button {
                    id: 8,
                    action: ButtonAction::Macro(_)
                }
            )));
            let (mut device, log) = gearwise_steelseries::simulated_model(0x1832).unwrap();
            let d = plan
                .settings
                .iter()
                .find(|s| matches!(s, Setting::Dpi(_)))
                .unwrap();
            device.apply(d).unwrap();
            let expected = if mask == 31 {
                vec![0, 0x55, 0, 31, 1, 8, 0, 16, 0, 24, 0, 48, 0, 64, 0]
            } else {
                vec![0, 0x55, 0, 7, 3, 8, 0, 24, 0, 64, 0]
            };
            assert_eq!(
                log.lock().unwrap().as_slice(),
                &[MockRecord::Write(expected)]
            );
            let simulation = crate::multibrand::simulate(plan).unwrap();
            assert_eq!(simulation["hardware_tested"], false);
            assert_eq!(simulation["software_macros"][0]["virtual_delay_ms"], 23);
            candidates.append(&mut result.candidates);
        }
        let mut bad = reference();
        bad["batch_resolution"]["enable_mask"] = json!(0);
        candidates.extend(prepared(&bad).candidates);
        if let Some(path) = std::env::var_os("OPEN_HUB_GG_SENSEI_TEN_FIXTURE") {
            std::fs::write(
                path,
                serde_json::to_vec_pretty(&Prepared {
                    name: "Sensei Ten 原厂配置".into(),
                    source_kind: "GG SQLite",
                    candidates,
                })
                .unwrap(),
            )
            .unwrap();
        }
    }
    #[test]
    fn wrong_slot_mask_model_raw_cpi_or_layout_never_becomes_partial_import() {
        for case in 0..12 {
            let mut s = reference();
            match case {
                0 => s["batch_resolution"]["enable_mask"] = json!(32),
                1 => s["batch_resolution"]["enable_mask"] = json!(0),
                2 => {
                    s["batch_resolution"]["enable_mask"] = json!(1);
                    s["batch_resolution"]["active_level"] = json!(2);
                }
                3 => s["batch_resolution"]["active_level"] = json!(6),
                4 => s["batch_resolution"]["resolution_5"] = json!(361),
                5 => s["batch_resolution"]["resolution_1"] = json!(0),
                6 => s["raw_cpi"]["level1"] = json!(401),
                7 => s["batch_resolution"]["enable_mask"] = json!(true),
                8 => s["resolution_1"] = json!({"resolution":7,"dcpi":0}),
                9 => s["button_mappings"]["buttons"]
                    .as_array_mut()
                    .unwrap()
                    .pop()
                    .map(|_| ())
                    .unwrap(),
                10 => s["logo_lighting"]["settings_mask"] = json!(8),
                _ => s["batch_resolution"]["unreviewed"] = json!(1),
            };
            let result = prepared(&s);
            assert!(result.candidates[0].profile.is_none(), "case {case}");
            assert!(!result.candidates[0].issues.is_empty());
        }
        for (product, name) in [
            (0x10381830u32, "sensei_ten"),
            (0x10381832, "sensei_310"),
            (0x10381722, "sensei_ten"),
        ] {
            let c = fixture(&reference());
            c.execute(
                "UPDATE devices SET product_id=?1,name=?2",
                rusqlite::params![product, name],
            )
            .unwrap();
            assert!(gg_profiles::read(&c).unwrap().candidates[0]
                .profile
                .is_none());
        }
    }
    #[test]
    #[ignore = "requires current Sensei Ten native UI profiles"]
    fn browser_five_stage_import_roundtrips_store_and_executes_real_native_encoding() {
        let profiles: Vec<Profile> = serde_json::from_slice(
            &std::fs::read(std::env::var_os("OPEN_HUB_SENSEI_TEN_UI_PROFILES").unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(profiles.len(), 3);
        let mut store = crate::profiles::ProfileStore::default();
        crate::profile_transfer::merge(&mut store, profiles.clone()).unwrap();
        let file = std::env::temp_dir().join(format!(
            "sensei-ten-browser-{}.json",
            crate::profiles::new_id()
        ));
        store.save(&file).unwrap();
        let restored = crate::profiles::ProfileStore::load(&file);
        std::fs::remove_file(file).unwrap();
        for source in profiles {
            let p = if source.id.is_empty() {
                restored
                    .profiles
                    .iter()
                    .find(|p| p.name == source.name)
                    .unwrap()
            } else {
                restored.get(&source.id).unwrap()
            };
            assert!(!p.id.is_empty());
            assert_eq!(p.target_model, Some(0x1832));
            let plan = p.multi.as_ref().unwrap();
            let dpi = plan
                .settings
                .iter()
                .find(|s| matches!(s, Setting::Dpi(_)))
                .unwrap();
            let Setting::Dpi(DpiState { stages, active }) = dpi else {
                unreachable!()
            };
            assert_eq!(stages.len(), 5);
            assert_eq!(*active, 4);
            assert_eq!(stages[0].x, 50);
            assert_eq!(stages[4].x, 18000);
            let (mut device, log) = gearwise_steelseries::simulated_model(0x1832).unwrap();
            device.apply(dpi).unwrap();
            let mut expected = vec![0, 0x55, 0, 31, 5];
            for stage in stages {
                expected.extend_from_slice(&(stage.x / 50).to_le_bytes());
            }
            assert_eq!(
                log.lock().unwrap().as_slice(),
                &[MockRecord::Write(expected)]
            );
            assert_eq!(
                plan.settings
                    .iter()
                    .filter(|s| matches!(s, Setting::Button { .. }))
                    .count(),
                7
            );
            assert_eq!(
                plan.settings
                    .iter()
                    .filter(|s| matches!(s, Setting::Lighting { .. }))
                    .count(),
                2
            );
            let result = crate::multibrand::simulate(plan).unwrap();
            assert_eq!(result["hardware_tested"], false);
            assert_eq!(result["software_macros"].as_array().unwrap().len(), 1);
            if p.name.contains("导入") {
                assert_eq!(result["software_macros"][0]["virtual_delay_ms"], 23);
            }
            assert!(!p.targets(0x1038, &[0x1722]));
            assert!(!p.targets(0x046d, &[0x1832]));
        }
    }
    #[test]
    #[ignore = "requires inert original GG122 Sensei Ten migration"]
    fn original_migration_hash_and_json_literal_match_checked_reference_without_executing_sql() {
        use sha2::{Digest, Sha256};
        let bytes = std::fs::read(std::env::var_os("OPEN_HUB_GG_SENSEI_TEN_SQL").unwrap()).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            "8ccb81f2303ea5b8c91359948c55e1d9781b5c1deaffaad0dab2af0485350048"
        );
        let text = String::from_utf8(bytes).unwrap();
        let start = text
            .find("VALUES (668, 272111666, 'sensei_ten', 'Sensei Ten', 1,")
            .unwrap();
        let entry = &text[start..];
        let first = entry.find("'{").unwrap() + 1;
        let end = first + entry[first..].find("}',").unwrap() + 1;
        let value: Value = serde_json::from_str(&entry[first..end]).unwrap();
        assert_eq!(value, reference());
        assert!(prepared(&value).candidates[0].profile.is_some());
    }
}
