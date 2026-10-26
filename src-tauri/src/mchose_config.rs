//! Full V3 settings. Command payloads are typed and validated; firmware/DFU
//! commands are deliberately absent. Source: public M HUB wire layout.
use crate::hidpp::{Error, Result};
use crate::mchose::Transport;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const SNAPSHOT_FORMAT: &str = "mchose-v3-full-v3";
const LEGACY_SNAPSHOT_FORMAT: &str = "mchose-v3-full-v2";

fn bad(s: impl Into<String>) -> Error {
    Error::Other(s.into())
}
fn profile(p: u8) -> Result<()> {
    if p > 2 {
        Err(bad("板载配置必须为 1–3"))
    } else {
        Ok(())
    }
}
fn rate(r: u8) -> Result<()> {
    if [0, 2, 3, 4, 5, 6].contains(&r) {
        Ok(())
    } else {
        Err(bad("不支持的回报率"))
    }
}
fn dpi_values(v: &[u16]) -> Result<()> {
    if v.len() != 6 || v.iter().any(|v| !(50..=50000).contains(v) || v % 50 != 0) {
        Err(bad("需要六个 50–50000、步长 50 的 DPI 值"))
    } else {
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct HardwareState {
    pub profile: u8,
    pub names: Vec<String>,
    pub dpi_x: Vec<u16>,
    pub dpi_y: Vec<u16>,
    pub count: u8,
    pub active: u8,
    pub independent: bool,
    pub sleep: u8,
    pub never_sleep: bool,
    pub left_debounce: u8,
    pub right_debounce: u8,
    pub sensor: u8,
    pub rotation: i16,
    pub lod: u8,
    pub buttons: Vec<Vec<u8>>,
    /// Legacy diagnostic bytes only. A7 has no OEM adaptive-rate capability.
    /// Current views/backups use an empty vector; v2 migration validates it.
    pub adaptive: Vec<u8>,
    pub firmware: String,
    pub receiver_firmware: String,
    pub raw_settings: Vec<u8>,
    pub raw_x: Vec<u8>,
    pub raw_y: Vec<u8>,
    pub raw_buttons: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MacroEvent {
    pub kind: String,
    pub code: u8,
    pub down: bool,
    pub delay_ms: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MouseMacro {
    pub slot: u8,
    pub name: String,
    pub mode: u8,
    pub events: Vec<MacroEvent>,
    pub raw: Vec<u8>,
}
#[derive(Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub format: String,
    pub model: String,
    pub state: HardwareState,
    pub macros: Vec<MouseMacro>,
}

pub(crate) fn validate_snapshot(s: &Snapshot) -> Result<()> {
    if ![SNAPSHOT_FORMAT, LEGACY_SNAPSHOT_FORMAT].contains(&s.format.as_str()) || s.model != "3837:4026" {
        return Err(bad("备份型号或版本不匹配"));
    }
    let t = &s.state;
    // The authenticated OEM capability map excludes A7 from adaptive rate.
    // Do not silently accept an enabled/unknown legacy value that we cannot
    // restore. Only the captured inert v2 diagnostic record may migrate.
    if !t.adaptive.is_empty()
        && (s.format != LEGACY_SNAPSHOT_FORMAT || t.adaptive != [0, 0, 3, 0])
    {
        return Err(bad("A7 V3 Ultra+ 未开放自动省电回报率；备份含不能恢复的非默认省电字段"));
    }
    profile(t.profile)?;
    dpi_values(&t.dpi_x)?;
    dpi_values(&t.dpi_y)?;
    if t.names.len() != 3
        || t.names.iter().any(|n| n.len() > 60 || n.contains('\0'))
        || !(1..=6).contains(&t.count)
        || t.active >= t.count
        || t.lod > 4
        || t.buttons.len() != 6
        || t.raw_settings.len() != 19
        || t.raw_x.len() != 17
        || t.raw_y.len() != 17
    {
        return Err(bad("备份结构或参数无效"));
    }
    let r = &t.raw_settings;
    if r[0] != t.profile
        || r[3] != t.sleep
        || r[4] & 1 != t.never_sleep as u8
        || r[5] != t.sensor
        || i16::from(r[6] as i8) * 4 != t.rotation
        || r[7] != t.left_debounce
        || r[8] != t.right_debounce
        || t.sleep > 60
        || t.left_debounce > 50
        || t.right_debounce > 50
        || t.sensor & 3 > 2
        || !(-120..=120).contains(&t.rotation)
    {
        return Err(bad("备份的原始参数与显示参数不一致"));
    }
    rate(r[1] >> 4)?;
    rate(r[2] >> 4)?;
    for (axis, raw, values) in [(0, &t.raw_x, &t.dpi_x), (1, &t.raw_y, &t.dpi_y)] {
        let mut expected = vec![t.profile, axis, t.count, t.active, t.independent as u8];
        for v in values {
            expected.extend_from_slice(&v.to_le_bytes())
        }
        if *raw != expected {
            return Err(bad("备份 DPI 表不一致"));
        }
    }
    if parse_buttons(&t.raw_buttons)? != t.buttons {
        return Err(bad("备份按键表不一致"));
    }
    let mut slots = BTreeSet::new();
    for m in &s.macros {
        if m.slot >= 18 || !slots.insert(m.slot) {
            return Err(bad("备份宏槽重复或越界"));
        }
        let parsed = decode_macro(m.slot, m.raw.clone())?;
        if &parsed != m {
            return Err(bad("备份宏数据不一致"));
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    ReportRate { value: u8 },
    Settings {
        sleep: u8,
        never_sleep: bool,
        left_debounce: u8,
        right_debounce: u8,
        sensor: u8,
        rotation: i16,
    },
    Dpi {
        x: Vec<u16>,
        y: Vec<u16>,
        count: u8,
        active: u8,
        independent: bool,
    },
    Lod {
        index: u8,
    },
    Button {
        index: u8,
        binding: Vec<u8>,
    },
    Profile {
        index: u8,
    },
    Rename {
        name: String,
    },
    Macro {
        slot: u8,
        name: String,
        mode: u8,
        events: Vec<MacroEvent>,
        #[serde(default)]
        raw: Option<Vec<u8>>,
    },
    DeleteMacro {
        slot: u8,
    },
}

pub fn parse_buttons(raw: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut at = 0;
    let mut out = Vec::new();
    for _ in 0..6 {
        let t = *raw.get(at).ok_or_else(|| bad("按键回复被截断"))?;
        let n = match t {
            1 | 0x22 => 4,
            0x23 | 0x24 => 8,
            _ => 3,
        };
        if at + n > raw.len() {
            return Err(bad("按键回复长度错误"));
        }
        out.push(raw[at..at + n].to_vec());
        at += n;
    }
    Ok(out)
}
pub(crate) fn validate_binding(b: &[u8]) -> Result<()> {
    let valid = match b.first().copied() {
        Some(0) => b.len() == 3 && b[2] == 0 && [1, 2, 4, 8, 16].contains(&b[1]),
        Some(5) => b.len() == 3 && b[2] == 0 && b[1] <= 1,
        Some(0x11) => b.len() == 3 && b[2] == 0 && b[1] > 0,
        Some(0x12) => b.len() == 3 && (1..=0x0f).contains(&b[1]) && b[2] == 0,
        Some(0x13) => {
            // The official 4D picker uses four generic modifiers and the
            // qM main-key set; modifier usages and reserved gaps are excluded.
            b.len() == 3
                && (1..=0x0f).contains(&b[1])
                && matches!(b[2], 0x04..=0x31 | 0x33..=0x63 | 0x65)
        }
        Some(0x14) => b.len() == 3,
        Some(0x16) => b.len() == 3 && b[2] > 0,
        Some(1) => b.len() == 4 && b[2] == 0 && b[3] == 0 && [0, 2, 3].contains(&b[1]),
        Some(0x21) => b.len() == 3 && b[1] < 18 && b[2] == 0,
        Some(0x33) => b.len() == 3 && b[2] == 0 && [0, 1, 2, 255].contains(&b[1]),
        Some(0xfe) => b == [0xfe, 0, 0],
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(bad("不支持或不完整的按键动作"))
    }
}
pub fn encode_macro(name: &str, mode: u8, events: &[MacroEvent]) -> Result<Vec<u8>> {
    if name.trim().is_empty()
        || name.len() > 56
        || name.contains('\0')
        || mode > 3
        || events.is_empty()
        || events.len() > 1000
    {
        return Err(bad("宏名称为 1–56 UTF-8 字节，模式 0–3，事件 1–1000 条"));
    }
    let mut held = BTreeSet::new();
    let mut raw = vec![0u8; 64];
    let size = (events.len() * 4) as u16;
    raw[..2].copy_from_slice(&size.to_le_bytes());
    raw[4] = mode;
    raw[8..8 + name.len()].copy_from_slice(name.as_bytes());
    for (i, e) in events.iter().enumerate() {
        if e.kind != "delay" && e.delay_ms != 0 || e.kind == "delay" && e.code != 0 {
            return Err(bad(
                "此事件含未解释的原始字段，不能将其清零；请保留原始宏或使用独立 delay 事件",
            ));
        }
        let kind = match e.kind.as_str() {
            "key" => 17,
            "mouse" => 0,
            "scroll" => 5,
            "delay" => 63,
            _ => return Err(bad("未知宏事件类型")),
        };
        if kind == 17 && e.code == 0
            || kind == 0 && ![1, 2, 4, 8, 16].contains(&e.code)
            || kind == 5 && e.code > 1
        {
            return Err(bad("宏按键代码无效"));
        }
        if kind == 17 || kind == 0 {
            let key = (kind, e.code);
            if e.down {
                if !held.insert(key) {
                    return Err(bad("宏中重复按下同一按键"));
                }
            } else if !held.remove(&key) {
                return Err(bad("宏释放了尚未按下的按键"));
            }
        }
        let delay = if kind == 63 { e.delay_ms } else { 0 };
        raw.extend_from_slice(&[
            delay as u8,
            (delay >> 8) as u8,
            (if i + 1 == events.len() { 128 } else { 0 }) | if e.down { 64 } else { 0 } | kind,
            if kind == 63 { 0 } else { e.code },
        ]);
    }
    if !held.is_empty() {
        return Err(bad("宏必须释放所有按下的键"));
    }
    Ok(raw)
}

/// `raw` is the content baseline of an imported/read macro, not an identity
/// claim about the target slot. Preserve opaque records exactly when unchanged;
/// only the independently verified canonical representation may be edited.
pub fn macro_record(
    slot: u8,
    name: &str,
    mode: u8,
    events: &[MacroEvent],
    raw: Option<&[u8]>,
) -> Result<MouseMacro> {
    if slot >= 18 {
        return Err(bad("宏槽无效"));
    }
    if events.is_empty() {
        return Err(bad("空宏不能通过保存删除；请使用删除空闲宏槽并核对引用"));
    }
    if let Some(raw) = raw {
        let original = decode_macro(slot, raw.to_vec())?;
        if original.name == name && original.mode == mode && original.events == events {
            return Ok(original);
        }
        if encode_macro(&original.name, original.mode, &original.events)
            .ok()
            .as_deref()
            != Some(raw)
        {
            return Err(bad(
                "原始宏包含未解释的字段；可原样保存或导出，暂不能无损修改，请保留原文件",
            ));
        }
    }
    decode_macro(slot, encode_macro(name, mode, events)?)
}

#[derive(Serialize)]
pub struct ApplyResult {
    pub target: crate::mchose_target::Target,
    #[serde(flatten)]
    pub state: HardwareState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_macro: Option<MouseMacro>,
}
fn decode_macro(slot: u8, raw: Vec<u8>) -> Result<MouseMacro> {
    if raw.len() < 64 {
        return Err(bad("宏头不完整"));
    }
    let size = u16::from_le_bytes([raw[0], raw[1]]) as usize;
    if size % 4 != 0 || size + 64 != raw.len() || raw.len() > 4096 {
        return Err(bad("宏数据长度错误"));
    }
    let name = String::from_utf8_lossy(&raw[8..64])
        .trim_end_matches('\0')
        .to_string();
    let mut events = Vec::new();
    for b in raw[64..].chunks_exact(4) {
        let kind = match b[2] & 63 {
            17 => "key",
            0 => "mouse",
            5 => "scroll",
            63 => "delay",
            _ => return Err(bad("宏含尚不支持的事件类型")),
        };
        events.push(MacroEvent {
            kind: kind.into(),
            code: b[3],
            down: b[2] & 64 != 0,
            delay_ms: u16::from_le_bytes([b[0], b[1]]),
        });
    }
    Ok(MouseMacro {
        slot,
        name,
        mode: raw[4],
        events,
        raw,
    })
}

impl Transport {
    pub fn hardware_self_test(&self) -> Result<serde_json::Value> {
        let before = self.full_snapshot()?;
        let original_macros = before.macros.clone();
        let s = before.state.clone();
        let free = (0..18).find(|slot| !before.macros.iter().any(|m| m.slot == *slot));
        let mut macro_touched = false;
        let mut tested = Vec::new();
        let trial = (|| -> Result<()> {
            let mut x = s.dpi_x.clone();
            let y = s.dpi_y.clone();
            x[s.active as usize] = if x[s.active as usize] < 50000 {
                x[s.active as usize] + 50
            } else {
                x[s.active as usize] - 50
            };
            self.apply_hardware_for(
                s.profile,
                Action::Dpi {
                    x,
                    y,
                    count: s.count,
                    active: s.active,
                    independent: true,
                },
            )?;
            tested.push("six-stage DPI and independent X/Y");
            self.apply_hardware_for(
                s.profile,
                Action::Lod {
                    index: (s.lod + 1) % 5,
                },
            )?;
            tested.push("LOD");
            self.apply_hardware_for(
                s.profile,
                Action::Settings {
                    sleep: if s.sleep == 3 { 4 } else { 3 },
                    never_sleep: s.never_sleep,
                    left_debounce: if s.left_debounce == 8 { 9 } else { 8 },
                    right_debounce: s.right_debounce,
                    sensor: s.sensor ^ 4,
                    rotation: s.rotation,
                },
            )?;
            tested.push("sleep, debounce and sensor flags");
            self.apply_hardware_for(
                s.profile,
                Action::Button {
                    index: 4,
                    binding: vec![0, 8, 0],
                },
            )?;
            tested.push("button remapping");
            self.apply_hardware_for(
                s.profile,
                Action::Rename {
                    name: "Open HUB test".into(),
                },
            )?;
            tested.push("board rename");
            self.apply_hardware_for(
                s.profile,
                Action::Profile {
                    index: (s.profile + 1) % 3,
                },
            )?;
            self.apply_hardware_for((s.profile + 1) % 3, Action::Profile { index: s.profile })?;
            tested.push("board selection");
            if let Some(slot) = free {
                macro_touched = true;
                self.apply_hardware_for(
                    s.profile,
                    Action::Macro {
                        slot,
                        name: "Open HUB test".into(),
                        mode: 0,
                        raw: None,
                        events: vec![
                            MacroEvent {
                                kind: "key".into(),
                                code: 4,
                                down: true,
                                delay_ms: 0,
                            },
                            MacroEvent {
                                kind: "delay".into(),
                                code: 0,
                                down: false,
                                delay_ms: 20,
                            },
                            MacroEvent {
                                kind: "key".into(),
                                code: 4,
                                down: false,
                                delay_ms: 0,
                            },
                        ],
                    },
                )?;
                self.apply_hardware_for(s.profile, Action::DeleteMacro { slot })?;
                macro_touched = false;
                tested.push("hardware macro write, read and delete");
            }
            Ok(())
        })();
        let mut cleanup_error = None;
        if macro_touched {
            if let Some(slot) = free {
                if let Err(e) = self.write_macro_raw(slot, &[0u8; 64]) {
                    cleanup_error = Some(e.to_string());
                }
            }
        }
        // Return to the original board even if the trial stopped on another board.
        if self.settings()?[0] != s.profile {
            self.request(0x0906, &[s.profile])?;
        }
        self.restore_snapshot_inner(before, false)?;
        let after = self.full_snapshot()?;
        if after.state != s || after.macros != original_macros {
            return Err(bad("完整测试后配置尚未完全还原，请检查备份"));
        }
        if let Some(e) = cleanup_error {
            return Err(bad(format!("宏清理未确认：{e}")));
        }
        trial?;
        Ok(
            serde_json::json!({"passed":true,"tested":tested,"restored":true,"dpi":s.dpi_x[s.active as usize]}),
        )
    }
    fn read_name(&self, p: u8) -> Result<String> {
        let mut bytes = Vec::new();
        for o in (0..60u8).step_by(23) {
            let n = (60 - o).min(23);
            let b = self.request(0x0905, &[p, o, n])?;
            if b.len() < usize::from(n) {
                return Err(bad("配置名称回复不完整"));
            }
            bytes.extend_from_slice(&b[..usize::from(n)]);
        }
        Ok(String::from_utf8_lossy(&bytes)
            .trim_end_matches('\0')
            .to_string())
    }
    pub fn hardware_state(&self) -> Result<HardwareState> {
        self.identity()?;
        let cfg = self.settings()?;
        let p = cfg[0];
        let x = self.dpi_table(p, 0)?;
        let y = self.dpi_table(p, 1)?;
        let raw_buttons = self.request(1, &[p, 0, 6])?;
        let buttons = parse_buttons(&raw_buttons)?;
        let lod = self.request(9, &[p])?;
        if lod.len() != 1 || lod[0] > 4 {
            return Err(bad("LOD 回复无效"));
        }
        // OEM all-info/UI/restore gates enable 000A/010A/010B only for the
        // allowlisted K5 models. Do not require an unsupported GET on A7.
        // See development/mchose-adaptive-capability-official-facts.json.
        let adaptive = Vec::new();
        let mut names = Vec::new();
        for p in 0..3 {
            names.push(self.read_name(p)?)
        }
        let firmware = |b: Vec<u8>| -> String {
            if b.len() < 2 {
                "未知".into()
            } else {
                format!("{:02x}{:02x}", b[0], b[1])
                    .trim_start_matches('0')
                    .to_string()
            }
        };
        let mouse = firmware(self.request(0x0901, &[0])?);
        let receiver = firmware(self.request(0x0901, &[1])?);
        Ok(HardwareState {
            profile: p,
            names,
            dpi_x: (0..6)
                .map(|i| u16::from_le_bytes([x[5 + 2 * i], x[6 + 2 * i]]))
                .collect(),
            dpi_y: (0..6)
                .map(|i| u16::from_le_bytes([y[5 + 2 * i], y[6 + 2 * i]]))
                .collect(),
            count: x[2],
            active: x[3],
            independent: x[4] != 0,
            sleep: cfg[3],
            never_sleep: cfg[4] & 1 != 0,
            left_debounce: cfg[7],
            right_debounce: cfg[8],
            sensor: cfg[5],
            rotation: i16::from(cfg[6] as i8) * 4,
            lod: lod[0],
            buttons,
            adaptive,
            firmware: mouse,
            receiver_firmware: receiver,
            raw_settings: cfg,
            raw_x: x,
            raw_y: y,
            raw_buttons,
        })
    }
    pub fn read_macro(&self, slot: u8) -> Result<Option<MouseMacro>> {
        if slot >= 18 {
            return Err(bad("宏槽必须为 0–17"));
        }
        let mut raw = Vec::new();
        let mut total = 64usize;
        while raw.len() < total {
            let at = raw.len();
            let n = (total - at).min(22);
            let b = self.request(0x090c, &[slot, at as u8, (at >> 8) as u8, n as u8])?;
            if b == [0] && at == 0 {
                return Ok(None);
            }
            // The leading byte is the returned data length, not a status.
            if b.len() != n + 1 || b[0] != n as u8 {
                return Err(bad("宏读取被拒绝或长度错误"));
            }
            raw.extend_from_slice(&b[1..]);
            if raw.len() >= 2 {
                let size = u16::from_le_bytes([raw[0], raw[1]]) as usize;
                if size % 4 != 0 || size > 4032 {
                    return Err(bad("宏事件长度无效"));
                }
                total = 64 + size;
            }
        }
        if raw.iter().all(|v| *v == 0) {
            return Ok(None);
        }
        Ok(Some(decode_macro(slot, raw)?))
    }
    pub fn full_snapshot(&self) -> Result<Snapshot> {
        let state = self.hardware_state()?;
        let mut macros = Vec::new();
        for slot in 0..18 {
            if let Some(m) = self.read_macro(slot)? {
                macros.push(m)
            }
        }
        Ok(Snapshot {
            format: SNAPSHOT_FORMAT.into(),
            model: "3837:4026".into(),
            state,
            macros,
        })
    }
    fn backup_full_for(&self, expected_board: u8) -> Result<()> {
        self.require_board(expected_board)?;
        let snapshot = self.full_snapshot()?;
        if snapshot.state.profile != expected_board {
            return Err(bad("板载配置已改变，请刷新重试"));
        }
        self.require_board(expected_board)?;
        self.persist_snapshot(&snapshot).map(|_| ())
    }
    pub(crate) fn persist_snapshot(&self, snapshot: &Snapshot) -> Result<std::path::PathBuf> {
        use std::io::Write;
        let dir = self.backup_directory()?;
        std::fs::create_dir_all(&dir).map_err(|e| bad(e.to_string()))?;
        let id = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| bad(e.to_string()))?.as_nanos();
        let path=dir.join(format!("full-{id}.json"));
        let data=serde_json::to_vec_pretty(snapshot).map_err(|e|bad(e.to_string()))?;
        let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e|bad(e.to_string()))?;
        file.write_all(&data).and_then(|()|file.sync_all()).map_err(|e|bad(format!("自动备份未保存完整，未开始写入鼠标：{e}")))?;
        Ok(path)
    }
    pub(crate) fn checked_settings(&self, args: &[u8]) -> Result<()> {
        self.request(0x0102, args)?;
        for _ in 0..5 {
            std::thread::sleep(Duration::from_millis(100));
            if self.settings()? == args {
                return Ok(());
            }
        }
        Err(bad("设置回读未匹配，请检查备份"))
    }
    fn write_dpi_table(
        &self,
        p: u8,
        x: &[u16],
        y: &[u16],
        count: u8,
        active: u8,
        independent: bool,
    ) -> Result<()> {
        self.write_dpi_table_checked(p,x,y,count,active,independent,||Ok(()))
    }
    pub(crate) fn write_dpi_table_checked(&self, p:u8,x:&[u16],y:&[u16],count:u8,active:u8,independent:bool,before_axis:impl Fn()->Result<()>) -> Result<()> {
        profile(p)?;
        dpi_values(x)?;
        dpi_values(y)?;
        if !(1..=6).contains(&count) || active >= count {
            return Err(bad("DPI 启用档数或当前档位无效"));
        }
        for (axis, values) in [(0, x), (1, y)] {
            before_axis()?;
            let mut args = vec![p, axis, independent as u8, count, active];
            for v in values {
                args.extend_from_slice(&v.to_le_bytes())
            }
            self.request(0x0103, &args)?;
        }
        let a = self.dpi_table(p, 0)?;
        let b = self.dpi_table(p, 1)?;
        let expected = |axis: u8, v: &[u16]| {
            let mut out = vec![p, axis, count, active, independent as u8];
            for v in v {
                out.extend_from_slice(&v.to_le_bytes())
            }
            out
        };
        if a != expected(0, x) || b != expected(1, y) {
            return Err(bad("DPI 表回读未匹配"));
        }
        Ok(())
    }
    pub(crate) fn write_binding(&self, p: u8, index: u8, b: &[u8]) -> Result<()> {
        let mut args = vec![p, index, 11];
        args.extend_from_slice(b);
        self.request(0x0101, &args)?;
        let all = parse_buttons(&self.request(1, &[p, 0, 6])?)?;
        if all.get(index as usize).map(Vec::as_slice) != Some(b) {
            return Err(bad("按键映射回读未匹配"));
        }
        Ok(())
    }
    fn write_macro_raw(&self, slot: u8, raw: &[u8]) -> Result<()> {
        self.write_macro_checked(slot,raw,||Ok(()))
    }
    pub(crate) fn write_macro_checked(&self,slot:u8,raw:&[u8],before_chunk:impl Fn()->Result<()>) -> Result<()> {
        for (i, part) in raw.chunks(19).enumerate() {
            before_chunk()?;
            let offset = i * 19;
            let flags = if offset == 0 {
                1
            } else if offset + part.len() == raw.len() {
                2
            } else {
                0
            };
            let mut a = vec![
                slot,
                offset as u8,
                (offset >> 8) as u8,
                ((part.len() as u8) << 2) | flags,
            ];
            a.extend_from_slice(part);
            self.request(0x090b, &a)?;
        }
        let now = self.read_macro(slot)?;
        if raw.iter().all(|v| *v == 0) && now.is_none() {
            return Ok(());
        }
        if now.ok_or_else(|| bad("宏写入后为空"))?.raw != raw {
            return Err(bad("宏数据回读未匹配"));
        }
        Ok(())
    }
    fn write_name(&self, p: u8, name: &str) -> Result<()> {
        self.write_name_checked(p,name,||Ok(()))
    }
    pub(crate) fn write_name_checked(&self,p:u8,name:&str,before_chunk:impl Fn()->Result<()>) -> Result<()> {
        let raw = name.as_bytes();
        if raw.len() > 60 || name.contains('\0') {
            return Err(bad("配置名称无效"));
        }
        let mut padded = [0u8; 60];
        padded[..raw.len()].copy_from_slice(raw);
        for (i, part) in padded.chunks(20).enumerate() {
            before_chunk()?;
            let mut a = vec![
                p,
                (i * 20) as u8,
                ((part.len() as u8) << 2)
                    | if i == 0 {
                        1
                    } else if i == 2 {
                        2
                    } else {
                        0
                    },
            ];
            a.extend_from_slice(part);
            self.request(0x0910, &a)?;
        }
        if self.read_name(p)? != name {
            return Err(bad("配置名称回读未匹配"));
        }
        Ok(())
    }
    pub fn apply_hardware(&self, action: Action) -> Result<HardwareState> {
        let board = self.settings()?[0];
        self.apply_hardware_for(board, action)
    }
    /// Pin a draft to the board it was read from. A physical board switch
    /// cannot be stopped by the app's command mutex and must never retarget it.
    pub fn apply_hardware_for(&self, expected_board: u8, action: Action) -> Result<HardwareState> {
        profile(expected_board)?;
        self.identity()?;
        let mut cfg = self.settings()?;
        if cfg[0] != expected_board {
            return Err(bad("板载配置已改变，请刷新重试"));
        }
        let p = expected_board;
        let expected_after = match &action {
            Action::Profile { index } => *index,
            _ => p,
        };
        // Validate before any backup or device write.
        match &action {
            Action::ReportRate { value } => rate(*value)?,
            Action::Settings {
                sleep,
                left_debounce,
                right_debounce,
                rotation,
                sensor,
                ..
            } => {
                if *sleep > 60
                    || *left_debounce > 50
                    || *right_debounce > 50
                    || !(-120..=120).contains(rotation)
                    || rotation % 4 != 0
                    || sensor & 3 > 2
                    || sensor & 0x60 != cfg[5] & 0x60
                {
                    return Err(bad(
                        "休眠 0–60 分钟，消抖 0–50 ms，角度 -120–120°、步长 4°；保留传感器保留位",
                    ));
                }
            }
            Action::Dpi {
                x,
                y,
                count,
                active,
                ..
            } => {
                dpi_values(x)?;
                dpi_values(y)?;
                if !(1..=6).contains(count) || active >= count {
                    return Err(bad("DPI 档位无效"));
                }
            }
            Action::Lod { index } => {
                if *index > 4 {
                    return Err(bad("LOD 档位无效"));
                }
            }
            Action::Button { index, binding } => {
                if *index > 5 {
                    return Err(bad("按键索引无效"));
                }
                validate_binding(binding)?;
                let mut all = parse_buttons(&self.request(1, &[p, 0, 6])?)?;
                if all[*index as usize][0] == 255 {
                    return Err(bad("这个按键不支持板载重映射"));
                }
                all[*index as usize] = binding.clone();
                if !all.iter().any(|b| b == &[0, 1, 0]) {
                    return Err(bad("至少保留一个左键"));
                }
                if binding[0] == 0x21 && self.read_macro(binding[1])?.is_none() {
                    return Err(bad("请先保存对应的宏"));
                }
            }
            Action::Profile { index } => profile(*index)?,
            Action::Rename { name } => {
                if name.len() > 60 || name.contains('\0') {
                    return Err(bad("名称最多 60 UTF-8 字节"));
                }
            }
            Action::Macro {
                slot,
                name,
                mode,
                events,
                raw,
            } => {
                if *slot >= 18 {
                    return Err(bad("宏槽无效"));
                }
                macro_record(*slot, name, *mode, events, raw.as_deref())?;
            }
            Action::DeleteMacro { slot } => {
                if *slot >= 18 {
                    return Err(bad("宏槽无效"));
                }
                for p in 0..3 {
                    let buttons = parse_buttons(&self.request(1, &[p, 0, 6])?)?;
                    if buttons.iter().any(|b| b == &[33, *slot, 0]) {
                        return Err(bad("宏仍被板载按键引用，请先解除绑定"));
                    }
                }
            }
        }
        self.backup_full_for(p)?;
        self.require_board(p)?;
        match action {
            Action::ReportRate { value } => {
                cfg[1] = (cfg[1] & 0x0f) | (value << 4);
                cfg[2] = (cfg[2] & 0x0f) | (value << 4);
                self.checked_settings(&cfg)?;
            }
            Action::Settings {
                sleep,
                never_sleep,
                left_debounce,
                right_debounce,
                sensor,
                rotation,
            } => {
                cfg[3] = sleep;
                cfg[4] = (cfg[4] & !1) | never_sleep as u8;
                cfg[5] = sensor;
                cfg[6] = (rotation / 4) as i8 as u8;
                cfg[7] = left_debounce;
                cfg[8] = right_debounce;
                self.checked_settings(&cfg)?;
            }
            Action::Dpi {
                x,
                y,
                count,
                active,
                independent,
            } => self.write_dpi_table(
                p,
                &x,
                if independent { &y } else { &x },
                count,
                active,
                independent,
            )?,
            Action::Lod { index } => {
                self.request(0x0109, &[p, index])?;
                if self.request(9, &[p])? != [index] {
                    return Err(bad("LOD 回读未匹配"));
                }
            }
            Action::Button { index, binding } => self.write_binding(p, index, &binding)?,
            Action::Profile { index } => {
                self.request(0x0906, &[index])?;
                if self.settings()?[0] != index {
                    return Err(bad("板载切换未确认"));
                }
            }
            Action::Rename { name } => self.write_name(p, &name)?,
            Action::Macro {
                slot,
                name,
                mode,
                events,
                raw,
            } => self.write_macro_raw(
                slot,
                &macro_record(slot, &name, mode, &events, raw.as_deref())?.raw,
            )?,
            Action::DeleteMacro { slot } => self.write_macro_raw(slot, &[0u8; 64])?,
        }
        let state = self.hardware_state()?;
        if state.profile != expected_after {
            return Err(bad("板载配置已改变，请刷新重试"));
        }
        self.require_board(expected_after)?;
        Ok(state)
    }
    pub fn restore_snapshot(&self, s: Snapshot) -> Result<HardwareState> {
        self.restore_snapshot_inner(s, true)
    }
    fn restore_snapshot_inner(&self, s: Snapshot, backup: bool) -> Result<HardwareState> {
        validate_snapshot(&s)?;
        if ![SNAPSHOT_FORMAT, LEGACY_SNAPSHOT_FORMAT].contains(&s.format.as_str()) || s.model != "3837:4026" {
            return Err(bad("备份型号或版本不匹配"));
        }
        let t = s.state;
        let cfg = self.settings()?;
        if t.profile != cfg[0]
            || t.raw_settings.len() != 19
            || t.raw_settings[0] != cfg[0]
            || t.lod > 4
            || t.buttons.len() != 6
        {
            return Err(bad("请先切到备份对应板载配置；备份结构必须完整"));
        }
        dpi_values(&t.dpi_x)?;
        dpi_values(&t.dpi_y)?;
        if !(1..=6).contains(&t.count)
            || t.active >= t.count
            || t.sleep > 60
            || t.left_debounce > 50
            || t.right_debounce > 50
        {
            return Err(bad("备份参数无效"));
        }
        rate(t.raw_settings[1] >> 4)?;
        rate(t.raw_settings[2] >> 4)?;
        if !t.buttons.iter().any(|b| b == &[0, 1, 0]) {
            return Err(bad("备份没有左键"));
        }
        for b in &t.buttons {
            if b != &[255, 255, 255] {
                validate_binding(b)?
            }
        }
        for m in &s.macros {
            if m.slot >= 18 {
                return Err(bad("宏槽无效"));
            }
            decode_macro(m.slot, m.raw.clone())?;
        }
        if backup {
            self.backup_full_for(t.profile)?;
        }
        self.require_board(t.profile)?;
        for m in s.macros {
            self.write_macro_raw(m.slot, &m.raw)?
        }
        self.write_dpi_table(
            t.profile,
            &t.dpi_x,
            &t.dpi_y,
            t.count,
            t.active,
            t.independent,
        )?;
        self.checked_settings(&t.raw_settings)?;
        for (i, b) in t.buttons.iter().enumerate() {
            let now = parse_buttons(&self.request(1, &[t.profile, 0, 6])?)?;
            if now[i] != *b {
                self.write_binding(t.profile, i as u8, b)?
            }
        }
        self.request(0x0109, &[t.profile, t.lod])?;
        if let Some(name) = t.names.get(t.profile as usize) {
            self.write_name(t.profile, name)?
        }
        let state = self.hardware_state()?;
        if state.profile != t.profile {
            return Err(bad("板载配置已改变，请刷新重试"));
        }
        self.require_board(t.profile)?;
        Ok(state)
    }
}

#[cfg(test)]
#[path = "mchose_macro_tests.rs"]
mod macro_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_captured_snapshot_before_any_write() {
        let mut s: Snapshot =
            serde_json::from_str(include_str!("../tests/fixtures/mchose.json")).unwrap();
        assert!(validate_snapshot(&s).is_ok());
        s.state.raw_settings[3] = 255;
        assert!(validate_snapshot(&s).is_err());
    }
    #[test]
    fn rejects_release_without_press() {
        let e = vec![MacroEvent {
            kind: "key".into(),
            code: 4,
            down: false,
            delay_ms: 0,
        }];
        assert!(encode_macro("unbalanced", 0, &e).is_err());
    }
    #[test]
    fn parses_existing_escape_side_button() {
        let b = parse_buttons(&[
            0, 1, 0, 0, 2, 0, 0, 4, 0, 0, 16, 0, 17, 41, 0, 255, 255, 255,
        ])
        .unwrap();
        assert_eq!(b[4], [17, 41, 0]);
        assert_eq!(b[5], [255, 255, 255]);
        assert!(parse_buttons(&[1, 0, 0]).is_err());
    }
    #[test]
    fn rejects_bad_bindings() {
        assert!(validate_binding(&[0, 1, 0]).is_ok());
        assert!(validate_binding(&[0x16, 1, 6]).is_ok());
        assert!(validate_binding(&[0x21, 18, 0]).is_err());
        assert!(validate_binding(&[0x90, 0, 0]).is_err());
    }
    #[test]
    fn macro_round_trip_and_key_balance() {
        let ev = vec![
            MacroEvent {
                kind: "key".into(),
                code: 4,
                down: true,
                delay_ms: 0,
            },
            MacroEvent {
                kind: "delay".into(),
                code: 0,
                down: false,
                delay_ms: 25,
            },
            MacroEvent {
                kind: "key".into(),
                code: 4,
                down: false,
                delay_ms: 0,
            },
        ];
        let raw = encode_macro("test", 0, &ev).unwrap();
        assert_eq!(raw.len(), 76);
        assert_eq!(raw[74], 145);
        assert_eq!(decode_macro(1, raw).unwrap().events[1].delay_ms, 25);
        assert!(encode_macro("bad", 0, &ev[..1]).is_err());
    }
}
