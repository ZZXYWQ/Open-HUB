//! Exact DA source conversion, independent of the legacy Naga dictionary.
//! Official product182 JSON/XML bridge facts and offsets are recorded in the
//! import static-facts document. Authored tests do not establish real hardware.
use super::{source_hash, Candidate, Level, Plan, Profile};
use gearwise_core::{
    model::{
        ButtonAction, DpiFunction, DpiStage, DpiState, Modifiers, MouseButton, ScrollDirection,
    },
    Setting,
};
use serde_json::{Map, Value};
use sidearm_synapse_parser::{
    raw_source::{self, RawEnvelope, RawSource},
    types::{ParsedProfile, ParsedSynapseProfiles, SourceDevice, SourceKind},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

fn preview_bindings(value: &Value) -> Vec<sidearm_synapse_parser::types::ParsedBinding> {
    use sidearm_synapse_parser::types::{ParsedAction, ParsedBinding};
    value
        .get("mappings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(32)
        .map(|row| {
            let input = row
                .get("inputID")
                .and_then(Value::as_str)
                .unwrap_or("未知输入");
            let kind = row.get("inputType").and_then(Value::as_str).unwrap_or("");
            let layer = row
                .get("isHyperShift")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let converted = physical(kind, input).and_then(|(id, _)| {
                let id = id | if layer { 0x100 } else { 0 };
                let action = action(row, id, "/mappings")?;
                gearwise_razer::dav3pro_buttons::encode(id, &action).map_err(|e| e.to_string())?;
                Ok(action)
            });
            let (label, action) = match converted {
                Ok(action) => {
                    let label = match action {
                        ButtonAction::Mouse(button) => match button {
                            MouseButton::Left => "左键",
                            MouseButton::Right => "右键",
                            MouseButton::Middle => "中键",
                            MouseButton::Back => "后退",
                            MouseButton::Forward => "前进",
                            _ => "鼠标动作",
                        }
                        .to_string(),
                        ButtonAction::Scroll(direction) => if direction == ScrollDirection::Up {
                            "向上滚动"
                        } else {
                            "向下滚动"
                        }
                        .to_string(),
                        ButtonAction::Key { mods, .. } => {
                            let names = [
                                "左 Ctrl",
                                "左 Shift",
                                "左 Alt",
                                "左 Win",
                                "右 Ctrl",
                                "右 Shift",
                                "右 Alt",
                                "右 Win",
                            ];
                            let mut chord = names
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| mods.0 & (1 << i) != 0)
                                .map(|(_, name)| name.to_string())
                                .collect::<Vec<_>>();
                            chord.push(
                                row["keyboardGroup"]["key"]
                                    .as_str()
                                    .unwrap_or("键盘按键")
                                    .trim_start_matches("KEY_")
                                    .replace('_', " "),
                            );
                            chord.join(" + ")
                        }
                        ButtonAction::Dpi(function) => match function {
                            DpiFunction::Up => "DPI 升档".into(),
                            DpiFunction::Down => "DPI 降档".into(),
                            DpiFunction::Cycle => "DPI 正向循环".into(),
                            DpiFunction::CycleDown => "DPI 反向循环".into(),
                            DpiFunction::Shift(value) => format!("按住切换 {value} DPI"),
                            DpiFunction::ShiftXY { x, y } => {
                                format!("按住切换 X={x} / Y={y} DPI")
                            }
                        },
                        ButtonAction::Media(usage) => match usage {
                            0xb5 => "下一曲",
                            0xb6 => "上一曲",
                            0xb7 => "停止播放",
                            0xcd => "播放/暂停",
                            0xe2 => "静音",
                            0xe9 => "提高音量",
                            0xea => "降低音量",
                            _ => "媒体动作",
                        }
                        .into(),
                        ButtonAction::Hypershift => "按住 Hypershift".into(),
                        ButtonAction::Disabled => "禁用".into(),
                        _ => "原生动作".into(),
                    };
                    (
                        label.clone(),
                        ParsedAction::ModelAction { description: label },
                    )
                }
                Err(reason) => ("未转换".into(), ParsedAction::Unmappable { reason }),
            };
            ParsedBinding {
                raw_mapping: None,
                control_id: input.to_string(),
                layer: if layer { "hypershift" } else { "standard" }.into(),
                source_input_id: input.to_string(),
                label,
                action,
            }
        })
        .collect()
}

/// Exact DA files bypass the lossy Naga DTO, including its string-only
/// modifiers mirror. The raw source is internal and is never serialized.
pub(super) fn preview_exact_v4(path: &Path) -> Result<Option<ParsedSynapseProfiles>, String> {
    let Ok(raw) = raw_source::read(path, SourceKind::SynapseV4) else {
        return Ok(None);
    };
    let RawSource::V4 {
        outer, profiles, ..
    } = &raw
    else {
        return Ok(None);
    };
    let Some(product) = outer
        .value
        .get("productId")
        .and_then(Value::as_u64)
        .filter(|v| matches!(v, 0xb6 | 0xb7))
    else {
        return Ok(None);
    };
    if outer.value.get("category").and_then(Value::as_str) != Some("MOUSE") {
        return Ok(None);
    }
    let profiles = profiles
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let value = e.decoded.as_ref().ok().map(|r| &r.value);
            ParsedProfile {
                performance: None,
                synapse_guid: value
                    .and_then(|v| v.get("guid"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                name: e
                    .envelope
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("来源配置 {}", i + 1)),
                bindings: value.map(preview_bindings).unwrap_or_default(),
                macros: vec![],
            }
        })
        .collect();
    Ok(Some(ParsedSynapseProfiles {
        raw_source: Some(Arc::new(raw)),
        raw_source_error: None,
        device: Some(SourceDevice {
            vendor: 0x1532,
            product: product as u16,
            name: "DeathAdder V3 Pro".into(),
        }),
        source_kind: SourceKind::SynapseV4,
        source_path: path.to_string_lossy().into_owned(),
        profiles,
        warnings: vec![],
    }))
}

