//! Complete known A7 template conversion and explicit file/editor preparation.
//! These functions never enumerate or write devices.
//! OEM JSON export names macros but does not include their bodies. Missing or
//! ambiguous bodies must remain an error, never become an empty macro.
use crate::mchose_config::{self as native, MacroEvent, MouseMacro, Snapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

type Result<T> = std::result::Result<T, String>;
const PRODUCT: &str = "MCHOSE A7 V3 Ultra+";
const BUTTONS: [&str; 6] = ["left", "right", "mid", "forward", "retreat", "dpi"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub vendor: u16,
    pub model: u16,
    pub board: u8,
    pub name: String,
    pub dpi_x: Vec<u16>,
    /// Inactive Y values remain in the plan, but are not applied by the OEM
    /// template writer until independent-Y is enabled.
    pub dpi_y: Option<Vec<u16>>,
    pub count: u8,
    pub active: u8,
    pub independent: bool,
    pub usb_rate: u8,
    pub wireless_rate: u8,
    pub sleep: u8,
    pub never_sleep: bool,
    pub left_debounce: u8,
    pub right_debounce: u8,
    pub sensor: u8,
    pub rotation: i16,
    pub lod: u8,
    pub buttons: Vec<Vec<u8>>,
    pub macros: Vec<MouseMacro>,
}

pub struct Compiled {
    pub snapshot: Snapshot,
    /// Only these records need a native upload. Existing identical content,
    /// other-board references and unassigned user macros remain untouched.
    pub macro_writes: Vec<MouseMacro>,
    pub slot_map: BTreeMap<u8, u8>,
}

pub fn rate_hz(code:u8)->Option<u32> {match code {0=>Some(125),2=>Some(500),3=>Some(1000),4=>Some(2000),5=>Some(4000),6=>Some(8000),_=>None}}
pub fn profile_levels(plan:&Plan)->Vec<crate::profiles::Level> {
    plan.dpi_x.iter().enumerate().map(|(index,x)|crate::profiles::Level{x:*x,y:if plan.independent {plan.dpi_y.as_ref().and_then(|y|y.get(index).copied())}else{None}}).collect()
}
pub fn to_profile(plan:Plan)->Result<crate::profiles::Profile> {
    validate(&plan)?;
    let profile=crate::profiles::Profile{
        id:format!("mhub-full-3837-4026-b{}:{}",plan.board,plan.name),name:format!("M HUB · {}",plan.name),
        target_vendor:Some(plan.vendor),target_model:Some(plan.model),mchose_board:None,sino:None,multi:None,
        levels:profile_levels(&plan),active_level:usize::from(plan.active),report_rate_hz:rate_hz(plan.wireless_rate),
        app_path:None,ghub_id:None,mchose:Some(plan),
    };
    crate::profile_transfer::validate(&profile)?;
    Ok(profile)
}

/// Explicit editor preparation regenerates canonical macro bytes and summaries.
/// Opaque raw baselines remain immutable unless the user replaces the macro.
/// Package import/save still uses strict validation, never this normalization.
pub fn prepare_profile(mut profile:crate::profiles::Profile)->Result<crate::profiles::Profile> {
    let plan=profile.mchose.as_mut().ok_or("缺少完整迈从方案")?;
    if plan.macros.len()>18 {return Err("完整方案最多18个宏".into());}
    for m in &mut plan.macros {
        *m=native::macro_record(m.slot,&m.name,m.mode,&m.events,if m.raw.is_empty(){None}else{Some(&m.raw)}).map_err(|e|e.to_string())?;
    }
    validate(plan)?;
    profile.levels=profile_levels(plan);
    profile.active_level=usize::from(plan.active);
    profile.report_rate_hz=rate_hz(plan.wireless_rate);
    crate::profile_transfer::validate(&profile)?;
    Ok(profile)
}
pub(crate) fn read_explicit_file(path:&std::path::Path)->Result<Vec<u8>> {
    use std::io::Read;
    const MAX:u64=8*1024*1024;
    let file=std::fs::File::open(path).map_err(|e|format!("无法读取所选配置文件：{e}"))?;
    let meta=file.metadata().map_err(|e|e.to_string())?;
    if !meta.is_file()||meta.len()>MAX {return Err("请选择不超过8 MiB的普通JSON文件".into());}
    let mut bytes=Vec::new();
    file.take(MAX+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    if bytes.len() as u64>MAX {return Err("配置文件超过8 MiB".into());}
    Ok(bytes)
}
/// Only user-selected files. No discovery of unrelated OEM/browser databases.
pub fn prepare_file(path:&std::path::Path,library:Option<&std::path::Path>)->Result<crate::profiles::Profile> {
    let bytes=read_explicit_file(path)?;
    let library=library.map(read_explicit_file).transpose()?;
    to_profile(parse_bytes(&bytes,library.as_deref())?)
}

// Preserve ambiguity as an error instead of letting a JSON map silently use
// the last duplicate setting. This also covers nested macro events.
pub(crate) struct StrictValue(pub(crate) Value);
impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = StrictValue;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("unambiguous JSON")
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                v: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::Bool(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::from(v)))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::from(v)))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| StrictValue(Value::Number(n)))
                    .ok_or_else(|| E::custom("nonfinite JSON number"))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                v: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::String(v.into())))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                v: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::String(v)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(StrictValue(v)) = a.next_element()? {
                    out.push(v)
                }
                Ok(StrictValue(Value::Array(out)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut out = Map::new();
                while let Some(key) = a.next_key::<String>()? {
                    if out.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON field {key}"
                        )));
                    }
                    let StrictValue(v) = a.next_value()?;
                    out.insert(key, v);
                }
                Ok(StrictValue(Value::Object(out)))
            }
        }
        d.deserialize_any(Visitor)
    }
}
pub fn parse_bytes(template: &[u8], library: Option<&[u8]>) -> Result<Plan> {
    if template.len() > 8 * 1024 * 1024 || library.is_some_and(|b| b.len() > 8 * 1024 * 1024) {
        return Err("M HUB 配置或宏库超过8 MB".into());
    }
    let StrictValue(template) =
        serde_json::from_slice(template).map_err(|e| format!("原厂配置 JSON 无效：{e}"))?;
    let library = match library {
        Some(bytes) => {
            serde_json::from_slice::<StrictValue>(bytes)
                .map_err(|e| format!("宏库 JSON 无效：{e}"))?
                .0
        }
        None => Value::Array(vec![]),
    };
    parse(&template, &library)
}

