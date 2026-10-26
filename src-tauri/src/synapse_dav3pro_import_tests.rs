//! Parser-entry regressions for the still-disabled exact-model importer.
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
            "payload": base64(&serde_json::to_vec(&payload).unwrap())}], "macros": []});
    ExportFile::new("synapse4", &serde_json::to_vec(&raw).unwrap())
}
fn payload() -> Value {
    json!({"guid": "synthetic-da", "name": "Synthetic DA import",
        "dpi": {"x": 1600, "y": 2400},
        "dpiStages": {"enable": true, "active": 2, "stages": [
            {"x": 400,"y": 800}, {"x": 800,"y": 1200}, {"x": 1600,"y": 2400},
            {"x": 3200,"y": 4800}, {"x": 15000,"y": 30000}]},
        "pollingRate": 1000, "pollingRateWireless": 500,
        "mappings": [{"inputType": "MouseInput", "inputID": "RightClick",
            "outputType": "keyboardGroup", "keyboardGroup": {"key": "KEY_C", "modifiers": ["KEY_RIGHT_ALT"]}}]})
}

#[test]
fn dav3pro_exact_transports_are_refused_after_real_v4_file_parsing() {
    for product in [0x00b6, 0x00b7] {
        let file = v4(product, payload());
        let prepared = prepare(&file.0).unwrap();
        assert_eq!(prepared.parsed.device.as_ref().unwrap().product, product);
        let performance = prepared.parsed.profiles[0].performance.as_ref().unwrap();
        assert_eq!(performance.stages.len(), 5);
        assert_eq!(performance.active, Some(2));
        assert_eq!(performance.stages[4].y, 30000);
        assert!(prepared.candidates.is_empty());
        let reason = prepared.conversion_error.as_deref().unwrap();
        assert!(reason.contains("DeathAdder V3 Pro") && reason.contains("不能无损表达"));
        // There is deliberately no executable Profile to persist or simulate.
    }
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
        let binding = &prepared.parsed.profiles[0].bindings[0];
        // This proves why merely enabling product IDs on the old converter is
        // unsafe: the upstream DTO has already discarded the extra option.
        assert!(binding.raw_mapping.as_ref().unwrap()["keyboardGroup"]
            .get("repeatWhilePressed")
            .is_none());
        assert!(prepared.candidates.is_empty() && prepared.conversion_error.is_some());
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
    assert_eq!(
        prepared.parsed.profiles[0].bindings[0].control_id,
        "thumb_03"
    );
    assert_eq!(
        prepared.parsed.profiles[0].bindings[1].control_id,
        "thumb_04"
    );
    assert!(prepared
        .parsed
        .warnings
        .iter()
        .any(|w| w.code == "unmappable_input_id" && w.message.contains("DKM_SB_03")));
    assert!(prepared.candidates.is_empty() && prepared.conversion_error.is_some());
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
