//! Read-only ATK HUB 3.2.27 .bin conversion. Only explicitly listed fields.
//! Source files never supply a HID route, input-device authorization or app path.
use crate::{
    atk,
    mhub_full::StrictValue,
    multibrand::Plan,
    profiles::{Level, Profile},
};
use gearwise_core::{
    model::{DpiStage, DpiState},
    Setting,
};
use serde::Serialize;
use serde_json::Value;
#[path = "compx_file_extras.rs"]
mod extras;
#[path = "compx_file_macros.rs"]
mod macro_file;
pub type MacroModes = std::collections::BTreeMap<u16, gearwise_core::profile::MacroMode>;
#[derive(Serialize)]
pub struct MacroRef {
    pub button: u16,
    pub slot: u8,
    pub name: String,
    pub stored_code: u8,
    pub steps: usize,
    pub events: Vec<gearwise_core::profile::MacroStep>,
}
#[derive(Serialize)]
pub struct Inspection {
    pub sha256: String,
    pub refs: Vec<MacroRef>,
}
type Result<T> = std::result::Result<T, String>;
const MAX: usize = 8 * 1024 * 1024;
#[derive(Debug, Serialize)]
pub struct Prepared {
    pub profile: Profile,
    pub note: String,
}

// Canonical, padded Base64 emitted by mV.encode. Reject ambiguous padding,
// ignored punctuation and invalid trailing bits rather than normalizing input.
fn decode(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.is_empty() || bytes.len() > MAX || bytes.len() % 4 != 0 {
        return Err("需要不超过 8 MiB 的原厂 Base64 .bin 文件".into());
    }
    fn digit(b: u8) -> Result<u8> {
        match b {
            b'A'..=b'Z' => Ok(b - b'A'),
            b'a'..=b'z' => Ok(b - b'a' + 26),
            b'0'..=b'9' => Ok(b - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err("原厂 .bin 的 Base64 字符无效".into()),
        }
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (i, q) in bytes.chunks_exact(4).enumerate() {
        let a = digit(q[0])?;
        let b = digit(q[1])?;
        out.push(a << 2 | b >> 4);
        let last = (i + 1) * 4 == bytes.len();
        if q[2] == b'=' {
            if !last || q[3] != b'=' || b & 15 != 0 {
                return Err("原厂 .bin 的 Base64 填充无效".into());
            }
        } else {
            let c = digit(q[2])?;
            out.push(b << 4 | c >> 2);
            if q[3] == b'=' {
                if !last || c & 3 != 0 {
                    return Err("原厂 .bin 的 Base64 填充无效".into());
                }
            } else {
                out.push(c << 6 | digit(q[3])?);
            }
        }
    }
    Ok(out)
}
fn integer(v: &Value, name: &str, max: u64) -> Result<u64> {
    v.as_u64()
        .filter(|n| *n <= max)
        .ok_or_else(|| format!("原厂配置 {name} 必须是有效整数"))
}
#[cfg(test)]
pub fn prepare_file(
    path: &std::path::Path,
    vendor: u16,
    product: u16,
    include_buttons: bool,
    include_tuning: bool,
    include_lighting: bool,
) -> Result<Prepared> {
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bin"))
    {
        return Err("请选择 ATK HUB 导出的 .bin 文件".into());
    }
    prepare_selected(
        &crate::mhub_full::read_explicit_file(path)?,
        vendor,
        product,
        include_buttons,
        include_tuning,
        include_lighting,
    )
}
#[cfg(test)]
pub fn prepare_bytes(bytes: &[u8], vendor: u16, product: u16) -> Result<Prepared> {
    prepare_with_buttons(bytes, vendor, product, false)
}
#[cfg(test)]
pub fn prepare_with_buttons(
    bytes: &[u8],
    vendor: u16,
    product: u16,
    include_buttons: bool,
) -> Result<Prepared> {
    prepare_selected(bytes, vendor, product, include_buttons, false, false)
}
#[cfg(test)]
pub fn prepare_selected(
    bytes: &[u8],
    vendor: u16,
    product: u16,
    include_buttons: bool,
    include_tuning: bool,
    include_lighting: bool,
) -> Result<Prepared> {
    prepare_selected_modes(
        bytes,
        vendor,
        product,
        include_buttons,
        include_tuning,
        include_lighting,
        &MacroModes::new(),
    )
}
pub fn prepare_selected_modes(
    bytes: &[u8],
    vendor: u16,
    product: u16,
    include_buttons: bool,
    include_tuning: bool,
    include_lighting: bool,
    modes: &MacroModes,
) -> Result<Prepared> {
    let model = atk::model(vendor, product)
        .filter(|m| m.native_macros && matches!(m.brand.as_str(), "atk" | "vxe" | "vgn"))
        .ok_or("此型号尚无 ATK HUB 文件转换器")?;
    let decoded = decode(bytes)?;
    let StrictValue(root) = serde_json::from_slice::<StrictValue>(&decoded)
        .map_err(|e| format!("原厂 .bin 不是有效且无重复字段的 UTF-8 JSON：{e}"))?;
    if !root.is_object() {
        return Err("原厂配置根节点无效".into());
    }
    let mouse = root
        .pointer("/deviceConfig/mouseConfig")
        .filter(|v| v.is_object())
        .ok_or("缺少原厂 mouseConfig")?;
    let cid = integer(&mouse["cidMid"]["cid"], "CID", 255)? as u8;
    let mid = integer(&mouse["cidMid"]["mid"], "MID", 255)? as u8;
    if cid != 2 || !model.mids.contains(&mid) {
        return Err("原厂文件 CID/MID 与所选鼠标型号不一致；未生成配置".into());
    }
    let count = integer(&mouse["dpiTotal"], "DPI 档数", u64::from(model.max_stages))? as usize;
    let active = integer(&mouse["dpiLevel"], "当前 DPI 档位", 255)? as u8;
    let list = mouse["dpiList"].as_array().ok_or("缺少原厂 DPI 列表")?;
    if count == 0
        || usize::from(active) >= count
        || list.len() < count
        || list.len() > usize::from(model.max_stages)
    {
        return Err("原厂 DPI 档数、列表或当前档位不一致".into());
    }
    let stages = list
        .iter()
        .take(count)
        .map(|item| {
            let d = &item["dpi"];
            if d["usesLogical"] != true {
                return Err(
                    "仅转换原厂明确标记 usesLogical 的实际 DPI 数值；未猜测旧版原始编码".into(),
                );
            }
            Ok(DpiStage {
                x: integer(&d["xDpi"], "X DPI", u16::MAX as u64)? as u16,
                y: integer(&d["yDpi"], "Y DPI", u16::MAX as u64)? as u16,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let dpi = DpiState { stages, active };
    atk::validate_dpi(&dpi, product).map_err(|e| e.to_string())?;
    let code = integer(&mouse["reportRate"], "回报率枚举", 6)? as usize;
    let rate = [125u16, 250, 500, 1000, 2000, 4000, 8000][code];
    if !model.rates.contains(&rate) {
        return Err("文件回报率超出当前型号与连接方式已适配范围".into());
    }
    let levels = dpi
        .stages
        .iter()
        .map(|d| Level {
            x: d.x,
            y: Some(d.y),
        })
        .collect();
    let mut settings = vec![Setting::Dpi(dpi), Setting::ReportRate(rate)];
    let mut macros = vec![];
    if include_buttons {
        let (bindings, definitions) = buttons_with_modes(&root, model.buttons, modes)?;
        settings.extend(bindings);
        macros = definitions;
    }
    if include_tuning {
        settings.push(extras::tuning(mouse, model)?);
    }
    if include_lighting {
        settings.extend(extras::lighting(&root, model)?);
    }
    let profile = Profile {
        id: String::new(),
        name: format!(
            "{} · 原厂{}导入",
            model.name,
            if include_buttons {
                "按键 / DPI "
            } else {
                " DPI "
            }
        ),
        target_vendor: Some(vendor),
        target_model: Some(product),
        levels,
        active_level: usize::from(active),
        report_rate_hz: Some(u32::from(rate)),
        multi: Some(Plan {
            vendor,
            product,
            settings,
            macros,
            source: None,
        }),
        app_path: None,
        ghub_id: None,
        mchose_board: None,
        mchose: None,
        sino: None,
    };
    crate::profile_transfer::validate(&profile)?;
    let note = if !include_buttons && !include_tuning && !include_lighting {
        "本次仅转换活动 DPI 档位、独立 X/Y 和回报率。文件中的按键、宏、灯效、性能、省电及非活动 DPI 槽位未导入，应用后沿用鼠标现有设置。导入不会自动应用。".to_string()
    } else {
        let mut included = vec!["活动 DPI 档位、独立 X/Y、回报率"];
        if include_buttons {
            included.push("已登记非左键及完整的已解析快捷序列");
        }
        if include_tuning {
            included.push("已支持的性能与省电字段");
        }
        if include_lighting {
            included.push("DPI 指示灯模式、亮度、速度及完整八档颜色");
        }
        let mut excluded =
            vec!["其他宏、未支持快捷事件、未开放性能项、接收器/装饰灯、未启用档位的 DPI 数值"];
        if !include_buttons {
            excluded.push("按键与快捷键");
        }
        if !include_tuning {
            excluded.push("性能与省电");
        }
        if !include_lighting {
            excluded.push("DPI 灯效与色表");
        }
        format!("本次转换{}。独立左键保持原值；单键/组合键使用原厂快捷键表。未导入：{}。未选择或未开放的设置保留鼠标原值；未知或无效的已选字段会拒绝导入。导入不会自动应用。",included.join("、"),excluded.join("、"))
    };
    let count = profile.multi.as_ref().map(|p| p.macros.len()).unwrap_or(0);
    let note = if count > 0 {
        format!("{note} 本次导入 {count} 份按键宏方案，名称保存在配置中心，播放方式按逐键确认。零延时按此原厂版本实际写入规则转为 1 ms。未绑定宏条目不在本次范围；已绑定宏含不支持事件时拒绝整份按键转换。")
    } else {
        note
    };
    Ok(Prepared {
        profile,
        note: note.into(),
    })
}
fn shortcut_chord(value: &Value) -> Result<gearwise_core::model::ButtonAction> {
    let events = value
        .as_array()
        .filter(|a| (2..=6).contains(&a.len()))
        .ok_or("缺少2–6条完整快捷键事件")?;
    let mut rows = vec![];
    for event in events {
        let a = event
            .as_array()
            .filter(|a| a.len() == 4)
            .ok_or("快捷键事件必须包含四个字段")?;
        let bits = a[0]
            .as_str()
            .ok_or("快捷键类型必须是原厂三位二进制字符串")?;
        if bits.len() != 3 || !bits.bytes().all(|b| matches!(b, b'0' | b'1')) {
            return Err("快捷键类型不是原厂三位二进制编码".into());
        }
        let kind = u8::from_str_radix(bits, 2).map_err(|e| e.to_string())?;
        rows.push([
            kind,
            integer(&a[1], "快捷键状态", 2)? as u8,
            integer(&a[2], "快捷键值", 255)? as u8,
            integer(&a[3], "快捷键附加值", 255)? as u8,
        ]);
    }
    atk::shortcuts::from_rows(&rows).map_err(|e| e.to_string())
}
fn triplet(value: &Value) -> Result<[u8; 3]> {
    let a = value
        .as_array()
        .filter(|a| a.len() == 3)
        .ok_or("按键必须包含三个完整编码字节")?;
    Ok([
        integer(&a[0], "按键类型", 255)? as u8,
        integer(&a[1], "按键值", 255)? as u8,
        integer(&a[2], "按键附加值", 255)? as u8,
    ])
}
#[cfg(test)]
fn buttons(root: &Value, count: u16) -> Result<Vec<Setting>> {
    buttons_with_modes(root, count, &MacroModes::new()).map(|v| v.0)
}
fn buttons_with_modes(
    root: &Value,
    count: u16,
    modes: &MacroModes,
) -> Result<(Vec<Setting>, Vec<gearwise_core::profile::Macro>)> {
    let layers = root["keyActions"]
        .as_array()
        .filter(|a| a.len() == 1)
        .ok_or("需要单层完整鼠标按键表")?;
    let rows = layers[0]
        .as_array()
        .filter(|a| a.len() == 1)
        .ok_or("需要单行完整鼠标按键表")?;
    let items = rows[0]
        .as_array()
        .filter(|a| a.len() == 6 && matches!(count, 5 | 6))
        .ok_or("原厂按键表数量与目标鼠标不一致")?;
    let defaults = [
        [1, 1, 0],
        [1, 2, 0],
        [1, 4, 0],
        [1, 8, 0],
        [1, 16, 0],
        [2, 1, 0],
    ];
    let mut definitions = vec![];
    let mut mapped = std::collections::BTreeMap::new();
    for item in items {
        let def = triplet(&item["defaultKey"])?;
        let index = defaults
            .iter()
            .position(|v| v == &def)
            .ok_or("未知实体按键标识，未猜测按键位置")?;
        if mapped.contains_key(&index) {
            return Err("原厂文件含重复实体按键".into());
        }
        let raw = triplet(&item["changeKey"])?;
        if index == 0 && raw != defaults[0] {
            return Err("当前驱动保留独立左键；原厂左键已改动，无法完整迁移此按键表".into());
        }
        if index >= usize::from(count) && raw != def {
            return Err("文件含当前型号未开放的额外控制，未静默丢弃".into());
        }
        let crc = 0x55u8
            .wrapping_sub(raw[0])
            .wrapping_sub(raw[1])
            .wrapping_sub(raw[2]);
        let action = if raw[0] == 6 {
            if raw[1] >= 12 {
                return Err("未知原厂宏槽".into());
            }
            let button = index as u16 + 1;
            let mode = modes.get(&button).copied().ok_or_else(|| {
                format!("请先确认按键 {button} 的宏播放方式；文件编码存在歧义，未自动选择")
            })?;
            let m = macro_file::parse(root, raw[1], button, mode)?;
            let action = gearwise_core::model::ButtonAction::Macro(m.id.clone());
            definitions.push(m);
            action
        } else if raw == [5, 0, 0] {
            let names = ["left", "right", "center", "side1", "side2", "bottom"];
            let value = &root["deviceConfig"]["mouseConfig"]["shortcutAction"][names[index]];
            shortcut_chord(value)
                .map_err(|e| format!("第 {} 个按键快捷键未完整转换：{e}", index + 1))?
        } else {
            atk::compx_button_action(&[raw[0], raw[1], raw[2], crc]).ok_or_else(|| {
                format!(
                    "第 {} 个按键的动作尚未完整转换（类型 {}）；宏必须具备已验证内容，未生成空动作",
                    index + 1,
                    raw[0]
                )
            })?
        };
        mapped.insert(index, action);
    }
    Ok((
        mapped
            .into_iter()
            .filter(|(index, _)| *index != 0 && *index < usize::from(count))
            .map(|(index, action)| Setting::Button {
                id: index as u16 + 1,
                action,
            })
            .collect(),
        definitions,
    ))
}
#[cfg(test)]
#[path = "compx_file_import_tests.rs"]
mod tests;

fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
fn source_file(path: &std::path::Path) -> Result<Vec<u8>> {
    if !path
        .extension()
        .is_some_and(|v| v.eq_ignore_ascii_case("bin"))
    {
        return Err("请选择 ATK HUB 导出的 .bin 文件".into());
    }
    crate::mhub_full::read_explicit_file(path)
}
pub fn prepare_reviewed_file(
    path: &std::path::Path,
    v: u16,
    p: u16,
    b: bool,
    t: bool,
    l: bool,
    modes: &MacroModes,
    expected: Option<&str>,
) -> Result<Prepared> {
    let bytes = source_file(path)?;
    if expected.is_some_and(|hash| hash != digest(&bytes)) {
        return Err("所选文件在核对后已改变，请重新选择并确认宏播放方式".into());
    }
    prepare_selected_modes(&bytes, v, p, b, t, l, modes)
}
pub fn inspect_macros(path: &std::path::Path, v: u16, p: u16) -> Result<Inspection> {
    let bytes = source_file(path)?;
    let modes = (2..=6)
        .map(|i| (i, gearwise_core::profile::MacroMode::Once))
        .collect();
    let draft = prepare_selected_modes(&bytes, v, p, true, false, false, &modes)?;
    let StrictValue(root) = serde_json::from_slice(&decode(&bytes)?).map_err(|e| e.to_string())?;
    let plan = draft.profile.multi.unwrap();
    let mut refs = vec![];
    let defaults = [
        [1, 1, 0],
        [1, 2, 0],
        [1, 4, 0],
        [1, 8, 0],
        [1, 16, 0],
        [2, 1, 0],
    ];
    for item in root["keyActions"][0][0]
        .as_array()
        .ok_or("缺少实体按键表")?
    {
        let def = triplet(&item["defaultKey"])?;
        let raw = triplet(&item["changeKey"])?;
        if raw[0] != 6 {
            continue;
        }
        let button = defaults
            .iter()
            .position(|v| v == &def)
            .ok_or("未知实体按键")? as u16
            + 1;
        let m = plan
            .macros
            .iter()
            .find(|m| m.id == format!("atk-file:slot:{}:button:{button}", raw[1]))
            .ok_or("宏正文未完整转换")?;
        refs.push(MacroRef {
            button,
            slot: raw[1],
            name: m.name.clone(),
            stored_code: raw[2],
            steps: m.steps.len(),
            events: m.steps.clone(),
        });
    }
    refs.sort_by_key(|r| r.button);
    Ok(Inspection {
        sha256: digest(&bytes),
        refs,
    })
}
