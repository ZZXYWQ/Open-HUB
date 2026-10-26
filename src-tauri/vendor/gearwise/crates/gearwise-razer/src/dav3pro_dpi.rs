//! Exact B6/B7 v3 stage-table shape, independently checked against the official
//! RzCtl_00B6 23.0.0.041210 DLL. Static reconstruction, not a hardware capture.
use crate::report::Report;
use gearwise_core::{
    Error, Result,
    model::{DpiStage, DpiState},
};
pub fn parse(r: &Report) -> Result<DpiState> {
    let count = usize::from(r.args[2]);
    let active = usize::from(r.args[1]);
    if r.data_size != 38
        || r.args[0] != 1
        || !(1..=5).contains(&count)
        || !(1..=count).contains(&active)
    {
        return Err(Error::protocol(
            "DeathAdder stage profile/count/active/length mismatch",
        ));
    }
    // Accept only a complete contiguous zero- or one-based index sequence.
    // This is parser validation policy, not a claim of two captured firmwares.
    let first = r.args[3];
    if first > 1 {
        return Err(Error::protocol("DeathAdder stage record index origin"));
    }
    let mut stages = Vec::with_capacity(count);
    for (i, row) in r.args[3..3 + 7 * count].chunks_exact(7).enumerate() {
        let x = u16::from_be_bytes([row[1], row[2]]);
        let y = u16::from_be_bytes([row[3], row[4]]);
        if row[0] != first + i as u8
            || row[5..7] != [0, 0]
            || !(100..=30000).contains(&x)
            || !(100..=30000).contains(&y)
        {
            return Err(Error::protocol(
                "DeathAdder stage record is not completely representable",
            ));
        }
        stages.push(DpiStage { x, y });
    }
    if r.args[3 + 7 * count..38].iter().any(|v| *v != 0) {
        return Err(Error::protocol(
            "unobserved trailing DeathAdder DPI rows retained",
        ));
    }
    Ok(DpiState {
        stages,
        active: (active - 1) as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RazerDevice, Timing, commands, link::Link, models};
    use gearwise_core::{
        Device, Setting,
        transport::{HidInterfaceInfo, MockOp, MockReply, MockTransport},
    };
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };
    fn factory() -> DpiState {
        DpiState {
            stages: [400, 800, 1600, 3200, 6400].map(DpiStage::uniform).to_vec(),
            active: 2,
        }
    }
    fn reply(d: &DpiState) -> Report {
        let mut r = commands::set_dpi_stages(d);
        r.status = 2;
        r.command = 0x86;
        for i in 0..d.stages.len() {
            r.args[3 + i * 7] = i as u8 + 1;
        }
        r
    }
    #[test]
    fn official_v3_static_reconstruction_has_fixed_38_byte_get_and_complete_five_stage_set() {
        // Independent static facts: RzCtl_00B6 23.0.0.041210, VA 2AA70/2E600,
        // GET dispatcher 2B2F0 sets bit 80; OEM DPIStages.xml supplies the values.
        let mut expected_get = vec![0; 91];
        expected_get[..10].copy_from_slice(&[0, 0, 31, 0, 0, 0, 38, 4, 134, 1]);
        expected_get[89] = 0xa5;
        let mut get = commands::get_dpi_stages();
        get.transaction_id = 31;
        assert_eq!(get.to_wire().as_slice(), expected_get);
        let prefix = [
            0, 0, 31, 0, 0, 0, 38, 4, 6, 1, 3, 5, 0, 1, 144, 1, 144, 0, 0, 1, 3, 32, 3, 32, 0, 0,
            2, 6, 64, 6, 64, 0, 0, 3, 12, 128, 12, 128, 0, 0, 4, 25, 0, 25, 0, 0, 0,
        ];
        let mut expected = vec![0; 91];
        expected[..prefix.len()].copy_from_slice(&prefix);
        expected[89] = 0x27;
        let mut set = commands::set_dpi_stages(&factory());
        set.transaction_id = 31;
        assert_eq!(set.to_wire().as_slice(), expected);
        assert_eq!(parse(&reply(&factory())).unwrap(), factory());
        let mut zero_based = reply(&factory());
        for i in 0..5 {
            zero_based.args[3 + i * 7] = i as u8;
        }
        assert_eq!(parse(&zero_based).unwrap(), factory());
    }
    #[test]
    fn truncated_mismatched_hidden_or_corrupt_tables_are_never_clamped_or_partially_read() {
        let original = reply(&factory());
        for (at, value) in [
            (0, 0),
            (1, 0),
            (1, 6),
            (2, 0),
            (2, 6),
            (3, 9),
            (10, 4),
            (8, 1),
            (4, 255),
        ] {
            let mut r = original.clone();
            r.args[at] = value;
            assert!(parse(&r).is_err(), "{at}:{value}");
        }
        for size in [0, 10, 37, 39, 80] {
            let mut r = original.clone();
            r.data_size = size;
            assert!(parse(&r).is_err());
        }
        let mut hidden = original;
        hidden.args[1] = 1;
        hidden.args[2] = 2;
        assert!(parse(&hidden).is_err());
    }
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Fault {
        None,
        SetterFailure,
        Busy,
        VerifyCorrupt,
        Foreign,
        RestoreFailure,
        MalformedBefore,
    }
    struct State {
        table: DpiState,
        reads: usize,
        writes: Vec<DpiState>,
    }
    fn rig(pid: u16, fault: Fault) -> (RazerDevice, Arc<Mutex<State>>) {
        let shared = Arc::new(Mutex::new(State {
            table: factory(),
            reads: 0,
            writes: vec![],
        }));
        let state = shared.clone();
        let io = MockTransport::new(move |op| {
            let MockOp::SetFeature(bytes) = op else {
                return MockReply::none();
            };
            let req = Report::from_wire(bytes).unwrap();
            let mut r = req.clone();
            r.status = 2;
            r.args = [0; 80];
            let mut corrupt = false;
            match (req.class, req.command) {
                (0, 0x82) => r.args[..19].copy_from_slice(b"SIMULATED-DAV3PRO-A"),
                (0, 0x81) => r.args[..2].copy_from_slice(&[2, 1]),
                (4, 0x86) => {
                    let mut s = state.lock().unwrap();
                    s.reads += 1;
                    r = reply(&s.table);
                    r.transaction_id = req.transaction_id;
                    if fault == Fault::MalformedBefore && s.reads == 1 {
                        r.args[1] = 0;
                    }
                    corrupt = fault == Fault::VerifyCorrupt && s.reads == 2;
                }
                (4, 6) => {
                    let mut s = state.lock().unwrap();
                    let next = parse(&req).unwrap();
                    s.table = next.clone();
                    s.writes.push(next);
                    if s.writes.len() == 1 {
                        if fault == Fault::Busy {
                            r.status = 1;
                        }
                        if matches!(
                            fault,
                            Fault::SetterFailure | Fault::Foreign | Fault::RestoreFailure
                        ) {
                            r.status = 3;
                        }
                        if fault == Fault::Foreign {
                            s.table.stages[0] = DpiStage::uniform(2500);
                        }
                    } else if fault == Fault::RestoreFailure {
                        r.status = 3;
                    }
                }
                _ => r.status = 5,
            }
            let mut wire = r.to_wire().to_vec();
            if corrupt {
                wire[89] ^= 1;
            }
            MockReply::feature(wire)
        });
        let model = models::find(pid).unwrap();
        let link = Link::new(
            Box::new(io),
            model.transaction_id,
            Timing::new(Duration::ZERO, Duration::ZERO, 1),
        );
        (
            RazerDevice::open(model, link, &HidInterfaceInfo::default()).unwrap(),
            shared,
        )
    }
    fn want() -> DpiState {
        DpiState {
            stages: vec![DpiStage::uniform(100), DpiStage { x: 30000, y: 29950 }],
            active: 1,
        }
    }
    #[test]
    fn complete_stages_and_active_switch_use_fresh_device_table_and_verified_values() {
        for pid in [0xb6, 0xb7] {
            let (mut d, s) = rig(pid, Fault::None);
            assert_eq!(d.read_state().unwrap().dpi, Some(factory()));
            d.apply(&Setting::Dpi(want())).unwrap();
            assert_eq!(d.read_state().unwrap().dpi, Some(want()));
            assert_eq!(s.lock().unwrap().writes.len(), 1);
            s.lock().unwrap().table.stages[0] = DpiStage::uniform(1200);
            d.apply(&Setting::DpiActiveStage(0)).unwrap();
            let table = s.lock().unwrap().table.clone();
            assert_eq!(table.stages[0].x, 1200);
            assert_eq!(table.active, 0);
            assert_eq!(d.read_state().unwrap().dpi, Some(table));
        }
    }
    #[test]
    fn uncertain_stage_writes_restore_once_only_while_owned_and_foreign_table_is_retained() {
        for pid in [0xb6, 0xb7] {
            for fault in [
                Fault::SetterFailure,
                Fault::Busy,
                Fault::VerifyCorrupt,
                Fault::Foreign,
                Fault::RestoreFailure,
            ] {
                let (mut d, s) = rig(pid, fault);
                let error = d.apply(&Setting::Dpi(want())).unwrap_err().to_string();
                assert!(error.contains("not retried"));
                let s = s.lock().unwrap();
                assert_eq!(s.writes.iter().filter(|d| **d == want()).count(), 1);
                if fault == Fault::Foreign {
                    assert_eq!(s.writes.len(), 1);
                    assert_eq!(s.table.stages[0].x, 2500);
                } else {
                    assert_eq!(s.writes.len(), 2);
                    assert_eq!(s.table, factory());
                }
            }
        }
    }
    #[test]
    fn malformed_original_and_old_current_only_plan_never_overwrite_five_stages() {
        for pid in [0xb6, 0xb7] {
            let (mut d, s) = rig(pid, Fault::MalformedBefore);
            assert!(d.apply(&Setting::Dpi(want())).is_err());
            assert!(s.lock().unwrap().writes.is_empty());
            let (mut d, s) = rig(pid, Fault::None);
            assert!(
                d.apply(&Setting::Dpi(DpiState {
                    stages: vec![DpiStage::uniform(800)],
                    active: 0
                }))
                .is_err()
            );
            assert!(s.lock().unwrap().writes.is_empty());
            assert_eq!(s.lock().unwrap().table, factory());
        }
    }
}
