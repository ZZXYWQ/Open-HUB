//! Prime GG122 schema, static official migration and descriptor evidence only.
use super::*;
use gearwise_core::model::{Rgb, ScrollDirection};
use gearwise_steelseries::{
    prime,
    prime_gradient::{Motion, Program, Section},
};
// GoLisp arithmetic uses float32 when one operand is a float.
// In particular100 *2.55f32 rounds to255 before integer truncation.
fn weight(percent: u32) -> u8 {
    ((percent as f32) * 2.55f32) as u8
}
fn shape(v: &Value, keys: &[&str]) -> Result<(), String> {
    let o = v.as_object().ok_or("GG Prime 字段不是对象")?;
    if o.len() != keys.len() || o.keys().any(|k| !keys.contains(&k.as_str())) {
        return Err("GG Prime 字段缺失或存在未核对数据".into());
    }
    Ok(())
}
fn rgb(v: &Value) -> Result<Rgb, String> {
    shape(v, &["r", "g", "b"])?;
    let c = |k| -> Result<u8, String> {
        field(v, k)?
            .try_into()
            .map_err(|_| "GG Prime RGB越界".into())
    };
    Ok(Rgb::new(c("r")?, c("g")?, c("b")?))
}
fn action(v: &Value) -> Result<ButtonAction, String> {
    shape(v, &["function", "key_codes"])?;
    let keys = codes(v)?;
    let zero = keys.iter().all(|k| *k == 0);
    Ok(match field(v, "function")? {
        f @ 0..=5 if zero => {
            if f == 0 {
                ButtonAction::Disabled
            } else {
                ButtonAction::Mouse(mouse(f)?)
            }
        }
        48 if zero => ButtonAction::Dpi(DpiFunction::Cycle),
        49 | 50 if zero => ButtonAction::Scroll(if field(v, "function")? == 49 {
            ScrollDirection::Up
        } else {
            ScrollDirection::Down
        }),
        97 if keys[0] != 0 && keys[1..].iter().all(|k| *k == 0) => {
            ButtonAction::Media(keys[0].into())
        }
        81 => {
            let used = keys.iter().copied().filter(|k| *k != 0).collect::<Vec<_>>();
            let unique = used.iter().copied().collect::<HashSet<_>>();
            let normal = used
                .iter()
                .copied()
                .filter(|k| !(0xe0..=0xe7).contains(k))
                .collect::<Vec<_>>();
            if used.len() != unique.len() || normal.len() != 1 {
                return Err("GG Prime 多普通键组合尚未转换".into());
            }
            ButtonAction::Key {
                usage: normal[0],
                mods: Modifiers(
                    used.iter()
                        .filter(|k| (0xe0..=0xe7).contains(*k))
                        .fold(0, |m, k| m | 1 << (*k - 0xe0)),
                ),
            }
        }
        _ => return Err("GG Prime 按键动作尚未核对".into()),
    })
}
pub(super) fn convert(
    conn: &Connection,
    id: &str,
    pid: u16,
    s: &Value,
    notes: &mut Vec<String>,
) -> Result<(Plan, Vec<Level>, usize, u16), String> {
    if pid != prime::PID {
        return Err("GG Prime 精确型号不符".into());
    }
    shape(
        s,
        &[
            "batch_resolution",
            "button_and_wheel_mappings",
            "polling_rate",
            "exact_accel",
            "exact_aim",
            "free_move",
            "led_brightness",
            "lighting_config",
            "lighting_enable",
        ],
    )?;
    for k in ["exact_accel", "exact_aim", "free_move"] {
        shape(&s[k], &["level"])?;
        if field(&s[k], "level")? != 0 {
            return Err("GG Prime 非默认驱动性能尚未转换".into());
        }
    }
    let d = &s["batch_resolution"];
    shape(
        d,
        &[
            "num_cpi_levels",
            "active_level_index",
            "resolution_1",
            "resolution_2",
            "resolution_3",
            "resolution_4",
            "resolution_5",
        ],
    )?;
    let count = field(d, "num_cpi_levels")? as usize;
    let active = field(d, "active_level_index")? as usize;
    if !(1..=5).contains(&count) || active >= count {
        return Err("GG Prime CPI档位无效".into());
    }
    let mut stages = vec![];
    for i in 1..=5 {
        let code = field(d, &format!("resolution_{i}"))?;
        if i <= count {
            if !(1..=360).contains(&code) {
                return Err("GG Prime CPI编码越界".into());
            }
            stages.push(DpiStage::uniform((code * 50) as u16));
        } else if code > 360 {
            return Err("GG Prime 未启用CPI编码越界".into());
        }
    }
    shape(&s["polling_rate"], &["level"])?;
    let rate = match field(&s["polling_rate"], "level")? {
        1 => 1000,
        2 => 500,
        3 => 250,
        4 => 125,
        _ => return Err("GG Prime 回报率无效".into()),
    };
    let mapping = &s["button_and_wheel_mappings"];
    shape(mapping, &["buttons", "wheels", "no_live_deploy"])?;
    if field(mapping, "no_live_deploy")? != 1 {
        return Err("GG Prime 部署标记未核对".into());
    }
    let physical = mapping["buttons"]
        .as_array()
        .filter(|a| a.len() == 5)
        .ok_or("GG Prime 需要五个上方实体按键")?;
    let wheel = mapping["wheels"]
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or("GG Prime 需要两个滚轮输入")?;
    let mut buttons = BTreeMap::new();
    for (i, v) in physical.iter().enumerate() {
        buttons.insert(i as u16 + 1, action(v)?);
    }
    for (i, v) in wheel.iter().enumerate() {
        buttons.insert(i as u16 + 7, action(v)?);
    }
    let mut macros = vec![];
    apply_bindings(conn, id, pid, 5, &mut buttons, &mut macros)?;
    if buttons.remove(&1) != Some(ButtonAction::Mouse(MouseButton::Left)) {
        return Err("GG Prime 导入保留主左键".into());
    }
    let light = &s["lighting_config"];
    shape(
        light,
        &[
            "type",
            "speed",
            "num_colors",
            "colors",
            "positions",
            "initial_color",
            "settings_mask",
        ],
    )?;
    let kind = field(light, "type")?;
    let mask = field(light, "settings_mask")?;
    if !matches!((kind, mask), (1 | 2, 0) | (0, 1)) {
        return Err("GG Prime 主机流、覆盖、响应或未核对灯效组合尚未转换".into());
    }
    let colors = light["colors"]
        .as_array()
        .filter(|a| a.len() == 14)
        .ok_or("GG Prime 原厂色表不是14项")?;
    let positions = light["positions"]
        .as_array()
        .filter(|a| a.len() == 14)
        .ok_or("GG Prime 原厂位置表不是14项")?;
    let n = field(light, "num_colors")? as usize;
    if (mask == 0 && !(1..=if kind == 2 { 7 } else { 14 }).contains(&n)) || (mask == 1 && n != 0) {
        return Err("GG Prime 颜色数量超出已核对容量".into());
    }
    let mut cs = vec![];
    let mut ps = vec![];
    for (c, p) in colors.iter().zip(positions) {
        cs.push(rgb(c)?);
        shape(p, &["pos"])?;
        let pos = field(p, "pos")?;
        if pos > 100 {
            return Err("GG Prime 位置越界".into());
        }
        ps.push(pos);
    }
    if mask == 0 && (ps[0] != 0 || ps[..n].windows(2).any(|p| p[0] > p[1])) {
        return Err("GG Prime 位置必须有序并从零开始".into());
    }
    let speed = field(light, "speed")?;
    if speed > 65535 {
        return Err("GG Prime 速度越界".into());
    }
    let initial = rgb(&light["initial_color"])?;
    let program = if mask == 1 {
        None
    } else {
        let mut sections = vec![];

        for i in 0..n {
            let prev = if i == 0 { 0 } else { ps[i - 1] };
            if kind == 1 {
                sections.push(Section {
                    color: cs[i],
                    weight: weight(ps[i] - prev),
                });
            } else {
                let next = if i + 1 == n { 100 } else { ps[i + 1] };
                sections.push(Section {
                    color: cs[i],
                    weight: weight(ps[i] - (prev + (ps[i] - prev) / 2)),
                });
                sections.push(Section {
                    color: Rgb::new(0, 0, 0),
                    weight: weight((next - ps[i]) / 2),
                });
            }
        }
        let prev = ps[n - 1];
        let remaining = if kind == 2 {
            100 - (prev + (100 - prev) / 2)
        } else {
            100 - prev
        };
        sections.push(Section {
            color: cs[0],
            weight: weight(remaining),
        });
        let p = Program {
            initial: cs[0],
            duration: speed.max((n as u32 + 1) * 33) * 10,
            motion: Motion::default(),
            sections,
        };
        p.packet().map_err(|e| e.to_string())?;
        Some(p)
    };
    shape(&s["led_brightness"], &["level"])?;
    let brightness = field(&s["led_brightness"], "level")?
        .try_into()
        .map_err(|_| "GG Prime 亮度越界")?;
    shape(&s["lighting_enable"], &["enable"])?;
    let enabled = match field(&s["lighting_enable"], "enable")? {
        0 => false,
        1 => true,
        _ => return Err("GG Prime 灯光开关无效".into()),
    };
    let lighting = prime::Profile {
        enabled,
        color: if mask == 1 { initial } else { cs[0] },
        brightness,
        program,
    };
    let levels = stages
        .iter()
        .map(|s| Level {
            x: s.x,
            y: Some(s.y),
        })
        .collect();
    let mut settings = vec![
        Setting::Dpi(DpiState {
            stages,
            active: active as u8,
        }),
        Setting::ReportRate(rate),
        prime::setting(&lighting),
    ];
    settings.extend(
        buttons
            .into_iter()
            .map(|(id, action)| Setting::Button { id, action }),
    );
    let plan = Plan {
        vendor: 0x1038,
        product: pid,
        settings,
        macros,
        source: None,
    };
    crate::multibrand::validate(&plan)?;
    notes.push("Prime 原厂五档CPI、五实体键/双滚轮和常亮或动态灯效已转换；宏为逐鼠标软件宏。原厂14项色表按原始位置展开并保留闭环，不静默截断。实机节奏、原厂驱动性能及板载宏仍待核实。".into());
    Ok((plan, levels, active, rate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn source() -> Value {
        serde_json::from_str(include_str!(
            "../../docs/development/gg122-prime-default-settings.json"
        ))
        .unwrap()
    }
    fn fixture(s: &Value) -> Connection {
        let c = super::super::tests::fixture();
        c.execute("UPDATE devices SET product_id=272111662,name='prime'", [])
            .unwrap();
        c.execute("UPDATE configurations SET settings=?1", [s.to_string()])
            .unwrap();
        c.execute("DELETE FROM bindings", []).unwrap();
        c
    }
    #[test]
    fn prime_original_sql_default_compiles_native_profile_and_packets() {
        let s = source();
        let c = fixture(&s);
        let out = super::super::read(&c).unwrap();
        let p = out.candidates[0].profile.as_ref().unwrap();
        assert_eq!(p.target_model, Some(0x182e));
        let plan = p.multi.as_ref().unwrap();
        assert_eq!(
            p.levels.iter().map(|s| s.x).collect::<Vec<_>>(),
            [400, 800, 1200, 2400, 3200]
        );
        let profile = plan
            .settings
            .iter()
            .find_map(|s| {
                if let Setting::VendorProfile { schema, data } = s {
                    Some(prime::Profile::decode(prime::PID, schema, data).unwrap())
                } else {
                    None
                }
            })
            .unwrap();
        let program = profile.program.unwrap();
        assert_eq!(program.initial, Rgb::new(0, 255, 255));
        assert_eq!(program.duration, 10000);
        assert_eq!(
            program
                .sections
                .iter()
                .map(|s| s.weight)
                .collect::<Vec<_>>(),
            [0, 84, 84, 86]
        );
        let bytes = program.packet().unwrap().bytes;
        assert_eq!(
            &bytes[17..33],
            &[0, 255, 255, 0, 255, 0, 255, 84, 255, 0, 64, 84, 0, 255, 255, 86]
        );
        assert_eq!(
            crate::multibrand::simulate(plan).unwrap()["hardware_tested"],
            false
        );
        if let Some(path) = std::env::var_os("OPEN_HUB_GG_PRIME_PREVIEW") {
            std::fs::write(path, serde_json::to_vec_pretty(&out).unwrap()).unwrap();
        }
    }
    #[test]
    fn prime_invalid_unknown_wrong_model_and_unsupported_effects_withhold_whole_profile() {
        let cases = [
            ("/lighting_config/type", json!(3)),
            ("/lighting_config/settings_mask", json!(1)),
            ("/lighting_config/num_colors", json!(16)),
            ("/lighting_config/positions/1/pos", json!(101)),
            ("/lighting_config/speed", json!(65536)),
            ("/batch_resolution/active_level_index", json!(5)),
            ("/batch_resolution/resolution_1", json!(361)),
            ("/led_brightness/level", json!(257)),
            ("/exact_accel/level", json!(1)),
            ("/button_and_wheel_mappings/buttons/0/function", json!(0)),
        ];
        for (path, value) in cases {
            let mut s = source();
            *s.pointer_mut(path).unwrap() = value;
            let out = super::super::read(&fixture(&s)).unwrap();
            assert!(out.candidates[0].profile.is_none(), "{path}");
            assert!(!out.candidates[0].issues.is_empty());
        }
        let mut s = source();
        s["unknown"] = true.into();
        assert!(super::super::read(&fixture(&s)).unwrap().candidates[0]
            .profile
            .is_none());
        let c = fixture(&source());
        c.execute("UPDATE devices SET name='prime_plus'", [])
            .unwrap();
        assert!(super::super::read(&c).unwrap().candidates[0]
            .profile
            .is_none());
    }
    #[test]
    fn prime_physical_macro_keeps_events_and_defaults_do_not_claim_native_macros() {
        let c = fixture(&source());
        c.execute(
            "INSERT INTO bindings VALUES('macro','source-guid',?1,'source-macro',?2,0)",
            rusqlite::params![
                json!({"hidCode":5,"page":1,"type":0,"direction":1}).to_string(),
                json!({"type":0,"num_repeat":1,"repeat_delay":0}).to_string()
            ],
        )
        .unwrap();
        let out = super::super::read(&c).unwrap();
        let p = out.candidates[0].profile.as_ref().unwrap();
        let plan = p.multi.as_ref().unwrap();
        assert_eq!(plan.macros.len(), 1);
        assert_eq!(plan.macros[0].steps.len(), 5);
        assert_eq!(
            crate::multibrand::simulate(plan).unwrap()["software_macros"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        c.execute(
            "UPDATE bindings SET trigger_data=?1",
            [json!({"hidCode":49,"page":1,"type":1,"direction":1}).to_string()],
        )
        .unwrap();
        assert!(super::super::read(&c).unwrap().candidates[0]
            .profile
            .is_none());
    }

    #[test]
    fn prime_steady_and_breathe_preserve_raw_color_brightness_and_native_expansion() {
        let mut s = source();
        s["lighting_config"]["type"] = 0.into();
        s["lighting_config"]["settings_mask"] = 1.into();
        s["lighting_config"]["num_colors"] = 0.into();
        s["led_brightness"]["level"] = 37.into();
        s["lighting_enable"]["enable"] = 0.into();
        let out = super::super::read(&fixture(&s)).unwrap();
        let plan = out.candidates[0]
            .profile
            .as_ref()
            .unwrap()
            .multi
            .as_ref()
            .unwrap();
        let profile = plan
            .settings
            .iter()
            .find_map(|s| {
                if let Setting::VendorProfile { schema, data } = s {
                    Some(prime::Profile::decode(prime::PID, schema, data).unwrap())
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(profile.color, Rgb::new(255, 0, 0));
        assert_eq!(profile.brightness, 37);
        assert!(!profile.enabled);
        assert!(profile.program.is_none());
        assert_eq!(
            crate::multibrand::simulate(plan).unwrap()["hardware_tested"],
            false
        );
        s = source();
        s["lighting_config"]["type"] = 2.into();
        let out = super::super::read(&fixture(&s)).unwrap();
        let plan = out.candidates[0]
            .profile
            .as_ref()
            .unwrap()
            .multi
            .as_ref()
            .unwrap();
        let p = plan
            .settings
            .iter()
            .find_map(|s| {
                if let Setting::VendorProfile { schema, data } = s {
                    Some(
                        prime::Profile::decode(prime::PID, schema, data)
                            .unwrap()
                            .program
                            .unwrap(),
                    )
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(p.sections.len(), 7);
        assert_eq!(
            p.sections.iter().map(|s| s.weight).collect::<Vec<_>>(),
            [0, 40, 43, 40, 43, 43, 43]
        );
        for i in [1, 3, 5] {
            assert_eq!(p.sections[i].color, Rgb::new(0, 0, 0));
        }
        assert_eq!(p.sections[6].color, p.initial);
        assert_eq!(p.duration, 10000);
        assert_eq!(
            crate::multibrand::simulate(plan).unwrap()["hardware_tested"],
            false
        );
    }
    #[test]
    fn prime_breathe_capacity_and_unsupported_masks_never_truncate() {
        let mut s = source();
        s["lighting_config"]["type"] = 2.into();
        s["lighting_config"]["num_colors"] = 7.into();
        for i in 0..14 {
            s["lighting_config"]["positions"][i]["pos"] = (if i < 7 { i * 14 } else { 0 }).into();
        }
        let out = super::super::read(&fixture(&s)).unwrap();
        let plan = out.candidates[0]
            .profile
            .as_ref()
            .unwrap()
            .multi
            .as_ref()
            .unwrap();
        let data = plan
            .settings
            .iter()
            .find_map(|s| {
                if let Setting::VendorProfile { data, .. } = s {
                    Some(data)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(data["program"]["sections"].as_array().unwrap().len(), 15);
        s["lighting_config"]["num_colors"] = 8.into();
        assert!(super::super::read(&fixture(&s)).unwrap().candidates[0]
            .profile
            .is_none());
        for mask in [2, 4, 8, 16, 255] {
            let mut bad = source();
            bad["lighting_config"]["settings_mask"] = mask.into();
            assert!(super::super::read(&fixture(&bad)).unwrap().candidates[0]
                .profile
                .is_none());
        }
    }

    #[test]
    fn prime_three_reviewed_modes_export_native_import_preview() {
        let mut out = super::super::read(&fixture(&source())).unwrap();
        for steady in [false, true] {
            let mut s = source();
            s["lighting_config"]["type"] = (if steady { 0 } else { 2 }).into();
            if steady {
                s["lighting_config"]["settings_mask"] = 1.into();
                s["lighting_config"]["num_colors"] = 0.into();
            }
            let c = fixture(&s);
            c.execute(
                "UPDATE configurations SET id=?1",
                [if steady {
                    "steady-source"
                } else {
                    "breathe-source"
                }],
            )
            .unwrap();
            let mut next = super::super::read(&c).unwrap();
            assert!(next.candidates[0].profile.is_some());
            out.candidates.append(&mut next.candidates);
        }
        assert_eq!(out.candidates.len(), 3);
        if let Some(path) = std::env::var_os("OPEN_HUB_GG_PRIME_MODES_PREVIEW") {
            std::fs::write(path, serde_json::to_vec_pretty(&out).unwrap()).unwrap();
        }
    }

    #[test]
    fn prime_golisp_float32_percent_weights_cover_entire_domain() {
        for percent in 0..=100 {
            assert_eq!(weight(percent), ((percent * 255) / 100) as u8, "{percent}");
        }
        assert_eq!(weight(100), 255);
        assert_eq!((100f64 * 2.55) as u8, 254);
        for kind in [1, 2] {
            let mut s = source();
            s["lighting_config"]["type"] = kind.into();
            s["lighting_config"]["num_colors"] = 1.into();
            let out = super::super::read(&fixture(&s)).unwrap();
            let plan = out.candidates[0]
                .profile
                .as_ref()
                .unwrap()
                .multi
                .as_ref()
                .unwrap();
            let data = plan
                .settings
                .iter()
                .find_map(|s| {
                    if let Setting::VendorProfile { data, .. } = s {
                        Some(data)
                    } else {
                        None
                    }
                })
                .unwrap();
            let profile = prime::Profile::decode(prime::PID, prime::SCHEMA, data).unwrap();
            let program = profile.program.unwrap();
            assert_eq!(
                program
                    .sections
                    .iter()
                    .map(|s| s.weight)
                    .collect::<Vec<_>>(),
                if kind == 1 {
                    vec![0, 255]
                } else {
                    vec![0, 127, 127]
                }
            );
            assert_eq!(
                crate::multibrand::simulate(plan).unwrap()["hardware_tested"],
                false
            );
        }
    }
}