fn object<'a>(v: &'a Value, field: &str) -> Result<&'a Map<String, Value>> {
    v.as_object().ok_or_else(|| format!("{field} 必须是对象"))
}
fn allowed(map: &Map<String, Value>, fields: &[&str], at: &str) -> Result<()> {
    if let Some(key) = map.keys().find(|k| !fields.contains(&k.as_str())) {
        return Err(format!(
            "{at} 包含尚未转换的字段 {key}，不能作为完整方案导入"
        ));
    }
    Ok(())
}
fn number(v: &Value, field: &str, max: u64) -> Result<u64> {
    let n = v.as_u64().or_else(|| {
        v.as_str()
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|s| s.parse().ok())
    });
    n.filter(|n| *n <= max)
        .ok_or_else(|| format!("{field} 必须是 0–{max} 的整数"))
}
fn byte(v: &Value, field: &str) -> Result<u8> {
    Ok(number(v, field, 255)? as u8)
}
fn text<'a>(v: &'a Value, field: &str) -> Result<&'a str> {
    v.as_str().ok_or_else(|| format!("{field} 必须是字符串"))
}
fn rate(v: &Value, field: &str) -> Result<u8> {
    Ok([0, 2, 3, 4, 5, 6][number(v, field, 5)? as usize])
}
fn hex(s: &str, bytes: usize, field: &str) -> Result<Vec<u8>> {
    if s.len() != bytes * 2 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{field} 必须是 {bytes} 字节的十六进制内容"));
    }
    (0..bytes)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|_| format!("{field} 无效")))
        .collect()
}
fn dpi_table(map: &Map<String, Value>, prefix: &str, required: bool) -> Result<Option<Vec<u16>>> {
    let count = (0..6)
        .filter(|i| map.contains_key(&format!("{prefix}{i}")))
        .count();
    if count == 0 && !required {
        return Ok(None);
    }
    if count != 6 {
        return Err(format!("{prefix} 必须完整提供六档，不能补造缺失值"));
    }
    (0..6)
        .map(|i| {
            number(&map[&format!("{prefix}{i}")], prefix, 50000).and_then(|v| {
                if v < 50 || v % 50 != 0 {
                    Err(format!("{prefix} 的 DPI 必须为 50–50000，步长 50"))
                } else {
                    Ok(v as u16)
                }
            })
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

fn usage(kind: &str, value: &Value) -> Result<u8> {
    static TABLES: OnceLock<Value> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        serde_json::from_str(include_str!("mhub-key-usages.json"))
            .expect("checked interoperability facts")
    });
    let key = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) if n.is_u64() => n.to_string(),
        _ => return Err("宏事件代码类型无效".into()),
    };
    tables[kind][&key]
        .as_u64()
        .and_then(|v| u8::try_from(v).ok())
        .ok_or_else(|| format!("原厂 {kind} 键表未列出 {key}"))
}
fn macro_body(slot: u8, record: &Value) -> Result<MouseMacro> {
    let m = object(record, "宏记录")?;
    allowed(
        m,
        &[
            "name",
            "type",
            "delay",
            "info",
            "uid",
            "cid",
            "id",
            "source",
            "updateTime",
            "orderNum",
            "originName",
        ],
        "宏记录",
    )?;
    let name = text(m.get("name").unwrap_or(&Value::Null), "宏名称")?;
    let mode = number(m.get("type").unwrap_or(&Value::Null), "宏播放方式", 3)? as u8;
    let info = object(m.get("info").unwrap_or(&Value::Null), "宏 info")?;
    allowed(info, &["keyList"], "宏 info")?;
    let list = info
        .get("keyList")
        .and_then(Value::as_array)
        .ok_or("宏缺少 keyList 正文")?;
    if list.is_empty() || list.len() > 1000 {
        return Err("宏事件需为 1–1000 条".into());
    }
    let mut events = Vec::new();
    for entry in list {
        let e = object(entry, "宏事件")?;
        allowed(e, &["press", "type", "val", "hover"], "宏事件")?;
        let kind = text(e.get("type").unwrap_or(&Value::Null), "宏事件类型")?;
        let value = e.get("val").ok_or("宏事件缺少 val")?;
        let press = e
            .get("press")
            .filter(|v| !v.is_null())
            .map(|v| text(v, "press"))
            .transpose()?;
        if press.is_some_and(|v| !["down", "up"].contains(&v)) {
            return Err("宏 press 必须为 down、up 或 null".into());
        }
        let down = press == Some("down");
        let event = if kind == "time" {
            if press.is_some() {
                return Err("原厂延时事件含非空 press，不能无损转换".into());
            }
            MacroEvent {
                kind: "delay".into(),
                code: 0,
                down: false,
                delay_ms: number(value, "宏延时", 65535)? as u16,
            }
        } else {
            if !["key", "mouse", "mousescroll"].contains(&kind) {
                return Err(format!("尚未转换的宏事件 {kind}"));
            }
            if kind != "mousescroll" && press.is_none() {
                return Err("键盘/鼠标宏事件缺少按下或释放状态".into());
            }
            MacroEvent {
                kind: if kind == "mousescroll" {
                    "scroll"
                } else {
                    kind
                }
                .into(),
                code: usage(kind, value)?,
                down,
                delay_ms: 0,
            }
        };
        events.push(event);
    }
    native::macro_record(slot, name, mode, &events, None).map_err(|e| e.to_string())
}

