use super::*;
use crate::mchose_macro_scope as scope;
fn shared() -> Rig {
    let rig = Rig::new(canonical());
    {
        let mut m = rig.mouse.lock().unwrap();
        let other = m.state.buttons.clone();
        m.other_keys = Some([other.clone(), other]);
        m.other_keys.as_mut().unwrap()[0][4] = vec![0x21, 2, 0];
        m.state.buttons[3] = vec![0x21, 2, 0];
        m.state.buttons[4] = vec![0x21, 2, 0];
        m.state.raw_buttons = m.state.buttons.concat();
    }
    rig
}
fn edit() -> MouseMacro {
    macro_record(2, "Edited private", 2, &sequence(), None).unwrap()
}
#[test]
fn shared_edit_forks_and_rebinds_only_current_board_and_keeps_other_mouse() {
    let rig = shared();
    let other = shared();
    let state = rig.transport.hardware_state().unwrap();
    let other_state = other.mouse.lock().unwrap().state.clone();
    let others = rig.mouse.lock().unwrap().other_keys.clone();
    let (actual, saved) = scope::save(&rig.transport, 0, edit()).unwrap();
    assert_eq!(saved.slot, 0);
    assert_eq!(saved.raw, edit().raw);
    assert_eq!(actual.buttons[3], vec![0x21, 0, 0]);
    assert_eq!(actual.buttons[4], vec![0x21, 0, 0]);
    let m = rig.mouse.lock().unwrap();
    assert_eq!(m.macros[&2], canonical().raw);
    assert_eq!(m.macros[&0], edit().raw);
    assert_eq!(m.other_keys, others);
    assert_eq!(m.binding_writes.len(), 2);
    let mut expected = state.clone();
    expected.buttons = actual.buttons.clone();
    expected.raw_buttons = actual.raw_buttons.clone();
    assert!(actual == expected);
    assert_eq!(rig.backups().len(), 1);
    drop(m);
    let m = other.mouse.lock().unwrap();
    assert!(m.state == other_state);
    assert!(m.writes.is_empty());
    assert!(m.binding_writes.is_empty());
    assert_eq!(m.macros.len(), 1);
}
#[test]
fn unbound_shared_slot_is_copied_without_creating_any_binding() {
    let rig = shared();
    {
        let mut m = rig.mouse.lock().unwrap();
        m.state.buttons[3] = vec![0, 8, 0];
        m.state.buttons[4] = vec![0, 16, 0];
        m.state.raw_buttons = m.state.buttons.concat();
    }
    let before = rig.transport.hardware_state().unwrap();
    let (actual, saved) = scope::save(&rig.transport, 0, edit()).unwrap();
    assert_eq!(saved.slot, 0);
    assert!(actual == before);
    assert!(rig.mouse.lock().unwrap().binding_writes.is_empty());
}
#[test]
fn identical_existing_content_is_reused_without_upload_or_overwrite() {
    let rig = shared();
    rig.mouse.lock().unwrap().macros.insert(7, edit().raw);
    let (_, saved) = scope::save(&rig.transport, 0, edit()).unwrap();
    assert_eq!(saved.slot, 7);
    let m = rig.mouse.lock().unwrap();
    assert!(m.writes.is_empty());
    assert_eq!(m.macros[&2], canonical().raw);
    assert_eq!(m.macros.len(), 2);
    assert_eq!(m.state.buttons[4], vec![33, 7, 0]);
}
#[test]
fn full_slots_reject_shared_edit_without_backup_or_native_set() {
    let rig = shared();
    {
        let mut m = rig.mouse.lock().unwrap();
        for slot in 0..18 {
            m.macros.insert(
                slot,
                macro_record(slot, &format!("Occupied {slot}"), 2, &sequence(), None)
                    .unwrap()
                    .raw,
            );
        }
    }
    let before = rig.mouse.lock().unwrap().macros.clone();
    let error = scope::save(&rig.transport, 0, edit()).err().unwrap();
    assert!(error.to_string().contains("没有安全可用宏槽"));
    let m = rig.mouse.lock().unwrap();
    assert_eq!(m.macros, before);
    assert!(m.writes.is_empty());
    assert!(m.binding_writes.is_empty());
    assert!(!rig.root.join("backups").exists());
}
#[test]
fn shared_identical_save_is_a_noop_without_backup_or_upload() {
    let rig = shared();
    let before = rig.transport.hardware_state().unwrap();
    let (actual, saved) = scope::save(&rig.transport, 0, canonical()).unwrap();
    assert_eq!(saved.slot, 2);
    assert!(actual == before);
    let m = rig.mouse.lock().unwrap();
    assert!(m.writes.is_empty());
    assert!(m.binding_writes.is_empty());
    assert!(!rig.root.join("backups").exists());
}
#[test]
fn unknown_other_board_action_prevents_allocation_but_allows_identical_reuse() {
    let rig = shared();
    rig.mouse.lock().unwrap().other_keys.as_mut().unwrap()[1][3] = vec![0x55, 2, 0];
    assert!(scope::save(&rig.transport, 0, edit()).is_err());
    assert!(rig.mouse.lock().unwrap().writes.is_empty());
    assert!(!rig.root.join("backups").exists());
    rig.mouse.lock().unwrap().macros.insert(7, edit().raw);
    assert_eq!(scope::save(&rig.transport, 0, edit()).unwrap().1.slot, 7);
    assert!(rig.mouse.lock().unwrap().writes.is_empty());
}
#[test]
fn dangling_other_board_reference_is_not_treated_as_an_empty_slot() {
    let rig = shared();
    rig.mouse.lock().unwrap().other_keys.as_mut().unwrap()[1][3] = vec![0x21, 0, 0];
    let saved = scope::save(&rig.transport, 0, edit()).unwrap().1;
    assert_eq!(saved.slot, 1);
    assert!(!rig.mouse.lock().unwrap().macros.contains_key(&0));
}
#[test]
fn new_other_board_reference_during_upload_stops_chunks_and_binding_changes() {
    let rig = shared();
    rig.mouse.lock().unwrap().change_reference_after_chunk = true;
    let error = scope::save(&rig.transport, 0, edit())
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("原始备份"));
    assert!(error.contains("引用已改变"));
    let m = rig.mouse.lock().unwrap();
    assert_eq!(m.writes.len(), 1);
    assert!(m.binding_writes.is_empty());
    assert_eq!(m.macros[&2], canonical().raw);
    assert!(!m.macros.contains_key(&0));
    assert_eq!(m.other_keys.as_ref().unwrap()[0][4], vec![0x21, 0, 0]);
    assert_eq!(rig.backups().len(), 1);
}

