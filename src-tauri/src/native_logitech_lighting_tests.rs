use super::*;
use crate::device::FeatureInfo;
use std::{ffi::CString, sync::Mutex, time::Duration};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fault {
    None,
    Reject,
    LostAck,
    ShortRead,
    ThirdValue,
    OwnerChanged,
    CapsChanged,
    RestoreFails,
}
struct State {
    current: Vec<[u8; 11]>,
    calls: Vec<(u8, u8, Vec<u8>)>,
    writes: usize,
    fault: Fault,
    mode: u8,
    profile: u8,
    short_used: bool,
    bad_echo: bool,
    zones: u8,
    ext: u16,
    unknown_slot: bool,
}
struct Memory(Mutex<State>);
fn mem() -> Memory {
    Memory(Mutex::new(State {
        current: vec![
            [1, 10, 20, 30, 2, 0, 0, 0, 0, 0, 0],
            [3, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 1, 2, 3, 4, 5],
        ],
        calls: vec![],
        writes: 0,
        fault: Fault::None,
        mode: 1,
        profile: 1,
        short_used: false,
        bad_echo: false,
        zones: 2,
        ext: 1,
        unknown_slot: false,
    }))
}
fn device() -> Device {
    Device {
        vendor_id: 0x046d,
        id: "lighting-unit-A".into(),
        path: CString::new("memory-only-A").unwrap(),
        index: 0xff,
        product_id: 0xc08b,
        product: "simulation".into(),
        name: "G502 HERO synthetic lighting protocol".into(),
        firmware_name: "simulation".into(),
        model_ids: vec![0xc08b],
        kind: "Mouse",
        protocol: "HID++2".into(),
        features: [
            (
                COLOR,
                FeatureInfo {
                    id: COLOR,
                    index: 0x24,
                    version: 4,
                    flags: 0,
                    name: "Color LED effects",
                },
            ),
            (
                0x8100,
                FeatureInfo {
                    id: 0x8100,
                    index: 0x12,
                    version: 0,
                    flags: 0,
                    name: "Onboard",
                },
            ),
        ]
        .into(),
    }
}
impl FeatureRequest for Memory {
    fn request(
        &self,
        index: u8,
        feature: u8,
        function: u8,
        p: &[u8],
        _: Duration,
    ) -> Result<Vec<u8>> {
        assert_eq!(index, 0xff);
        let mut s = self.0.lock().unwrap();
        s.calls.push((feature, function, p.to_vec()));
        if feature == 0x12 {
            return match function {
                2 => Ok(vec![s.mode]),
                4 => Ok(vec![0, s.profile]),
                _ => panic!("unexpected ownership command"),
            };
        }
        assert_eq!(feature, 0x24);
        match function {
            0 => {
                assert!(p.is_empty());
                let ext = s.ext.to_be_bytes();
                Ok(vec![s.zones, 0, 0, ext[0], ext[1]])
            }
            1 => {
                let z = p[0];
                let loc = if s.fault == Fault::CapsChanged && s.writes > 0 {
                    9
                } else {
                    z + 1
                };
                Ok(vec![z, 0, loc, 4, 3])
            }
            2 => {
                let id = [3u16, 1, 0, 0x99][usize::from(p[1])].to_be_bytes();
                Ok(vec![
                    p[0],
                    if s.bad_echo { p[1] + 1 } else { p[1] },
                    id[0],
                    id[1],
                    0,
                    0,
                    0x13,
                    0x88,
                ])
            }
            14 => {
                if s.fault == Fault::ShortRead && s.writes == 1 && !s.short_used {
                    s.short_used = true;
                    return Ok(vec![p[0]]);
                }
                let mut r = vec![p[0]];
                let mut state = s.current[usize::from(p[0])];
                if s.unknown_slot {
                    state[0] = 0xfe;
                }
                r.extend(state);
                Ok(r)
            }
            3 => {
                assert_eq!(p.len(), 13);
                assert_eq!(p[12], 0, "must never persist flash");
                s.writes += 1;
                if s.fault == Fault::Reject && s.writes == 1 {
                    return Err(Error::Hidpp20(5, "internal error"));
                }
                if s.fault == Fault::RestoreFails && s.writes == 2 {
                    return Err(Error::Timeout);
                }
                s.current[usize::from(p[0])] = p[1..12].try_into().unwrap();
                if s.writes == 1 {
                    if s.fault == Fault::ThirdValue {
                        s.current[usize::from(p[0])][2] ^= 0x80;
                    }
                    if s.fault == Fault::OwnerChanged {
                        s.profile = 2;
                    }
                    if matches!(
                        s.fault,
                        Fault::LostAck | Fault::RestoreFails | Fault::CapsChanged
                    ) {
                        return Err(Error::Timeout);
                    }
                }
                Ok(vec![p[0]])
            }
            _ => panic!("unexpected lighting command {function}"),
        }
    }
}
fn change(m: &Memory) -> Change {
    Change {
        zone: 1,
        expected: m.0.lock().unwrap().current[1],
        effect: Effect::Fixed {
            color: [0x12, 0x34, 0x56],
        },
    }
}
#[test]
fn discovered_effect_slots_and_original_opaque_state_are_used_not_fixed_numeric_effect_ids() {
    let d = device();
    let m = mem();
    let v = read(&d, &m).unwrap();
    assert_eq!(v.zones.len(), 2);
    assert_eq!(
        v.zones[0]
            .effects
            .iter()
            .map(|s| s.effect_id)
            .collect::<Vec<_>>(),
        vec![3, 1, 0, 0x99]
    );
    assert_eq!(v.zones[1].current, Some(change(&m).expected));
    assert!(v.writable);
    assert_eq!(m.0.lock().unwrap().writes, 0);
    let result = apply(&d, &m, &change(&m)).unwrap();
    assert!(!result.unchanged);
    let s = m.0.lock().unwrap();
    let writes = s
        .calls
        .iter()
        .filter(|(f, n, _)| *f == 0x24 && *n == 3)
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 1);
    assert_eq!(
        writes[0].2,
        vec![1, 1, 0x12, 0x34, 0x56, 2, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(s.current[0], [1, 10, 20, 30, 2, 0, 0, 0, 0, 0, 0]);
    assert_eq!(s.mode, 1);
    assert_eq!(s.profile, 1);
}
#[test]
fn off_uses_advertised_slot_and_a_second_unchanged_apply_sends_no_write() {
    let d = device();
    let m = mem();
    let mut c = change(&m);
    c.effect = Effect::Off;
    let r = apply(&d, &m, &c).unwrap();
    assert_eq!(
        r.view.zones[1].current.unwrap(),
        [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    c.expected = r.view.zones[1].current.unwrap();
    assert!(apply(&d, &m, &c).unwrap().unchanged);
    assert_eq!(m.0.lock().unwrap().writes, 1);
}
#[test]
fn stale_foreign_receiver_unknown_layout_and_unreadable_current_values_fail_before_writes() {
    let d = device();
    for edit in [0, 1, 2, 3, 4] {
        let m = mem();
        let mut c = change(&m);
        let mut d = d.clone();
        match edit {
            0 => c.expected[3] ^= 1,
            1 => c.zone = 9,
            2 => d.features.get_mut(&COLOR).unwrap().version = 5,
            3 => m.0.lock().unwrap().ext = 2,
            _ => m.0.lock().unwrap().unknown_slot = true,
        };
        assert!(apply(&d, &m, &c).is_err());
        assert_eq!(m.0.lock().unwrap().writes, 0);
    }
    for index in [1, 2, 6] {
        let m = mem();
        let mut d = d.clone();
        d.index = index;
        assert!(apply(&d, &m, &change(&m)).is_err());
        assert!(m.0.lock().unwrap().calls.is_empty());
    }
    let m = mem();
    let mut wrong = d.clone();
    wrong.vendor_id = 0x3837;
    assert!(read(&wrong, &m).is_err());
    assert!(m.0.lock().unwrap().calls.is_empty());
}
#[test]
fn uncertain_write_or_short_read_restores_only_selected_zone_when_owner_and_state_match() {
    for fault in [Fault::Reject, Fault::LostAck, Fault::ShortRead] {
        let d = device();
        let m = mem();
        let old = m.0.lock().unwrap().current.clone();
        m.0.lock().unwrap().fault = fault;
        let err = apply(&d, &m, &change(&m)).unwrap_err().to_string();
        assert!(err.contains("restored/verified"), "{fault:?}: {err}");
        assert_eq!(m.0.lock().unwrap().current, old);
        assert_eq!(
            m.0.lock().unwrap().writes,
            if fault == Fault::Reject { 1 } else { 2 }
        );
    }
}
#[test]
fn changed_profile_capabilities_third_party_value_or_failed_restore_are_not_reported_successful() {
    for fault in [
        Fault::ThirdValue,
        Fault::OwnerChanged,
        Fault::CapsChanged,
        Fault::RestoreFails,
    ] {
        let d = device();
        let m = mem();
        m.0.lock().unwrap().fault = fault;
        let err = apply(&d, &m, &change(&m)).unwrap_err().to_string();
        assert!(err.contains("state uncertain"), "{fault:?}: {err}");
        assert_eq!(
            m.0.lock().unwrap().writes,
            if fault == Fault::RestoreFails { 2 } else { 1 }
        );
    }
}
#[test]
fn malformed_discovery_and_huge_counts_are_rejected_without_mode_or_write_commands() {
    for fault in [0, 1] {
        let d = device();
        let m = mem();
        {
            let mut s = m.0.lock().unwrap();
            if fault == 0 {
                s.bad_echo = true;
            } else {
                s.zones = 255;
            }
        }
        assert!(read(&d, &m).is_err());
        let s = m.0.lock().unwrap();
        assert_eq!(s.writes, 0);
        assert!(!s.calls.iter().any(|(f, _, _)| *f == 0x12));
    }
    let d = device();
    let mut v = read(&d, &mem()).unwrap();
    let c = Change {
        zone: 0,
        expected: v.zones[0].current.unwrap(),
        effect: Effect::Fixed { color: [1, 2, 3] },
    };
    v.zones[0].effects.push(Slot {
        index: 9,
        effect_id: 1,
        capabilities: 0,
        period: 0,
    });
    assert!(compile(&v, &c).is_err());
    assert!(serde_json::from_str::<Change>(
        r#"{"zone":0,"expected":[0,0,0,0,0,0,0,0,0,0,0],"effect":{"Raw":[1,2,3]}}"#
    )
    .is_err());
    assert!(
        serde_json::from_str::<Effect>(r#"{"Fixed":{"color":[1,2,3],"persist":true}}"#).is_err()
    );
}
#[test]
fn two_same_model_devices_on_distinct_transports_do_not_share_effects() {
    let a = mem();
    let b = mem();
    apply(&device(), &a, &change(&a)).unwrap();
    assert_eq!(b.0.lock().unwrap().writes, 0);
    assert_ne!(a.0.lock().unwrap().current, b.0.lock().unwrap().current);
}

#[test]
fn native_logitech_light_fixture_export() {
    let d = device();
    let m = mem();
    let view = read(&d, &m).unwrap();
    let applied = apply(&d, &m, &change(&m)).unwrap();
    if let Some(path) = std::env::var_os("OPEN_HUB_LOGITECH_LIGHTING_FIXTURE") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &serde_json::json!({"view":view,"applied_view":applied.view}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}
#[test]
#[ignore = "requires current native Logitech lighting browser change"]
fn browser_logitech_lighting_change_executes_production_transaction() {
    let path = std::env::var_os("OPEN_HUB_LOGITECH_LIGHTING_UI_CHANGE").unwrap();
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let change: Change = serde_json::from_value(value["change"].clone()).unwrap();
    assert_eq!(change.zone, 1);
    assert_eq!(
        change.effect,
        Effect::Fixed {
            color: [18, 52, 86]
        }
    );
    let m = mem();
    let before = m.0.lock().unwrap().current.clone();
    let result = apply(&device(), &m, &change).unwrap();
    assert!(!result.unchanged);
    assert_eq!(m.0.lock().unwrap().current[0], before[0]);
    assert_eq!(m.0.lock().unwrap().writes, 1);
    assert_eq!(
        result.view.zones[1].current.unwrap(),
        [1, 18, 52, 86, 2, 0, 0, 0, 0, 0, 0]
    );
}

#[test]
fn lighting_and_onboard_reads_use_unpadded_transport_payloads() {
    struct Short;
    impl FeatureRequest for Short {
        fn request(&self, _: u8, _: u8, _: u8, _: &[u8], _: Duration) -> Result<Vec<u8>> {
            panic!("strict read reached legacy padded request");
        }
        fn request_unpadded(&self, _: u8, _: u8, _: u8, _: &[u8], _: Duration) -> Result<Vec<u8>> {
            Ok(vec![1, 0, 0])
        }
    }
    let d = device();
    assert!(read(&d, &Short)
        .unwrap_err()
        .to_string()
        .contains("short feature response"));
    assert!(d.onboard_info(&Short).is_err());
}
