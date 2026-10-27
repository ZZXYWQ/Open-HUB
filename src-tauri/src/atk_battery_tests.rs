//! Production transport telemetry, not physical battery measurements.
use super::*;
use crate::atk::*;
use gearwise_core::{transport::MockRecord, HidTransport};
use std::sync::{Arc, Mutex};
fn commands(log: &Arc<Mutex<Vec<MockRecord>>>, command: u8) -> Vec<Vec<u8>> {
    log.lock()
        .unwrap()
        .iter()
        .filter_map(|r| match r {
            MockRecord::Write(p) if p[1] == command => Some(p.clone()),
            _ => None,
        })
        .collect()
}
struct Telemetry {
    inner: Box<dyn HidTransport>,
    raw: Vec<u8>,
    fault: u8,
    queried: bool,
}
impl HidTransport for Telemetry {
    fn write(&mut self, p: &[u8]) -> Result<usize> {
        if p[1] == 4 {
            self.queried = true;
        }
        let written=self.inner.write(p)?;
        if p[1]==4&&self.fault==6{return Ok(16);}
        Ok(written)
    }
    fn read_timeout(&mut self, p: &mut [u8], ms: i32) -> Result<usize> {
        let n = self.inner.read_timeout(p, ms)?;
        if n == 17 && p[1] == 4 {
            if self.fault==5{return Ok(0);}
            let reply = frame(4, 0, self.raw.len(), &self.raw)?;
            p[..17].copy_from_slice(&reply);
            match self.fault {
                1 => p[16] ^= 1,
                2 => return Ok(16),
                3 => {
                    p[2] = 1;
                    p[16] = 0x55u8.wrapping_sub(sum(&p[..16]));
                }
                _ => {}
            }
        } else if n == 17 && p[1] == 0x10 && self.fault == 4 && self.queried {
            p[7] = 255;
            p[16] = 0x55u8.wrapping_sub(sum(&p[..16]));
        }
        Ok(n)
    }
    fn send_feature_report(&mut self, p: &[u8]) -> Result<()> {
        self.inner.send_feature_report(p)
    }
    fn get_feature_report(&mut self, p: &mut [u8]) -> Result<usize> {
        self.inner.get_feature_report(p)
    }
}
#[test]
fn battery_fields_do_not_infer_unknown_percent_status_or_voltage_units() {
    for (percent, charge) in [(0, 0), (0, 1), (73, 0), (73, 1), (100, 0), (100, 1)] {
        assert_eq!(
            decode(&[percent, charge, 0xff, 0xff]).unwrap(),
            gearwise_core::model::Battery {
                percent: Some(percent),
                charging: charge == 1
            }
        );
    }
    for raw in [
        vec![],
        vec![73],
        vec![73, 1],
        vec![73, 1, 2],
        vec![73, 1, 2, 3, 4],
        vec![101, 0, 0, 0],
        vec![255, 0, 0, 0],
        vec![73, 2, 0, 0],
        vec![73, 255, 0, 0],
    ] {
        assert!(decode(&raw).is_err());
    }
}
#[test]
fn all_nine_battery_transports_read_without_eeprom_or_other_mouse_changes() {
    let eligible = models()
        .iter()
        .filter(|m| battery_supported(m.vendor, m.product))
        .collect::<Vec<_>>();
    assert_eq!(eligible.len(), 9);
    for m in eligible {
        let (mut d, log) = memory_device(m.product).unwrap();
        let (mut other, _) = memory_device(m.product).unwrap();
        let peer = other.read_state().unwrap();
        let before = d.read_blob(0, 768 + 12 * 384).unwrap();
        log.lock().unwrap().clear();
        let state = d.read_state().unwrap();
        assert!(d.capabilities().battery);
        assert_eq!(
            state.battery,
            Some(gearwise_core::model::Battery {
                percent: Some(73),
                charging: true
            })
        );
        assert_eq!(commands(&log, 4).len(), 1);
        assert!(commands(&log, 7).is_empty());
        assert_eq!(d.read_blob(0, before.len()).unwrap(), before);
        assert_eq!(other.read_state().unwrap(), peer);
    }
}
#[test]
fn battery_complete_request_is_source_reconstructed_zero_length_query() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    log.lock().unwrap().clear();
    d.read_state().unwrap();
    assert_eq!(
        commands(&log, 4),
        vec![vec![8, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 73]]
    );
}
#[test]
fn pulsar_does_not_inherit_battery_command_from_shared_eeprom_codec() {
    for product in [0xf507, 0xf508] {
        let (mut d, log) = memory_device(product).unwrap();
        assert!(!d.capabilities().battery);
        log.lock().unwrap().clear();
        assert!(d.read_state().unwrap().battery.is_none());
        assert!(commands(&log, 4).is_empty());
    }
}
#[test]
fn invalid_battery_data_and_transport_replies_are_unknown_without_stale_values() {
    for (raw, fault) in [
        (vec![101, 0, 0, 0], 0),
        (vec![73, 2, 0, 0], 0),
        (vec![73, 1], 0),
        (vec![73, 1, 0, 0], 1),
        (vec![73, 1, 0, 0], 2),
        (vec![73, 1, 0, 0], 3),
        (vec![73, 1, 0, 0], 5),
        (vec![73, 1, 0, 0], 6),
    ] {
        let (mut d, log) = memory_device(0x1017).unwrap();
        assert!(d.read_state().unwrap().battery.is_some());
        d.io = Box::new(Telemetry {
            inner: d.io,
            raw,
            fault,
            queried: false,
        });
        log.lock().unwrap().clear();
        let state = d.read_state().unwrap();
        assert!(state.battery.is_none());
        assert!(state.dpi.is_some());
        assert_eq!(commands(&log, 4).len(), 1);
        assert!(commands(&log, 7).is_empty());
    }
}
#[test]
fn identity_change_after_battery_query_does_not_return_a_mixed_device_state() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    d.io = Box::new(Telemetry {
        inner: d.io,
        raw: vec![35, 0, 0, 0],
        fault: 4,
        queried: false,
    });
    log.lock().unwrap().clear();
    assert!(d.read_state().is_err());
    assert_eq!(commands(&log, 4).len(), 1);
    assert!(commands(&log, 7).is_empty());
}
#[test]
fn export_compx_battery_ui_fixture() {
    let mut views = vec![];
    for m in models() {
        let (mut d, _) = memory_device(m.product).unwrap();
        let state = d.read_state().unwrap();
        views.push(serde_json::json!({"descriptor":d.descriptor(),"capabilities":d.capabilities(),"state":state,"readback":"device_partial","hardware_tested":false,"native_macros":d.macro_definitions(),"session":null}));
    }
    if let Some(path) = std::env::var_os("OPEN_HUB_COMPX_BATTERY_FIXTURE") {
        std::fs::write(path, serde_json::to_vec_pretty(&views).unwrap()).unwrap();
    }
}
