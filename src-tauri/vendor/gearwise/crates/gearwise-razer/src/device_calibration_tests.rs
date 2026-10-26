//! Driver-boundary checks using synthetic firmware, never a hardware claim.
use super::*;
use crate::testutil::{Fake, INSTANT, sent};
use gearwise_core::transport::MockRecord;
use std::sync::{Arc, Mutex};

type Log = Arc<Mutex<Vec<MockRecord>>>;

fn open(pid: u16, fake: &Fake) -> (RazerDevice, Log) {
    let model = crate::models::find(pid).unwrap();
    let fake = if crate::basilisk::supported(VID_RAZER, pid) {
        fake.clone().on(5, 0x84, 2, &[1])
    } else { fake.clone() };
    let (io, log) = fake.transport();
    let link = Link::new(Box::new(io), model.transaction_id, INSTANT);
    let iface = HidInterfaceInfo { path: "simulation:calibration-boundary".into(), ..Default::default() };
    let device = RazerDevice::open(model, link, &iface).unwrap();
    log.lock().unwrap().clear();
    (device, log)
}

fn readable_firmware() -> Fake {
    Fake::default()
        .on(4, 0x86, 5, &[])
        .on(4, 0x85, 2, &[1, 0x06, 0x40, 0x06, 0x40, 0, 0])
        .on(0, 0x85, 2, &[1])
        .on(7, 0x83, 2, &[1, 44])
        .on(7, 0x81, 2, &[77, 0])
        .on(7, 0x80, 2, &[0, 128])
        .on(7, 0x84, 2, &[0, 1])
}

#[test]
fn sensor_calibration_requires_confirmed_read_and_preserves_independent_settings() {
    let (mut device, log) = crate::dav3pro_sim::mouse(0x00b6).unwrap();
    assert!(device.capabilities().calibration.is_none());
    log.lock().unwrap().clear();
    assert!(matches!(device.apply(&Setting::SensorCalibration(
        SensorCalibration::Standard { distance: 2 })), Err(Error::Unsupported(_))));
    assert!(sent(&log).is_empty(), "an unconfirmed open must not write or probe");

    let before = device.read_state().unwrap();
    assert_eq!(before.calibration, Some(SensorCalibration::Standard { distance: 2 }));
    assert_eq!(device.capabilities().calibration, crate::dav3pro_calibration::calibration_caps(0x00b6));
    assert!(before.dpi.is_some() && before.tuning.is_some() && before.battery.is_some());
    assert_eq!(before.report_rate, Some(1000));
    assert!(before.lighting.is_empty());

    for value in [
        SensorCalibration::Asymmetric { lift: 26, landing: 25 },
        SensorCalibration::Standard { distance: 3 },
        SensorCalibration::Asymmetric { lift: 2, landing: 1 },
        SensorCalibration::Standard { distance: 1 },
    ] {
        device.apply(&Setting::SensorCalibration(value)).unwrap();
        let actual = device.read_state().unwrap();
        assert_eq!(actual.calibration, Some(value));
        assert_eq!(actual.dpi, before.dpi);
        assert_eq!(actual.report_rate, before.report_rate);
        assert_eq!(actual.tuning, before.tuning);
        assert_eq!(actual.battery, before.battery);
        assert_eq!(actual.buttons, before.buttons);
        assert!(device.capabilities().calibration.is_some());
    }
}

#[test]
fn sensor_calibration_invalid_request_sends_nothing_and_keeps_confirmed_state() {
    let (mut device, log) = crate::dav3pro_sim::mouse(0x00b6).unwrap();
    let before = device.read_state().unwrap();
    let caps = device.capabilities().calibration.clone();
    log.lock().unwrap().clear();
    for value in [
        SensorCalibration::Standard { distance: 0 },
        SensorCalibration::Standard { distance: 4 },
        SensorCalibration::Asymmetric { lift: 1, landing: 0 },
        SensorCalibration::Asymmetric { lift: 26, landing: 26 },
        SensorCalibration::Asymmetric { lift: 27, landing: 1 },
    ] {
        assert!(matches!(device.apply(&Setting::SensorCalibration(value)), Err(Error::InvalidValue(_))));
    }
    assert!(sent(&log).is_empty());
    assert_eq!(device.capabilities().calibration, caps);
    assert_eq!(device.read_state().unwrap().calibration, before.calibration);
}