fn object<'a>(
    value: &'a Value,
    path: &str,
    keys: &[&str],
) -> Result<&'a Map<String, Value>, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| format!("{path}: 必须是对象"))?;
    if let Some(key) = obj.keys().find(|k| !keys.contains(&k.as_str())) {
        return Err(format!(
            "{path}/{key}: 尚未支持的原厂字段；已保留原始来源，未转换任何部分"
        ));
    }
    Ok(obj)
}
fn field<'a>(obj: &'a Map<String, Value>, key: &str, path: &str) -> Result<&'a Value, String> {
    obj.get(key)
        .ok_or_else(|| format!("{path}/{key}: 缺少完整原厂值"))
}
fn text<'a>(obj: &'a Map<String, Value>, key: &str, path: &str) -> Result<&'a str, String> {
    field(obj, key, path)?
        .as_str()
        .ok_or_else(|| format!("{path}/{key}: 必须是字符串"))
}
fn number(obj: &Map<String, Value>, key: &str, path: &str) -> Result<u64, String> {
    field(obj, key, path)?
        .as_u64()
        .ok_or_else(|| format!("{path}/{key}: 必须是非负整数"))
}
fn boolean(obj: &Map<String, Value>, key: &str, path: &str) -> Result<bool, String> {
    field(obj, key, path)?
        .as_bool()
        .ok_or_else(|| format!("{path}/{key}: 必须是布尔值"))
}

// The retained renderer loads these from the device-level storage object,
// separately from its selected profile. DEFAULTPROFILE alone also contains
// placeholder power values, which is not proof that an export owns them.
// Keep a specific source-path diagnostic instead of treating those defaults
// as an instruction to overwrite the live device's independent settings.
fn reject_unproved_device_features(value: &Value, path: &str) -> Result<(), String> {
    for key in ["powerSavingValue", "lowPowerMode", "calibration", "smartTracking"] {
        if value.get(key).is_some() {
            return Err(format!(
                "{path}/{key}: 已识别原厂设备级功能，但配置导出位置/覆盖规则尚未证实；原始来源完整保留，未转换任何部分"
            ));
        }
    }
    Ok(())
}