/// `library` is the explicit M HUB macroListM7 JSON array, not a name-only
/// lookup on a newly connected mouse. Importing does not access any HID target.
pub fn parse(doc: &Value, library: &Value) -> Result<Plan> {
    let root = object(doc, "M HUB 导出文件")?;
    if root.len() != 1 || !root.contains_key(PRODUCT) {
        return Err("需要仅含 MCHOSE A7 V3 Ultra+ 的原厂 JSON 导出；其他型号不能套用".into());
    }
    let m = object(&root[PRODUCT], "原厂配置")?;
    let mut fields = vec![
        "name",
        "profileIndex",
        "usbDpiIndex",
        "usbRateIndex",
        "gRateIndex",
        "sleep",
        "sleepMode",
        "leftKeyDebounce",
        "rightKeyDebounce",
        "sensor",
        "rotateVal",
        "dpiHasY",
        "dpiSum",
        "lodIndex",
        "macroInfo",
        "isSupportDpiKey",
        "deviceName",
    ];
    let dynamic = (0..6)
        .flat_map(|i| [format!("dpi{i}"), format!("dpiY{i}")])
        .chain(
            BUTTONS
                .into_iter()
                .flat_map(|b| [format!("{b}KeyType"), format!("{b}KeyVal")]),
        )
        .collect::<Vec<_>>();
    fields.extend(dynamic.iter().map(String::as_str));
    allowed(m, &fields, "原厂配置")?;
    let get = |key: &str| m.get(key).ok_or_else(|| format!("完整配置缺少 {key}"));
    if let Some(device) = m.get("deviceName") {
        if text(device, "deviceName")?.replace(' ', "") != PRODUCT.replace(' ', "") {
            return Err("配置内部设备名称不匹配".into());
        }
    }
    let name = text(get("name")?, "配置名称")?.to_owned();
    let board = number(get("profileIndex")?, "板载组", 2)? as u8;
    let independent = number(get("dpiHasY")?, "独立 Y", 1)? == 1;
    let mut buttons = Vec::new();
    let mut refs = BTreeSet::new();
    for button in BUTTONS {
        let kt = format!("{button}KeyType");
        let kv = format!("{button}KeyVal");
        if button == "dpi"
            && !m.contains_key(&kt)
            && !m.contains_key(&kv)
            && m.get("isSupportDpiKey") == Some(&Value::Bool(false))
        {
            buttons.push(vec![255; 3]);
            continue;
        }
        let kind = hex(text(get(&kt)?, &kt)?, 1, &kt)?[0];
        let value = text(get(&kv)?, &kv)?;
        if kind == 255 {
            if button != "dpi" || !value.is_empty() {
                return Err("不支持的按键能力标记".into());
            }
            buttons.push(vec![255; 3]);
            continue;
        }
        let bytes = if [1, 0x22].contains(&kind) {
            3
        } else if [0x23, 0x24].contains(&kind) {
            7
        } else {
            2
        };
        let mut data = hex(value, bytes, &kv)?;
        if ![0x13, 0x16].contains(&kind) {
            data.reverse();
        }
        data.insert(0, kind);
        native::validate_binding(&data).map_err(|e| format!("{button}: {e}"))?;
        if kind == 0x21 {
            refs.insert(data[1]);
        }
        buttons.push(data);
    }
    if let Some(flag) = m.get("isSupportDpiKey") {
        let flag = flag.as_bool().ok_or("isSupportDpiKey 必须是布尔值")?;
        if flag != (buttons[5] != [255; 3]) {
            return Err("DPI 键能力标记与映射不一致".into());
        }
    }
    let info = m
        .get("macroInfo")
        .map(|v| object(v, "macroInfo"))
        .transpose()?;
    let mut names = BTreeMap::new();
    if let Some(info) = info {
        for (key, value) in info {
            let bytes = hex(key, 2, "宏槽引用")?;
            let slot = u16::from_be_bytes([bytes[0], bytes[1]]);
            if slot >= 18
                || names
                    .insert(slot as u8, text(value, "宏引用名称")?)
                    .is_some()
            {
                return Err("宏槽引用越界或重复".into());
            }
        }
    }
    if names.keys().copied().collect::<BTreeSet<_>>() != refs {
        return Err("宏槽名称引用与按键不完整对应，不能补造或丢弃宏".into());
    }
    let library = library
        .as_array()
        .ok_or("宏库必须是 M HUB macroListM7 的 JSON 数组")?;
    let mut macros = Vec::new();
    for (slot, name) in names {
        let matches = library
            .iter()
            .filter(|m| m.get("name").and_then(Value::as_str) == Some(name))
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(format!(
                "宏“{name}”缺少正文或同名不唯一；模板只有名称引用，需要明确的宏库内容"
            ));
        }
        macros.push(macro_body(slot, matches[0])?);
    }
    let plan = Plan {
        vendor: crate::mchose::VID,
        model: crate::mchose::MODEL_PID,
        board,
        name,
        dpi_x: dpi_table(m, "dpi", true)?.unwrap(),
        dpi_y: dpi_table(m, "dpiY", independent)?,
        count: number(get("dpiSum")?, "启用 DPI 档数", 6)? as u8,
        active: number(get("usbDpiIndex")?, "活动 DPI 档", 5)? as u8,
        independent,
        usb_rate: rate(get("usbRateIndex")?, "USB 回报率索引")?,
        wireless_rate: rate(get("gRateIndex")?, "无线回报率索引")?,
        sleep: number(get("sleep")?, "休眠分钟", 60)? as u8,
        never_sleep: number(get("sleepMode")?, "永不休眠", 1)? == 1,
        left_debounce: number(get("leftKeyDebounce")?, "左键消抖", 50)? as u8,
        right_debounce: number(get("rightKeyDebounce")?, "右键消抖", 50)? as u8,
        sensor: byte(get("sensor")?, "传感器")?,
        rotation: i16::from(byte(get("rotateVal")?, "旋转原始值")? as i8) * 4,
        lod: number(get("lodIndex")?, "LOD", 4)? as u8,
        buttons,
        macros,
    };
    validate(&plan)?;
    Ok(plan)
}

