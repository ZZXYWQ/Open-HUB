//! Exact Basilisk V3 Pro (1532:00AA/00AB) normal-layer button codec.
//! Wire facts: kev-landry/basilisk-configurator @ 2bb8b3c (MIT).
//! Other Basilisk generations, Hypershift and native macro flash are not inferred.
use gearwise_core::{
    model::{ButtonAction, DpiFunction, Modifiers, MouseButton},
    Error, Result,
};
pub const BUTTONS: [(u16, &str); 13] = [
    (1, "左键"),
    (2, "右键"),
    (3, "滚轮中键"),
    (4, "侧键后退"),
    (5, "侧键前进"),
    (9, "滚轮向上"),
    (10, "滚轮向下"),
    (14, "底部配置键"),
    (15, "拇指拨片"),
    (52, "滚轮左倾"),
    (53, "滚轮右倾"),
    (96, "顶部后键"),
    (106, "顶部前键"),
];
pub fn supported(v: u16, p: u16) -> bool {
    v == 0x1532 && matches!(p, 0x00aa | 0x00ab)
}
pub fn default_block(id: u16) -> Option<[u8; 7]> {
    Some(match id {
        1..=5 | 9 | 10 => [1, 1, id as u8, 0, 0, 0, 0],
        14 => [7, 1, 4, 0, 0, 0, 0],
        15 => [6, 1, 5, 1, 0x90, 1, 0x90],
        52 => [14, 3, 0x68, 0, 20, 0, 0],
        53 => [14, 3, 0x69, 0, 20, 0, 0],
        96 | 106 => [6, 1, 6, 0, 0, 0, 0],
        _ => return None,
    })
}
pub fn encode(id: u16, action: &ButtonAction) -> Result<[u8; 7]> {
    if id == 1 || default_block(id).is_none() {
        return Err(Error::Unsupported(
            "Basilisk protected/unknown physical button",
        ));
    }
    Ok(match action {
        ButtonAction::Default => default_block(id).unwrap(),
        ButtonAction::Disabled => [0; 7],
        ButtonAction::Key { usage, mods } if (4..=0xa4).contains(usage) => {
            [2, 2, mods.0, *usage, 0, 0, 0]
        }
        ButtonAction::Mouse(b) => [
            1,
            1,
            match b {
                MouseButton::Left => 1,
                MouseButton::Right => 2,
                MouseButton::Middle => 3,
                MouseButton::Back => 4,
                MouseButton::Forward => 5,
                MouseButton::WheelLeft => 0x68,
                MouseButton::WheelRight => 0x69,
            },
            0,
            0,
            0,
            0,
        ],
        ButtonAction::Dpi(DpiFunction::Cycle) => [6, 1, 6, 0, 0, 0, 0],
        _ => return Err(Error::Unsupported("Basilisk verified native action")),
    })
}
pub fn decode(id: u16, b: &[u8]) -> Option<ButtonAction> {
    if b.len() != 7 || default_block(id).is_none() {
        return None;
    }
    let mut block = [0; 7];
    block.copy_from_slice(b);
    if block == [0; 7] {
        return Some(ButtonAction::Disabled);
    }
    if b[3..] == [0; 4] && b[..2] == [1, 1] {
        let key = match b[2] {
            1 => MouseButton::Left,
            2 => MouseButton::Right,
            3 => MouseButton::Middle,
            4 => MouseButton::Back,
            5 => MouseButton::Forward,
            0x68 => MouseButton::WheelLeft,
            0x69 => MouseButton::WheelRight,
            _ => return (Some(block) == default_block(id)).then_some(ButtonAction::Default),
        };
        return Some(ButtonAction::Mouse(key));
    }
    if b[..2] == [2, 2] && (4..=0xa4).contains(&b[3]) && b[4..] == [0; 3] {
        return Some(ButtonAction::Key {
            usage: b[3],
            mods: Modifiers(b[2]),
        });
    }
    if block == [6, 1, 6, 0, 0, 0, 0] {
        return Some(ButtonAction::Dpi(DpiFunction::Cycle));
    }
    (Some(block) == default_block(id)).then_some(ButtonAction::Default)
}