pub(super) fn convert(
    parsed: &ParsedSynapseProfiles,
    product: u16,
) -> Result<Vec<Candidate>, String> {
    let raw = parsed.raw_source.as_deref().ok_or_else(|| {
        format!(
            "DeathAdder V3 Pro 需要完整原始来源，不能从有损预览 DTO 转换：{}",
            parsed
                .raw_source_error
                .as_deref()
                .unwrap_or("原始字段未保留")
        )
    })?;
    let RawSource::V4 {
        outer,
        profiles,
        macros,
    } = raw
    else {
        return Err("Synapse 3 DeviceInfo 导出身份字段尚未证实；原始 XML、性能及动作块已内部保留，暂不创建可执行方案".into());
    };
    if profiles.is_empty() {
        return Err("/profiles: 没有可转换配置".into());
    }
    let mut global = Vec::new();
    let check = (|| {
        reject_unproved_device_features(&outer.value, "")?;
        let obj = object(
            &outer.value,
            "",
            &[
                "productId",
                "vendorId",
                "deviceName",
                "category",
                "profiles",
                "macros",
            ],
        )?;
        if number(obj, "productId", "")? != u64::from(product)
            || text(obj, "category", "")? != "MOUSE"
        {
            return Err("/productId: 来源型号/类别与选择的精确连接不一致".into());
        }
        if obj
            .get("vendorId")
            .is_some_and(|v| v.as_u64() != Some(0x1532))
        {
            return Err("/vendorId: 来源不是 Razer 1532".into());
        }
        if !macros.is_empty() {
            return Err("/macros: DeathAdder 原厂宏输入桥接尚未启用；完整宏及其选项保留在来源中，本文件不会部分导入".into());
        }
        Ok::<_, String>(())
    })();
    if let Err(e) = check {
        global.push(e);
    }
    let mut seen = BTreeSet::new();
    for envelope in profiles {
        if let Ok(raw) = &envelope.decoded {
            if let Some(guid) = raw.value.get("guid").and_then(Value::as_str) {
                if !seen.insert(guid) {
                    global.push(format!("/profiles/guid: 重复来源标识 {guid}"));
                }
            }
        }
    }
    Ok(profiles
        .iter()
        .enumerate()
        .map(|(i, env)| candidate(env, product, i, &global))
        .collect())
}

