//! Shared import key translation. USB keyboard usages are independent of brand.
use gearwise_core::{
    model::MouseButton,
    profile::{Macro, MacroMode, MacroStep},
};
use sidearm_synapse_parser::{
    makecode,
    types::{ParsedMacro, RuntimeEvent},
};
pub fn usage(key: &str) -> Result<u8, String> {
    let upper = key.to_ascii_uppercase();
    let k = upper.as_str();
    if k.len() == 1 {
        let b = k.as_bytes()[0];
        if b.is_ascii_uppercase() {
            return Ok(4 + b - b'A');
        }
        if (b'1'..=b'9').contains(&b) {
            return Ok(0x1e + b - b'1');
        }
        if b == b'0' {
            return Ok(0x27);
        }
    }
    if let Some(n) = k.strip_prefix('F').and_then(|v| v.parse::<u8>().ok()) {
        if (1..=12).contains(&n) {
            return Ok(0x3a + n - 1);
        }
        if (13..=24).contains(&n) {
            return Ok(0x68 + n - 13);
        }
    }
    if let Some(n) = k.strip_prefix("NUM").and_then(|v| v.parse::<u8>().ok()) {
        if n == 0 {
            return Ok(0x62);
        }
        if n <= 9 {
            return Ok(0x59 + n - 1);
        }
    }
    let code = match k {
        "ENTER" | "RETURN" => 0x28,
        "ESCAPE" | "ESC" => 0x29,
        "BACKSPACE" => 0x2a,
        "TAB" => 0x2b,
        "SPACE" => 0x2c,
        "-" => 0x2d,
        "=" => 0x2e,
        "[" => 0x2f,
        "]" => 0x30,
        "\\" => 0x31,
        ";" => 0x33,
        "'" => 0x34,
        "`" => 0x35,
        "," => 0x36,
        "." => 0x37,
        "/" => 0x38,
        "CAPSLOCK" => 0x39,
        "PRINTSCREEN" => 0x46,
        "SCROLLLOCK" => 0x47,
        "PAUSE" => 0x48,
        "INSERT" => 0x49,
        "HOME" => 0x4a,
        "PAGEUP" => 0x4b,
        "DELETE" => 0x4c,
        "END" => 0x4d,
        "PAGEDOWN" => 0x4e,
        "RIGHT" => 0x4f,
        "LEFT" => 0x50,
        "DOWN" => 0x51,
        "UP" => 0x52,
        "NUMLOCK" => 0x53,
        "NUMDIVIDE" => 0x54,
        "NUMMULTIPLY" => 0x55,
        "NUMSUBTRACT" => 0x56,
        "NUMADD" => 0x57,
        "NUMENTER" => 0x58,
        "NUMDECIMAL" => 0x63,
        "OEM102" | "INTLBACKSLASH" => 0x64,
        "APPS" => 0x65,
        "LEFTCTRL" | "LCTRL" => 0xe0,
        "LEFTSHIFT" | "LSHIFT" => 0xe1,
        "LEFTALT" | "LALT" => 0xe2,
        "LEFTWIN" | "LWIN" => 0xe3,
        "RIGHTCTRL" | "RCTRL" => 0xe4,
        "RIGHTSHIFT" | "RSHIFT" => 0xe5,
        "RIGHTALT" | "RALT" => 0xe6,
        "RIGHTWIN" | "RWIN" => 0xe7,
        _ => return Err(format!("暂不支持导入键码 {key}")),
    };
    Ok(code)
}
pub fn scan(code: u16, extended: bool) -> Result<u8, String> {
    usage(
        makecode::makecode_to_key(code, extended)
            .ok_or_else(|| format!("未知扫描码 {code:04X}（extended={extended}）"))?,
    )
}
pub fn mouse(code: u8) -> Result<MouseButton, String> {
    match code {
        1 => Ok(MouseButton::Left),
        2 => Ok(MouseButton::Right),
        3 => Ok(MouseButton::Middle),
        4 => Ok(MouseButton::Back),
        5 => Ok(MouseButton::Forward),
        _ => Err(format!("未知宏鼠标键码 {code}")),
    }
}
pub fn macro_body(source: &ParsedMacro, mode: MacroMode) -> Result<Macro, String> {
    let events = source
        .events
        .as_ref()
        .ok_or("宏包含未知或有损事件，不能把预览文本当成原始宏执行")?;
    let steps = events
        .iter()
        .map(|e| {
            Ok(match e {
                RuntimeEvent::Key {
                    makecode,
                    is_extended,
                    is_down,
                } => {
                    let k = scan(*makecode, *is_extended)?;
                    if *is_down {
                        MacroStep::KeyDown(k)
                    } else {
                        MacroStep::KeyUp(k)
                    }
                }
                RuntimeEvent::Mouse { button, is_down } => {
                    let b = mouse(*button)?;
                    if *is_down {
                        MacroStep::MouseDown(b)
                    } else {
                        MacroStep::MouseUp(b)
                    }
                }
                RuntimeEvent::Delay { delay_ms } => MacroStep::Delay(*delay_ms),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let out = Macro {
        id: format!("synapse:{}", source.synapse_guid),
        name: source.name.clone(),
        mode,
        steps,
    };
    crate::host_macros::validate(&out)?;
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extended_keys_and_modifiers_are_distinct() {
        assert_eq!(scan(0x1d, false).unwrap(), 0xe0);
        assert_eq!(scan(0x1d, true).unwrap(), 0xe4);
        assert_eq!(scan(0x4f, false).unwrap(), 0x59);
        assert_eq!(scan(0x4f, true).unwrap(), 0x4d);
        assert!(scan(0xffff, true).is_err());
        assert_eq!(usage("F24").unwrap(), 0x73);
    }
    #[test]
    fn native_macro_conversion_keeps_simultaneous_holds() {
        let source = ParsedMacro {
            synapse_guid: "m".into(),
            name: "hold".into(),
            steps: vec![],
            events: Some(vec![
                RuntimeEvent::Key {
                    makecode: 30,
                    is_extended: false,
                    is_down: true,
                },
                RuntimeEvent::Key {
                    makecode: 48,
                    is_extended: false,
                    is_down: true,
                },
                RuntimeEvent::Delay { delay_ms: 25 },
                RuntimeEvent::Key {
                    makecode: 30,
                    is_extended: false,
                    is_down: false,
                },
                RuntimeEvent::Key {
                    makecode: 48,
                    is_extended: false,
                    is_down: false,
                },
            ]),
        };
        let m = macro_body(&source, MacroMode::Once).unwrap();
        assert_eq!(
            m.steps,
            vec![
                MacroStep::KeyDown(4),
                MacroStep::KeyDown(5),
                MacroStep::Delay(25),
                MacroStep::KeyUp(4),
                MacroStep::KeyUp(5)
            ]
        );
        let r = crate::host_macros::simulate(
            crate::host_macros::Source {
                vendor: 0x1532,
                product: 0x00b4,
                path: "simulation:import".into(),
            },
            0x40,
            m,
        )
        .unwrap();
        assert_eq!(
            r["events"],
            serde_json::json!(["key_down:4", "key_down:5", "key_up:4", "key_up:5"])
        );
    }
    #[test]
    fn lossy_previews_and_unknown_mouse_codes_never_become_executable() {
        let mut source = ParsedMacro {
            synapse_guid: "bad".into(),
            name: "bad".into(),
            steps: vec![],
            events: None,
        };
        assert!(macro_body(&source, MacroMode::Once).is_err());
        source.events = Some(vec![RuntimeEvent::Mouse {
            button: 99,
            is_down: true,
        }]);
        assert!(macro_body(&source, MacroMode::Once).is_err());
    }
}