#[test]
fn sensor_calibration_unknown_or_failed_read_clears_only_calibration_and_can_recover() {
    for (status, config) in [(2, vec![0, 4, 9, 1]), (5, vec![]), (4, vec![])] {
        let fake = readable_firmware()
            .on(0x0b, 0x83, 2, &[0, 4, 0])
            .on(0x0b, 0x8b, 2, &[0, 4, 1, 1])
            .on(0x0b, 0x8b, status, &config)
            .on(0x0b, 0x8b, 2, &[0, 4, 1, 2])
            .on(0x0b, 0x85, 2, &[0, 4, 12, 11]);
        let (mut device, log) = open(0x00b6, &fake);
        let first = device.read_state().unwrap();
        assert_eq!(first.calibration, Some(SensorCalibration::Standard { distance: 2 }));
        let failed = device.read_state().unwrap();
        assert_eq!(failed.calibration, None);
        assert!(device.capabilities().calibration.is_none());
        assert_eq!(device.state.calibration, None);
        assert_eq!(failed.dpi, first.dpi);
        assert_eq!(failed.report_rate, first.report_rate);
        assert_eq!(failed.tuning, first.tuning);
        assert_eq!(failed.battery, first.battery);
        log.lock().unwrap().clear();
        assert!(matches!(device.apply(&Setting::SensorCalibration(
            SensorCalibration::Standard { distance: 1 })), Err(Error::Unsupported(_))));
        assert!(sent(&log).is_empty());
        assert_eq!(device.read_state().unwrap().calibration,
            Some(SensorCalibration::Standard { distance: 3 }));
        assert!(device.capabilities().calibration.is_some());
    }
}

#[test]
fn sensor_calibration_other_razer_models_are_unsupported_before_board_or_mode_io() {
    for model in crate::models::MODELS {
        if crate::dav3pro_calibration::calibration_caps(model.pid).is_some() { continue; }
        let (mut device, log) = open(model.pid, &Fake::default());
        assert!(device.capabilities().calibration.is_none());
        assert!(matches!(device.apply(&Setting::SensorCalibration(
            SensorCalibration::Standard { distance: 2 })), Err(Error::Unsupported(_))), "{:04x}", model.pid);
        assert!(sent(&log).is_empty(), "{:04x}: unsupported calibration touched HID", model.pid);
    }
}

#[test]
fn sensor_calibration_uncertain_apply_invalidates_cached_capability_only() {
    let fake = readable_firmware()
        .on(0x0b, 0x83, 2, &[0, 4, 0])
        .on(0x0b, 0x8b, 2, &[0, 4, 1, 1])
        .on(0x0b, 0x85, 2, &[0, 4, 12, 11])
        .on(0x0b, 0x03, 4, &[])
        .on(0x0b, 0x0b, 4, &[])
        .on(0x0b, 0x05, 4, &[]);
    let (mut device, log) = open(0x00b6, &fake);
    let before = device.read_state().unwrap();
    assert!(device.apply(&Setting::SensorCalibration(
        SensorCalibration::Asymmetric { lift: 26, landing: 25 })).is_err());
    assert_eq!(device.state.calibration, None);
    assert!(device.capabilities().calibration.is_none());
    assert_eq!(device.state.dpi, before.dpi);
    assert_eq!(device.state.report_rate, before.report_rate);
    assert_eq!(device.state.tuning, before.tuning);
    assert_eq!(device.state.battery, before.battery);
    log.lock().unwrap().clear();
    assert!(matches!(device.apply(&Setting::SensorCalibration(
        SensorCalibration::Standard { distance: 1 })), Err(Error::Unsupported(_))));
    assert!(sent(&log).is_empty(), "a fresh confirmed read is needed after failure");
}

#[test]
fn sensor_calibration_unsupported_simulation_models_do_not_probe_or_publish_it() {
    for pid in [0x00b6, 0x00b7] {
        if crate::dav3pro_calibration::calibration_caps(pid).is_some() { continue; }
        let (mut device, log) = crate::dav3pro_sim::mouse(pid).unwrap();
        log.lock().unwrap().clear();
        assert_eq!(device.read_state().unwrap().calibration, None);
        assert!(device.capabilities().calibration.is_none());
        assert!(!sent(&log).iter().any(|p| p[7] == 0x0b));
    }
}
