//! Regression inputs are real read payloads/DTOs, not simulated success outputs.
use super::*;
use std::sync::{Arc, Mutex};
fn facts() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../tests/fixtures/mchose-dpi-physical-2026-10-27.json"
    ))
    .unwrap()
}
#[test]
fn captured_inactive_bank_requests_return_current_bank_and_are_never_relabelled() {
    let f = facts();
    for entry in f["direct_dpi_gets"].as_array().unwrap() {
        let board = entry["board"].as_u64().unwrap() as u8;
        let axis = entry["axis"].as_u64().unwrap() as u8;
        let payload: Vec<u8> = serde_json::from_value(entry["payload"].clone()).unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let used = log.clone();
        let t = Transport::memory(
            move |cmd, args| {
                used.lock().unwrap().push((cmd, args.to_vec()));
                assert_eq!(cmd, 3);
                Ok(payload.clone())
            },
            std::env::temp_dir().join(crate::profiles::new_id()),
        );
        let result = t.dpi_table(board, axis);
        if board == 0 {
            assert_eq!(result.unwrap()[0], 0);
        } else {
            assert!(result
                .err()
                .unwrap()
                .to_string()
                .contains("未把其他组数据当作本组"));
        }
        assert_eq!(log.lock().unwrap().len(), 1);
    }
}
#[test]
fn recorded_linked_roundtrip_compiles_factory_empty_names_and_preserves_other_banks() {
    let f = facts();
    let before: Snapshot = serde_json::from_value(f["before"].clone()).unwrap();
    let observed: Snapshot = serde_json::from_value(f["observed"].clone()).unwrap();
    let after: Snapshot = serde_json::from_value(f["after"].clone()).unwrap();
    let keys: crate::mhub_transaction::Keys =
        serde_json::from_value(f["keys_before"].clone()).unwrap();
    assert!(mirrored_linked_axes(&before.state));
    assert!(before == after);
    assert_eq!(f["banks_before"], f["banks_after"]);
    let active = before.state.active as usize;
    assert_eq!(
        (before.state.dpi_x[active], observed.state.dpi_x[active]),
        (1600, 1650)
    );
    assert_eq!(observed.state.dpi_y, observed.state.dpi_x);
    let forward = crate::mchose_backup::compile(&observed, &before, &keys).unwrap();
    assert!(forward.snapshot == observed);
    assert!(forward.macro_writes.is_empty());
    let reverse = crate::mchose_backup::compile(&before, &observed, &keys).unwrap();
    assert!(reverse.snapshot == before);
    assert!(reverse.macro_writes.is_empty());
    let mut other = before.state.clone();
    other.firmware = "18".into();
    assert!(!mirrored_linked_axes(&other));
}
