//! Typed, offline source editing. Unknown untouched fields are retained.
use super::{
    bindings::{Action, Media, Mouse, Playback},
    oem_config,
    oem_profile::{self, Source},
    source_preview::{self, Preview},
    ConfigPatch,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Choice {
    Basic(Action),
    Macro(i64),
    Disabled,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonEdit {
    pub control: i64,
    pub choice: Choice,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacroEdit {
    pub id: i64,
    pub playback: Playback,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    #[serde(default)]
    pub settings: ConfigPatch,
    #[serde(default)]
    pub buttons: Vec<ButtonEdit>,
    #[serde(default)]
    pub macros: Vec<MacroEdit>,
}
fn setting(source: &mut Source, key: &str, value: impl ToString) {
    source.settings.fields.insert(key.into(), value.to_string());
}
fn apply_settings(s: &mut Source, p: &ConfigPatch) -> Result<(), String> {
    if let Some(v) = p.report_rate_hz {
        setting(
            s,
            "report_rate",
            match v {
                125 => 1,
                250 => 2,
                500 => 3,
                1000 => 4,
                _ => return Err("SC620回报率范围125..1000Hz".into()),
            },
        );
    }
    if let Some(v) = p.debounce_ms {
        setting(s, "button_respondtime", v);
    }
    if let Some(v) = p.active {
        setting(s, "dpi_index", v);
    }
    if let Some(v) = p.light {
        setting(s, "light_mode", v as u8);
    }
    if let Some(v) = p.dpi {
        for (i, n) in v.into_iter().enumerate() {
            setting(s, &format!("dpi_group{i}_value"), n);
        }
    }
    if let Some(v) = p.colors {
        for (i, [r, g, b]) in v.into_iter().enumerate() {
            setting(
                s,
                &format!("dpi_group{i}_rgb"),
                u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16),
            );
        }
    }
    if let Some(v) = p.lod_mm {
        if !matches!(v, 1 | 2) {
            return Err("LOD须为1或2mm".into());
        }
        setting(s, "liftoff_height", v);
    }
    if let Some(v) = p.scroll_reverse {
        setting(s, "scroll_flag", u8::from(v));
    }
    if let Some(v) = p.esports {
        setting(s, "e-sports_flag", if v { 2 } else { 1 });
    }
    if let Some(v) = p.sleep_minutes {
        if !matches!(v, 1 | 3 | 5 | 10 | 20 | 30 | 60) {
            return Err("未知休眠分钟值".into());
        }
        setting(s, "sleep_light", v);
    }
    if p.ripple_control.is_some() || p.angle_snap.is_some() || p.motion_sync.is_some() {
        let value = s
            .settings
            .fields
            .get("sensor_flag")
            .ok_or("来源没有传感器位字段，不能推测其未修改位")?;
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err("传感器字段不是十进制".into());
        }
        let mut bits = value
            .parse::<u8>()
            .map_err(|_| "传感器字段超出原始字节范围")?;
        for (v, bit) in [
            (p.ripple_control, 4),
            (p.angle_snap, 8),
            (p.motion_sync, 16),
        ] {
            if let Some(enabled) = v {
                if enabled {
                    bits |= bit
                } else {
                    bits &= !bit
                }
            }
        }
        setting(s, "sensor_flag", bits);
    }
    s.settings = oem_config::convert(s.settings.profile_id, s.settings.fields.clone())
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn basic_fields(a: &Action) -> Result<(i64, Option<i64>, Option<i64>), String> {
    a.encode().map_err(|e| e.to_string())?;
    let (kind, value, mods) = match a {
        Action::Mouse(m) => (
            match m {
                Mouse::Left => 1,
                Mouse::Middle => 2,
                Mouse::Right => 3,
                Mouse::Forward => 5,
                Mouse::Back => 6,
            },
            None,
            None,
        ),
        Action::DoubleClick => (4, None, None),
        Action::WheelUp => (7, None, None),
        Action::WheelDown => (8, None, None),
        Action::DpiCycle => (9, None, None),
        Action::Fire => (11, None, None),
        Action::DpiIncrease => (15, None, None),
        Action::DpiDecrease => (16, None, None),
        Action::DpiLoop => (17, None, None),
        Action::ReportRateIncrease => (19, None, None),
        Action::ReportRateDecrease => (20, None, None),
        Action::ReportRateLoop => (21, None, None),
        Action::Key { usage, modifiers } => {
            (22, Some(i64::from(*usage)), Some(i64::from(*modifiers)))
        }
        Action::Media(m) => {
            let (k, v) = match m {
                Media::Player => (25, 1),
                Media::PlayPause => (25, 2),
                Media::Stop => (25, 3),
                Media::Previous => (25, 4),
                Media::Next => (25, 5),
                Media::VolumeUp => (25, 6),
                Media::VolumeDown => (25, 7),
                Media::Mute => (25, 8),
                Media::ScreenBrightnessUp => (26, 1),
                Media::ScreenBrightnessDown => (26, 2),
                Media::Calculator => (26, 3),
                Media::BrowserHome => (26, 6),
                Media::Mail => (26, 7),
            };
            (k, Some(v), None)
        }
        Action::Disabled(_) => return Err("请使用按控制编号生成的禁用动作".into()),
    };
    Ok((kind, value, mods))
}
pub fn update(source: Source, request: Request) -> Result<Preview, String> {
    let id = source.settings.profile_id;
    source_preview::assess(source.clone(), vec![id])?;
    if request.buttons.len() > 11 || request.macros.len() > 32 {
        return Err("编辑数量超限".into());
    }
    let mut next = source;
    apply_settings(&mut next, &request.settings)?;
    let mut seen = std::collections::BTreeSet::new();
    for edit in request.buttons {
        if !seen.insert(edit.control) {
            return Err("重复编辑同一控制".into());
        }
        oem_profile::record(edit.control).map_err(|e| e.to_string())?;
        let row = next
            .keys
            .iter_mut()
            .find(|k| k.key_code == edit.control && k.layer == 0)
            .ok_or("没有此普通层控制记录")?;
        match edit.choice {
            Choice::Basic(a) => {
                let (kind, value, mods) = basic_fields(&a)?;
                row.kind = kind;
                if let Some(v) = value {
                    row.value = v;
                }
                if let Some(v) = mods {
                    row.value2 = v;
                }
                if oem_profile::basic(row)
                    .map_err(|e| e.to_string())?
                    .encode()
                    .map_err(|e| e.to_string())?
                    != a.encode().map_err(|e| e.to_string())?
                {
                    return Err("编辑动作不能忠实还原为原厂字段".into());
                }
            }
            Choice::Macro(id) => {
                if !next.macros.iter().any(|m| m.id == id) {
                    return Err("引用的宏ID不存在".into());
                }
                row.kind = 23;
                row.value = id;
            }
            Choice::Disabled => {
                row.kind = 24;
                oem_profile::basic(row).map_err(|e| e.to_string())?;
            }
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for edit in request.macros {
        if !seen.insert(edit.id) {
            return Err("重复编辑同一宏".into());
        }
        edit.playback.wire().map_err(|e| e.to_string())?;
        let m = next
            .macros
            .iter_mut()
            .find(|m| m.id == edit.id)
            .ok_or("宏ID不存在")?;
        match edit.playback {
            Playback::Once => {
                m.play_type = 0;
                m.play_times = 1;
            }
            Playback::Repeat(n) => {
                m.play_type = 0;
                m.play_times = i64::from(n);
            }
            Playback::WhileHeld => m.play_type = 1,
            Playback::UntilAnyClick => m.play_type = 2,
        };
        m.playback = edit.playback;
    }
    source_preview::assess(next, vec![id])
}
#[cfg(test)]
#[path = "yjx_sc620_source_edit_tests.rs"]
mod tests;