fn candidate(env: &RawEnvelope, product: u16, index: usize, global: &[String]) -> Candidate {
    let mut issues = global.to_vec();
    let name = env
        .envelope
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("未命名配置")
        .to_string();
    let guid = env
        .decoded
        .as_ref()
        .ok()
        .and_then(|r| r.value.get("guid"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let result = (|| {
        let path = format!("/profiles/{index}");
        let envelope = object(&env.envelope, &path, &["name", "payload", "hash"])?;
        if text(envelope, "name", &path)?.is_empty() {
            return Err(format!("{path}/name: 配置名称为空"));
        }
        let raw = env
            .decoded
            .as_ref()
            .map_err(|e| format!("{path}/payload: {e}"))?;
        let hash = text(envelope, "hash", &path)?;
        if hash != source_hash::profile_hash(&raw.value)? {
            return Err(format!(
                "{path}/hash: 原厂排序 JSON 校验不符；不能确认完整来源"
            ));
        }
        reject_unproved_device_features(&raw.value, &format!("{path}/payload"))?;
        let obj = object(
            &raw.value,
            &format!("{path}/payload"),
            &[
                "guid",
                "name",
                "dpi",
                "dpiStages",
                "pollingRate",
                "pollingRateWireless",
                "mappings",
            ],
        )?;
        if text(obj, "guid", &path)?.is_empty() {
            return Err(format!("{path}/guid: 缺少原始标识"));
        }
        let dpi = performance(obj, &path)?;
        let selected = if product == 0x00b6 {
            "pollingRate"
        } else {
            "pollingRateWireless"
        };
        for key in ["pollingRate", "pollingRateWireless"] {
            if obj.contains_key(key) {
                let rate = number(obj, key, &path)?;
                if ![125, 500, 1000].contains(&rate) {
                    return Err(format!("{path}/{key}: 原配连接只支持 125/500/1000 Hz"));
                }
            }
        }
        let rate = number(obj, selected, &path)? as u16;
        let bindings = mappings(field(obj, "mappings", &path)?, &format!("{path}/mappings"))?;
        let mut settings = vec![Setting::Dpi(dpi), Setting::ReportRate(rate)];
        settings.extend(
            bindings
                .into_iter()
                .map(|(id, action)| Setting::Button { id, action }),
        );
        let plan = Plan {
            vendor: 0x1532,
            product,
            settings,
            macros: vec![],
            source: None,
        };
        crate::multibrand::validate(&plan)?;
        make_profile(plan, &name)
    })();
    let profile = match result {
        Ok(profile) if issues.is_empty() => Some(profile),
        Ok(_) => None,
        Err(e) => {
            issues.push(e);
            None
        }
    };
    Candidate{guid,name,profile,issues,notes:vec![
        "仅转换已核对的精确型号 JSON 子集；来源原文内部保留，未知字段会阻止整个配置转换。".into(),
            "DPI 活动档按原厂 JSON 的 1 起始编号读取；所有 16 条普通/Hypershift 输入必须由来源明确提供，稀疏导出不会被补成默认值。".into(),
        "方案只绑定来源的有线或原配接收器编号；另一连接的回报率仍保留在原文件。导入不会自动写入鼠标。".into(),
    ]}
}

fn xy(value: &Value, path: &str, stage: Option<usize>) -> Result<DpiStage, String> {
    let obj = object(
        value,
        path,
        if stage.is_some() {
            &["x", "y", "independent", "visible", "index"]
        } else {
            &["x", "y", "independent"]
        },
    )?;
    let x = number(obj, "x", path)?;
    let y = number(obj, "y", path)?;
    if !(100..=30000).contains(&x) || !(100..=30000).contains(&y) {
        return Err(format!("{path}: DPI 超出本型号 100–30000 范围"));
    }
    if x % 50 != 0 || y % 50 != 0 {
        return Err(format!(
            "{path}: 来源 DPI 必须符合此型号已确认的 50 DPI 步进"
        ));
    }
    if x != y && obj.get("independent") != Some(&Value::Bool(true)) {
        return Err(format!("{path}/independent: 不同 XY 值必须明确启用独立轴"));
    }
    if let Some(v) = obj.get("independent") {
        let independent = v
            .as_bool()
            .ok_or_else(|| format!("{path}/independent: 必须是布尔值"))?;
        if !independent && x != y {
            return Err(format!("{path}: XY 值与独立轴标志冲突"));
        }
    }
    if let Some(index) = stage {
        if !boolean(obj, "visible", path)? {
            return Err(format!(
                "{path}/visible: 隐藏档位的导出语义尚未接通，不能删除或压缩"
            ));
        }
        if let Some(v) = obj.get("index") {
            if v.as_u64() != Some(index as u64) {
                return Err(format!("{path}/index: 重排或重复档位未支持"));
            }
        }
    }
    Ok(DpiStage {
        x: x as u16,
        y: y as u16,
    })
}
fn performance(obj: &Map<String, Value>, path: &str) -> Result<DpiState, String> {
    let current = xy(field(obj, "dpi", path)?, &format!("{path}/dpi"), None)?;
    let st = object(
        field(obj, "dpiStages", path)?,
        &format!("{path}/dpiStages"),
        &["stages", "active", "enable", "count"],
    )?;
    if !boolean(st, "enable", path)? {
        return Err(format!(
            "{path}/dpiStages/enable: 只支持完整启用的 2–5 档表，不能丢弃停用表"
        ));
    }
    let rows = field(st, "stages", path)?
        .as_array()
        .ok_or_else(|| format!("{path}/dpiStages/stages: 必须是数组"))?;
    if !(2..=5).contains(&rows.len()) {
        return Err(format!(
            "{path}/dpiStages/stages: 需要 2–5 个完整已启用档位"
        ));
    }
    if let Some(count) = st.get("count") {
        if count.as_u64() != Some(rows.len() as u64) {
            return Err(format!("{path}/dpiStages/count: 数量与完整表不一致"));
        }
    }
    let active = number(st, "active", path)?;
    if active == 0 || active > rows.len() as u64 {
        return Err(format!(
            "{path}/dpiStages/active: 原厂 JSON 活动档必须为 1–{}",
            rows.len()
        ));
    }
    let stages = rows
        .iter()
        .enumerate()
        .map(|(i, v)| xy(v, &format!("{path}/dpiStages/stages/{i}"), Some(i)))
        .collect::<Result<Vec<_>, _>>()?;
    if current != stages[active as usize - 1] {
        return Err(format!(
            "{path}/dpi: 当前 XY 与活动档不一致，不能丢失其中一个值"
        ));
    }
    Ok(DpiState {
        stages,
        active: active as u8 - 1,
    })
}

fn physical(kind: &str, input: &str) -> Result<(u16, bool), String> {
    Ok(match (kind, input) {
        ("MouseInput", "LeftClick") => (1, false),
        ("MouseInput", "RightClick") => (2, false),
        ("MouseInput", "ScrollButton") => (3, false),
        ("MouseInput", "Button4") => (4, false),
        ("MouseInput", "Button5") => (5, false),
        ("MouseInput", "ScrollUp") => (9, false),
        ("MouseInput", "ScrollDown") => (10, false),
        ("DKMInput", "DKM_SB_03") => (96, false),
        ("DKMInput", "DKM_M_03") => (4, true),
        ("DKMInput", "DKM_M_04") => (5, true),
        _ => {
            return Err(format!(
                "未知 DeathAdder 输入 {kind}/{input}；不能套用 Naga 布局"
            ))
        }
    })
}
fn mappings(value: &Value, path: &str) -> Result<BTreeMap<u16, ButtonAction>, String> {
    let rows = value
        .as_array()
        .ok_or_else(|| format!("{path}: 必须是数组"))?;
    if rows.len() > 32 {
        return Err(format!("{path}: 输入记录过多"));
    }
    let mut result = BTreeMap::new();
    let mut aliases = Vec::new();
    let mut sources = BTreeSet::new();
    let mut explicit = BTreeSet::new();
    for (index, row) in rows.iter().enumerate() {
        let at = format!("{path}/{index}");
        let obj = row.as_object().ok_or_else(|| format!("{at}: 必须是对象"))?;
        let kind = text(obj, "inputType", &at)?;
        let input = text(obj, "inputID", &at)?;
        let (physical, alias) = physical(kind, input).map_err(|e| format!("{at}/inputID: {e}"))?;
        let layer = if obj.contains_key("isHyperShift") {
            boolean(obj, "isHyperShift", &at)?
        } else {
            false
        };
        if !sources.insert((kind, input, layer)) {
            return Err(format!("{at}: 重复来源输入/层"));
        }
        let id = physical | if layer { 0x100 } else { 0 };
        let action = action(row, id, &at)?;
        if obj
            .get("isDefault")
            .is_some_and(|v| v.as_bool() != Some(false) && v.as_bool() != Some(true))
        {
            return Err(format!("{at}/isDefault: 必须是布尔值"));
        }
        if obj.get("isDefault") == Some(&Value::Bool(true)) && action != default_action(id) {
            return Err(format!("{at}/isDefault: 默认标记与动作冲突"));
        }
        gearwise_razer::dav3pro_buttons::encode(id, &action).map_err(|e| format!("{at}: {e}"))?;
        if alias {
            aliases.push((id, action, at));
        } else {
            if result.insert(id, action).is_some() {
                return Err(format!("{at}: 重复物理输入/层"));
            }
            explicit.insert(id);
        }
    }
    for (id, action, at) in aliases {
        if !explicit.contains(&id) || result.get(&id) != Some(&action) {
            return Err(format!(
                "{at}: DKM 兼容记录缺少一致的同层 Button4/5 记录；别名优先级未证实"
            ));
        }
    }
    let missing = gearwise_razer::dav3pro_buttons::controls()
        .into_iter()
        .filter_map(|(id, _)| (!explicit.contains(&id)).then_some(id.to_string()))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!("{path}: 缺少明确来源的输入记录 {}；稀疏导出/默认值合并语义尚未证实，不能覆盖省略的硬件绑定",missing.join(", ")));
    }
    for physical in 1..=5 {
        let normal = result.get(&physical) == Some(&ButtonAction::Hypershift);
        let second = result.get(&(physical | 0x100)) == Some(&ButtonAction::Hypershift);
        if normal != second {
            return Err(format!(
                "{path}: 输入 {physical} 的 Hypershift 必须保留原厂两层成对记录"
            ));
        }
    }
    if !result
        .iter()
        .any(|(id, action)| *id < 0x100 && *action == ButtonAction::Mouse(MouseButton::Left))
    {
        return Err(format!("{path}: 普通层没有直接左键，不能导入覆盖唯一点击"));
    }
    Ok(result)
}
fn default_action(id: u16) -> ButtonAction {
    let physical = id & 0xff;
    match physical {
        1 => ButtonAction::Mouse(MouseButton::Left),
        2 => ButtonAction::Mouse(MouseButton::Right),
        3 => ButtonAction::Mouse(MouseButton::Middle),
        4 => ButtonAction::Mouse(MouseButton::Back),
        5 => ButtonAction::Mouse(MouseButton::Forward),
        9 => ButtonAction::Scroll(ScrollDirection::Up),
        10 => ButtonAction::Scroll(ScrollDirection::Down),
        96 => ButtonAction::Dpi(DpiFunction::Cycle),
        _ => unreachable!(),
    }
}
fn action(row: &Value, _id: u16, path: &str) -> Result<ButtonAction, String> {
    let original = row.as_object().unwrap();
    let output = text(original, "outputType", path)?;
    let mut keys = vec![
        "inputType",
        "inputID",
        "isHyperShift",
        "isDefault",
        "outputType",
    ];
    if !["disableGroup", "hyperShiftGroup", "hypershiftGroup"].contains(&output) {
        keys.push(output);
    }
    let obj = object(row, path, &keys)?;
    let group_path = format!("{path}/{output}");
    Ok(match output {
        "disableGroup" => ButtonAction::Disabled,
        "hyperShiftGroup" | "hypershiftGroup" => ButtonAction::Hypershift,
        "mouseGroup" => {
            let group = object(field(obj, output, path)?, &group_path, &["mouseAssignment"])?;
            match text(group, "mouseAssignment", &group_path)? {
                "Click" => ButtonAction::Mouse(MouseButton::Left),
                "Menu" => ButtonAction::Mouse(MouseButton::Right),
                "ScrollClick" | "ScrollButton" => ButtonAction::Mouse(MouseButton::Middle),
                "Previous" => ButtonAction::Mouse(MouseButton::Back),
                "Next" => ButtonAction::Mouse(MouseButton::Forward),
                "ScrollUp" => ButtonAction::Scroll(ScrollDirection::Up),
                "ScrollDown" => ButtonAction::Scroll(ScrollDirection::Down),
                other => return Err(format!("{group_path}/mouseAssignment: 未支持动作 {other}")),
            }
        }
        "keyboardGroup" => {
            let group = object(
                field(obj, output, path)?,
                &group_path,
                &["key", "modifiers", "keyGroup"],
            )?;
            if let Some(key_group) = group.get("keyGroup") {
                if !key_group.as_u64().is_some_and(|v| v <= 6) {
                    return Err(format!("{group_path}/keyGroup: 未知键类别"));
                }
            }
            let token = text(group, "key", &group_path)?;
            let usage = match token {
                "KEY_YEN" => 0x89,
                "KEY_RO" => 0x87,
                "KEY_MUHENKAN" => 0x8b,
                "KEY_HENKAN" => 0x8a,
                "KEY_KATAKANA_HIRAGANA" => 0x88,
                "KEY_NON_US_BACKSLASH" => 0x64,
                _ => {
                    let key = sidearm_synapse_parser::mapping::translate_key_token(token)
                        .map_err(|_| format!("{group_path}/key: 未支持主键 {token}"))?;
                    crate::import_keys::usage(&key)?
                }
            };
            let mods = field(group, "modifiers", &group_path)?
                .as_array()
                .ok_or_else(|| format!("{group_path}/modifiers: 需要原始修饰键数组"))?;
            let mut mask = 0;
            let mut seen = BTreeSet::new();
            for modifier in mods {
                let token = modifier.as_str().ok_or_else(|| {
                    format!("{group_path}/modifiers: 对象修饰键的完整字段尚未接通")
                })?;
                if !seen.insert(token) {
                    return Err(format!("{group_path}/modifiers: 重复修饰键"));
                }
                mask |= match token {
                    "KEY_LEFT_CTRL" => 1,
                    "KEY_LEFT_SHIFT" => 2,
                    "KEY_LEFT_ALT" => 4,
                    "KEY_LEFT_WINDOWS" | "KEY_LEFT_WIN" => 8,
                    "KEY_RIGHT_CTRL" => 16,
                    "KEY_RIGHT_SHIFT" => 32,
                    "KEY_RIGHT_ALT" => 64,
                    "KEY_RIGHT_WINDOWS" | "KEY_RIGHT_WIN" => 128,
                    _ => return Err(format!("{group_path}/modifiers: 未知修饰键 {token}")),
                };
            }
            ButtonAction::Key {
                usage,
                mods: Modifiers(mask),
            }
        }
        "sensitivityGroup" => {
            let group = object(
                field(obj, output, path)?,
                &group_path,
                &["sensitivityAssignment", "x", "y", "independent"],
            )?;
            let selected = text(group, "sensitivityAssignment", &group_path)?;
            let fun = match selected {
                "DPI_Up" => DpiFunction::Up,
                "DPI_Down" => DpiFunction::Down,
                "DPI_CycleUp" => DpiFunction::Cycle,
                "DPI_CycleDown" => DpiFunction::CycleDown,
                "DPI_Clutch" => {
                    let stage = xy(
                        &Value::Object(
                            group
                                .iter()
                                .filter(|(k, _)| k.as_str() != "sensitivityAssignment")
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect(),
                        ),
                        &group_path,
                        None,
                    )?;
                    if group.get("independent") == Some(&Value::Bool(true)) {
                        DpiFunction::ShiftXY { x: stage.x, y: stage.y }
                    } else {
                        DpiFunction::Shift(stage.x)
                    }
                }
                other => return Err(format!("{group_path}: 未支持灵敏度动作 {other}")),
            };
            if selected != "DPI_Clutch" && group.keys().any(|k| k != "sensitivityAssignment") {
                return Err(format!("{group_path}: 非狙击动作带额外 XY 参数，不能忽略"));
            }
            ButtonAction::Dpi(fun)
        }
        "multimediaGroup" => {
            let group = object(
                field(obj, output, path)?,
                &group_path,
                &["multimediaAssignment"],
            )?;
            let usage = match text(group, "multimediaAssignment", &group_path)? {
                "NextTrack" => 0xb5,
                "PrevTrack" => 0xb6,
                "MediaStop" => 0xb7,
                "Play" => 0xcd,
                "MuteVolume" => 0xe2,
                "VolumeUp" => 0xe9,
                "VolumeDown" => 0xea,
                other => {
                    return Err(format!(
                        "{group_path}/multimediaAssignment: 未支持媒体动作 {other}"
                    ))
                }
            };
            ButtonAction::Media(usage)
        }
        "macroGroup" | "textBlockGroup" => {
            return Err(format!(
                "{group_path}: 此型号原厂宏/文本桥接未启用，原始配置完整保留"
            ))
        }
        _ => {
            return Err(format!(
                "{group_path}: 未接通的原厂输出类型；原始动作保留，不能变为禁用"
            ))
        }
    })
}
fn make_profile(plan: Plan, name: &str) -> Result<Profile, String> {
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
    let profile = Profile {
        target_vendor: Some(plan.vendor),
        target_model: Some(plan.product),
        mchose_board: None,
        sino: None,
        id: format!("synapse-{}", crate::profiles::new_id()),
        name: name.into(),
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
        report_rate_hz: plan.settings.iter().find_map(|s| {
            if let Setting::ReportRate(r) = s {
                Some(u32::from(*r))
            } else {
                None
            }
        }),
        ghub_id: None,
        multi: Some(plan),
    };
    crate::profile_transfer::validate(&profile)?;
    Ok(profile)
}
