//! Exact GG Rival600 native settings and eight-zone schema. Read-only conversion.
use super::field;
use serde_json::Value;
pub(super) const FIELDS: &[&str] = &[
    "resolution_1",
    "resolution_2",
    "raw_cpi",
    "resolution_level",
    "polling_rate",
    "proximity_threshold",
    "accel",
    "decel",
    "angle_snapping",
    "button_mappings",
    "wheel_mappings",
    "logo_lighting",
    "wheel_lighting",
    "z2_lighting",
    "z3_lighting",
    "z4_lighting",
    "z5_lighting",
    "z6_lighting",
    "z7_lighting",
];
pub(super) fn proximity(s: &Value) -> Result<u8, String> {
    let p = s
        .get("proximity_threshold")
        .ok_or("GG Rival600 缺少抬升距离记录")?;
    let obj = p.as_object().ok_or("GG 抬升记录不是对象")?;
    if obj.len() != 3
        || obj
            .keys()
            .any(|k| !["enable", "low", "high"].contains(&k.as_str()))
    {
        return Err("GG 抬升记录包含缺失或未核对字段".into());
    }
    if field(p, "enable")? != 1 {
        return Err("GG 关闭抬升传感器的行为尚未转换，未保存部分配置".into());
    }
    let high = field(p, "high")?;
    let low = field(p, "low")?;
    if low > 255 || high > 255 {
        return Err("GG 抬升阈值超出字节范围".into());
    }
    let index = [120, 115, 110, 105, 100, 95, 90, 85]
        .iter()
        .position(|v| *v == high)
        .ok_or("GG 抬升阈值不是已核对的八级值")?;
    if low + 4 != high {
        return Err("GG 抬升上下阈值与原厂八级规则不一致".into());
    }
    for key in ["accel", "decel", "angle_snapping"] {
        let value = s.get(key).ok_or("GG Rival600 缺少原厂性能字段")?;
        if value.as_object().is_none_or(|o| o.len() != 1) || field(value, "level")? != 0 {
            return Err(format!(
                "GG {key} 使用原厂内核驱动的非默认行为，尚不能逐鼠标无损转换"
            ));
        }
    }
    if let Some(mapping) = s.get("button_mappings") {
        let obj = mapping.as_object().ok_or("GG 按键记录不是对象")?;
        if obj
            .keys()
            .any(|k| !["buttons", "no_live_deploy"].contains(&k.as_str()))
        {
            return Err("GG Rival600 按键表含未核对字段".into());
        }
        if mapping
            .get("no_live_deploy")
            .is_some_and(|v| !matches!(v.as_u64(), Some(0 | 1)))
        {
            return Err("GG 按键部署标志无效".into());
        }
    }
    Ok(index as u8)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{gg_profiles, icue_profiles::Prepared, profiles::Profile};
    use gearwise_core::{
        model::{ButtonAction, Modifiers},
        Setting,
    };
    use rusqlite::Connection;
    use serde_json::json;
    fn reference() -> Value {
        serde_json::from_str::<Value>(include_str!(
            "../../docs/development/gg-rival600-default-reference.json"
        ))
        .unwrap()["settings"]
            .clone()
    }
    fn fixture(s: &Value) -> Connection {
        let c = gg_profiles::tests::fixture();
        c.execute(
            "UPDATE devices SET product_id=272111396,name='rival_600'",
            [],
        )
        .unwrap();
        c.execute("UPDATE configurations SET settings=?1", [s.to_string()])
            .unwrap();
        c.execute(
            "UPDATE bindings SET trigger_data=?1",
            [json!({"hidCode":6,"page":1,"type":0,"direction":1}).to_string()],
        )
        .unwrap();
        c
    }
    #[test]
    fn official_rival600_three_full_native_gg_cases_keep_seven_keys_eight_zones_and_original_side_macro(
    ) {
        let keys = [
            "wheel_lighting",
            "logo_lighting",
            "z2_lighting",
            "z3_lighting",
            "z4_lighting",
            "z5_lighting",
            "z6_lighting",
            "z7_lighting",
        ];
        let mut candidates = vec![];
        for mode in ["factory", "steady-off", "breathing-half"] {
            let mut source = reference();
            for (zone, key) in keys.iter().enumerate() {
                if mode == "steady-off" {
                    source[*key]["type"] = json!(0);
                    source[*key]["settings_mask"] = json!(1);
                    source[*key]["initial_color"] = if zone % 2 == 0 {
                        json!({"r":18+zone,"g":52,"b":86})
                    } else {
                        json!({"r":0,"g":0,"b":0})
                    };
                }
                if mode == "breathing-half" {
                    source[*key]["type"] = json!(2);
                }
            }
            let c = fixture(&source);
            c.execute("UPDATE configurations SET id=?1,name=?1", [mode])
                .unwrap();
            c.execute("UPDATE bindings SET config_id=?1", [mode])
                .unwrap();
            let mut prepared = gg_profiles::read(&c).unwrap();
            let p = prepared.candidates[0]
                .profile
                .as_ref()
                .unwrap_or_else(|| panic!("{:?}", prepared.candidates[0].issues));
            let plan = p.multi.as_ref().unwrap();
            assert_eq!(p.target_model, Some(0x1724));
            assert!(plan.settings.contains(&Setting::LiftOff(4)));
            assert_eq!(
                plan.settings
                    .iter()
                    .filter(|s| matches!(s, Setting::Button { .. }))
                    .count(),
                6
            );
            assert_eq!(
                plan.settings
                    .iter()
                    .filter(|s| matches!(s, Setting::Lighting { .. }))
                    .count(),
                8
            );
            assert!(plan.settings.iter().any(|s| matches!(
                s,
                Setting::Button {
                    id: 6,
                    action: ButtonAction::Macro(_)
                }
            )));
            let result = crate::multibrand::simulate(plan).unwrap();
            assert_eq!(result["software_macros"][0]["virtual_delay_ms"], 23);
            assert_eq!(result["hardware_tested"], false);
            let (mut d, log) = gearwise_steelseries::simulated_model(0x1724).unwrap();
            for zone in 0..8 {
                let setting = plan
                    .settings
                    .iter()
                    .find(|s| matches!(s,Setting::Lighting{zone:z,..}if *z==zone))
                    .unwrap();
                log.lock().unwrap().clear();
                d.apply(setting).unwrap();
                let ops = log.lock().unwrap();
                let gearwise_core::transport::MockRecord::SetFeature(bytes) = &ops[0] else {
                    panic!()
                };
                assert_eq!((bytes[3], bytes[8]), (zone, zone));
                if mode == "factory" {
                    let refs: Value = serde_json::from_str(include_str!(
                        "../../docs/development/rival600-independent-gradient-vectors.json"
                    ))
                    .unwrap();
                    let hex = refs["cases"][zone as usize]["hidapi_feature_hex"]
                        .as_str()
                        .unwrap();
                    let mut expected = (0..hex.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                        .collect::<Vec<_>>();
                    expected[9..11].copy_from_slice(&10000u16.to_le_bytes());
                    assert_eq!(bytes, &expected);
                }
                if mode == "breathing-half" {
                    assert_eq!(bytes[30], 7);
                    assert_eq!(
                        (0..7).map(|i| bytes[37 + i * 4]).collect::<Vec<_>>(),
                        [0, 42, 42, 42, 42, 43, 44]
                    );
                }
            }
            candidates.append(&mut prepared.candidates);
        }
        let mut bad = reference();
        bad.as_object_mut().unwrap().remove("z7_lighting");
        candidates.extend(gg_profiles::read(&fixture(&bad)).unwrap().candidates);
        if let Some(path) = std::env::var_os("OPEN_HUB_GG_RIVAL600_FIXTURE") {
            std::fs::write(
                path,
                serde_json::to_vec_pretty(&Prepared {
                    name: "Rival600 八区原厂配置".into(),
                    source_kind: "GG SQLite",
                    candidates,
                })
                .unwrap(),
            )
            .unwrap();
        }
    }
    #[test]
    fn wrong_lift_pair_kernel_behavior_missing_zone_or_model_never_imports_partial_state() {
        for case in 0..11 {
            let mut s = reference();
            match case {
                0 => s["proximity_threshold"]["low"] = json!(95),
                1 => s["proximity_threshold"]["high"] = json!(101),
                2 => s["proximity_threshold"]["enable"] = json!(0),
                3 => s["accel"]["level"] = json!(1),
                4 => s["angle_snapping"]["level"] = json!(1),
                5 => s["logo_element"] = json!({}),
                6 => s["button_mappings"]["buttons"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"function":0,"key_codes":[0,0,0,0]})),
                7 => s["z7_lighting"]["settings_mask"] = json!(8),
                8 => s["macros"] = json!({"unreviewed":1}),
                9 => s["proximity_threshold"]["low"] = json!(4294967295u32),
                _ => s["z7_lighting"]["element_pos"]["x"] = json!(0),
            };
            assert!(
                gg_profiles::read(&fixture(&s)).unwrap().candidates[0]
                    .profile
                    .is_none(),
                "case {case}"
            );
        }
        for (i, high) in [120, 115, 110, 105, 100, 95, 90, 85]
            .into_iter()
            .enumerate()
        {
            let mut s = reference();
            s["proximity_threshold"]["low"] = json!(high - 4);
            s["proximity_threshold"]["high"] = json!(high);
            assert_eq!(proximity(&s).unwrap(), i as u8);
        }
        let c = fixture(&reference());
        c.execute("UPDATE devices SET name='rival_650_wireless'", [])
            .unwrap();
        assert!(gg_profiles::read(&c).unwrap().candidates[0]
            .profile
            .is_none());
    }
    #[test]
    #[ignore = "requires current GG Rival600 browser saves"]
    fn browser_gg_rival600_full_plans_reload_and_replay_eight_zones_lift_and_original_macro() {
        let profiles: Vec<Profile> = serde_json::from_slice(
            &std::fs::read(std::env::var_os("OPEN_HUB_GG_RIVAL600_UI_PROFILES").unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(profiles.len(), 3);
        let mut store = crate::profiles::ProfileStore::default();
        crate::profile_transfer::merge(&mut store, profiles.clone()).unwrap();
        let file =
            std::env::temp_dir().join(format!("gg-rival600-{}.json", crate::profiles::new_id()));
        store.save(&file).unwrap();
        let restored = crate::profiles::ProfileStore::load(&file);
        std::fs::remove_file(file).unwrap();
        for source in profiles {
            let p = restored.get(&source.id).unwrap();
            let plan = p.multi.as_ref().unwrap();
            assert_eq!(p.target_model, Some(0x1724));
            assert!(plan.settings.contains(&Setting::LiftOff(7)));
            assert_eq!(
                plan.settings
                    .iter()
                    .filter(|s| matches!(s, Setting::Lighting { .. }))
                    .count(),
                8
            );
            assert!(plan.settings.contains(&Setting::Button {
                id: 7,
                action: ButtonAction::Key {
                    usage: 6,
                    mods: Modifiers(17)
                }
            }));
            let r = crate::multibrand::simulate(plan).unwrap();
            assert_eq!(r["software_macros"][0]["virtual_delay_ms"], 23);
            assert_eq!(r["hardware_tested"], false);
            assert!(!p.targets(0x1038, &[0x1722]));
        }
    }
    #[test]
    #[ignore = "requires original GG122 migration and locally decoded inert descriptor"]
    fn original_gg122_migration_and_native_descriptor_confirm_proximity_and_eight_zone_facts_without_execution(
    ) {
        use sha2::{Digest, Sha256};
        let bytes = std::fs::read(std::env::var_os("OPEN_HUB_GG_RIVAL600_SQL").unwrap()).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            "a0378f34e280c9a3dd45ecb0e6e7bdf03b09bd1e4d0d88c11f37c66fb5a5b4e4"
        );
        let text = String::from_utf8(bytes).unwrap();
        let first = text.find("'{").unwrap() + 1;
        let last = first + text[first..].find("}',").unwrap() + 1;
        let settings: Value = serde_json::from_str(&text[first..last]).unwrap();
        assert_eq!(settings, reference());
        let result = gg_profiles::read(&fixture(&settings)).unwrap();
        assert!(result.candidates[0].profile.is_some());
        let bytes =
            std::fs::read(std::env::var_os("OPEN_HUB_GG122_RIVAL600_DESCRIPTOR").unwrap()).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            "0b1c2c7474fedeaa07a17eac35a248a421493781db70d47c2c54034e3f0511ed"
        );
        let source = String::from_utf8(bytes).unwrap();
        let first = source.find("(struct proximity_threshold").unwrap();
        let end = first
            + source[first..]
                .find(";; mouse button mapping table")
                .unwrap();
        let proximity = &source[first..end];
        for fact in [
            "(field report_id uint8 (constant 0x00))",
            "(field command uint8 (constant 0x96))",
            "(field reserved uint8 (constant 0x00))",
            "(field enable uint8 (values 0x00 0x01))",
            "(field low uint8 (range 0x00 0xff))",
            "(field high uint8 (range 0x00 0xff))",
            "(api-write HID (chunk HIDIO 33 payload))",
        ] {
            assert!(proximity.contains(fact));
        }
    }
}
