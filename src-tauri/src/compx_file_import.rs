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
type Result<T> = std::result::Result<T, String>;
const MAX: usize = 8 * 1024 * 1024;
#[derive(Serialize)]
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
pub fn prepare_file(
    path: &std::path::Path,
    vendor: u16,
    product: u16,
    include_buttons: bool,
) -> Result<Prepared> {
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bin"))
    {
        return Err("请选择 ATK HUB 导出的 .bin 文件".into());
    }
    prepare_with_buttons(
        &crate::mhub_full::read_explicit_file(path)?,
        vendor,
        product,
        include_buttons,
    )
}
#[cfg(test)]
pub fn prepare_bytes(bytes: &[u8], vendor: u16, product: u16) -> Result<Prepared> {
    prepare_with_buttons(bytes, vendor, product, false)
}
pub fn prepare_with_buttons(
    bytes: &[u8],
    vendor: u16,
    product: u16,
    include_buttons: bool,
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
    if include_buttons {
        settings.extend(buttons(&root, model.buttons)?);
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
            macros: vec![],
            source: None,
        }),
        app_path: None,
        ghub_id: None,
        mchose_board: None,
        mchose: None,
        sino: None,
    };
    crate::profile_transfer::validate(&profile)?;
    let note = if include_buttons {
        "本次转换活动 DPI 档位、独立 X/Y、回报率及已登记非左键的已解析动作；独立左键保持原值。单键/组合键保留修饰键和完整按下抬起；应用时使用本程序原生单次宏格式。文件中的其他宏、多键快捷序列、灯效、性能、省电和非活动 DPI 槽位未导入。未知动作或缺少宏内容时整份按键导入拒绝，未替换成默认键。导入不会自动应用。"
    } else {
        "本次仅转换活动 DPI 档位、独立 X/Y 和回报率。文件中的按键、宏、灯效、性能、省电及非活动 DPI 槽位未导入，应用后沿用鼠标现有设置。导入不会自动应用。"
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
    atk::shortcuts::chord(&rows).map_err(|e| e.to_string())
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
fn buttons(root: &Value, count: u16) -> Result<Vec<Setting>> {
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
        let action = if raw == [5, 0, 0] {
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
    Ok(mapped
        .into_iter()
        .filter(|(index, _)| *index != 0 && *index < usize::from(count))
        .map(|(index, action)| Setting::Button {
            id: index as u16 + 1,
            action,
        })
        .collect())
}
#[cfg(test)]
#[path = "compx_file_import_tests.rs"]
mod tests;
