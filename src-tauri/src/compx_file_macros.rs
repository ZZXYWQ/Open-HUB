//! Typed OEM library bodies. Playback is explicitly supplied per physical key.
//! No guess is made from ambiguous raw/UI binding numbers.
use super::*;
use gearwise_core::{
    model::MouseButton,
    profile::{Macro, MacroMode, MacroStep},
};
pub(super) fn parse(root: &Value, slot: u8, button: u16, mode: MacroMode) -> Result<Macro> {
    let list = root["deviceConfig"]["macroList"]
        .as_array()
        .filter(|v| v.len() <= 12)
        .ok_or("原厂文件缺少完整宏正文；请在原厂软件载入宏页后重新导出")?;
    let item = list
        .get(usize::from(slot))
        .ok_or("原厂文件中引用的宏槽缺少正文")?;
    let name = item["name"]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 256)
        .ok_or("原厂宏名称无效")?;
    let actions = item["actions"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 70)
        .ok_or("宏正文为空或超过70个原厂事件")?;
    let mut steps = vec![];
    let mut at = 0;
    while at < actions.len() {
        let a = &actions[at];
        let kind = integer(&a["type"], "宏事件类型", 255)?;
        let state = integer(&a["keyStatus"], "宏事件状态", 3)?;
        let delay = integer(&a["delay"], "宏事件延时", 65535)? as u32;
        let delay = delay.max(1);
        if !matches!(state, 0 | 1) {
            return Err("滚轮方向/中性记录仍需核对，尚未完整转换此宏".into());
        }
        let key = integer(&a["keyCode"], "宏键值", 255)? as u8;
        let step = match kind {
            0 if (224..=231).contains(&key) => {
                if state == 0 {
                    MacroStep::KeyDown(key)
                } else {
                    MacroStep::KeyUp(key)
                }
            }
            1 if (4..=0xa4).contains(&key) => {
                if state == 0 {
                    MacroStep::KeyDown(key)
                } else {
                    MacroStep::KeyUp(key)
                }
            }
            4 => {
                let b = match key {
                    1 => MouseButton::Left,
                    2 => MouseButton::Right,
                    4 => MouseButton::Middle,
                    8 => MouseButton::Back,
                    16 => MouseButton::Forward,
                    _ => return Err("未知原厂宏鼠标输出".into()),
                };
                if state == 0 {
                    MacroStep::MouseDown(b)
                } else {
                    MacroStep::MouseUp(b)
                }
            }
            2 if state == 0 && delay == 1 => {
                let next = actions.get(at + 1).ok_or("媒体宏缺少抬起事件")?;
                if integer(&next["type"], "媒体事件类型", 255)? != 2
                    || integer(&next["keyStatus"], "媒体事件状态", 3)? != 1
                    || integer(&next["keyCode"], "媒体键值", 255)? != u64::from(key)
                {
                    return Err("媒体宏必须有完整相邻按下/抬起，尚未转换其他媒体保持方式".into());
                }
                let up = integer(&next["delay"], "媒体抬起延时", 65535)? as u32;
                steps.push(MacroStep::Media(u16::from(key)));
                steps.push(MacroStep::Delay(up.max(1)));
                at += 2;
                continue;
            }
            _ => return Err("此宏含尚未完整转换的事件或保持时序，未丢弃或改成默认动作".into()),
        };
        steps.push(step);
        steps.push(MacroStep::Delay(delay));
        at += 1;
    }
    let m = Macro {
        id: format!("atk-file:slot:{slot}:button:{button}"),
        name: name.into(),
        mode,
        steps,
    };
    atk::validate_macro(&m).map_err(|e| e.to_string())?;
    Ok(m)
}
