//! Parser-entry regressions for the strict exact-model importer.
//! All exports below are authored synthetic inputs, not captured DA exports.
//! Model-specific XML token facts were read statically from official module182;
//! no OEM program, DLL, or JavaScript is executed by these tests.
use super::*;
use serde_json::{json, Value};
use std::path::PathBuf;

struct ExportFile(PathBuf);
impl ExportFile {
    fn new(extension: &str, bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "open-hub-dav3pro-import-{}.{}",
            crate::profiles::new_id(),
            extension
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}
impl Drop for ExportFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

// Test-only envelope writer. The production parser is still responsible for
// decoding and bounding the actual source format.
fn base64(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        out.push(CHARS[(a >> 2) as usize] as char);
        out.push(CHARS[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
fn v4(product: u16, payload: Value) -> ExportFile {
    let raw = json!({"productId": product, "category": "MOUSE",
        "deviceName": "DeathAdder V3 Pro", "profiles": [{"name": "Synthetic DA import",
            "payload": base64(&serde_json::to_vec(&payload).unwrap()),
            "hash": source_hash::profile_hash(&payload).unwrap()}], "macros": []});
    ExportFile::new("synapse4", &serde_json::to_vec(&raw).unwrap())
}
fn payload() -> Value {
    json!({"guid": "synthetic-da", "name": "Synthetic DA import",
        "dpi": {"x": 1600, "y": 2400,"independent":true},
        "dpiStages": {"enable": true, "active": 3, "count":5,"stages": [
            {"x": 400,"y": 800,"independent":true,"visible":true,"index":0}, {"x": 800,"y": 1200,"independent":true,"visible":true,"index":1}, {"x": 1600,"y": 2400,"independent":true,"visible":true,"index":2},
            {"x": 3200,"y": 4800,"independent":true,"visible":true,"index":3}, {"x": 15000,"y": 30000,"independent":true,"visible":true,"index":4}]},
        "pollingRate": 1000, "pollingRateWireless": 500,
        "mappings": canonical_mappings()})
}

fn canonical_mappings() -> Vec<Value> {
    let mut rows = vec![];
    for layer in [false, true] {
        for (input, assignment) in [
            ("RightClick", "Menu"),
            ("LeftClick", "Click"),
            ("ScrollButton", "ScrollClick"),
            ("Button4", "Previous"),
            ("Button5", "Next"),
            ("ScrollUp", "ScrollUp"),
            ("ScrollDown", "ScrollDown"),
        ] {
            rows.push(if input == "RightClick" && !layer {
                mapping(
                    input,
                    layer,
                    "keyboardGroup",
                    Some(json!({"key":"KEY_C","modifiers":["KEY_RIGHT_ALT"]})),
                )
            } else {
                mapping(
                    input,
                    layer,
                    "mouseGroup",
                    Some(json!({"mouseAssignment":assignment})),
                )
            });
        }
        rows.push(mapping(
            "DKM_SB_03",
            layer,
            "sensitivityGroup",
            Some(json!({"sensitivityAssignment":"DPI_CycleUp"})),
        ));
    }
    rows
}

fn clutch_payload() -> Value {
    let mut source = payload();
    for row in source["mappings"].as_array_mut().unwrap() {
        let second = row["isHyperShift"] == true;
        let group = match row["inputID"].as_str().unwrap() {
            "RightClick" => Some(json!({"sensitivityAssignment":"DPI_Clutch",
                "x":if second {2400}else{800},"y":if second {3200}else{1600},"independent":true})),
            "ScrollButton" => Some(json!({"sensitivityAssignment":"DPI_Clutch",
                "x":if second {1800}else{1200},"y":if second {1800}else{1200},"independent":false})),
            _ => None,
        };
        if let Some(group) = group {
            *row = mapping(
                row["inputID"].as_str().unwrap(),
                second,
                "sensitivityGroup",
                Some(group),
            );
        }
    }
    source
}

fn assert_clutch_profile_store_and_native_readback(profile: &Profile) {
    let mut store = crate::profiles::ProfileStore::default();
    crate::profile_transfer::merge(&mut store, vec![profile.clone()]).unwrap();
    let file = ExportFile::new("json", b"");
    store.save(&file.0).unwrap();
    let loaded = crate::profiles::ProfileStore::load(&file.0);
    let stored = loaded.get(&profile.id).unwrap();
    assert_eq!(
        serde_json::to_value(stored).unwrap(),
        serde_json::to_value(profile).unwrap()
    );
    let plan = stored.multi.as_ref().unwrap();
    crate::multibrand::validate(plan).unwrap();
    assert_eq!(plan.settings.len(), 18);
    assert_eq!(
        plan.settings
            .iter()
            .filter(|s| matches!(s, Setting::Button { .. }))
            .count(),
        16
    );
    assert!(plan.macros.is_empty() && plan.source.is_none());
    assert_eq!(
        crate::multibrand::simulate(plan).unwrap()["software_macros"],
        json!([])
    );
    let (mut device, _) = gearwise_razer::simulated_dav3pro(plan.product).unwrap();
    let before = device.read_state().unwrap();
    for setting in &plan.settings {
        device.apply(setting).unwrap();
    }
    let state = device.read_state().unwrap();
    for setting in &plan.settings {
        match setting {
            Setting::Button { id, action } => assert_eq!(
                gearwise_razer::dav3pro_buttons::encode(*id, action).unwrap(),
                gearwise_razer::dav3pro_buttons::encode(*id, &state.buttons[id]).unwrap()
            ),
            Setting::Dpi(dpi) => assert_eq!(state.dpi.as_ref(), Some(dpi)),
            Setting::ReportRate(rate) => assert_eq!(state.report_rate, Some(*rate)),
            _ => panic!("unexpected imported setting"),
        }
    }
    assert!(state.lighting.is_empty());
    assert_eq!(state.tuning, before.tuning);
    assert_eq!(state.calibration, before.calibration);
}

#[test]
fn dav3pro_v4_independent_xy_and_uniform_clutches_parse_persist_and_execute_without_source_loss() {
    let mut exported = vec![];
    for product in [0xb6, 0xb7] {
        let source = clutch_payload();
        let file = v4(product, source.clone());
        let original = std::fs::read(&file.0).unwrap();
        let prepared = prepare(&file.0).unwrap();
        let profile = prepared.candidates[0]
            .profile
            .as_ref()
            .unwrap_or_else(|| panic!("{:?}", prepared.candidates[0].issues));
        let plan = profile.multi.as_ref().unwrap();
        for (id, expected, bytes) in [
            (
                2,
                json!({"Dpi":{"ShiftXY":{"x":800,"y":1600}}}),
                [6, 5, 5, 3, 32, 6, 64],
            ),
            (3, json!({"Dpi":{"Shift":1200}}), [6, 5, 5, 4, 176, 4, 176]),
            (
                258,
                json!({"Dpi":{"ShiftXY":{"x":2400,"y":3200}}}),
                [6, 5, 5, 9, 96, 12, 128],
            ),
            (259, json!({"Dpi":{"Shift":1800}}), [6, 5, 5, 7, 8, 7, 8]),
        ] {
            let action = plan
                .settings
                .iter()
                .find_map(|s| match s {
                    Setting::Button { id: actual, action } if *actual == id => Some(action),
                    _ => None,
                })
                .unwrap();
            assert_eq!(serde_json::to_value(action).unwrap(), expected);
            assert_eq!(
                gearwise_razer::dav3pro_buttons::encode(id, action).unwrap(),
                bytes
            );
        }
        assert!(prepared.parsed.profiles[0]
            .bindings
            .iter()
            .any(|b| b.label == "按住切换 X=800 / Y=1600 DPI"));
        assert!(prepared.parsed.profiles[0]
            .bindings
            .iter()
            .any(|b| b.label == "按住切换 1200 DPI"));
        let sidearm_synapse_parser::raw_source::RawSource::V4 { profiles, .. } =
            prepared.parsed.raw_source.as_deref().unwrap()
        else {
            panic!()
        };
        assert_eq!(profiles[0].decoded.as_ref().unwrap().value, source);
        assert_eq!(prepared.parsed.profiles[0].bindings.len(), 16);
        assert_clutch_profile_store_and_native_readback(profile);
        assert_eq!(std::fs::read(&file.0).unwrap(), original);
        exported.push(serde_json::to_value(prepared).unwrap());
    }
    if let Some(path) = std::env::var_os("OPEN_HUB_DAV3PRO_SHIFTXY_IMPORT_PREPARED") {
        std::fs::write(path, serde_json::to_vec_pretty(&exported).unwrap()).unwrap();
    }
}

#[test]
fn dav3pro_v4_explicit_independent_equal_axes_preserve_source_intent_and_legacy_wire() {
    for product in [0xb6, 0xb7] {
        let mut source = clutch_payload();
        for row in source["mappings"].as_array_mut().unwrap() {
            if row["inputID"] == "RightClick" {
                row["sensitivityGroup"] = json!({"sensitivityAssignment":"DPI_Clutch",
                    "x":1600,"y":1600,"independent":true});
            }
        }
        let prepared = prepare(&v4(product, source).0).unwrap();
        let profile = prepared.candidates[0].profile.as_ref().unwrap();
        let (mut device, _) = gearwise_razer::simulated_dav3pro(product).unwrap();
        device.read_state().unwrap();
        for id in [2, 258] {
            let setting = profile
                .multi
                .as_ref()
                .unwrap()
                .settings
                .iter()
                .find(|s| matches!(s, Setting::Button { id: actual, .. } if *actual == id))
                .unwrap();
            let Setting::Button { action, .. } = setting else {
                panic!()
            };
            assert_eq!(
                serde_json::to_value(action).unwrap(),
                json!({"Dpi":{"ShiftXY":{"x":1600,"y":1600}}})
            );
            assert_eq!(
                gearwise_razer::dav3pro_buttons::encode(id, action).unwrap(),
                [6, 5, 5, 6, 64, 6, 64]
            );
            device.apply(setting).unwrap();
            assert_eq!(
                serde_json::to_value(&device.read_state().unwrap().buttons[&id]).unwrap(),
                json!({"Dpi":{"Shift":1600}})
            );
        }
        assert_clutch_profile_store_and_native_readback(profile);
    }
}

#[test]
fn dav3pro_v4_invalid_independent_clutches_and_unproved_options_block_complete_sources() {
    for product in [0xb6, 0xb7] {
        for layer in [false, true] {
            for group in [
                json!({"sensitivityAssignment":"DPI_Clutch","x":800,"y":1600,"independent":false}),
                json!({"sensitivityAssignment":"DPI_Clutch","x":800,"y":1600}),
                json!({"sensitivityAssignment":"DPI_Clutch","x":99,"y":1600,"independent":true}),
                json!({"sensitivityAssignment":"DPI_Clutch","x":800,"y":30050,"independent":true}),
                json!({"sensitivityAssignment":"DPI_Clutch","x":800,"y":1601,"independent":true}),
                json!({"sensitivityAssignment":"DPI_Clutch","x":800,"independent":true}),
                json!({"sensitivityAssignment":"DPI_Clutch","x":800,"y":1600,"independent":"true"}),
                json!({"sensitivityAssignment":"DPI_Clutch","x":800,"y":1600,"independent":true,"turbo":false}),
            ] {
                let mut source = clutch_payload();
                let row = source["mappings"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|r| r["inputID"] == "RightClick" && r["isHyperShift"] == layer)
                    .unwrap();
                *row = mapping("RightClick", layer, "sensitivityGroup", Some(group));
                let file = v4(product, source.clone());
                let original = std::fs::read(&file.0).unwrap();
                let prepared = prepare(&file.0).unwrap();
                assert!(prepared.candidates[0].profile.is_none());
                assert!(prepared.candidates[0]
                    .issues
                    .iter()
                    .any(|issue| issue.contains("/sensitivityGroup")));
                let sidearm_synapse_parser::raw_source::RawSource::V4 { profiles, .. } =
                    prepared.parsed.raw_source.as_deref().unwrap()
                else {
                    panic!()
                };
                assert_eq!(profiles[0].decoded.as_ref().unwrap().value, source);
                assert_eq!(std::fs::read(&file.0).unwrap(), original);
            }
        }
    }
}

#[test]
#[ignore = "requires saved profiles from the current exact DA ShiftXY source UI"]
fn browser_imported_dav3pro_xy_clutches_preserve_full_profiles_through_store_and_native_codec() {
    use std::collections::{BTreeMap, BTreeSet};
    let profiles: Vec<Profile> = serde_json::from_slice(
        &std::fs::read(std::env::var_os("OPEN_HUB_DAV3PRO_SHIFTXY_UI_PROFILES").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(profiles.len(), 2);
    let mut transports = BTreeSet::new();
    for profile in profiles {
        let plan = profile.multi.as_ref().unwrap();
        assert_eq!(profile.target_vendor, Some(0x1532));
        assert_eq!(profile.target_model, Some(plan.product));
        assert!([0xb6, 0xb7].contains(&plan.product));
        assert!(transports.insert(plan.product));
        let source = prepare(&v4(plan.product, clutch_payload()).0).unwrap();
        let expected = source.candidates[0].profile.as_ref().unwrap();
        assert_eq!(profile.levels, expected.levels);
        assert_eq!(profile.active_level, expected.active_level);
        assert_eq!(profile.report_rate_hz, expected.report_rate_hz);
        let actions = |plan: &Plan| {
            plan.settings
                .iter()
                .filter_map(|s| match s {
                    Setting::Button { id, action } => Some((*id, action.clone())),
                    _ => None,
                })
                .collect::<BTreeMap<_, _>>()
        };
        let mut desired = actions(expected.multi.as_ref().unwrap());
        desired.insert(
            2,
            serde_json::from_value(json!({"Dpi":{"ShiftXY":{"x":1000,"y":2000}}})).unwrap(),
        );
        desired.insert(
            258,
            serde_json::from_value(json!({"Dpi":{"ShiftXY":{"x":2400,"y":4800}}})).unwrap(),
        );
        assert_eq!(actions(plan), desired);
        assert_clutch_profile_store_and_native_readback(&profile);
    }
    assert_eq!(transports, BTreeSet::from([0xb6, 0xb7]));
}

#[test]
fn dav3pro_exact_transports_parse_persist_and_execute_native_stage_and_binding_profiles() {
    let mut exported = vec![];
    for product in [0x00b6, 0x00b7] {
        let file = v4(product, payload());
        let prepared = prepare(&file.0).unwrap();
        assert_eq!(prepared.parsed.profiles[0].bindings.len(), 16);
        assert!(prepared.parsed.profiles[0]
            .bindings
            .iter()
            .all(|b| matches!(b.action, ParsedAction::ModelAction { .. })));
        exported.push(serde_json::to_value(&prepared).unwrap());
        assert_eq!(prepared.parsed.device.as_ref().unwrap().product, product);
        assert!(prepared.conversion_error.is_none());
        let profile = prepared.candidates[0]
            .profile
            .as_ref()
            .unwrap_or_else(|| panic!("{:?}", prepared.candidates[0].issues));
        assert_eq!(profile.target_model, Some(product));
        assert_eq!(profile.active_level, 2);
        assert_eq!(profile.levels.len(), 5);
        assert_eq!(profile.levels[4].y, Some(30000));
        assert_eq!(
            profile.report_rate_hz,
            Some(if product == 0xb6 { 1000 } else { 500 })
        );
        let mut store = crate::profiles::ProfileStore::default();
        crate::profile_transfer::merge(&mut store, vec![profile.clone()]).unwrap();
        let path = std::env::temp_dir().join(format!(
            "dav3pro-import-store-{}.json",
            crate::profiles::new_id()
        ));
        store.save(&path).unwrap();
        let loaded = crate::profiles::ProfileStore::load(&path);
        std::fs::remove_file(path).unwrap();
        let plan = loaded.get(&profile.id).unwrap().multi.as_ref().unwrap();
        assert_eq!(
            plan.settings
                .iter()
                .filter(|s| matches!(s, Setting::Button { .. }))
                .count(),
            16
        );
        let report = crate::multibrand::simulate(plan).unwrap();
        assert_eq!(report["hardware_tested"], false);
        let (mut device, _) = gearwise_razer::simulated_dav3pro(product).unwrap();
        device.read_state().unwrap();
        for setting in &plan.settings {
            device.apply(setting).unwrap();
        }
        let state = device.read_state().unwrap();
        assert_eq!(state.dpi.as_ref().unwrap().active, 2);
        assert_eq!(state.dpi.as_ref().unwrap().stages[4].y, 30000);
        for setting in &plan.settings {
            if let Setting::Button { id, action } = setting {
                assert_eq!(
                    gearwise_razer::dav3pro_buttons::encode(*id, action).unwrap(),
                    gearwise_razer::dav3pro_buttons::encode(*id, &state.buttons[id]).unwrap()
                );
            }
        }
        assert!(state.lighting.is_empty());
        assert!(plan.macros.is_empty());
        assert!(!profile.targets(0x046d, &[product]));
        assert!(!profile.targets(0x1532, &[if product == 0xb6 { 0xb7 } else { 0xb6 }]));
    }
    if let Some(path) = std::env::var_os("OPEN_HUB_DAV3PRO_IMPORT_PREPARED") {
        let path = std::path::PathBuf::from(path);
        std::fs::write(&path, serde_json::to_vec_pretty(&exported).unwrap()).unwrap();
        let mut blocked = vec![];
        let mut sparse = payload();
        sparse["mappings"] = json!([mapping("RightClick", false, "disableGroup", None)]);
        blocked.push(prepare(&v4(0xb6, sparse).0).unwrap());
        let mut unknown = payload();
        unknown["unknownOEMFeature"] = json!({"enabled":true});
        blocked.push(prepare(&v4(0xb7, unknown).0).unwrap());
        let source = payload();
        let outer = json!({"productId":0xb6,"category":"MOUSE","deviceName":"DeathAdder V3 Pro","profiles":[{"name":"Bad checksum source","payload":base64(&serde_json::to_vec(&source).unwrap()),"hash":"00000000000000000000000000000000"}],"macros":[]});
        blocked.push(
            prepare(&ExportFile::new("synapse4", &serde_json::to_vec(&outer).unwrap()).0).unwrap(),
        );
        let mut macro_source = outer;
        macro_source["productId"] = json!(0xb7);
        macro_source["profiles"][0]["name"] = json!("Unimplemented OEM macro source");
        macro_source["profiles"][0]["hash"] = json!(source_hash::profile_hash(&source).unwrap());
        macro_source["macros"] = json!([{"name":"Synthetic preserved macro","payload":base64(br#"{"guid":"m","macroEvents":[{"Type":99,"vendorEvent":{"bytes":[1,2,3]}}]}"#),"hash":"unverified"}]);
        blocked.push(
            prepare(&ExportFile::new("synapse4", &serde_json::to_vec(&macro_source).unwrap()).0)
                .unwrap(),
        );
        assert!(blocked.iter().all(|p| !p.candidates.is_empty()
            && p.candidates
                .iter()
                .all(|c| c.profile.is_none() && !c.issues.is_empty())));
        std::fs::write(
            path.parent()
                .unwrap()
                .join("dav3pro-import-blocked-prepared.json"),
            serde_json::to_vec_pretty(&blocked).unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "requires saved profiles from the current exact DA import UI"]
fn browser_imported_dav3pro_profiles_roundtrip_store_then_execute_native_codec() {
    use std::collections::{BTreeMap, BTreeSet};
    let mut profiles: Vec<Profile> = serde_json::from_slice(
        &std::fs::read(std::env::var_os("OPEN_HUB_DAV3PRO_IMPORT_UI_PROFILES").unwrap()).unwrap(),
    )
    .unwrap();
    assert!(profiles.len() >= 2);
    for profile in &mut profiles {
        profile.id = crate::profiles::new_id();
    }
    let mut store = crate::profiles::ProfileStore::default();
    crate::profile_transfer::merge(&mut store, profiles.clone()).unwrap();
    let file = ExportFile::new("json", b"");
    store.save(&file.0).unwrap();
    let loaded = crate::profiles::ProfileStore::load(&file.0);
    let mut transports = BTreeSet::new();
    for profile in profiles {
        let profile = loaded.get(&profile.id).unwrap();
        let plan = profile.multi.as_ref().unwrap();
        assert_eq!(plan.vendor, 0x1532);
        assert!([0xb6, 0xb7].contains(&plan.product));
        transports.insert(plan.product);
        assert_eq!(profile.target_vendor, Some(plan.vendor));
        assert_eq!(profile.target_model, Some(plan.product));
        assert!(!profile.targets(0x046d, &[plan.product]));
        assert!(!profile.targets(0x1532, &[if plan.product == 0xb6 { 0xb7 } else { 0xb6 }]));
        assert!(plan.source.is_none() && plan.macros.is_empty());
        crate::multibrand::validate(plan).unwrap();
        let expected = converted(plan.product, payload());
        let expected_plan = expected.multi.as_ref().unwrap();
        assert_eq!(profile.levels, expected.levels);
        assert_eq!(profile.active_level, 2);
        assert_eq!(profile.report_rate_hz, expected.report_rate_hz);
        let source_buttons = |plan: &Plan| {
            plan.settings
                .iter()
                .filter_map(|s| {
                    if let Setting::Button { id, action } = s {
                        Some((*id, action.clone()))
                    } else {
                        None
                    }
                })
                .collect::<BTreeMap<_, _>>()
        };
        let desired = source_buttons(plan);
        assert_eq!(desired.len(), 16);
        assert_eq!(desired, source_buttons(expected_plan));
        assert_eq!(
            plan.settings.len(),
            18,
            "no hidden or duplicate source settings"
        );
        for expected in &expected_plan.settings {
            if matches!(expected, Setting::Dpi(_) | Setting::ReportRate(_)) {
                assert!(plan.settings.contains(expected));
            }
        }
        let simulation = crate::multibrand::simulate(plan).unwrap();
        assert_eq!(simulation["hardware_tested"], false);
        assert_eq!(simulation["software_macros"], json!([]));
        let (mut device, _) = gearwise_razer::simulated_dav3pro(plan.product).unwrap();
        let before = device.read_state().unwrap();
        for setting in &plan.settings {
            device.apply(setting).unwrap();
        }
        let actual = device.read_state().unwrap();
        let expected_dpi = plan
            .settings
            .iter()
            .find_map(|s| {
                if let Setting::Dpi(d) = s {
                    Some(d)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(actual.dpi.as_ref(), Some(expected_dpi));
        assert_eq!(actual.report_rate, profile.report_rate_hz.map(|r| r as u16));
        for (id, action) in desired {
            assert_eq!(
                gearwise_razer::dav3pro_buttons::encode(id, &action).unwrap(),
                gearwise_razer::dav3pro_buttons::encode(id, &actual.buttons[&id]).unwrap()
            );
        }
        assert!(actual.lighting.is_empty());
        assert_eq!(actual.tuning, before.tuning);
        assert_eq!(actual.calibration, before.calibration);
    }
    assert_eq!(transports, BTreeSet::from([0xb6, 0xb7]));
}

#[test]
fn dav3pro_v4_unknown_feature_and_nested_action_options_cannot_be_imported() {
    for product in [0x00b6, 0x00b7] {
        let mut source = payload();
        source["powerSavingValue"] = json!(5);
        source["lowPowerMode"] = json!(30);
        source["unimplementedFeature"] = json!({"enabled": true});
        source["mappings"][0]["keyboardGroup"]["repeatWhilePressed"] = json!(true);
        let file = v4(product, source);
        let prepared = prepare(&file.0).unwrap();
        assert!(prepared.candidates[0].profile.is_none());
        assert!(!prepared.candidates[0].issues.is_empty());
        let sidearm_synapse_parser::raw_source::RawSource::V4 { profiles, .. } =
            prepared.parsed.raw_source.as_deref().unwrap()
        else {
            panic!()
        };
        assert_eq!(
            profiles[0].decoded.as_ref().unwrap().value["mappings"][0]["keyboardGroup"]
                ["repeatWhilePressed"],
            true
        );
        assert_eq!(
            profiles[0].decoded.as_ref().unwrap().value["unimplementedFeature"]["enabled"],
            true
        );
        assert!(serde_json::to_value(&prepared.parsed)
            .unwrap()
            .get("rawSource")
            .is_none());
    }
}

#[test]
fn dav3pro_v4_legacy_aliases_must_not_become_naga_thumb_slots() {
    let mut source = payload();
    source["mappings"] = json!([
        {"inputType":"DKMInput","inputID":"DKM_M_03","outputType":"mouseGroup","mouseGroup":{"mouseAssignment":"Previous"}},
        {"inputType":"DKMInput","inputID":"DKM_M_04","outputType":"mouseGroup","mouseGroup":{"mouseAssignment":"Next"}},
        {"inputType":"DKMInput","inputID":"DKM_SB_03","outputType":"sensitivityGroup","sensitivityGroup":{"sensitivityAssignment":"DPI_CycleUp"}}
    ]);
    let file = v4(0x00b6, source);
    let prepared = prepare(&file.0).unwrap();
    assert!(prepared.candidates[0].profile.is_none());
    assert!(prepared.candidates[0]
        .issues
        .iter()
        .any(|e| e.contains("DKM 兼容记录")));
    assert!(prepared.parsed.raw_source.is_some());
}

#[test]
fn dav3pro_v4_sparse_export_never_invents_or_overwrites_omitted_hardware_bindings() {
    for product in [0xb6, 0xb7] {
        let mut source = payload();
        source["mappings"] = json!([mapping(
            "RightClick",
            false,
            "mouseGroup",
            Some(json!({"mouseAssignment":"Next"}))
        )]);
        let file = v4(product, source.clone());
        let prepared = prepare(&file.0).unwrap();
        assert_eq!(prepared.parsed.profiles[0].bindings.len(), 1);
        assert!(prepared.candidates[0].profile.is_none());
        assert!(prepared.candidates[0]
            .issues
            .iter()
            .any(|e| e.contains("缺少明确来源") && e.contains("稀疏导出")));
        let sidearm_synapse_parser::raw_source::RawSource::V4 { profiles, .. } =
            prepared.parsed.raw_source.as_deref().unwrap()
        else {
            panic!()
        };
        assert_eq!(profiles[0].decoded.as_ref().unwrap().value, source);
    }
}

fn mapping(input: &str, layer: bool, output: &str, group: Option<Value>) -> Value {
    let mut value = json!({"inputType":if input.starts_with("DKM_"){"DKMInput"}else{"MouseInput"},
        "inputID":input,"isHyperShift":layer,"outputType":output});
    if let Some(group) = group {
        value[output] = group;
    }
    value
}
fn converted(product: u16, mut source: Value) -> Profile {
    // Author a complete explicit source; the production converter must not
    // invent records when a sparse export omits a canonical input.
    let mut rows = canonical_mappings();
    for row in source["mappings"].as_array().unwrap() {
        if let Some(index) = rows.iter().position(|r| {
            r["inputType"] == row["inputType"]
                && r["inputID"] == row["inputID"]
                && r["isHyperShift"] == row["isHyperShift"]
        }) {
            rows[index] = row.clone();
        } else {
            rows.push(row.clone());
        }
    }
    source["mappings"] = json!(rows);
    let file = v4(product, source);
    let prepared = prepare(&file.0).unwrap();
    assert!(
        prepared.conversion_error.is_none(),
        "{:?}",
        prepared.conversion_error
    );
    prepared.candidates[0]
        .profile
        .clone()
        .unwrap_or_else(|| panic!("{:?}", prepared.candidates[0].issues))
}

#[test]
fn dav3pro_v4_known_actions_layers_and_paired_hypershift_execute_without_host_macros() {
    for product in [0xb6, 0xb7] {
        let mut source = payload();
        source["mappings"] = json!([
            mapping(
                "RightClick",
                false,
                "keyboardGroup",
                Some(
                    json!({"keyGroup":2,"key":"KEY_F24","modifiers":["KEY_LEFT_CTRL","KEY_RIGHT_ALT"]})
                )
            ),
            mapping(
                "ScrollButton",
                false,
                "multimediaGroup",
                Some(json!({"multimediaAssignment":"Play"}))
            ),
            mapping("Button4", false, "hyperShiftGroup", None),
            mapping("Button4", true, "hyperShiftGroup", None),
            mapping(
                "Button5",
                false,
                "mouseGroup",
                Some(json!({"mouseAssignment":"Previous"}))
            ),
            mapping(
                "DKM_M_04",
                false,
                "mouseGroup",
                Some(json!({"mouseAssignment":"Previous"}))
            ),
            mapping(
                "DKM_SB_03",
                false,
                "sensitivityGroup",
                Some(json!({"sensitivityAssignment":"DPI_CycleDown"}))
            ),
            mapping(
                "ScrollUp",
                false,
                "mouseGroup",
                Some(json!({"mouseAssignment":"ScrollDown"}))
            ),
            mapping(
                "LeftClick",
                true,
                "keyboardGroup",
                Some(json!({"key":"KEY_YEN","modifiers":["KEY_RIGHT_WINDOWS"]}))
            ),
            mapping(
                "RightClick",
                true,
                "sensitivityGroup",
                Some(
                    json!({"sensitivityAssignment":"DPI_Clutch","x":2400,"y":2400,"independent":false})
                )
            ),
            mapping("Button5", true, "disableGroup", None)
        ]);
        let profile = converted(product, source);
        let plan = profile.multi.as_ref().unwrap();
        assert!(plan.macros.is_empty());
        assert!(plan.settings.contains(&Setting::Button {
            id: 2,
            action: ButtonAction::Key {
                usage: 0x73,
                mods: Modifiers(65)
            }
        }));
        assert!(plan.settings.contains(&Setting::Button {
            id: 257,
            action: ButtonAction::Key {
                usage: 0x89,
                mods: Modifiers(128)
            }
        }));
        assert!(plan.settings.contains(&Setting::Button {
            id: 96,
            action: ButtonAction::Dpi(gearwise_core::model::DpiFunction::CycleDown)
        }));
        assert!(plan.settings.contains(&Setting::Button {
            id: 4,
            action: ButtonAction::Hypershift
        }));
        assert!(plan.settings.contains(&Setting::Button {
            id: 260,
            action: ButtonAction::Hypershift
        }));
        let report = crate::multibrand::simulate(plan).unwrap();
        assert_eq!(report["hardware_tested"], false);
        assert_eq!(report["software_macros"], json!([]));
        let (mut device, _) = gearwise_razer::simulated_dav3pro(product).unwrap();
        device.read_state().unwrap();
        for setting in &plan.settings {
            device.apply(setting).unwrap();
        }
        let actual = device.read_state().unwrap();
        for setting in &plan.settings {
            if let Setting::Button { id, action } = setting {
                assert_eq!(
                    gearwise_razer::dav3pro_buttons::encode(*id, action).unwrap(),
                    gearwise_razer::dav3pro_buttons::encode(*id, &actual.buttons[id]).unwrap()
                );
            }
        }
        assert_eq!(actual.buttons[&4], ButtonAction::Hypershift);
        assert_eq!(actual.buttons[&260], ButtonAction::Hypershift);
    }
}

#[test]
fn dav3pro_v4_all_seven_proved_media_and_sensitivity_actions_roundtrip_on_both_transports() {
    let mut actions = vec![];
    for name in [
        "NextTrack",
        "PrevTrack",
        "MediaStop",
        "Play",
        "MuteVolume",
        "VolumeUp",
        "VolumeDown",
    ] {
        actions.push(("multimediaGroup", json!({"multimediaAssignment":name})));
    }
    for name in ["DPI_Up", "DPI_Down", "DPI_CycleUp", "DPI_CycleDown"] {
        actions.push(("sensitivityGroup", json!({"sensitivityAssignment":name})));
    }
    for product in [0xb6, 0xb7] {
        for (output, group) in &actions {
            let mut source = payload();
            source["mappings"] = json!([
                mapping("RightClick", false, output, Some(group.clone())),
                mapping("RightClick", true, output, Some(group.clone()))
            ]);
            let profile = converted(product, source);
            assert_eq!(
                crate::multibrand::simulate(profile.multi.as_ref().unwrap()).unwrap()
                    ["hardware_tested"],
                false
            );
        }
    }
}

#[test]
fn dav3pro_v4_hidden_reordered_inconsistent_or_disabled_stage_tables_are_blocked() {
    for product in [0xb6, 0xb7] {
        let mut source = payload();
        source["dpi"]["x"] = json!(1601);
        source["dpiStages"]["stages"][2]["x"] = json!(1601);
        let result = prepare(&v4(product, source).0).unwrap();
        assert!(result.candidates[0].profile.is_none());
        assert!(result.candidates[0].issues[0].contains("50 DPI"));
    }
    for (key, value) in [
        ("active", json!(0)),
        ("active", json!(6)),
        ("count", json!(4)),
        ("enable", json!(false)),
    ] {
        let mut source = payload();
        source["dpiStages"][key] = value;
        let file = v4(0xb6, source);
        let result = prepare(&file.0).unwrap();
        assert!(result.candidates[0].profile.is_none());
        assert!(result.candidates[0].issues[0].contains("dpiStages"));
    }
    for (field, value) in [
        ("visible", json!(false)),
        ("index", json!(4)),
        ("independent", json!(false)),
        ("unknownStageFlag", json!(true)),
    ] {
        let mut source = payload();
        source["dpiStages"]["stages"][0][field] = value;
        assert!(prepare(&v4(0xb7, source).0).unwrap().candidates[0]
            .profile
            .is_none());
    }
    let mut source = payload();
    source["dpi"]["x"] = json!(800);
    assert!(prepare(&v4(0xb6, source).0).unwrap().candidates[0]
        .profile
        .is_none());
}

#[test]
fn dav3pro_v4_unknown_action_options_and_unsafe_control_combinations_are_never_disabled() {
    let cases = [
        mapping(
            "RightClick",
            false,
            "keyboardGroup",
            Some(json!({"key":"KEY_C","modifiers":[],"repeatWhilePressed":true})),
        ),
        mapping(
            "RightClick",
            false,
            "macroGroup",
            Some(json!({"guid":"m","macroPlaybackOption":"WhilePressed","repeatCount":9})),
        ),
        mapping(
            "RightClick",
            false,
            "textBlockGroup",
            Some(json!({"text":"blocked text"})),
        ),
        mapping(
            "RightClick",
            false,
            "unknownOEMGroup",
            Some(json!({"nativeBytes":[3,1,2]})),
        ),
        mapping(
            "ScrollUp",
            false,
            "sensitivityGroup",
            Some(json!({"sensitivityAssignment":"DPI_Up"})),
        ),
        mapping(
            "RightClick",
            false,
            "sensitivityGroup",
            Some(json!({"sensitivityAssignment":"DPI_Clutch","x":800,"y":1600,"independent":false})),
        ),
        mapping("Button4", false, "hyperShiftGroup", None),
        mapping("LeftClick", false, "disableGroup", None),
    ];
    for product in [0xb6, 0xb7] {
        for action in &cases {
            let mut source = payload();
            let rows = source["mappings"].as_array_mut().unwrap();
            let index = rows
                .iter()
                .position(|r| {
                    r["inputType"] == action["inputType"]
                        && r["inputID"] == action["inputID"]
                        && r["isHyperShift"] == action["isHyperShift"]
                })
                .unwrap();
            rows[index] = action.clone();
            let file = v4(product, source.clone());
            let result = prepare(&file.0).unwrap();
            assert!(result.candidates[0].profile.is_none());
            assert!(!result.candidates[0].issues.is_empty());
            if action["outputType"] == "hyperShiftGroup" {
                assert!(result.candidates[0]
                    .issues
                    .iter()
                    .any(|e| e.contains("Hypershift 必须保留原厂两层成对记录")));
            }
            if action["inputID"] == "LeftClick" {
                assert!(result.candidates[0]
                    .issues
                    .iter()
                    .any(|e| e.contains("普通层没有直接左键")));
            }
            let sidearm_synapse_parser::raw_source::RawSource::V4 { profiles, .. } =
                result.parsed.raw_source.as_deref().unwrap()
            else {
                panic!()
            };
            assert_eq!(profiles[0].decoded.as_ref().unwrap().value, source);
        }
    }
    let mut source = payload();
    source["mappings"] = json!([
        mapping("RightClick", false, "disableGroup", None),
        mapping("RightClick", false, "disableGroup", None)
    ]);
    assert!(prepare(&v4(0xb6, source).0).unwrap().candidates[0].issues[0].contains("重复"));
}

#[test]
fn dav3pro_v4_device_feature_context_is_not_inferred_from_profile_defaults() {
    // The names/values below occur in the retained renderer's device storage
    // and reducers. Their placement in these authored exports is deliberately
    // unproved: neither a complete value nor a default-looking value grants
    // profile ownership of a separate device-wide setting.
    let smart = json!({"isAsymmetric":true,"trackingDistance":2,
        "liftOffDistance":13,"landingDistance":12});
    for product in [0xb6, 0xb7] {
        for outer_feature in [false, true] {
            for (key, value) in [
                ("powerSavingValue", json!(5)),
                ("lowPowerMode", json!(30)),
                ("calibration", json!({"smartTracking":smart.clone()})),
                ("smartTracking", smart.clone()),
            ] {
                let mut source = payload();
                if !outer_feature {
                    source[key] = value.clone();
                }
                let file = v4(product, source.clone());
                let mut outer: Value =
                    serde_json::from_slice(&std::fs::read(&file.0).unwrap()).unwrap();
                if outer_feature {
                    outer[key] = value;
                    std::fs::write(&file.0, serde_json::to_vec(&outer).unwrap()).unwrap();
                }
                let original = std::fs::read(&file.0).unwrap();
                let prepared = prepare(&file.0).unwrap();
                let candidate = &prepared.candidates[0];
                assert!(candidate.profile.is_none());
                let expected = if outer_feature {
                    format!("/{key}")
                } else {
                    format!("/profiles/0/payload/{key}")
                };
                assert!(candidate.issues.iter().any(|issue| {
                    issue.contains(&expected) && issue.contains("配置导出位置/覆盖规则尚未证实")
                }), "{:?}", candidate.issues);
                let sidearm_synapse_parser::raw_source::RawSource::V4 {
                    outer: retained_outer,
                    profiles,
                    ..
                } = prepared.parsed.raw_source.as_deref().unwrap()
                else {
                    panic!()
                };
                assert_eq!(retained_outer.value, outer);
                assert_eq!(profiles[0].decoded.as_ref().unwrap().value, source);
                assert_eq!(prepared.parsed.profiles[0].bindings.len(), 16);
                assert_eq!(std::fs::read(&file.0).unwrap(), original);
                // Refusal does not leak the calibration cache into preview.
                assert!(!serde_json::to_string(&prepared)
                    .unwrap()
                    .contains("liftOffDistance"));
            }
        }
    }
}

#[test]
fn dav3pro_v4_authored_lighting_and_calibration_blocks_are_retained_and_refused() {
    // These deliberately unsupported field names are authored negative data,
    // not a claim that this is an established OEM export feature schema.
    for product in [0xb6, 0xb7] {
        for (nested, key) in [
            (false, "lighting"),
            (false, "calibration"),
            (true, "lighting"),
            (true, "calibration"),
        ] {
            let mut source = payload();
            if nested {
                source["mappings"][0]["keyboardGroup"][key] = json!({"unknownSourceOption":true});
            } else {
                source[key] = if key == "lighting" {
                    json!({"effect":"static","color":[1,2,3],"brightness":50,"speed":3})
                } else {
                    json!({"mode":"asymmetric","lift":13,"landing":12})
                };
            }
            let file = v4(product, source.clone());
            let original = std::fs::read(&file.0).unwrap();
            let result = prepare(&file.0).unwrap();
            let expected = if nested {
                format!("/keyboardGroup/{key}")
            } else {
                format!("/payload/{key}")
            };
            assert!(result.candidates[0].profile.is_none());
            assert!(
                result.candidates[0]
                    .issues
                    .iter()
                    .any(|e| e.contains(&expected)),
                "{:?}",
                result.candidates[0].issues
            );
            let sidearm_synapse_parser::raw_source::RawSource::V4 { profiles, .. } =
                result.parsed.raw_source.as_deref().unwrap()
            else {
                panic!()
            };
            assert_eq!(profiles[0].decoded.as_ref().unwrap().value, source);
            assert_eq!(std::fs::read(&file.0).unwrap(), original);
        }
    }
}

#[test]
fn dav3pro_v4_macros_and_unknown_json_are_retained_without_frontend_raw_leak() {
    let source = payload();
    let outer = json!({"productId":0xb7,"category":"MOUSE","deviceName":"DeathAdder V3 Pro",
        "profiles":[{"name":"macro source","payload":base64(&serde_json::to_vec(&source).unwrap()),"hash":source_hash::profile_hash(&source).unwrap()}],
        "macros":[{"name":"retained OEM macro","payload":base64(br#"{"guid":"m","macroEvents":[{"Type":99,"vendorEvent":{"bytes":[1,2,3]}}]}"#),"hash":"unverified"}]});
    let file = ExportFile::new("synapse4", &serde_json::to_vec(&outer).unwrap());
    let result = prepare(&file.0).unwrap();
    assert!(result.candidates[0].profile.is_none());
    assert!(result.candidates[0]
        .issues
        .iter()
        .any(|e| e.contains("/macros")));
    let sidearm_synapse_parser::raw_source::RawSource::V4 { macros, .. } =
        result.parsed.raw_source.as_deref().unwrap()
    else {
        panic!()
    };
    assert_eq!(
        macros[0].decoded.as_ref().unwrap().value["macroEvents"][0]["vendorEvent"]["bytes"],
        json!([1, 2, 3])
    );
    let wire = serde_json::to_string(&result).unwrap();
    assert!(!wire.contains("vendorEvent") && !wire.contains("raw_source"));
}

#[test]
fn dav3pro_v4_checksum_duplicate_fields_and_missing_transport_rate_cannot_bypass_source_validation()
{
    let empty = ExportFile::new(
        "synapse4",
        br#"{"productId":182,"category":"MOUSE","profiles":[],"macros":[]}"#,
    );
    let result = prepare(&empty.0).unwrap();
    assert!(result.candidates.is_empty());
    assert!(result.conversion_error.unwrap().contains("/profiles"));
    let mut source = payload();
    source
        .as_object_mut()
        .unwrap()
        .remove("pollingRateWireless");
    assert!(
        prepare(&v4(0xb7, source).0).unwrap().candidates[0].issues[0]
            .contains("pollingRateWireless")
    );
    let source = payload();
    let mut outer = json!({"productId":0xb6,"category":"MOUSE","profiles":[{"name":"bad hash","payload":base64(&serde_json::to_vec(&source).unwrap()),"hash":"00000000000000000000000000000000"}],"macros":[]});
    let file = ExportFile::new("synapse4", &serde_json::to_vec(&outer).unwrap());
    assert!(prepare(&file.0).unwrap().candidates[0].issues[0].contains("hash"));
    let duplicate =
        serde_json::to_string(&source)
            .unwrap()
            .replacen('{', "{\"shadow\":1,\"shadow\":2,", 1);
    outer["profiles"][0]["payload"] = json!(base64(duplicate.as_bytes()));
    let file = ExportFile::new("synapse4", &serde_json::to_vec(&outer).unwrap());
    let original = std::fs::read(&file.0).unwrap();
    let result = prepare(&file.0).unwrap();
    assert!(result.candidates[0].profile.is_none());
    assert!(result.candidates[0]
        .issues
        .iter()
        .any(|e| e.contains("duplicate JSON key")));
    assert_eq!(std::fs::read(&file.0).unwrap(), original);
}

// Minimal stored ZIP writer for synthetic v3 source tests, with correct CRCs
// and central-directory offsets. Avoids a new application dependency.
fn stored_zip(entries: &[(&str, &str)]) -> Vec<u8> {
    fn u16le(out: &mut Vec<u8>, value: u16) {
        out.extend(value.to_le_bytes());
    }
    fn u32le(out: &mut Vec<u8>, value: u32) {
        out.extend(value.to_le_bytes());
    }
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        !crc
    }
    let mut out = Vec::new();
    let mut directory = Vec::new();
    for (name, body) in entries {
        let offset = out.len() as u32;
        let size = body.len() as u32;
        let checksum = crc32(body.as_bytes());
        u32le(&mut out, 0x04034b50);
        for value in [20, 0, 0, 0, 0] {
            u16le(&mut out, value);
        }
        for value in [checksum, size, size] {
            u32le(&mut out, value);
        }
        u16le(&mut out, name.len() as u16);
        u16le(&mut out, 0);
        out.extend(name.as_bytes());
        out.extend(body.as_bytes());
        u32le(&mut directory, 0x02014b50);
        for value in [20, 20, 0, 0, 0, 0] {
            u16le(&mut directory, value);
        }
        for value in [checksum, size, size] {
            u32le(&mut directory, value);
        }
        for value in [name.len() as u16, 0, 0, 0, 0] {
            u16le(&mut directory, value);
        }
        u32le(&mut directory, 0);
        u32le(&mut directory, offset);
        directory.extend(name.as_bytes());
    }
    let offset = out.len() as u32;
    let size = directory.len() as u32;
    out.extend(directory);
    u32le(&mut out, 0x06054b50);
    for value in [0, 0, entries.len() as u16, entries.len() as u16] {
        u16le(&mut out, value);
    }
    u32le(&mut out, size);
    u32le(&mut out, offset);
    u16le(&mut out, 0);
    out
}

#[test]
fn dav3pro_v3_zip_parser_entry_exposes_missing_identity_features_and_action_blocks() {
    let mappings = "<Mappings><MappingList>\
        <Mapping><InputType>DKMInput</InputType><DKMInput>DKM_M_03</DKMInput><MouseGroup><MouseAssignment>Previous</MouseAssignment></MouseGroup></Mapping>\
        <Mapping><InputType>DKMInput</InputType><DKMInput>DKM_SB_03</DKMInput><SensitivityGroup><SensitivityAssignment>DPI_CycleUp</SensitivityAssignment></SensitivityGroup></Mapping>\
        <Mapping><InputType>MouseInput</InputType><MouseInput>RightClick</MouseInput><SensitivityGroup><SensitivityAssignment>DPI_CycleUp</SensitivityAssignment></SensitivityGroup></Mapping>\
        <Mapping><InputType>MouseInput</InputType><MouseInput>Button5</MouseInput><UnknownOEMGroup><Option>1</Option></UnknownOEMGroup></Mapping>\
        </MappingList></Mappings>";
    let bytes = stored_zip(&[
        // Deliberately opaque identity: the parser never opens this entry, so
        // neither these bytes nor a guessed DeviceInfo schema can establish it.
        ("DeviceInfo.xml", "<DeviceInfo><OpaqueIdentity>1532:00B6</OpaqueIdentity></DeviceInfo>"),
        ("Profiles/p.xml", "<Profile><ProfileId>p</ProfileId><Name>Synthetic DA ZIP</Name></Profile>"),
        ("Features/p/mappings.xml", mappings),
        ("Features/p/dpi.xml", "<DPI><X>1600</X><Y>2400</Y><Independent>true</Independent></DPI>"),
        ("Features/p/stages.xml", "<DPIStages><Stages><DPIStage><X>400</X><Y>800</Y><Active>true</Active></DPIStage><DPIStage><X>1600</X><Y>2400</Y><Active>true</Active></DPIStage></Stages><Active>1</Active></DPIStages>"),
        ("Features/p/polling.xml", "<PollingRate><Value>1000</Value></PollingRate>"),
        ("Features/p/power.xml", "<PowerManagement><SleepModeInSeconds>300</SleepModeInSeconds><BlinkLedPercentage>5</BlinkLedPercentage></PowerManagement>"),
    ]);
    let file = ExportFile::new("synapse3", &bytes);
    let prepared = prepare(&file.0).unwrap();
    assert!(matches!(
        prepared.parsed.source_kind,
        sidearm_synapse_parser::types::SourceKind::SynapseV3Zip
    ));
    assert!(prepared.parsed.device.is_none());
    let profile = &prepared.parsed.profiles[0];
    assert!(profile.performance.is_none());
    assert_eq!(profile.bindings[0].control_id, "thumb_03");
    assert!(prepared
        .parsed
        .warnings
        .iter()
        .any(|w| w.code == "v3_unmappable_dkm" && w.message.contains("DKM_SB_03")));
    // Unknown output blocks are currently collapsed to Disabled upstream.
    // The application must reject the source rather than execute that value.
    for input in ["RightClick", "Button5"] {
        let binding = profile
            .bindings
            .iter()
            .find(|b| b.source_input_id == input)
            .unwrap();
        assert!(matches!(binding.action, ParsedAction::Disabled));
        assert!(binding.raw_mapping.is_none());
    }
    let sidearm_synapse_parser::raw_source::RawSource::V3 { entries } =
        prepared.parsed.raw_source.as_deref().unwrap()
    else {
        panic!()
    };
    let original = entries
        .iter()
        .find(|e| e.path == "Features/p/mappings.xml")
        .unwrap();
    assert_eq!(original.bytes, mappings.as_bytes());
    assert!(entries.iter().any(|e| e.path == "Features/p/stages.xml"));
    assert!(!serde_json::to_string(&prepared.parsed)
        .unwrap()
        .contains("UnknownOEMGroup"));
    assert!(prepared.candidates.is_empty());
    let reason = prepared.conversion_error.unwrap();
    assert!(
        reason.contains("DeviceInfo")
            && reason.contains("DPI/回报率")
            && reason.contains("完整原厂动作块")
    );
}

#[test]
fn dav3pro_name_cannot_bypass_brand_model_or_transport_identity_gate() {
    let file = v4(0x00b6, payload());
    let mut parsed = sidearm_synapse_parser::preview(&file.0).unwrap();
    for (vendor, product) in [(0x3837, 0x00b6), (0x1532, 0x00b8), (0x046d, 0x00b7)] {
        parsed.device.as_mut().unwrap().vendor = vendor;
        parsed.device.as_mut().unwrap().product = product;
        assert!(convert(&parsed).err().unwrap().contains("仅适配 Naga"));
    }
}

fn horizontal_payload()->Value{
    let mut source=payload();for row in source["mappings"].as_array_mut().unwrap(){let second=row["isHyperShift"]==true;let output=match row["inputID"].as_str().unwrap(){"Button4"=>Some(if second{"ScrollRight"}else{"ScrollLeft"}),"Button5"=>Some(if second{"ScrollLeft"}else{"ScrollRight"}),_=>None};if let Some(output)=output{*row=mapping(row["inputID"].as_str().unwrap(),second,"mouseGroup",Some(json!({"mouseAssignment":output})));}}source
}
#[test]
fn horizontal_synapse_outputs_preserve_both_layers_through_native_import_and_store(){
    let mut cases=vec![];
    for pid in[0xb6,0xb7]{let source=horizontal_payload();let file=v4(pid,source.clone());let bytes=std::fs::read(&file.0).unwrap();let prepared=prepare(&file.0).unwrap();let profile=prepared.candidates[0].profile.as_ref().unwrap();let plan=profile.multi.as_ref().unwrap();crate::profile_transfer::validate(profile).unwrap();crate::multibrand::validate(plan).unwrap();let(mut d,_)=gearwise_razer::simulated_dav3pro(pid).unwrap();for s in &plan.settings{d.apply(s).unwrap();}let state=d.read_state().unwrap();for(id,code)in[(4,0x68),(5,0x69),(260,0x69),(261,0x68)]{assert_eq!(gearwise_razer::dav3pro_buttons::encode(id,&state.buttons[&id]).unwrap(),[1,1,code,0,0,0,0]);}assert_eq!(std::fs::read(&file.0).unwrap(),bytes);assert!(plan.source.is_none()&&plan.macros.is_empty());cases.push(prepared);}
    if let Some(path)=std::env::var_os("OPEN_HUB_DAV3PRO_HORIZONTAL_PREPARED"){std::fs::write(path,serde_json::to_vec_pretty(&cases).unwrap()).unwrap();}
}
#[test]
#[ignore="requires horizontal import/editor UI saves"]
fn browser_horizontal_dav3pro_source_and_editor_saves_execute_each_exact_layer(){
    let profiles:Vec<Profile>=serde_json::from_slice(&std::fs::read(std::env::var_os("OPEN_HUB_DAV3PRO_HORIZONTAL_UI_PROFILES").unwrap()).unwrap()).unwrap();assert_eq!(profiles.len(),4);
    for(index,p)in profiles.into_iter().enumerate(){crate::profile_transfer::validate(&p).unwrap();let plan=p.multi.unwrap();crate::multibrand::validate(&plan).unwrap();let(mut d,_)=gearwise_razer::simulated_dav3pro(plan.product).unwrap();for s in &plan.settings{d.apply(s).unwrap();}let state=d.read_state().unwrap();for(id,code)in[(4,if index%2==0{0x68}else{0x69}),(5,0x69),(260,0x69),(261,0x68)]{assert_eq!(gearwise_razer::dav3pro_buttons::encode(id,&state.buttons[&id]).unwrap(),[1,1,code,0,0,0,0]);}assert!(plan.source.is_none()&&plan.macros.is_empty());}
}