#[test]
#[ignore = "requires built UI request through OPEN_HUB_MCHOSE_MACRO_SCOPE_REQUEST"]
fn mchose_macro_scope_ui_bridge() {
    use serde_json::json;
    let request: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("OPEN_HUB_MCHOSE_MACRO_SCOPE_REQUEST").unwrap()).unwrap(),
    )
    .unwrap();
    let rig = shared();
    let original = canonical().raw;
    match request["scenario"].as_str().unwrap_or("fork") {
        "full" => {
            let mut m = rig.mouse.lock().unwrap();
            for slot in 0..18 {
                m.macros.insert(
                    slot,
                    macro_record(slot, &format!("Occupied {slot}"), 2, &sequence(), None)
                        .unwrap()
                        .raw,
                );
            }
            m.macros.insert(2, original.clone());
        }
        "dedupe" => {
            rig.mouse.lock().unwrap().macros.insert(7, edit().raw);
        }
        "fork" => {}
        _ => panic!("unknown scenario"),
    }
    let device = crate::mchose_target::simulated_device("macro-scope-A");
    let value = (|| -> Result<serde_json::Value> {
        if request["operation"] == "hardware" {
            return serde_json::to_value(crate::mchose_target::read_hardware(
                &device,
                &rig.transport,
                &device.connection_key,
            )?)
            .map_err(|e| bad(e.to_string()));
        }
        let args = &request["args"];
        assert_eq!(args["id"], device.id);
        let target: crate::mchose_target::Target =
            serde_json::from_value(args["target"].clone()).unwrap();
        let action: Action = serde_json::from_value(args["action"].clone()).unwrap();
        crate::mchose_target::with_target(&device, &rig.transport, &target, |t| {
            let Action::Macro {
                slot,
                name,
                mode,
                events,
                raw,
            } = action
            else {
                panic!("expected macro action")
            };
            let record = macro_record(slot, &name, mode, &events, raw.as_deref())?;
            let (state, saved) =
                scope::save(t, args["expectedProfile"].as_u64().unwrap() as u8, record)?;
            serde_json::to_value(ApplyResult {
                state,
                saved_macro: Some(saved),
                target: target.clone(),
            })
            .map_err(|e| bad(e.to_string()))
        })
    })();
    let m = rig.mouse.lock().unwrap();
    assert_eq!(m.macros[&2], original);
    assert_eq!(m.other_keys.as_ref().unwrap()[0][4], vec![33, 2, 0]);
    let slots = m.macros.keys().copied().collect::<Vec<_>>();
    let audit = json!({"macro_set_reports":m.writes.len(),"binding_set_reports":m.binding_writes.len(),"other_boards_preserved":true,"original_macro_preserved":true,"has_backup":rig.root.join("backups").exists()});
    drop(m);
    let macros = slots
        .into_iter()
        .map(|slot| rig.transport.read_macro(slot).unwrap().unwrap())
        .collect::<Vec<_>>();
    let response = match value {
        Ok(value) => json!({"ok":true,"value":value,"audit":audit,"macros":macros}),
        Err(e) => json!({"ok":false,"error":e.to_string(),"audit":audit,"macros":macros}),
    };
    std::fs::write(
        std::env::var("OPEN_HUB_MCHOSE_MACRO_SCOPE_RESPONSE").unwrap(),
        serde_json::to_vec_pretty(&response).unwrap(),
    )
    .unwrap();
}