pub fn validate(plan: &Plan) -> Result<()> {
    if plan.vendor != crate::mchose::VID || plan.model != crate::mchose::MODEL_PID || plan.board > 2
    {
        return Err("完整方案的品牌、型号或板载组不匹配".into());
    }
    if plan.name.trim().is_empty() || plan.name.len() > 60 || plan.name.contains('\0') {
        return Err("配置名称需为 1–60 UTF-8 字节".into());
    }
    for values in [Some(&plan.dpi_x), plan.dpi_y.as_ref()]
        .into_iter()
        .flatten()
    {
        if values.len() != 6
            || values
                .iter()
                .any(|v| !(50..=50000).contains(v) || v % 50 != 0)
        {
            return Err("完整方案需要六档有效 DPI".into());
        }
    }
    if plan.independent && plan.dpi_y.is_none()
        || !(1..=6).contains(&plan.count)
        || plan.active >= plan.count
        || plan.sleep > 60
        || plan.left_debounce > 50
        || plan.right_debounce > 50
        || plan.sensor & 3 > 2
        || !(-120..=120).contains(&plan.rotation)
        || plan.rotation % 4 != 0
        || plan.lod > 4
        || ![0, 2, 3, 4, 5, 6].contains(&plan.usb_rate)
        || ![0, 2, 3, 4, 5, 6].contains(&plan.wireless_rate)
    {
        return Err("完整方案的 DPI、回报率或性能参数无效".into());
    }
    if plan.buttons.len() != 6 || !plan.buttons.iter().any(|b| b == &[0, 1, 0]) {
        return Err("完整方案必须保留一个普通左键".into());
    }
    let mut refs = BTreeSet::new();
    for (index, b) in plan.buttons.iter().enumerate() {
        if index == 5 && b == &[255; 3] {
            continue;
        }
        native::validate_binding(b).map_err(|e| e.to_string())?;
        if b[0] == 0x21 {
            refs.insert(b[1]);
        }
    }
    let mut slots = BTreeSet::new();
    for m in &plan.macros {
        if !slots.insert(m.slot) {
            return Err("完整方案宏槽重复".into());
        }
        let checked = native::macro_record(m.slot, &m.name, m.mode, &m.events, Some(&m.raw))
            .map_err(|e| e.to_string())?;
        if checked != *m {
            return Err("宏原始内容与字段不一致".into());
        }
    }
    if slots != refs {
        return Err("完整方案的宏正文与按键引用不一致".into());
    }
    Ok(())
}

