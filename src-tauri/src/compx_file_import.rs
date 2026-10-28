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
pub fn prepare_file(path: &std::path::Path, vendor: u16, product: u16) -> Result<Prepared> {
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bin"))
    {
        return Err("请选择 ATK HUB 导出的 .bin 文件".into());
    }
    prepare_bytes(
        &crate::mhub_full::read_explicit_file(path)?,
        vendor,
        product,
    )
}
pub fn prepare_bytes(bytes: &[u8], vendor: u16, product: u16) -> Result<Prepared> {
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
    let profile = Profile {
        id: String::new(),
        name: format!("{} · 原厂 DPI 导入", model.name),
        target_vendor: Some(vendor),
        target_model: Some(product),
        levels,
        active_level: usize::from(active),
        report_rate_hz: Some(u32::from(rate)),
        multi: Some(Plan {
            vendor,
            product,
            settings: vec![Setting::Dpi(dpi), Setting::ReportRate(rate)],
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
    Ok(Prepared{profile,note:"本次仅转换活动 DPI 档位、独立 X/Y 和回报率。文件中的按键、宏、灯效、性能、省电及非活动 DPI 槽位未导入，应用后沿用鼠标现有设置。导入不会自动应用。".into()})
}
#[cfg(test)]
#[path = "compx_file_import_tests.rs"]
mod tests;
