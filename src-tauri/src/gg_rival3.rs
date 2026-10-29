//! Original Rival3 GG122 private schema. No device or vendor code execution.
use super::*;
use gearwise_core::model::{Rgb, ScrollDirection};
use gearwise_steelseries::rival3::{self, Profile as Lights};
fn shape(v: &Value, keys: &[&str]) -> Result<(), String> {
    let o = v.as_object().ok_or("GG Rival3 记录不是对象")?;
    if o.keys().any(|k| !keys.contains(&k.as_str())) {
        return Err("GG Rival3 包含未核对字段，未导入部分方案".into());
    }
    Ok(())
}
fn code(v: &Value) -> Result<u8, String> {
    shape(v, &["function", "key_codes"])?;
    let a = v
        .get("key_codes")
        .and_then(Value::as_array)
        .ok_or("GG Rival3 缺少单字节键码")?;
    if a.len() != 1 {
        return Err("GG Rival3 键码必须恰好一字节".into());
    }
    number(&a[0])?
        .try_into()
        .map_err(|_| "GG Rival3 键码超出字节范围".into())
}
fn dpi(s: &Value) -> Result<(Vec<DpiStage>, usize), String> {
    let r = s
        .get("set_all_resolution")
        .ok_or("GG Rival3 缺少完整 CPI 表")?;
    shape(
        r,
        &[
            "level_active",
            "level_max",
            "resolution_1",
            "resolution_2",
            "resolution_3",
            "resolution_4",
            "resolution_5",
        ],
    )?;
    let count = field(r, "level_max")?;
    let active = field(r, "level_active")?;
    if !(1..=5).contains(&count) || !(1..=count).contains(&active) {
        return Err("GG Rival3 CPI 数量或活动档无效".into());
    }
    let mut stages = Vec::new();
    for i in 1..=5 {
        let c: u8 = field(r, &format!("resolution_{i}"))?
            .try_into()
            .map_err(|_| "GG Rival3 CPI 编码越界")?;
        let value = rival3::dpi_from_code(c);
        if i <= count {
            stages.push(DpiStage::uniform(value.ok_or("GG Rival3 CPI 编码未核对")?));
        } else if c != 0 && value.is_none() {
            return Err("GG Rival3 未启用槽也含未知 CPI 编码".into());
        }
    }
    if let Some(raw) = s.get("raw_cpi") {
        shape(raw, &["level1", "level2"])?;
        for value in raw.as_object().unwrap().values() {
            let d = number(value)?;
            if d != 0 && !stages.iter().any(|s| u32::from(s.x) == d) {
                return Err("GG Rival3 显示 CPI 与启用档位不一致".into());
            }
        }
    }
    Ok((stages, (active - 1) as usize))
}
fn lights(s: &Value) -> Result<Setting, String> {
    let colors = s
        .get("set_all_colors")
        .ok_or("GG Rival3 缺少四区颜色记录")?;
    shape(
        colors,
        &[
            "zone_mask",
            "z1_color",
            "z2_color",
            "z3_color",
            "logo_color",
        ],
    )?;
    if field(colors, "zone_mask")? != 15 {
        return Err("GG Rival3 需要明确包含全部四区的颜色方案".into());
    }
    let mut p = Lights::default();
    for (i, name) in ["z1", "z2", "z3", "logo"].iter().enumerate() {
        let value = colors
            .get(format!("{name}_color"))
            .ok_or("GG Rival3 缺少灯区颜色")?;
        shape(value, &["red", "green", "blue"])?;
        let channel = |k| -> Result<u8, String> {
            field(value, k)?
                .try_into()
                .map_err(|_| "GG Rival3 颜色越界".into())
        };
        let rgb = Rgb {
            r: channel("red")?,
            g: channel("green")?,
            b: channel("blue")?,
        };
        p.zones[i].color = rgb;
        let sequence = s
            .get(format!("rival_{name}_color_sequence"))
            .ok_or("GG Rival3 缺少灯区颜色序列")?;
        let sequence = gearwise_steelseries::rival3_sequence::Sequence::decode(sequence)
            .map_err(|e| e.to_string())?;
        if !matches!(sequence.repeat, 0 | 255) {
            return Err("GG Rival3 颜色序列播放方式尚未转换".into());
        }
        for frame in &sequence.frames {
            if *frame != [rgb.r, rgb.g, rgb.b] {
                return Err(
                    "GG Rival3 动态主机颜色序列尚未完整移植，不能冒充原生预设或只导入 DPI".into(),
                );
            }
        }
        if !sequence.frames.is_empty() && sequence.repeat != 255 {
            return Err("GG Rival3 有限颜色序列的停止行为尚未转换".into());
        }
    }
    // All-black steady and explicit disable are visually equivalent; preserve the
    // desired exact RGB palette in a native steady profile, without inventing state.
    Ok(rival3::setting(&p))
}
pub(super) fn convert(
    conn: &Connection,
    id: &str,
    pid: u16,
    s: &Value,
    notes: &mut Vec<String>,
) -> Result<(Plan, Vec<Level>, usize, u16), String> {
    shape(
        s,
        &[
            "button_mappings",
            "exact_accel",
            "exact_aim",
            "free_move",
            "polling_rate",
            "raw_cpi",
            "set_all_resolution",
            "set_all_colors",
            "rival_logo_color_sequence",
            "rival_z1_color_sequence",
            "rival_z2_color_sequence",
            "rival_z3_color_sequence",
        ],
    )?;
    for name in ["exact_accel", "exact_aim", "free_move"] {
        let value = s.get(name).ok_or("GG Rival3 缺少性能记录")?;
        shape(value, &["level"])?;
        if field(value, "level")? != 0 {
            return Err(format!("GG Rival3 {name} 的非默认原厂驱动行为尚未转换"));
        }
    }
    let (stages, active) = dpi(s)?;
    let rate = s.get("polling_rate").ok_or("GG Rival3 缺少回报率")?;
    shape(rate, &["level", "hz"])?;
    let rate = match field(rate, "level")? {
        1 => 1000,
        2 => 500,
        3 => 250,
        4 => 125,
        _ => return Err("GG Rival3 回报率编码未核对".into()),
    };
    if s.pointer("/polling_rate/hz")
        .is_some_and(|v| v.as_u64() != Some(u64::from(rate)))
    {
        return Err("GG Rival3 回报率显示与编码冲突".into());
    }
    let mapping = s.get("button_mappings").ok_or("GG Rival3 缺少八输入表")?;
    shape(mapping, &["buttons", "no_live_deploy"])?;
    if mapping
        .get("no_live_deploy")
        .is_some_and(|v| !matches!(v.as_u64(), Some(0 | 1)))
    {
        return Err("GG Rival3 部署标志无效".into());
    }
    let inputs = mapping
        .get("buttons")
        .and_then(Value::as_array)
        .ok_or("GG Rival3 输入表无效")?;
    if inputs.len() != 8 {
        return Err("GG Rival3 需要六键加滚轮上下的八条记录".into());
    }
    let mut buttons = BTreeMap::new();
    let mut macros = Vec::new();
    for (i, value) in inputs.iter().enumerate() {
        let key = code(value)?;
        let function = field(value, "function")?;
        let button = i as u16 + 1;
        let action = match function {
            0 | 1..=5 | 48 | 49 | 50 => {
                if key != 0 {
                    return Err("GG Rival3 非键盘动作包含附加键码".into());
                }
                match function {
                    0 => ButtonAction::Disabled,
                    1..=5 => ButtonAction::Mouse(mouse(function)?),
                    48 => ButtonAction::Dpi(DpiFunction::Cycle),
                    49 => ButtonAction::Scroll(ScrollDirection::Up),
                    _ => ButtonAction::Scroll(ScrollDirection::Down),
                }
            }
            51 => ButtonAction::Key {
                usage: key,
                mods: Modifiers(0),
            },
            52 => ButtonAction::Media(u16::from(key)),
            _ => {
                return Err(format!(
                    "GG Rival3 功能 {function} 尚未转换，未保存部分方案"
                ))
            }
        };
        buttons.insert(button, action);
    }
    apply_bindings(conn, id, pid, 6, &mut buttons, &mut macros)?;
    if buttons.get(&1) != Some(&ButtonAction::Mouse(MouseButton::Left)) {
        return Err("GG Rival3 导入保留主左键点击".into());
    }
    buttons.remove(&1);
    let lighting = lights(s)?;
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
    ];
    settings.extend(
        buttons
            .into_iter()
            .map(|(id, action)| Setting::Button { id, action }),
    );
    settings.push(lighting);
    let plan = Plan {
        vendor: 0x1038,
        product: pid,
        settings,
        macros,
        source: None,
    };
    crate::multibrand::validate(&plan)?;
    notes.push("原版有线 Rival 3 的八输入、专用 CPI 编码和四区常亮已转换；宏为逐鼠标软件宏。非恒定颜色序列需原厂主机持续播放，目前会阻止整份导入；不替换成板载灯效。".into());
    Ok((plan, levels, active, rate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn settings() -> Value {
        let mut s = json!({"set_all_resolution":{"level_active":2,"level_max":2,"resolution_1":18,"resolution_2":36,"resolution_3":46,"resolution_4":111,"resolution_5":167},"raw_cpi":{"level1":800,"level2":1600},"polling_rate":{"level":1,"hz":1000},"exact_accel":{"level":0},"exact_aim":{"level":0},"free_move":{"level":0},"button_mappings":{"no_live_deploy":1,"buttons":[{"function":1,"key_codes":[0]},{"function":2,"key_codes":[0]},{"function":3,"key_codes":[0]},{"function":51,"key_codes":[6]},{"function":52,"key_codes":[234]},{"function":48,"key_codes":[0]},{"function":49,"key_codes":[0]},{"function":50,"key_codes":[0]}]},"set_all_colors":{"zone_mask":15}});
        for (i, name) in ["z1", "z2", "z3", "logo"].iter().enumerate() {
            s["set_all_colors"][format!("{name}_color")] = json!({"red":i+1,"green":34,"blue":51});
            s[format!("rival_{name}_color_sequence")] = json!({"data":[[i+1,34,51]],"repeat":255});
        }
        s
    }
    fn fixture(s: &Value, pid: u16) -> Connection {
        let c = super::super::tests::fixture();
        c.execute(
            "UPDATE devices SET product_id=?1,name='rival_3'",
            [0x10380000u32 + u32::from(pid)],
        )
        .unwrap();
        c.execute("UPDATE configurations SET settings=?1", [s.to_string()])
            .unwrap();
        c.execute("DELETE FROM bindings", []).unwrap();
        c
    }
    fn profile(s: &Value, pid: u16) -> Profile {
        read(&fixture(s, pid))
            .unwrap()
            .candidates
            .remove(0)
            .profile
            .unwrap()
    }
    #[test]
    fn original_rival3_gg_native_fields_and_four_color_plan_reach_exact_codec() {
        for pid in [0x1824, 0x184c] {
            let p = profile(&settings(), pid);
            assert_eq!(p.target_model, Some(pid));
            assert_eq!(p.active_level, 1);
            assert_eq!(
                p.levels.iter().map(|l| l.x).collect::<Vec<_>>(),
                [800, 1600]
            );
            let plan = p.multi.as_ref().unwrap();
            assert_eq!(plan.settings.len(), 10);
            let result = crate::multibrand::simulate(plan).unwrap();
            assert_eq!(result["hardware_tested"], false);
            let (mut d, log) = gearwise_steelseries::simulated_model(pid).unwrap();
            for s in &plan.settings {
                d.apply(s).unwrap();
            }
            let packets = log.lock().unwrap();
            assert_eq!(
                packets[0],
                gearwise_core::transport::MockRecord::Write(vec![0, 11, 0, 2, 2, 18, 36])
            );
            drop(packets);
            let state = d.read_state().unwrap();
            assert_eq!(state.buttons[&7], ButtonAction::Scroll(ScrollDirection::Up));
            assert_eq!(
                state.buttons[&8],
                ButtonAction::Scroll(ScrollDirection::Down)
            );
            assert_eq!(
                state.buttons[&4],
                ButtonAction::Key {
                    usage: 6,
                    mods: Modifiers(0)
                }
            );
            assert_eq!(
                state.vendor_profile.as_ref().unwrap()["zones"][3]["color"],
                json!({"r":4,"g":34,"b":51})
            );
        }
    }
    #[test]
    fn all_84_cpi_codes_match_independent_oem_table_and_unknown_codes_are_not_guessed() {
        let facts: Value = serde_json::from_str(include_str!(
            "../../docs/development/gg122-rival3-facts.json"
        ))
        .unwrap();
        let pairs = facts["dpi_code_pairs"].as_array().unwrap();
        assert_eq!(pairs.len(), 85);
        for pair in pairs.iter().skip(1) {
            let code = pair[0].as_u64().unwrap() as u8;
            let want = pair[1].as_u64().unwrap() as u16;
            assert_eq!(rival3::dpi_from_code(code), Some(want));
            let mut s = settings();
            s["set_all_resolution"]["resolution_1"] = json!(code);
            s["set_all_resolution"]["level_max"] = json!(1);
            s["set_all_resolution"]["level_active"] = json!(1);
            s["raw_cpi"] = json!({"level1":want,"level2":want});
            assert_eq!(profile(&s, 0x1824).levels[0].x, want);
        }
        for code in 0..=255u8 {
            if !pairs.iter().skip(1).any(|p| p[0] == code) {
                assert_eq!(rival3::dpi_from_code(code), None);
            }
        }
    }
    #[test]
    fn full_five_stage_order_and_selected_slot_survive_without_frontend_sorting() {
        let mut s = settings();
        s["set_all_resolution"] = json!({"level_active":4,"level_max":5,"resolution_1":197,"resolution_2":4,"resolution_3":36,"resolution_4":18,"resolution_5":8});
        s["raw_cpi"] = json!({"level1":800,"level2":800});
        let p = profile(&s, 0x1824);
        assert_eq!(
            p.levels.iter().map(|l| l.x).collect::<Vec<_>>(),
            [8500, 200, 1600, 800, 400]
        );
        assert_eq!(p.active_level, 3);
    }
    #[test]
    fn malformed_or_dynamic_rival3_settings_never_save_a_partial_candidate() {
        for case in 0..17 {
            let mut s = settings();
            match case {
                0 => s["set_all_resolution"]["level_max"] = json!(0),
                1 => s["set_all_resolution"]["level_active"] = json!(3),
                2 => s["set_all_resolution"]["resolution_1"] = json!(19),
                3 => s["raw_cpi"]["level1"] = json!(801),
                4 => s["polling_rate"]["hz"] = json!(500),
                5 => s["button_mappings"]["buttons"][3]["key_codes"] = json!([6, 0, 0, 0]),
                6 => s["button_mappings"]["buttons"][7]["function"] = json!(53),
                7 => s["set_all_colors"]["zone_mask"] = json!(7),
                8 => s["rival_logo_color_sequence"]["data"] = json!([[4, 34, 51], [7, 8, 9]]),
                9 => s["rival_z2_color_sequence"]["repeat"] = json!(1),
                10 => s["exact_aim"]["level"] = json!(2),
                11 => s["unknown"] = json!(1),
                12 => s["button_mappings"]["buttons"][1]["key_codes"] = json!([1]),
                13 => s["set_all_colors"]["z1_color"]["red"] = json!(256),
                14 => s
                    .as_object_mut()
                    .unwrap()
                    .remove("rival_z1_color_sequence")
                    .map(|_| ())
                    .unwrap(),
                15 => s["button_mappings"]["buttons"] = json!([]),
                _ => s["button_mappings"]["buttons"][0]["function"] = json!(0),
            }
            let r = read(&fixture(&s, 0x1824)).unwrap();
            assert!(r.candidates[0].profile.is_none(), "case {case}");
            assert!(!r.candidates[0].issues.is_empty());
        }
    }
    #[test]
    fn rival3_wheel_and_physical_macro_bindings_keep_original_events_and_trigger_identity() {
        let mut candidates = Vec::new();
        for (pid, trigger, kind, input) in
            [(0x1824, 6, 0, 6), (0x184c, 49, 1, 7), (0x1824, 50, 1, 8)]
        {
            let c = fixture(&settings(), pid);
            c.execute(
                "INSERT INTO bindings VALUES('macro','source-guid',?1,'source-macro',?2,0)",
                rusqlite::params![
                    json!({"hidCode":trigger,"page":1,"type":kind,"direction":1}).to_string(),
                    json!({"type":0,"num_repeat":1,"repeat_delay":15}).to_string()
                ],
            )
            .unwrap();
            let prepared = read(&c).unwrap();
            let p = prepared.candidates[0].profile.as_ref().unwrap();
            let plan = p.multi.as_ref().unwrap();
            assert!(plan.settings.contains(&Setting::Button {
                id: input,
                action: ButtonAction::Macro(plan.macros[0].id.clone())
            }));
            assert_eq!(plan.macros[0].steps.len(), 5);
            assert_eq!(
                crate::multibrand::simulate(plan).unwrap()["software_macros"][0]
                    ["virtual_delay_ms"],
                23
            );
            let mut candidate = prepared.candidates.into_iter().next().unwrap();
            let p = candidate.profile.as_mut().unwrap();
            p.id.push_str(&format!(":fixture-{input}"));
            p.name = format!("Rival3 输入 {input} 原始宏");
            candidates.push(candidate);
        }
        let mut bad = settings();
        bad["rival_logo_color_sequence"]["data"] = json!([[1, 2, 3], [4, 5, 6]]);
        candidates.push(read(&fixture(&bad, 0x1824)).unwrap().candidates.remove(0));
        if let Some(path) = std::env::var_os("OPEN_HUB_GG_RIVAL3_PREVIEW") {
            std::fs::write(
                path,
                serde_json::to_vec_pretty(&Prepared {
                    name: "Rival3 原厂格式方案".into(),
                    source_kind: "GG SQLite",
                    candidates,
                })
                .unwrap(),
            )
            .unwrap();
        }
    }
    #[test]
    fn foreign_model_and_wrong_wheel_trigger_types_do_not_borrow_rival3_layout() {
        let s = settings();
        for pid in [0x1720, 0x1830] {
            let c = fixture(&s, pid);
            assert!(read(&c).unwrap().candidates[0].profile.is_none());
        }
        for (kind, code) in [(0, 49), (1, 6), (1, 7), (2, 49)] {
            let c = fixture(&s, 0x1824);
            c.execute(
                "INSERT INTO bindings VALUES('macro','source-guid',?1,'source-macro',?2,0)",
                rusqlite::params![
                    json!({"hidCode":code,"page":1,"type":kind,"direction":1}).to_string(),
                    json!({"type":0,"num_repeat":1,"repeat_delay":15}).to_string()
                ],
            )
            .unwrap();
            assert!(read(&c).unwrap().candidates[0].profile.is_none());
        }
    }
    #[test]
    fn rival3_database_preview_is_read_only_and_preserves_its_program_condition() {
        let c = fixture(&settings(), 0x1824);
        c.execute_batch("INSERT INTO applications VALUES(9,'C:/Games/Rival3.exe');INSERT INTO apps_configs VALUES('source-guid',9,250);").unwrap();
        let path = std::env::temp_dir().join(format!(
            "open-hub-rival3-gg-{}.db",
            crate::profiles::new_id()
        ));
        c.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        let p = prepare(&path)
            .unwrap()
            .candidates
            .remove(0)
            .profile
            .unwrap();
        assert_eq!(p.app_path.as_deref(), Some("C:/Games/Rival3.exe"));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn rival3_each_native_action_on_every_nonprimary_input_remains_exact_after_import() {
        for input in 2..=8 {
            for (function, key) in [
                (0, 0),
                (1, 0),
                (2, 0),
                (3, 0),
                (4, 0),
                (5, 0),
                (48, 0),
                (49, 0),
                (50, 0),
                (51, 6),
                (52, 234),
            ] {
                let mut s = settings();
                s["button_mappings"]["buttons"][input - 1] =
                    json!({"function":function,"key_codes":[key]});
                let p = profile(&s, 0x1824);
                let plan = p.multi.as_ref().unwrap();
                let compiled = crate::multibrand::simulate(plan).unwrap();
                assert_eq!(compiled["hardware_tested"], false);
                assert!(plan.macros.is_empty());
            }
        }
    }
    #[test]
    #[ignore = "requires inert official GG122 original Rival3 SQL"]
    fn official_rival3_default_is_recognized_but_dynamic_host_sequences_are_explicitly_unconverted()
    {
        let path = std::env::var_os("OPEN_HUB_GG_RIVAL3_SQL").unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        let tag = text.find("'rival_3'").unwrap();
        let start = text[..tag].rfind("VALUES (").unwrap();
        let t = &text[start..];
        let a = t.find("'{").unwrap() + 1;
        let mut parser = serde_json::Deserializer::from_str(&t[a..]).into_iter::<Value>();
        let s = parser.next().unwrap().unwrap();
        let r = read(&fixture(&s, 0x1824)).unwrap();
        assert!(r.candidates[0].profile.is_none());
        assert!(
            r.candidates[0].issues[0].contains("动态主机颜色序列"),
            "{:?}",
            r.candidates[0].issues
        );
    }
}

#[cfg(test)]
mod browser_replay {
    #[test]
    #[ignore = "requires current GG Rival3 UI saved profiles"]
    fn gg_rival3_browser_edits_reimport_and_replay_exact_native_codec() {
        let ps = crate::multibrand::roundtrip_steelseries_browser_profiles(
            "OPEN_HUB_GG_RIVAL3_UI_PROFILES",
        );
        assert_eq!(ps.len(), 3);
        for p in ps {
            let plan = p.multi.as_ref().unwrap();
            assert!(gearwise_steelseries::rival3::supported(plan.product));
            assert_eq!(p.levels[0].x, 200);
            let result = crate::multibrand::simulate(plan).unwrap();
            assert_eq!(result["software_macros"][0]["virtual_delay_ms"], 23);
            assert_eq!(result["hardware_tested"], false);
            let light = plan
                .settings
                .iter()
                .find_map(|s| {
                    if let gearwise_core::Setting::VendorProfile { data, .. } = s {
                        Some(data)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert_eq!(light["mode"], 4);
            assert_eq!(
                light["zones"][3]["color"],
                serde_json::json!({"r":171,"g":205,"b":239})
            );
        }
    }
}