/// Compile against one freshly read board plus all three boards' key tables.
/// The native caller must bind these reads and every write to its live target.
pub fn compile(plan: &Plan, current: &Snapshot, boards: &[Vec<Vec<u8>>; 3]) -> Result<Compiled> {
    validate(plan)?;
    native::validate_snapshot(current).map_err(|e| e.to_string())?;
    let board = plan.board as usize;
    if current.state.profile != plan.board || boards[board] != current.state.buttons {
        return Err("目标板载组或按键已改变，需要重新读取".into());
    }
    if plan.sensor & 0x60 != current.state.sensor & 0x60 {
        return Err("文件与设备的传感器保留位不一致，不能覆盖未知字段".into());
    }
    for (a, b) in plan.buttons.iter().zip(&current.state.buttons) {
        if (a == &[255; 3]) != (b == &[255; 3]) {
            return Err("源文件与设备的可编程按键能力不一致".into());
        }
    }
    let mut protected = BTreeSet::new();
    let mut exclusive = BTreeSet::new();
    for (index, keys) in boards.iter().enumerate() {
        if keys.len() != 6
            || native::parse_buttons(&keys.concat()).map_err(|e| e.to_string())? != *keys
        {
            return Err("三组按键快照不完整".into());
        }
        for key in keys {
            // An unrecognized other-board action may conceal a macro reference.
            // Reuse identical content if possible, but never allocate/overwrite
            // any slot while that reference cannot be decoded.
            if index!=board && key!=&[255;3] && native::validate_binding(key).is_err() {
                protected.extend(0..18);
            }
            if key.first() == Some(&0x21) {
                if key.len() != 3 || key[1] >= 18 || key[2] != 0 {
                    return Err("设备含未知宏槽引用，不能覆盖".into());
                }
                if index == board {
                    exclusive.insert(key[1]);
                } else {
                    protected.insert(key[1]);
                }
            }
        }
    }
    let mut records = current
        .macros
        .iter()
        .map(|m| (m.slot, m.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut occupied = records.keys().copied().collect::<BTreeSet<_>>();
    occupied.extend(protected.iter().copied());
    let mut slot_map = BTreeMap::new();
    let mut writes = Vec::new();
    let mut claimed = BTreeSet::<u8>::new();
    for source in &plan.macros {
        let equal = records
            .values()
            .find(|m| m.raw == source.raw)
            .map(|m| m.slot);
        let target = equal
            .or_else(|| (0..18).find(|slot| !occupied.contains(slot) && !claimed.contains(slot)))
            .or_else(|| {
                exclusive
                    .iter()
                    .copied()
                    .find(|slot| !protected.contains(slot) && !claimed.contains(slot))
            })
            .ok_or("没有可用宏槽；其他板载组引用和未分配的现有宏不能覆盖")?;
        slot_map.insert(source.slot, target);
        claimed.insert(target);
        occupied.insert(target);
        if equal.is_none() {
            let mut record = source.clone();
            record.slot = target;
            records.insert(target, record.clone());
            writes.push(record);
        }
    }
    let mut state = current.state.clone();
    state.names[board] = plan.name.clone();
    state.dpi_x = plan.dpi_x.clone();
    if plan.independent {
        state.dpi_y = plan.dpi_y.clone().ok_or("独立 Y 缺少六档")?;
    }
    state.count = plan.count;
    state.active = plan.active;
    state.independent = plan.independent;
    state.sleep = plan.sleep;
    state.never_sleep = plan.never_sleep;
    state.left_debounce = plan.left_debounce;
    state.right_debounce = plan.right_debounce;
    state.sensor = plan.sensor;
    state.rotation = plan.rotation;
    state.lod = plan.lod;
    state.raw_settings[1] = (plan.usb_rate << 4) | plan.active;
    state.raw_settings[2] = (plan.wireless_rate << 4) | plan.active;
    state.raw_settings[3] = plan.sleep;
    state.raw_settings[4] = (state.raw_settings[4] & !1) | u8::from(plan.never_sleep);
    state.raw_settings[5] = plan.sensor;
    state.raw_settings[6] = (plan.rotation / 4) as i8 as u8;
    state.raw_settings[7] = plan.left_debounce;
    state.raw_settings[8] = plan.right_debounce;
    for (axis, values) in [(0, &state.dpi_x), (1, &state.dpi_y)] {
        let mut table = vec![
            plan.board,
            axis,
            plan.count,
            plan.active,
            u8::from(plan.independent),
        ];
        for value in values {
            table.extend_from_slice(&value.to_le_bytes());
        }
        if axis == 0 {
            state.raw_x = table;
        } else {
            state.raw_y = table;
        }
    }
    state.buttons = plan.buttons.clone();
    for key in &mut state.buttons {
        if key[0] == 0x21 {
            key[1] = *slot_map.get(&key[1]).ok_or("宏槽映射缺失")?;
        }
    }
    state.raw_buttons = state.buttons.concat();
    state.adaptive.clear();
    let snapshot = Snapshot {
        format: native::SNAPSHOT_FORMAT.into(),
        model: "3837:4026".into(),
        state,
        macros: records.into_values().collect(),
    };
    native::validate_snapshot(&snapshot).map_err(|e| e.to_string())?;
    Ok(Compiled {
        snapshot,
        macro_writes: writes,
        slot_map,
    })
}

#[cfg(test)]
#[path = "mhub_full_tests.rs"]
mod tests;
