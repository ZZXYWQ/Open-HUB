//! Synapse data -> model-scoped executable Open HUB profiles. No HID access.
use crate::{
    multibrand::Plan,
    profiles::{Level, Profile},
};
use gearwise_core::{
    Setting,
    model::{ButtonAction, DpiStage, DpiState, Modifiers, MouseButton},
    profile::{Macro, MacroMode, MacroStep},
};
use serde::Serialize;
use sidearm_synapse_parser::types::{ParsedAction, ParsedProfile, ParsedSynapseProfiles};
#[path = "synapse_dav3pro.rs"]
mod dav3pro;
#[path = "synapse_source_hash.rs"]
mod source_hash;
#[derive(Serialize)]
pub struct Candidate {
    pub guid: String,
    pub name: String,
    pub profile: Option<Profile>,
    pub issues: Vec<String>,
    pub notes: Vec<String>,
}
#[derive(Serialize)]
pub struct Prepared {
    pub parsed: ParsedSynapseProfiles,
    pub candidates: Vec<Candidate>,
    pub conversion_error: Option<String>,
}
pub fn prepare(path: &std::path::Path) -> Result<Prepared, String> {
    let parsed = match dav3pro::preview_exact_v4(path)? {
        Some(parsed) => parsed,
        None => sidearm_synapse_parser::preview(path)?,
    };
    let (candidates, conversion_error) = match convert(&parsed) {
        Ok(c) => (c, None),
        Err(e) => (vec![], Some(e)),
    };
    Ok(Prepared {
        parsed,
        candidates,
        conversion_error,
    })
}
pub fn convert(parsed: &ParsedSynapseProfiles) -> Result<Vec<Candidate>, String> {
    let device = parsed
        .device
        .as_ref()
        .ok_or_else(|| match parsed.source_kind {
            sidearm_synapse_parser::types::SourceKind::SynapseV3Zip =>
                "Synapse 3 DeviceInfo 精确型号导出结构尚未核实；DPI/回报率及完整原厂动作块已内部保留，确认身份前不能保存设备方案或套用 Naga 按键布局",
            _ => "导出文件未提供可验证的鼠标型号，不能自动套用 Naga 按键布局",
        })?;
    if device.vendor == 0x1532 && matches!(device.product, 0x00b6 | 0x00b7) {
        return dav3pro::convert(parsed,device.product);
    }
    if (device.vendor, device.product) != (0x1532, 0x00b4) {
        return Err("此 Synapse 按键布局目前仅适配 Naga V2 HyperSpeed (1532:00B4)".into());
    }
    let dropped = parsed
        .warnings
        .iter()
        .filter(|w| {
            !matches!(
                w.code.as_str(),
                "macro_hold_flattened" | "macro_delay_clamped" | "macro_mouse_event_skipped"
            )
        })
        .map(|w| format!("源文件转换警告 {}：{}", w.code, w.message))
        .collect::<Vec<_>>();
    Ok(parsed
        .profiles
        .iter()
        .map(|p| candidate(p, &dropped))
        .collect())
}
fn button(control: &str) -> Result<u16, String> {
    Ok(match control {
        "mouse_left" => 1,
        "mouse_right" => 2,
        "wheel_click" => 3,
        "mouse_4" => 0x34,
        "mouse_5" => 0x35,
        "top_aux_01" => 0x0b,
        "top_aux_02" => 0x0c,
        _ => {
            let n = control
                .strip_prefix("thumb_")
                .and_then(|n| n.parse::<u16>().ok())
                .filter(|n| (1..=12).contains(n))
                .ok_or_else(|| format!("该物理输入尚未接通：{control}"))?;
            0x40 + n - 1
        }
    })
}
fn mouse(action: &str) -> Result<ButtonAction, String> {
    let a = action
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    let b = match a.as_str() {
        "left" | "leftclick" | "mouseleft" => MouseButton::Left,
        "right" | "rightclick" | "mouseright" => MouseButton::Right,
        "middle" | "middleclick" | "wheelclick" | "mousemiddle" => MouseButton::Middle,
        "back" | "backward" | "xbutton1" | "mouseback" => MouseButton::Back,
        "forward" | "xbutton2" | "mouseforward" => MouseButton::Forward,
        _ => return Err(format!("尚未接通的鼠标动作：{action}")),
    };
    Ok(ButtonAction::Mouse(b))
}
fn candidate(source: &ParsedProfile, source_issues: &[String]) -> Candidate {
    let mut issues = source_issues.to_vec();
    let mut notes = Vec::new();
    let mut settings = Vec::new();
    let mut macros = Vec::new();
    if let Some(performance) = &source.performance {
        let dpi = if performance.stages_enabled {
            if performance.stages.len() != 1 {
                issues.push(
                    "来源启用了多档 DPI；当前 Naga 驱动尚未接通原生档位表，不能静默压成一档".into(),
                );
                None
            } else {
                performance.active.and_then(|i| performance.stages.get(i))
            }
        } else {
            performance.dpi.as_ref()
        };
        if let Some(d) = dpi {
            settings.push(Setting::Dpi(DpiState {
                stages: vec![DpiStage { x: d.x, y: d.y }],
                active: 0,
            }));
        } else {
            issues.push("缺少可执行的当前 DPI 值".into());
        }
        match performance
            .polling_rate_wireless
            .or(performance.polling_rate)
            .and_then(|r| u16::try_from(r).ok())
        {
            Some(r) => settings.push(Setting::ReportRate(r)),
            None => issues.push("缺少有效回报率".into()),
        }
    } else {
        issues.push("此来源尚未解析 DPI 与回报率，不能保存为完整设备方案".into());
    }
    for binding in &source.bindings {
        if binding.layer != "standard" {
            issues.push(format!(
                "{}：尚未接通 {} 层行为",
                binding.control_id, binding.layer
            ));
            continue;
        }
        let result = (|| -> Result<(), String> {
            let id = button(&binding.control_id)?;
            let action = match &binding.action {
                ParsedAction::ModelAction { .. } => return Err("精确型号的原生动作预览不能通过 Naga 转换器执行".into()),
                ParsedAction::Shortcut {
                    key,
                    ctrl,
                    shift,
                    alt,
                    win,
                } => ButtonAction::Key {
                    usage: crate::import_keys::usage(key)?,
                    mods: Modifiers(modifiers(
                        binding,
                        u8::from(*ctrl)
                            | (u8::from(*shift) << 1)
                            | (u8::from(*alt) << 2)
                            | (u8::from(*win) << 3),
                    )?),
                },
                ParsedAction::MouseAction { action } => mouse(action)?,
                ParsedAction::Sequence { macro_guid } => {
                    let options = binding
                        .raw_mapping
                        .as_ref()
                        .and_then(|v| v.get("macroGroup"))
                        .and_then(|v| v.as_object())
                        .ok_or("缺少原始宏播放参数，不能猜测播放方式")?;
                    if options.keys().any(|k| k != "guid") {
                        return Err("来源宏含额外播放选项，目前尚不能自动转换".into());
                    }
                    let body = source
                        .macros
                        .iter()
                        .find(|m| m.synapse_guid == *macro_guid)
                        .ok_or("宏引用没有对应的原始事件")?;
                    let m = crate::import_keys::macro_body(body, MacroMode::Once)?;
                    let macro_id = m.id.clone();
                    if !macros.iter().any(|existing: &Macro| existing.id == m.id) {
                        macros.push(m);
                    }
                    ButtonAction::Macro(macro_id)
                }
                ParsedAction::TextSnippet { text } => {
                    let m = Macro {
                        id: format!(
                            "synapse-text:{}:{}",
                            source.synapse_guid, binding.control_id
                        ),
                        name: format!("{} · 文本", binding.control_id),
                        mode: MacroMode::Once,
                        steps: vec![MacroStep::Text(text.clone())],
                    };
                    crate::host_macros::validate(&m)?;
                    let macro_id = m.id.clone();
                    macros.push(m);
                    ButtonAction::Macro(macro_id)
                }
                ParsedAction::Disabled => return Err("此 Naga 驱动尚未接通禁用动作".into()),
                ParsedAction::Unmappable { reason } => return Err(reason.clone()),
            };
            if id == 1 {
                if action == ButtonAction::Mouse(MouseButton::Left) {
                    return Ok(());
                }
                return Err("保留左键原始点击，不导入覆盖左键的动作".into());
            }
            settings.push(Setting::Button { id, action });
            Ok(())
        })();
        if let Err(error) = result {
            issues.push(format!("{}：{error}", binding.control_id));
        }
    }
    if !macros.is_empty() {
        notes.push("当前仅接受未带额外播放参数的单次宏；带循环/按住等来源选项的宏会阻止导入。保存前可在编辑器检查播放方式。".into());
    }
    notes.push("导入不会自动写入鼠标；软件宏需先在设备方案中选择对应鼠标输入接口。".into());
    let plan = Plan {
        vendor: 0x1532,
        product: 0x00b4,
        settings,
        macros,
        source: None,
    };
    if let Err(e) = crate::multibrand::validate(&plan) {
        issues.push(e);
    }
    let profile = if issues.is_empty() {
        let dpi = plan
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
        let rate = plan.settings.iter().find_map(|s| {
            if let Setting::ReportRate(r) = s {
                Some(u32::from(*r))
            } else {
                None
            }
        });
        let p = Profile {
            target_vendor: Some(plan.vendor),
            target_model: Some(plan.product),
            mchose_board: None,
            sino: None,
            id: format!("synapse-{}", crate::profiles::new_id()),
            name: source.name.clone(),
            app_path: None,
            levels: dpi
                .stages
                .iter()
                .map(|d| Level {
                    x: d.x,
                    y: Some(d.y),
                })
                .collect(),
            active_level: dpi.active as usize,
            report_rate_hz: rate,
            ghub_id: None,
            multi: Some(plan),
        };
        match crate::profile_transfer::validate(&p) {
            Ok(()) => Some(p),
            Err(e) => {
                issues.push(e);
                None
            }
        }
    } else {
        None
    };
    Candidate {
        guid: source.synapse_guid.clone(),
        name: source.name.clone(),
        profile,
        issues,
        notes,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> ParsedSynapseProfiles {
        serde_json::from_value(serde_json::json!({"sourceKind":"synapseV4","sourcePath":"simulation:import","device":{"vendor":0x1532,"product":0x00b4,"name":"Naga V2 HyperSpeed"},"profiles":[{"synapseGuid":"p","name":"Imported profile","performance":{"dpi":{"x":1600,"y":2400},"stages":[],"active":null,"stagesEnabled":false,"pollingRate":1000,"pollingRateWireless":500},"bindings":[{"controlId":"thumb_01","layer":"standard","sourceInputId":"KEY_1","label":"Ctrl+C","action":{"kind":"shortcut","key":"C","ctrl":true}},{"controlId":"thumb_02","layer":"standard","sourceInputId":"KEY_2","rawMapping":{"macroGroup":{"guid":"m"}},"label":"macro","action":{"kind":"sequence","macro_guid":"m"}}],"macros":[{"synapseGuid":"m","name":"A then B held","steps":[],"events":[{"kind":"key","makecode":30,"is_extended":false,"is_down":true},{"kind":"key","makecode":48,"is_extended":false,"is_down":true},{"kind":"delay","delay_ms":25},{"kind":"key","makecode":30,"is_extended":false,"is_down":false},{"kind":"key","makecode":48,"is_extended":false,"is_down":false}]}]}],"warnings":[]})).unwrap()
    }
    #[test]
    fn imported_dpi_shortcut_and_macro_save_and_execute_real_driver_simulation() {
        let candidates = convert(&source()).unwrap();
        let p = candidates[0].profile.as_ref().unwrap();
        assert_eq!(
            p.levels,
            vec![Level {
                x: 1600,
                y: Some(2400)
            }]
        );
        assert_eq!(p.report_rate_hz, Some(500));
        let mut store = crate::profiles::ProfileStore::default();
        crate::profile_transfer::merge(&mut store, vec![p.clone()]).unwrap();
        let roundtrip: Profile = serde_json::from_slice(&serde_json::to_vec(p).unwrap()).unwrap();
        let report = crate::multibrand::simulate(roundtrip.multi.as_ref().unwrap()).unwrap();
        assert_eq!(
            report["software_macros"][0]["events"],
            serde_json::json!(["key_down:4", "key_down:5", "key_up:4", "key_up:5"])
        );
        assert!(!p.targets(0x046d, &[0x00b4]));
        assert!(!p.targets(0x3837, &[0x00b4]));
    }
    #[test]
    fn original_right_modifier_is_not_folded_into_left_modifier() {
        let mut s = source();
        s.profiles[0].bindings[0].raw_mapping =
            Some(serde_json::json!({"keyboardGroup":{"modifiers":["KEY_RIGHT_ALT"]}}));
        let p = convert(&s).unwrap().remove(0).profile.unwrap();
        assert!(p.multi.unwrap().settings.iter().any(|s| matches!(
            s,
            Setting::Button {
                id: 0x40,
                action: ButtonAction::Key {
                    usage: 6,
                    mods: Modifiers(64)
                }
            }
        )));
    }
    #[test]
    fn unimplemented_source_macro_playback_options_block_incomplete_import() {
        let mut s = source();
        s.profiles[0].bindings[1].raw_mapping =
            Some(serde_json::json!({"macroGroup":{"guid":"m","playbackOption":"whilePressed"}}));
        let result = convert(&s).unwrap();
        assert!(result[0].profile.is_none());
        assert!(result[0].issues.iter().any(|e| e.contains("播放选项")));
    }
    #[test]
    fn foreign_model_and_lossy_macro_are_not_accepted() {
        let mut s = source();
        s.device.as_mut().unwrap().product = 0x0084;
        assert!(convert(&s).is_err());
        s = source();
        s.profiles[0].macros[0].events = None;
        assert!(convert(&s).unwrap()[0].profile.is_none());
    }
    #[test]
    fn hypershift_and_multistage_dpi_cannot_be_silently_dropped() {
        let mut s = source();
        s.profiles[0].bindings[0].layer = "hypershift".into();
        let c = convert(&s).unwrap();
        assert!(c[0].profile.is_none() && c[0].issues.iter().any(|e| e.contains("hypershift")));
        s = source();
        let performance = s.profiles[0].performance.as_mut().unwrap();
        performance.stages_enabled = true;
        performance.stages = vec![
            sidearm_synapse_parser::types::SourceDpi { x: 800, y: 800 },
            sidearm_synapse_parser::types::SourceDpi { x: 1600, y: 1600 },
        ];
        assert!(convert(&s).unwrap()[0].profile.is_none());
    }
}
fn modifiers(
    binding: &sidearm_synapse_parser::types::ParsedBinding,
    fallback: u8,
) -> Result<u8, String> {
    let Some(values) = binding
        .raw_mapping
        .as_ref()
        .and_then(|v| v.get("keyboardGroup"))
        .and_then(|v| v.get("modifiers"))
        .and_then(|v| v.as_array())
    else {
        return Ok(fallback);
    };
    let mut mask = 0;
    for value in values {
        mask |= match value.as_str() {
            Some("KEY_LEFT_CTRL") => 1,
            Some("KEY_LEFT_SHIFT") => 2,
            Some("KEY_LEFT_ALT") => 4,
            Some("KEY_LEFT_WINDOWS") | Some("KEY_LEFT_WIN") => 8,
            Some("KEY_RIGHT_CTRL") => 16,
            Some("KEY_RIGHT_SHIFT") => 32,
            Some("KEY_RIGHT_ALT") => 64,
            Some("KEY_RIGHT_WINDOWS") | Some("KEY_RIGHT_WIN") => 128,
            _ => return Err("来源包含无法精确保留的修饰键".into()),
        };
    }
    Ok(mask)
}

#[cfg(test)]
#[path = "synapse_dav3pro_import_tests.rs"]
mod dav3pro_import_tests;
