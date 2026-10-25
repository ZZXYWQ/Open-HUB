//! GG122 exact flat Rival310/Sensei310 lighting schema. Read/compile only.
//! Mode masks, percent stops and preview timing are independently documented
//! in the inert OEM factories; no vendor JS is loaded or executed.
use super::field;
use gearwise_core::{
    model::{ColorPosition, ColorStop, LightingEffect, Rgb, ZoneState},
    Setting,
};
use serde_json::Value;
fn fields(value: &Value, allowed: &[&str]) -> Result<(), String> {
    let object = value.as_object().ok_or("灯效字段不是对象")?;
    if let Some(name) = object.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(format!("灯效字段 {name} 尚未核对"));
    }
    Ok(())
}
fn rgb(value: &Value) -> Result<Rgb, String> {
    fields(value, &["r", "g", "b"])?;
    let channel = |k| -> Result<u8, String> {
        field(value, k)?
            .try_into()
            .map_err(|_| "GG RGB 超出 0–255".into())
    };
    Ok(Rgb {
        r: channel("r")?,
        g: channel("g")?,
        b: channel("b")?,
    })
}
fn zero(value: &Value, key: &str) -> Result<(), String> {
    match value.get(key) {
        Some(v) if v.as_u64() == Some(0) => Ok(()),
        _ => Err(format!("GG 灯效 {key} 的非默认行为尚未转换")),
    }
}
fn zone(value: &Value) -> Result<ZoneState, String> {
    fields(
        value,
        &[
            "type",
            "has_direction",
            "direction_type",
            "direction_inverted",
            "focal_x",
            "focal_y",
            "speed",
            "scale",
            "num_colors",
            "colors",
            "positions",
            "initial_color",
            "react_color",
            "trigger_mask",
            "settings_mask",
        ],
    )?;
    for key in ["has_direction", "direction_inverted"] {
        if value.get(key) != Some(&Value::Bool(false)) {
            zero(value, key)?;
        }
    }
    for key in ["direction_type", "focal_x", "focal_y"] {
        zero(value, key)?;
    }
    if ![0, 1].contains(&field(value, "scale")?) {
        return Err("GG 空间缩放字段尚未转换，不作为亮度解释".into());
    }
    let trigger = field(value, "trigger_mask")?;
    if trigger > 7 {
        return Err("GG 灯效触发掩码包含未知位".into());
    }
    let initial = rgb(value.get("initial_color").ok_or("缺少灯区初始颜色")?)?;
    let reactive = value.get("react_color").ok_or("缺少灯区响应颜色")?;
    fields(reactive, &["r", "g", "b", "time"])?;
    for key in ["r", "g", "b"] {
        let _: u8 = field(reactive, key)?
            .try_into()
            .map_err(|_| "GG 响应颜色超出范围")?;
    }
    let _time = field(reactive, "time")?;
    let colors = value
        .get("colors")
        .and_then(Value::as_array)
        .filter(|v| v.len() == 14)
        .ok_or("GG 色表不是已核对的 14 项布局")?;
    let positions = value
        .get("positions")
        .and_then(Value::as_array)
        .filter(|v| v.len() == 14)
        .ok_or("GG 位置表不是已核对的 14 项布局")?;
    let colors = colors.iter().map(rgb).collect::<Result<Vec<_>, _>>()?;
    let positions = positions
        .iter()
        .map(|v| {
            fields(v, &["pos"])?;
            let n = field(v, "pos")?;
            if n > 100 {
                return Err("GG 颜色位置超出 0–100%".into());
            }
            Ok(n as u8)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let kind = field(value, "type")?;
    let count = field(value, "num_colors")? as usize;
    let speed = field(value, "speed")?;
    let effect = match field(value, "settings_mask")? {
        // OEM steady/disabled selection sets subtype zero and mask one. Off is
        // represented by the initial colour becoming black, not a subtype-six packet.
        1 => {
            if kind != 0 {
                return Err("GG 常亮掩码与灯效子类型不一致".into());
            }
            if initial == Rgb::BLACK {
                LightingEffect::Off
            } else {
                LightingEffect::Static { color: initial }
            }
        }
        0 => {
            let max = match kind {
                1 => 10,
                2 => 4,
                _ => return Err("此 GG 动态灯效子类型尚未转换".into()),
            };
            if count == 0
                || count > max
                || positions[0] != 0
                || positions[..count].windows(2).any(|s| s[0] > s[1])
            {
                return Err("GG 动态灯效颜色数量或位置不符合当前型号布局".into());
            }
            if !(33..=3000).contains(&speed) {
                return Err("GG 颜色节奏超出已核对的滑块范围".into());
            }
            // OEM widget's period calculation. Stored speed is not raw native ms.
            let period_ms =
                (10 * speed).max(330 * count as u32 * if kind == 2 { 2 } else { 1 }) as u16;
            let mut stops = [ColorStop::EMPTY; 14];
            let mut used = 0;
            for index in 0..count {
                stops[used] = ColorStop {
                    position: positions[index].into(),
                    color: colors[index],
                };
                used += 1;
                if kind == 2 {
                    let end = if index + 1 < count {
                        positions[index + 1]
                    } else {
                        100
                    };
                    let midpoint = u16::from(positions[index]) + u16::from(end);
                    stops[used] = ColorStop {
                        position: ColorPosition::from_half_percent(midpoint),
                        color: Rgb::BLACK,
                    };
                    used += 1;
                }
            }
            LightingEffect::Gradient {
                stops,
                count: used as u8,
                period_ms,
            }
        }
        _ => return Err("GG 响应、主机串流或覆盖颜色模式尚未转换，未保存部分灯效".into()),
    };
    Ok(ZoneState {
        effect,
        brightness: 100,
    })
}
fn legacy_zone(settings:&Value,name:&str)->Result<ZoneState,String>{
 let color=settings.get(format!("{name}_color")).ok_or("旧版 GG 缺少灯区颜色")?;fields(color,&["red","green","blue"])?;
 let channel=|k|->Result<u8,String>{field(color,k)?.try_into().map_err(|_|"旧版 GG 颜色超出 0–255".into())};let color=Rgb{r:channel("red")?,g:channel("green")?,b:channel("blue")?};
 let pulsation=settings.get(format!("{name}_pulsation")).ok_or("旧版 GG 缺少固件呼吸模式")?;fields(pulsation,&["mode"])?;
 let sequence=settings.get(format!("rival_{name}_color_sequence")).ok_or("旧版 GG 缺少主机颜色序列")?;fields(sequence,&["repeat","data"])?;
 if sequence.get("data").and_then(Value::as_array).is_none_or(|v|!v.is_empty())||sequence.get("repeat").and_then(Value::as_str)!=Some(""){return Err("此 Rival 300 GG 主机颜色序列尚未转换，不能冒充板载灯效".into());}
 let effect=match field(pulsation,"mode")?{1=>if color==Rgb::BLACK{LightingEffect::Off}else{LightingEffect::Static{color}},mode@2..=4=>LightingEffect::Breathing{color,period_ms:match mode{2=>7000,3=>5000,_=>3000}},_=>return Err("此 Rival 300 原厂灯效模式尚未核对".into())};Ok(ZoneState{effect,brightness:100})
}
fn legacy(settings:&Value)->Result<Option<Vec<Setting>>,String>{
 let keys=["logo_color","wheel_color","logo_pulsation","wheel_pulsation","rival_logo_color_sequence","rival_wheel_color_sequence"];
 if keys.iter().all(|k|settings.get(k).is_none()){return Ok(None);}
 let mut out=Vec::new();for(zone,name)in["logo","wheel"].into_iter().enumerate(){let state=legacy_zone(settings,name).map_err(|e|format!("{name}：{e}"))?;let setting=Setting::Lighting{zone:zone as u8,state};crate::steelseries::validate_setting(0x1710,&setting)?;out.push(setting);}Ok(Some(out))
}
pub(super) fn parse(pid: u16, settings: &Value) -> Result<Option<Vec<Setting>>, String> {
    if pid==0x1710{return legacy(settings);}
    if ![0x1720, 0x1722].contains(&pid) {
        return Err("此 GG 灯效没有已核对的精确型号".into());
    }
    let logo = settings.get("logo_lighting");
    let wheel = settings.get("wheel_lighting");
    if logo.is_none() && wheel.is_none() {
        if settings.get("logo_element").is_some() || settings.get("wheel_element").is_some() {
            return Err("GG 灯区引用没有对应灯效内容".into());
        }
        return Ok(None);
    }
    let mut lights = Vec::new();
    for (index, (name, value)) in [("Logo", logo), ("滚轮", wheel)].into_iter().enumerate() {
        let value = value.ok_or("GG 只记录了一个灯区，不能补另一灯区的默认值")?;
        let state = zone(value).map_err(|e| format!("{name}：{e}"))?;
        let key = if index == 0 {
            "logo_element"
        } else {
            "wheel_element"
        };
        let element = settings
            .get(key)
            .ok_or_else(|| format!("GG 缺少 {key} 的区域引用"))?;
        fields(element, &["pos", "effect_index"])?;
        let position = element.get("pos").ok_or("GG 缺少灯区坐标")?;
        fields(position, &["x", "y"])?;
        zero(position, "x")?;
        zero(position, "y")?;
        if field(element, "effect_index")? != index as u32 {
            return Err("GG 灯区效果引用与当前灯区不一致".into());
        }
        let setting = Setting::Lighting {
            zone: index as u8,
            state,
        };
        crate::steelseries::validate_setting(pid, &setting)?;
        lights.push(setting);
    }
    Ok(Some(lights))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    pub(super) fn lamp(kind: u32, mask: u32) -> Value {
        let mut colors = vec![json!({"r":0,"g":0,"b":0}); 14];
        colors[0] = json!({"r":255,"g":0,"b":0});
        colors[1] = json!({"r":0,"g":255,"b":0});
        colors[2] = json!({"r":0,"g":0,"b":255});
        let mut positions = vec![json!({"pos":0}); 14];
        positions[1] = json!({"pos":33});
        positions[2] = json!({"pos":66});
        json!({"type":kind,"settings_mask":mask,"has_direction":0,"direction_type":0,"direction_inverted":0,"focal_x":0,"focal_y":0,"speed":1000,"scale":1,"num_colors":3,"colors":colors,"positions":positions,"initial_color":{"r":17,"g":34,"b":51},"react_color":{"r":0,"g":0,"b":0,"time":0},"trigger_mask":7})
    }
    fn settings() -> Value {
        json!({"logo_lighting":lamp(1,0),"wheel_lighting":lamp(1,0),"logo_element":{"pos":{"x":0,"y":0},"effect_index":0},"wheel_element":{"pos":{"x":0,"y":0},"effect_index":1}})
    }
    #[test]
    fn official_flat_colorshift_semantics_keep_two_zones_positions_and_gui_period_in_native_full_vector(
    ) {
        let vectors: Value = serde_json::from_str(include_str!(
            "../../docs/development/steelseries-independent-gradient-vectors.json"
        ))
        .unwrap();
        for pid in [0x1720, 0x1722] {
            let settings = parse(pid, &settings()).unwrap().unwrap();
            for setting in settings {
                let Setting::Lighting { zone, state } = setting else {
                    unreachable!()
                };
                let LightingEffect::Gradient {
                    stops,
                    count,
                    period_ms,
                } = state.effect
                else {
                    panic!()
                };
                assert_eq!((count, period_ms), (3, 10000));
                assert_eq!(
                    [stops[0].position, stops[1].position, stops[2].position],
                    [0, 33, 66]
                );
                let (mut device, log) = gearwise_steelseries::simulated_model(pid).unwrap();
                device.apply(&Setting::Lighting { zone, state }).unwrap();
                let hex = vectors["cases"][0]["hidapi_feature_hex"].as_str().unwrap();
                let mut expected = (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect::<Vec<_>>();
                expected[3] = zone;
                expected[4..6].copy_from_slice(&10000u16.to_le_bytes());
                assert_eq!(
                    log.lock().unwrap().as_slice(),
                    &[gearwise_core::transport::MockRecord::SetFeature(expected)]
                );
            }
        }
    }
    #[test]
    fn steady_and_disabled_use_the_oem_mask_and_initial_colour_without_inventing_brightness() {
        let mut s = settings();
        s["logo_lighting"] = lamp(0, 1);
        s["wheel_lighting"] = lamp(0, 1);
        s["wheel_lighting"]["initial_color"] = json!({"r":0,"g":0,"b":0});
        let v = parse(0x1722, &s).unwrap().unwrap();
        assert!(matches!(
            v[0],
            Setting::Lighting {
                state: ZoneState {
                    effect: LightingEffect::Static {
                        color: Rgb {
                            r: 17,
                            g: 34,
                            b: 51
                        }
                    },
                    brightness: 100
                },
                ..
            }
        ));
        assert!(matches!(
            v[1],
            Setting::Lighting {
                state: ZoneState {
                    effect: LightingEffect::Off,
                    brightness: 100
                },
                ..
            }
        ));
    }
    #[test]
    fn multicolor_breathing_keeps_exact_whole_and_half_percent_black_midpoints() {
        let mut lamp = lamp(2, 0);
        lamp["num_colors"] = json!(2);
        lamp["positions"][1]["pos"] = json!(50);
        let state = zone(&lamp).unwrap();
        let LightingEffect::Gradient {
            stops,
            count,
            period_ms,
        } = state.effect
        else {
            panic!()
        };
        assert_eq!((count, period_ms), (4, 10000));
        assert_eq!(
            stops[..4].iter().map(|s| s.position).collect::<Vec<_>>(),
            [0, 25, 50, 75]
        );
        assert_eq!(stops[1].color, Rgb::BLACK);
        assert_eq!(stops[3].color, Rgb::BLACK);
        lamp["positions"][1]["pos"] = json!(33);
        let half=zone(&lamp).unwrap();let LightingEffect::Gradient{stops,count,..}=half.effect else{panic!()};assert_eq!(count,4);assert_eq!(stops[..4].iter().map(|s|s.position.half_percent()).collect::<Vec<_>>(),[0,33,66,133]);
    }
    #[test]
    fn incomplete_unproven_masks_modes_directions_layouts_and_units_do_not_save_partial_lighting() {
        for case in 0..13 {
            let mut s = settings();
            let v = &mut s["logo_lighting"];
            match case {
                0 => v["settings_mask"] = json!(8),
                1 => v["type"] = json!(3),
                2 => v["has_direction"] = json!(1),
                3 => v["scale"] = json!(100),
                4 => v["num_colors"] = json!(11),
                5 => v["speed"] = json!(0),
                6 => v["speed"] = json!(3001),
                7 => v["positions"][0]["pos"] = json!(1),
                8 => v["colors"][0]["r"] = json!(256),
                9 => {
                    v["positions"].as_array_mut().unwrap().pop();
                }
                10 => v["randomize"] = json!(true),
                11 => s["logo_element"]["effect_index"] = json!(1),
                _ => {
                    s.as_object_mut().unwrap().remove("wheel_lighting");
                }
            }
            assert!(parse(0x1722, &s).is_err(), "case {case}");
        }
        assert!(parse(0x1724, &settings()).is_err());
        assert!(parse(0x1722, &json!({})).unwrap().is_none());
    }
}
