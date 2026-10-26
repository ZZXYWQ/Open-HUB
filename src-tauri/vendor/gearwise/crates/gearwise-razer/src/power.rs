//! Exact DeathAdder V3 Pro power fields. Public captured requests and replies,
//! no global input changes, no automatic retries of persistent writes.
use crate::{link::Link, report::Report};
use gearwise_core::{
    tuning::{MouseTuning, TuningCaps},
    Error, Result,
};
pub fn supported(pid: u16) -> bool {
    matches!(pid, 0x00b6 | 0x00b7)
}
pub fn caps() -> TuningCaps {
    TuningCaps {
        device_sleep_seconds: (1..=15).map(|v| v * 60).collect(),
        low_power_percent: (1..=20).map(|v| v * 5).collect(),
        ..Default::default()
    }
}
pub fn validate(pid: u16, patch: &MouseTuning) -> Result<()> {
    if !supported(pid) {
        return Err(Error::Unsupported("exact Razer power model"));
    }
    patch.validate(&caps())
}
#[derive(Clone, Copy, Debug)]
enum Field {
    Sleep,
    LowPower,
}
impl Field {
    fn command(self) -> u8 {
        match self {
            Self::Sleep => 3,
            Self::LowPower => 1,
        }
    }
    fn get(self) -> Report {
        Report::new(7, self.command() | 0x80, 2, &[])
    }
    fn set(self, raw: [u8; 2]) -> Report {
        Report::new(7, self.command(), 2, &raw)
    }
}
fn raw(link: &mut Link, field: Field) -> Result<[u8; 2]> {
    let r = link.exec(field.get())?;
    if r.data_size != 2 || matches!(field, Field::LowPower) && r.args[1] != 0 {
        return Err(Error::protocol("Razer power reply length/reserved byte"));
    }
    Ok([r.args[0], r.args[1]])
}
fn percent(raw: u8) -> u8 {
    ((u16::from(raw) * 100 + 127) / 255) as u8
}
fn encoded(percent: u8) -> u8 {
    ((u16::from(percent) * 255 + 50) / 100) as u8
}
pub fn read(link: &mut Link) -> Result<MouseTuning> {
    let seconds = u16::from_be_bytes(raw(link, Field::Sleep)?);
    let low = percent(raw(link, Field::LowPower)?[0]);
    let c = caps();
    Ok(MouseTuning {
        device_sleep_seconds: c.device_sleep_seconds.contains(&seconds).then_some(seconds),
        low_power_percent: c.low_power_percent.contains(&low).then_some(low),
        ..Default::default()
    })
}
#[derive(Clone, Copy)]
struct Change {
    field: Field,
    before: [u8; 2],
    want: [u8; 2],
}
fn rollback(link: &mut Link, attempted: &[Change]) -> Vec<String> {
    let mut errors = vec![];
    for change in attempted.iter().rev() {
        let result = (|| {
            let current = raw(link, change.field)?;
            if current == change.before {
                return Ok(());
            }
            if current != change.want {
                return Err(Error::protocol("foreign power value retained"));
            }
            link.exec(change.field.set(change.before))?;
            if raw(link, change.field)? != change.before {
                return Err(Error::protocol("power rollback readback mismatch"));
            }
            Ok(())
        })();
        if let Err(e) = result {
            errors.push(format!("{:?}: {e}", change.field));
        }
    }
    errors
}
pub fn apply(link: &mut Link, pid: u16, patch: &MouseTuning) -> Result<()> {
    validate(pid, patch)?;
    let mut changes = vec![];
    if let Some(seconds) = patch.device_sleep_seconds {
        let before = raw(link, Field::Sleep)?;
        let want = seconds.to_be_bytes();
        if before != want {
            changes.push(Change {
                field: Field::Sleep,
                before,
                want,
            });
        }
    }
    if let Some(value) = patch.low_power_percent {
        let before = raw(link, Field::LowPower)?;
        if percent(before[0]) != value {
            changes.push(Change {
                field: Field::LowPower,
                before,
                want: [encoded(value), 0],
            });
        }
    }
    let mut attempted = vec![];
    for change in changes {
        attempted.push(change);
        let result = link
            .exec(change.field.set(change.want))
            .and_then(|_| raw(link, change.field))
            .and_then(|r| {
                if r == change.want {
                    Ok(())
                } else {
                    Err(Error::protocol("power setting readback mismatch"))
                }
            });
        if let Err(error) = result {
            let errors = rollback(link, &attempted);
            return Err(Error::protocol(if errors.is_empty() {
                format!("{error}; attempted power fields restored/verified")
            } else {
                format!(
                    "{error}; power state uncertain, no retry: {}",
                    errors.join("; ")
                )
            }));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{report::Report, Timing};
    use gearwise_core::transport::{MockOp, MockReply, MockTransport};
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Fault {
        None,
        BeforeLowMalformed,
        SleepSetFailure,
        LowSetFailure,
        LowForeign,
        SleepForeign,
        LowRestoreFailure,
        SleepBusy,
        SleepVerifyCorrupt,
    }
    struct State {
        sleep: [u8; 2],
        low: [u8; 2],
        sleep_sets: usize,
        low_sets: usize,
        sleep_reads: usize,
        low_reads: usize,
        writes: Vec<(u8, [u8; 2])>,
    }
    fn rig(fault: Fault) -> (Link, Arc<Mutex<State>>) {
        let state = Arc::new(Mutex::new(State {
            sleep: 300u16.to_be_bytes(),
            low: [76, 0],
            sleep_sets: 0,
            low_sets: 0,
            sleep_reads: 0,
            low_reads: 0,
            writes: vec![],
        }));
        let shared = state.clone();
        let io = MockTransport::new(move |op| {
            let MockOp::SetFeature(data) = op else {
                panic!()
            };
            let req = Report::from_wire(data).unwrap();
            assert_eq!((req.class, req.transaction_id), (7, 31));
            assert_eq!(req.data_size, 2);
            let mut r = req.clone();
            r.status = 2;
            r.args = [0; 80];
            let mut s = shared.lock().unwrap();
            let mut corrupt = false;
            match req.command {
                0x83 => {
                    s.sleep_reads += 1;
                    r.args[..2].copy_from_slice(&s.sleep);
                    corrupt = fault == Fault::SleepVerifyCorrupt && s.sleep_reads == 2;
                }
                0x81 => {
                    s.low_reads += 1;
                    r.args[..2].copy_from_slice(&s.low);
                    if fault == Fault::BeforeLowMalformed && s.low_reads == 1 {
                        r.args[1] = 1;
                    }
                }
                3 => {
                    s.sleep_sets += 1;
                    s.sleep = [req.args[0], req.args[1]];
                    let value = s.sleep;
                    s.writes.push((3, value));
                    if s.sleep_sets == 1 {
                        if fault == Fault::SleepSetFailure {
                            r.status = 3;
                        }
                        if fault == Fault::SleepBusy {
                            r.status = 1;
                        }
                    }
                }
                1 => {
                    s.low_sets += 1;
                    s.low = [req.args[0], req.args[1]];
                    let value = s.low;
                    s.writes.push((1, value));
                    if s.low_sets == 1 {
                        if matches!(
                            fault,
                            Fault::LowSetFailure | Fault::SleepForeign | Fault::LowRestoreFailure
                        ) {
                            r.status = 3;
                        }
                        if fault == Fault::LowForeign {
                            s.low = [199, 0];
                        }
                        if fault == Fault::SleepForeign {
                            s.sleep = 444u16.to_be_bytes();
                        }
                    } else if fault == Fault::LowRestoreFailure {
                        r.status = 3;
                    }
                }
                _ => panic!("unexpected power command"),
            }
            let mut bytes = r.to_wire().to_vec();
            if corrupt {
                bytes[89] ^= 1;
            }
            MockReply::feature(bytes)
        });
        let mut link = Link::new(
            Box::new(io),
            31,
            Timing::new(Duration::ZERO, Duration::ZERO, 1),
        );
        link.strict_model_replies();
        (link, state)
    }
    fn both() -> MouseTuning {
        MouseTuning {
            device_sleep_seconds: Some(60),
            low_power_percent: Some(50),
            ..Default::default()
        }
    }
    #[test]
    fn success_reads_actual_fields_preserves_unchanged_raw_rounding_and_patches_only_selected_field(
    ) {
        let (mut l, s) = rig(Fault::None);
        let p = read(&mut l).unwrap();
        assert_eq!(p.device_sleep_seconds, Some(300));
        assert_eq!(p.low_power_percent, Some(30));
        apply(
            &mut l,
            0xb6,
            &MouseTuning {
                low_power_percent: Some(30),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(s.lock().unwrap().writes.is_empty());
        apply(
            &mut l,
            0xb6,
            &MouseTuning {
                device_sleep_seconds: Some(60),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(s.lock().unwrap().writes, [(3, 60u16.to_be_bytes())]);
        assert_eq!(s.lock().unwrap().low, [76, 0]);
        apply(
            &mut l,
            0xb6,
            &MouseTuning {
                low_power_percent: Some(50),
                ..Default::default()
            },
        )
        .unwrap();
        let p = read(&mut l).unwrap();
        assert_eq!(p.device_sleep_seconds, Some(60));
        assert_eq!(p.low_power_percent, Some(50));
    }
    #[test]
    fn uncertain_setters_busy_and_corrupt_readback_do_not_retry_and_restore_exact_original_bytes() {
        for fault in [
            Fault::SleepSetFailure,
            Fault::LowSetFailure,
            Fault::SleepBusy,
            Fault::SleepVerifyCorrupt,
        ] {
            let (mut l, s) = rig(fault);
            assert!(apply(&mut l, 0xb7, &both()).is_err(), "{fault:?}");
            let s = s.lock().unwrap();
            assert_eq!(s.sleep, 300u16.to_be_bytes(), "{fault:?}");
            assert_eq!(s.low, [76, 0], "{fault:?}");
            assert!(s.sleep_sets <= 2 && s.low_sets <= 2);
            assert_eq!(
                s.writes
                    .iter()
                    .filter(|(id, v)| *id == 3 && *v == 60u16.to_be_bytes())
                    .count(),
                1
            );
        }
    }
    #[test]
    fn foreign_state_is_not_restored_and_uncertain_restore_is_never_resent() {
        for fault in [
            Fault::LowForeign,
            Fault::SleepForeign,
            Fault::LowRestoreFailure,
        ] {
            let (mut l, s) = rig(fault);
            let e = apply(&mut l, 0xb6, &both()).unwrap_err().to_string();
            assert!(e.contains("uncertain"));
            let s = s.lock().unwrap();
            match fault {
                Fault::LowForeign => {
                    assert_eq!(s.low, [199, 0]);
                    assert_eq!(s.low_sets, 1);
                    assert_eq!(s.sleep, 300u16.to_be_bytes());
                }
                Fault::SleepForeign => {
                    assert_eq!(s.sleep, 444u16.to_be_bytes());
                    assert_eq!(s.sleep_sets, 1);
                    assert_eq!(s.low, [76, 0]);
                }
                _ => {
                    assert_eq!(s.low_sets, 2);
                    assert_eq!(s.sleep, 300u16.to_be_bytes());
                }
            }
        }
    }
    #[test]
    fn all_originals_are_checked_before_first_write_and_other_model_features_fail_preflight() {
        let (mut l, s) = rig(Fault::BeforeLowMalformed);
        assert!(apply(&mut l, 0xb6, &both()).is_err());
        assert!(s.lock().unwrap().writes.is_empty());
        for pid in [0x00aa, 0x00c0, 0xffff] {
            assert!(validate(pid, &both()).is_err());
        }
        for bad in [
            MouseTuning::default(),
            MouseTuning {
                device_sleep_seconds: Some(59),
                ..Default::default()
            },
            MouseTuning {
                low_power_percent: Some(4),
                ..Default::default()
            },
            MouseTuning {
                motion_sync: Some(false),
                ..Default::default()
            },
        ] {
            let (mut l, s) = rig(Fault::None);
            assert!(apply(&mut l, 0xb6, &bad).is_err());
            let s = s.lock().unwrap();
            assert!(s.writes.is_empty());
            assert_eq!((s.sleep_reads, s.low_reads), (0, 0));
        }
    }
}
