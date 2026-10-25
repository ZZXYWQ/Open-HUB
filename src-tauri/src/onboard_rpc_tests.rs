//! Executes production on-board operations at the HID++ feature-request
//! boundary. No HID handle, simulated OS input or physical mouse is opened.
use super::*;
use crate::{
    device::FeatureInfo,
    hidpp::FeatureRequest,
    onboard_bindings::{Action, Change, Layer, Settings},
};
use std::{collections::BTreeMap, ffi::CString, sync::Mutex};
#[derive(Default)]
struct State {
    sectors: BTreeMap<u16, Vec<u8>>,
    staging: Option<(u16, usize, Vec<u8>)>,
    calls: Vec<(u8, Vec<u8>)>,
    fault: Option<(u8, usize)>,
    seen: usize,
    change_directory_on_fault: bool,
    mode: u8,
    active: u8,
    reload_fail: bool,
    corrupt_readback: bool,
    committed: bool,
    small_reply: bool,
}
struct Memory(Mutex<State>);
fn crc(mut b: Vec<u8>) -> Vec<u8> {
    let n = b.len();
    let c = crc_ccitt(&b[..n - 2]);
    b[n - 2..].copy_from_slice(&c.to_be_bytes());
    b
}
fn profile() -> Vec<u8> {
    let mut b = (0..256).map(|n| (n % 251) as u8).collect::<Vec<_>>();
    b[0] = 1;
    b[1] = 0;
    b[2] = 0;
    b[3..13].copy_from_slice(&[0x20, 3, 0x40, 6, 0x80, 12, 0, 0, 0, 0]);
    b[0x20..0x24].copy_from_slice(&[128, 1, 0, 1]);
    b[0x30..0x34].copy_from_slice(&[0, 3, 0, 5]);
    crc(b)
}
fn memory() -> Memory {
    let mut dir = vec![255; 256];
    dir[..8].copy_from_slice(&[0, 1, 1, 0, 0, 2, 1, 0]);
    Memory(Mutex::new(State {
        sectors: [(0, crc(dir)), (1, profile()), (2, profile())].into(),
        mode: 1,
        active: 1,
        ..Default::default()
    }))
}
fn device() -> Device {
    Device {
        vendor_id: 0x046d,
        profile_key: "session:memory-only".into(),
        id: "logitech-test-A".into(),
        path: CString::new("memory-only").unwrap(),
        index: 0xff,
        product_id: 0xc08b,
        product: "simulation".into(),
        name: "G502 HERO protocol simulation".into(),
        firmware_name: "simulation".into(),
        model_ids: vec![0xc08b],
        kind: "Mouse",
        protocol: "HID++ 2.0".into(),
        features: [(
            ONBOARD_PROFILES,
            FeatureInfo {
                id: ONBOARD_PROFILES,
                index: 0x12,
                version: 0,
                flags: 0,
                name: "Onboard profiles",
            },
        )]
        .into(),
    }
}
fn settings(m: &Memory) -> Settings {
    let s = m.0.lock().unwrap();
    Settings {
        slot: 1,
        sector: 1,
        changes: vec![
            Change {
                button: 4,
                layer: Layer::Normal,
                expected: s.sectors[&1][0x2c..0x30].try_into().unwrap(),
                action: Action::Key {
                    usage: 6,
                    modifiers: 5,
                },
            },
            Change {
                button: 4,
                layer: Layer::GShift,
                expected: s.sectors[&1][0x6c..0x70].try_into().unwrap(),
                action: Action::Consumer(0xcd),
            },
        ],
    }
}
impl FeatureRequest for Memory {
    fn request(
        &self,
        device: u8,
        feature: u8,
        function: u8,
        p: &[u8],
        timeout: Duration,
    ) -> Result<Vec<u8>> {
        assert_eq!(
            (device, feature),
            (0xff, 0x12),
            "exact device and feature index"
        );
        if function == 8 {
            assert_eq!(timeout, COMMIT_TIMEOUT);
        }
        let mut s = self.0.lock().unwrap();
        s.calls.push((function, p.to_vec()));
        if s.small_reply && matches!(function, 0 | 5) {
            return Ok(vec![0; 3]);
        }
        let mut r = vec![0; 16];
        match function {
            0 => r[..11].copy_from_slice(&[1, 3, 1, 2, 0, 11, 8, 1, 0, 10, 1]),
            2 => r[0] = s.mode,
            11 => r[0] = 0,
            4 => r[1] = s.active,
            3 => {
                if s.reload_fail {
                    return Err(Error::Disconnected);
                }
                assert_eq!(p, [0, 1]);
                s.active = p[1];
            }
            5 => {
                assert_eq!(p.len(), 4);
                let sector = u16::from_be_bytes([p[0], p[1]]);
                let at = usize::from(u16::from_be_bytes([p[2], p[3]]));
                r.copy_from_slice(&s.sectors[&sector][at..at + 16]);
                if sector == 1 && s.committed && s.corrupt_readback {
                    s.corrupt_readback = false;
                    r[3] ^= 1;
                }
            }
            6 => {
                assert_eq!(p.len(), 6);
                assert_eq!(&p[2..4], &[0, 0]);
                let sector = u16::from_be_bytes([p[0], p[1]]);
                assert!((1..0x100).contains(&sector));
                s.staging = Some((
                    sector,
                    usize::from(u16::from_be_bytes([p[4], p[5]])),
                    Vec::new(),
                ));
            }
            7 => {
                assert_eq!(p.len(), 16);
                s.staging.as_mut().expect("write start").2.extend(p);
            }
            8 => {
                let (sector, size, mut bytes) = s.staging.take().expect("write in progress");
                assert!(bytes.len() >= size);
                bytes.truncate(size);
                assert!(sector_crc_ok(&bytes));
                s.sectors.insert(sector, bytes);
                s.committed = true;
            }
            _ => panic!("unexpected function {function}"),
        }
        if let Some((target, nth)) = s.fault {
            if target == function {
                s.seen += 1;
                if s.seen == nth {
                    s.fault = None;
                    if s.change_directory_on_fault {
                        let mut dir = s.sectors[&0].clone();
                        dir[1] = 2;
                        s.sectors.insert(0, crc(dir));
                    }
                    return Err(Error::Other(
                        "simulated uncertain transfer after device effect".into(),
                    ));
                }
            }
        }
        Ok(r)
    }
}
#[test]
fn complete_native_binding_transaction_reads_writes_verifies_and_preserves_dpi_rgb_and_other_profile(
) {
    let m = memory();
    let before = m.0.lock().unwrap().sectors.clone();
    let d = device();
    let report = d.write_slot_bindings(&m, &settings(&m)).unwrap();
    assert!(!report.unchanged);
    assert!(report.active_reloaded);
    assert!(report.reload_note.is_none());
    let s = m.0.lock().unwrap();
    assert_eq!(s.sectors[&0], before[&0]);
    assert_eq!(s.sectors[&2], before[&2]);
    let p = &s.sectors[&1];
    assert_eq!(&p[0x2c..0x30], &[128, 2, 5, 6]);
    assert_eq!(&p[0x6c..0x70], &[128, 3, 0, 205]);
    for n in 0..254 {
        if !((0x2c..0x30).contains(&n) || (0x6c..0x70).contains(&n)) {
            assert_eq!(p[n], before[&1][n], "unselected byte {n}");
        }
    }
    assert_eq!(s.calls.iter().filter(|(f, _)| *f == 6).count(), 1);
}
#[test]
fn all_uncertain_write_stages_restore_original_profile_when_ownership_is_unchanged() {
    let faults = [(6, 1), (8, 1)].into_iter().chain((1..=16).map(|n| (7, n)));
    for fault in faults {
        let m = memory();
        let before = m.0.lock().unwrap().sectors.clone();
        let settings = settings(&m);
        m.0.lock().unwrap().fault = Some(fault);
        let error = device()
            .write_slot_bindings(&m, &settings)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("restored and verified"),
            "{fault:?}: {error}"
        );
        assert_eq!(m.0.lock().unwrap().sectors, before, "{fault:?}");
    }
}
#[test]
fn mismatched_readback_is_not_reported_as_success_and_is_restored() {
    let m = memory();
    let before = m.0.lock().unwrap().sectors.clone();
    let settings = settings(&m);
    m.0.lock().unwrap().corrupt_readback = true;
    assert!(device()
        .write_slot_bindings(&m, &settings)
        .unwrap_err()
        .to_string()
        .contains("restored and verified"));
    assert_eq!(m.0.lock().unwrap().sectors, before);
}
#[test]
fn directory_ownership_change_blocks_blind_restore_and_stale_slot_blocks_first_write() {
    let m = memory();
    let settings = settings(&m);
    {
        let mut s = m.0.lock().unwrap();
        s.fault = Some((7, 1));
        s.change_directory_on_fault = true;
    }
    let error = device()
        .write_slot_bindings(&m, &settings)
        .unwrap_err()
        .to_string();
    assert!(error.contains("no blind retry"));
    assert_eq!(
        m.0.lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(f, _)| *f == 6)
            .count(),
        1
    );
    let m = memory();
    let mut settings = settings.clone();
    settings.sector = 2;
    assert!(device().write_slot_bindings(&m, &settings).is_err());
    assert!(!m
        .0
        .lock()
        .unwrap()
        .calls
        .iter()
        .any(|(f, _)| matches!(f, 6 | 7 | 8)));
}
#[test]
fn inactive_or_software_mode_is_not_forced_and_reload_error_keeps_verified_save() {
    for (mode, active) in [(2, 1), (1, 2)] {
        let m = memory();
        let settings = settings(&m);
        {
            let mut s = m.0.lock().unwrap();
            s.mode = mode;
            s.active = active;
        }
        let report = device().write_slot_bindings(&m, &settings).unwrap();
        assert!(!report.active_reloaded);
        assert!(!m.0.lock().unwrap().calls.iter().any(|(f, _)| *f == 3));
    }
    let m = memory();
    let settings = settings(&m);
    m.0.lock().unwrap().reload_fail = true;
    let report = device().write_slot_bindings(&m, &settings).unwrap();
    assert!(report
        .reload_note
        .unwrap()
        .contains("saved and readback verified"));
    assert_eq!(
        &m.0.lock().unwrap().sectors[&1][0x2c..0x30],
        &[128, 2, 5, 6]
    );
}
#[test]
fn short_replies_and_invalid_sizes_fail_without_padding_or_memory_writes() {
    let m = memory();
    m.0.lock().unwrap().small_reply = true;
    assert!(device().onboard_info(&m).is_err());
    assert!(device().read_sector(&m, 1, 256).is_err());
    for size in [0, 1, 15, 4097, 65535] {
        assert!(device().read_sector(&m, 1, size).is_err());
    }
    assert!(!m
        .0
        .lock()
        .unwrap()
        .calls
        .iter()
        .any(|(f, _)| matches!(f, 6 | 7 | 8)));
}

