//! Exact-model calibration levels reconstructed from official product182 CIL.
//! Standard/Pro conversion is in LitaT1FeatureImpl and LitaT1DongleFeatureImpl;
//! see docs/development/razer-dav3pro-calibration-static-facts.json. No capture
//! or physical-device validation is claimed by these codecs and simulations.
use crate::{link::Link, report::Report};
use gearwise_core::{Error, Result, SensorCalibration, SensorCalibrationCaps};

/// Static model support; callers expose these bounds only after an exact GET.
pub fn calibration_caps(product: u16) -> Option<SensorCalibrationCaps> {
    (product == 0x00b6).then(|| SensorCalibrationCaps {
        standard_levels: vec![1, 2, 3],
        lift_levels: (2..=26).collect(),
        landing_levels: (1..=25).collect(),
    })
}

pub fn validate_sensor_calibration(product: u16, value: &SensorCalibration) -> Result<()> {
    let caps = calibration_caps(product)
        .ok_or(Error::Unsupported("sensor calibration for this exact Razer model"))?;
    value.validate(&caps)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field { State, Mode, Configuration }
const FIELDS: [Field; 3] = [Field::State, Field::Mode, Field::Configuration];

impl Field {
    fn command(self) -> u8 {
        match self { Self::State => 0x03, Self::Mode => 0x0b, Self::Configuration => 0x05 }
    }
    fn size(self) -> u8 { if self == Self::State { 3 } else { 4 } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Raw {
    state: u8,
    mode: [u8; 2],
    configuration: [u8; 2],
}

impl Raw {
    fn value(self, field: Field) -> [u8; 2] {
        match field { Field::State => [self.state, 0], Field::Mode => self.mode,
            Field::Configuration => self.configuration }
    }
    fn put(&mut self, field: Field, value: [u8; 2]) {
        match field { Field::State => self.state = value[0], Field::Mode => self.mode = value,
            Field::Configuration => self.configuration = value }
    }
    fn semantic(self) -> Result<SensorCalibration> {
        // Inactive configuration need not form an asymmetric pair: [0,0] is
        // retained. Bytes outside the proved per-field ranges lock all writes.
        if self.configuration[0] > 25 || self.configuration[1] > 24 {
            return Err(Error::protocol("unknown DeathAdder calibration configuration retained"));
        }
        match (self.state, self.mode) {
            (0, [1, height @ 0..=2]) => Ok(SensorCalibration::Standard { distance: height + 1 }),
            (1, [4, 0]) if (1..=25).contains(&self.configuration[0])
                && self.configuration[1] < self.configuration[0] => {
                Ok(SensorCalibration::Asymmetric {
                    lift: self.configuration[0] + 1,
                    landing: self.configuration[1] + 1,
                })
            }
            _ => Err(Error::protocol("unknown or transitional DeathAdder calibration retained")),
        }
    }
    fn target(mut self, value: &SensorCalibration) -> Self {
        match *value {
            SensorCalibration::Standard { distance } => { self.state = 0; self.mode = [1, distance - 1]; }
            SensorCalibration::Asymmetric { lift, landing } => {
                self.state = 1;
                self.mode = [4, 0];
                self.configuration = [lift - 1, landing - 1];
            }
        }
        self
    }
}

fn request(field: Field, value: Option<[u8; 2]>) -> Report {
    let mut args = [0, 4, 0, 0];
    if let Some(value) = value { args[2..4].copy_from_slice(&value); }
    Report::new(0x0b, field.command() | if value.is_none() { 0x80 } else { 0 },
        field.size(), &args[..usize::from(field.size())])
}

fn read_raw(link: &mut Link) -> Result<Raw> {
    let mut raw = Raw { state: 0, mode: [0; 2], configuration: [0; 2] };
    for field in FIELDS {
        // Explicit two-byte configuration length avoids the OEM JSON wrapper's
        // zero-length size inquiry, which never dispatched a hardware GET.
        let reply = link.exec(request(field, None))?;
        if reply.class != 0x0b || reply.command != field.command() | 0x80
            || reply.data_size != field.size() || reply.args[..2] != [0, 4] {
            return Err(Error::protocol("DeathAdder calibration GET shape/selectors mismatch"));
        }
        raw.put(field, [reply.args[2], if field == Field::State { 0 } else { reply.args[3] }]);
    }
    Ok(raw)
}

pub(crate) fn read(link: &mut Link, product: u16) -> Result<SensorCalibration> {
    calibration_caps(product)
        .ok_or(Error::Unsupported("sensor calibration for this exact Razer model"))?;
    link.strict_model_replies();
    read_raw(link)?.semantic()
}

// Intermediate state/mode combinations are expected during a multi-record
// transaction, so ownership compares raw records without semantic decoding.
fn owned_progress(current: Raw, before: Raw, target: Raw, attempted: &[Field]) -> bool {
    FIELDS.into_iter().all(|field| {
        current.value(field) == before.value(field)
            || attempted.contains(&field) && current.value(field) == target.value(field)
    })
}

fn recover(link: &mut Link, before: Raw, target: Raw, attempted: &[Field]) -> &'static str {
    for &field in attempted.iter().rev() {
        let Ok(current) = read_raw(link) else { return "recovery GET failed; no uncertain restore resent"; };
        // A foreign change to any associated record stops recovery completely:
        // restoring our mode/state around someone else's config is unsafe.
        if !owned_progress(current, before, target, attempted) {
            return "foreign calibration retained; recovery stopped";
        }
        if current.value(field) == before.value(field) { continue; }
        let _ack = link.exec(request(field, Some(before.value(field))));
        let Ok(after) = read_raw(link) else { return "restore result unverified; restore not resent"; };
        let mut expected = current;
        expected.put(field, before.value(field));
        if after != expected { return "restore result changed or foreign; restore not resent"; }
    }
    match read_raw(link) {
        Ok(current) if current == before => "original calibration restored and verified",
        _ => "original calibration not fully verified; no further writes",
    }
}

pub(crate) fn apply(link: &mut Link, product: u16, value: &SensorCalibration) -> Result<()> {
    validate_sensor_calibration(product, value)?;
    link.strict_model_replies();
    let before = read_raw(link)?;
    before.semantic()?;
    let target = before.target(value);
    let mut expected = before;
    let mut attempted = Vec::new();
    for field in FIELDS {
        if expected.value(field) == target.value(field) { continue; }
        let result = (|| -> Result<()> {
            if read_raw(link)? != expected {
                return Err(Error::protocol("DeathAdder calibration changed before write"));
            }
            attempted.push(field);
            // Link's strict path sends once and only polls the ACK. A failed or
            // missing ACK is reconciled by GET, never by replaying this SET.
            link.exec(request(field, Some(target.value(field))))?;
            expected.put(field, target.value(field));
            if read_raw(link)? != expected {
                return Err(Error::protocol("DeathAdder calibration readback mismatch"));
            }
            Ok(())
        })();
        if let Err(error) = result {
            let recovery = recover(link, before, target, &attempted);
            return Err(Error::protocol(format!("calibration SET not retried: {error}; {recovery}")));
        }
    }
    match read_raw(link) {
        Ok(current) if current == target => {}
        result => {
            let reason = match result {
                Ok(_) => "calibration final GET mismatch".to_owned(),
                Err(error) => format!("calibration final GET failed: {error}"),
            };
            let recovery = recover(link, before, target, &attempted);
            return Err(Error::protocol(format!("{reason}; SET not retried; {recovery}")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Timing;
    use gearwise_core::transport::{MockOp, MockReply, MockTransport};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn original() -> Raw { Raw { state: 0, mode: [1, 1], configuration: [12, 11] } }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Fault { None, Reject(usize), Busy(usize), Foreign(usize), Corrupt(usize), MalformedGet(usize), PersistentGet(usize), ForeignGet(usize), RestoreFailure }
    struct State { raw: Raw, writes: Vec<(u8, [u8; 2])>, requests: Vec<Report>, reads: usize, fault: Fault }
    fn rig(raw: Raw, fault: Fault) -> (Link, Arc<Mutex<State>>) {
        let state = Arc::new(Mutex::new(State { raw, writes: vec![], requests: vec![], reads: 0, fault }));
        let shared = state.clone();
        let io = MockTransport::new(move |op| {
            let MockOp::SetFeature(bytes) = op else { return MockReply::none(); };
            let req = Report::from_wire(bytes).unwrap();
            let mut state = shared.lock().unwrap();
            state.requests.push(req.clone());
            let field = match req.command & 0x7f {
                3 => Field::State, 11 => Field::Mode, 5 => Field::Configuration, _ => panic!("unexpected command"),
            };
            let mut reply = req.clone();
            reply.status = 2;
            let mut corrupt = false;
            if req.command & 0x80 != 0 {
                state.reads += 1;
                if state.fault == Fault::ForeignGet(state.reads) { state.raw.configuration = [20,10]; }
                let value = state.raw.value(field);
                reply.args[2] = value[0];
                if field != Field::State { reply.args[3] = value[1]; }
                if state.fault == Fault::MalformedGet(state.reads) { reply.args[1] = 5; }
                if matches!(state.fault, Fault::PersistentGet(at) if state.reads >= at) { reply.status = 3; }
            } else {
                let value = [req.args[2], if field == Field::State { 0 } else { req.args[3] }];
                state.raw.put(field, value);
                state.writes.push((req.command, value));
                let n = state.writes.len();
                match state.fault {
                    Fault::Reject(at) if n == at => reply.status = 3,
                    Fault::Busy(at) if n == at => reply.status = 1,
                    Fault::Corrupt(at) if n == at => corrupt = true,
                    Fault::Foreign(at) if n == at => { reply.status = 3; state.raw.configuration = [20, 10]; }
                    Fault::RestoreFailure if n == 2 => { reply.status = 3; }
                    Fault::RestoreFailure if n >= 3 => { state.raw.put(field, [255, 255]); reply.status = 3; }
                    _ => {}
                }
            }
            let mut wire = reply.to_wire().to_vec();
            if corrupt { wire[89] ^= 1; }
            MockReply::feature(wire)
        });
        (Link::new(Box::new(io), 31, Timing::new(Duration::ZERO, Duration::ZERO, 1)), state)
    }

    #[test]
    fn exact_primary_get_and_set_vectors_have_explicit_lengths_and_selectors() {
        // Independent primary report shapes; no OEM executable or code runs.
        for (field, value, prefix, crc) in [
            (Field::State, None, vec![0,0,31,0,0,0,3,11,131,0,4,0], 0x8f),
            (Field::Mode, None, vec![0,0,31,0,0,0,4,11,139,0,4,0,0], 0x80),
            (Field::Configuration, None, vec![0,0,31,0,0,0,4,11,133,0,4,0,0], 0x8e),
            (Field::State, Some([1,0]), vec![0,0,31,0,0,0,3,11,3,0,4,1], 0x0e),
            (Field::Mode, Some([4,0]), vec![0,0,31,0,0,0,4,11,11,0,4,4,0], 0x04),
            (Field::Configuration, Some([25,24]), vec![0,0,31,0,0,0,4,11,5,0,4,25,24], 0x0f),
        ] {
            let mut expected = [0; 91];
            expected[..prefix.len()].copy_from_slice(&prefix);
            expected[89] = crc;
            let mut report = request(field, value);
            report.transaction_id = 31;
            assert_eq!(report.to_wire(), expected);
        }
    }

    #[test]
    fn explicit_levels_decode_without_treating_inactive_pair_as_active() {
        for distance in 1..=3 {
            let value = SensorCalibration::Standard { distance };
            let mut raw = original().target(&value);
            raw.configuration = [0, 0];
            assert_eq!(raw.semantic().unwrap(), value);
        }
        for lift in 2..=26 { for landing in 1..lift {
            let value = SensorCalibration::Asymmetric { lift, landing };
            validate_sensor_calibration(0xb6, &value).unwrap();
            assert_eq!(original().target(&value).semantic().unwrap(), value);
        }}
        for raw in [Raw { state: 2, ..original() }, Raw { mode: [4,0], ..original() },
            Raw { configuration: [255,0], ..original() }, Raw { state:1, mode:[4,1], ..original() },
            Raw { state:1, mode:[4,0], configuration:[12,12] }] {
            assert!(raw.semantic().is_err());
        }
    }

    #[test]
    fn unsupported_models_and_invalid_levels_do_zero_io() {
        let (mut link, state) = rig(original(), Fault::None);
        for product in [0xb7, 0xb4, 0xc0, 0, 0xffff] {
            assert!(read(&mut link, product).is_err());
            assert!(apply(&mut link, product, &SensorCalibration::Standard { distance: 2 }).is_err());
        }
        for value in [SensorCalibration::Standard { distance: 0 }, SensorCalibration::Standard { distance: 4 },
            SensorCalibration::Asymmetric { lift: 1, landing: 1 }, SensorCalibration::Asymmetric { lift: 27, landing: 12 },
            SensorCalibration::Asymmetric { lift: 13, landing: 13 }] {
            assert!(apply(&mut link, 0xb6, &value).is_err());
        }
        assert!(state.lock().unwrap().requests.is_empty());
    }

    #[test]
    fn fresh_get_and_each_partial_progress_verify_before_success() {
        let (mut link, state) = rig(original(), Fault::None);
        assert_eq!(read(&mut link, 0xb6).unwrap(), SensorCalibration::Standard { distance: 2 });
        let value = SensorCalibration::Asymmetric { lift: 26, landing: 25 };
        apply(&mut link, 0xb6, &value).unwrap();
        assert_eq!(read(&mut link, 0xb6).unwrap(), value);
        assert_eq!(state.lock().unwrap().writes, vec![(3,[1,0]),(11,[4,0]),(5,[25,24])]);
        state.lock().unwrap().raw.configuration = [0,0];
        assert!(read(&mut link, 0xb6).is_err());
        assert!(apply(&mut link, 0xb6, &SensorCalibration::Standard { distance: 3 }).is_err());
        assert_eq!(state.lock().unwrap().writes.len(), 3);
    }

    #[test]
    fn uncertain_set_is_not_replayed_and_only_owned_progress_is_restored() {
        let value = SensorCalibration::Asymmetric { lift: 26, landing: 25 };
        for at in 1..=3 { for fault in [Fault::Reject(at), Fault::Busy(at), Fault::Corrupt(at)] {
            let (mut link, state) = rig(original(), fault);
            let error = apply(&mut link, 0xb6, &value).unwrap_err().to_string();
            assert!(error.contains("not retried"));
            assert_eq!(state.lock().unwrap().raw, original(), "{fault:?}");
            let writes = &state.lock().unwrap().writes;
            for command in [3,11,5] {
                let target = original().target(&value).value(match command {3=>Field::State,11=>Field::Mode,_=>Field::Configuration});
                assert!(writes.iter().filter(|x| **x == (command,target)).count() <= 1, "{fault:?}");
            }
        }}
    }

    #[test]
    fn foreign_associated_record_stops_all_recovery_and_failed_restore_is_not_replayed() {
        let value = SensorCalibration::Asymmetric { lift: 26, landing: 25 };
        for at in 1..=3 {
            let (mut link, state) = rig(original(), Fault::Foreign(at));
            let error = apply(&mut link, 0xb6, &value).unwrap_err().to_string();
            assert!(error.contains("foreign calibration retained"));
            let state = state.lock().unwrap();
            assert_eq!(state.raw.configuration, [20,10]);
            assert_eq!(state.writes.len(), at);
        }
        let (mut link, state) = rig(original(), Fault::RestoreFailure);
        assert!(apply(&mut link, 0xb6, &value).is_err());
        assert_eq!(state.lock().unwrap().writes.len(), 3);
    }

    #[test]
    fn malformed_original_midway_and_final_gets_never_leave_unverified_success() {
        let value = SensorCalibration::Asymmetric { lift: 26, landing: 25 };
        for at in [1,7,22] {
            let (mut link, state) = rig(original(), Fault::MalformedGet(at));
            assert!(apply(&mut link, 0xb6, &value).is_err());
            let state = state.lock().unwrap();
            assert_eq!(state.raw, original());
            if at == 1 { assert!(state.writes.is_empty()); }
        }
        let (mut link, state) = rig(original(), Fault::ForeignGet(4));
        assert!(apply(&mut link, 0xb6, &value).is_err());
        let state = state.lock().unwrap();
        assert!(state.writes.is_empty());
        assert_eq!(state.raw.configuration, [20,10]);
    }

    #[test]
    fn final_get_failure_without_recovery_evidence_and_final_foreign_config_never_trigger_blind_restore() {
        let value = SensorCalibration::Asymmetric { lift: 26, landing: 25 };
        for fault in [Fault::PersistentGet(22), Fault::ForeignGet(22)] {
            let (mut link, state) = rig(original(), fault);
            assert!(apply(&mut link, 0xb6, &value).is_err());
            let state = state.lock().unwrap();
            assert_eq!(state.writes.len(), 3);
            assert_eq!(state.raw.state, 1);
            assert_eq!(state.raw.mode, [4,0]);
            assert_eq!(state.raw.configuration, if fault == Fault::ForeignGet(22) { [20,10] } else { [25,24] });
        }
    }

    #[test]
    fn noop_and_unknown_initial_records_issue_no_set() {
        let (mut link, state) = rig(original(), Fault::None);
        apply(&mut link, 0xb6, &SensorCalibration::Standard { distance: 2 }).unwrap();
        assert!(state.lock().unwrap().writes.is_empty());
        for raw in [Raw { state: 3, ..original() }, Raw { mode: [4,0], ..original() },
            Raw { configuration: [26,0], ..original() }, Raw { state:1, mode:[4,0], configuration:[0,0] }] {
            let (mut link, state) = rig(raw, Fault::None);
            assert!(apply(&mut link, 0xb6, &SensorCalibration::Standard { distance: 1 }).is_err());
            assert!(state.lock().unwrap().writes.is_empty());
            assert_eq!(state.lock().unwrap().raw, raw);
        }
    }
}