#[test]
fn native_logitech_onboard_fixture_export() {
    let m = memory();
    let (snapshot, _) = device().onboard_snapshot(&m).unwrap();
    assert_eq!(snapshot.profiles.len(), 2);
    assert_eq!(snapshot.info.button_count, 11);
    assert_eq!(snapshot.profiles[0].buttons[4].kind, "macro");
    if let Some(path) = std::env::var_os("OPEN_HUB_LOGITECH_BINDING_FIXTURE") {
        let d = device();
        let summary = serde_json::json!({"vendor_id":d.vendor_id,"id":d.id,"name":"G502 HERO","product":"G502 HERO","product_id":"c08b","kind":"mouse","firmware_name":"protocol simulation","model_ids":["c08b"],"connection":"wired","protocol":"HID++ 2.0","features":[{"id":ONBOARD_PROFILES,"index":0x12,"version":0,"flags":0,"name":"Onboard profiles"}]});
        let mut edited = settings(&m);
        edited.changes[0].action = Action::Key {
            usage: 6,
            modifiers: 17,
        };
        let before = m.0.lock().unwrap().sectors[&1].clone();
        edited.changes.push(Change {
            button: 7,
            layer: Layer::Normal,
            expected: before[0x38..0x3c].try_into().unwrap(),
            action: Action::Special(crate::onboard_bindings::Special::GShift),
        });
        d.write_slot_bindings(&m, &edited).unwrap();
        let (after_snapshot, _) = d.onboard_snapshot(&m).unwrap();
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &serde_json::json!({"device":summary,"snapshot":snapshot,"after_snapshot":after_snapshot,"hardware_tested":false}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}
#[test]
#[ignore = "requires current Logitech button editor output"]
fn browser_logitech_bindings_execute_native_memory_transaction() {
    let path = std::env::var_os("OPEN_HUB_LOGITECH_BINDING_UI_SETTINGS").unwrap();
    let settings: Settings = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let m = memory();
    let original = m.0.lock().unwrap().sectors[&1].clone();
    assert_eq!(settings.slot, 1);
    assert_eq!(settings.changes.len(), 3);
    let report = device().write_slot_bindings(&m, &settings).unwrap();
    assert!(report.bytes_changed > 0);
    let s = m.0.lock().unwrap();
    let after = &s.sectors[&1];
    assert_eq!(&after[0x2c..0x30], &[128, 2, 17, 6]);
    assert_eq!(&after[0x6c..0x70], &[128, 3, 0, 205]);
    assert_eq!(&after[0x38..0x3c], &[144, 11, 0, 0]);
    for n in 0..254 {
        if !((0x2c..0x30).contains(&n) || (0x6c..0x70).contains(&n) || (0x38..0x3c).contains(&n)) {
            assert_eq!(after[n], original[n]);
        }
    }
}

#[test]
fn receiver_or_other_brand_never_writes_through_reused_hidpp_index() {
    let m = memory();
    let settings = settings(&m);
    for (index, vendor) in [(1, 0x046d), (0xff, 0x3837)] {
        let mut d = device();
        d.index = index;
        d.vendor_id = vendor;
        assert!(d.write_slot_bindings(&m, &settings).is_err());
    }
    assert!(m.0.lock().unwrap().calls.is_empty());
}
