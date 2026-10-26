//! Driver-boundary checks using synthetic firmware, never a hardware claim.
use super::*;
use crate::testutil::{sent, Fake, INSTANT};
use gearwise_core::transport::MockRecord;
use std::sync::{Arc, Mutex};

type Log = Arc<Mutex<Vec<MockRecord>>>;
const EXACT_TRANSPORTS: [u16; 2] = [0x00b6, 0x00b7];

fn open(pid: u16, fake: &Fake) -> (RazerDevice, Log) {
    let model = crate::models::find(pid).unwrap();
    let fake = if crate::basilisk::supported(VID_RAZER, pid) {
        fake.clone().on(5, 0x84, 2, &[1])
    } else {
        fake.clone()
    };
    let (io, log) = fake.transport();
    let link = Link::new(Box::new(io), model.transaction_id, INSTANT);
    let iface = HidInterfaceInfo {
        path: "simulation:calibration-boundary".into(),
        ..Default::default()
    };
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
    for pid in EXACT_TRANSPORTS {
        for staged in [false, true] {
            assert!(
                crate::dav3pro_calibration::calibration_caps(pid).is_some(),
                "{pid:04x}"
            );
            let (mut device, log) = if staged {
                crate::dav3pro_sim::mouse(pid)
            } else {
                crate::dav3pro_sim::current_mouse(pid)
            }
            .unwrap();
            assert!(device.capabilities().calibration.is_none());
            log.lock().unwrap().clear();
            assert!(matches!(
                device.apply(&Setting::SensorCalibration(SensorCalibration::Standard {
                    distance: 2
                })),
                Err(Error::Unsupported(_))
            ));
            assert!(
                sent(&log).is_empty(),
                "an unconfirmed open must not write or probe"
            );

            let before = device.read_state().unwrap();
            assert_eq!(
                before.calibration,
                Some(SensorCalibration::Standard { distance: 2 })
            );
            assert_eq!(
                device.capabilities().calibration,
                crate::dav3pro_calibration::calibration_caps(pid)
            );
            assert!(before.dpi.is_some() && before.tuning.is_some() && before.battery.is_some());
            assert_eq!(before.report_rate, Some(1000));
            assert!(before.lighting.is_empty());

            for value in [
                SensorCalibration::Asymmetric {
                    lift: 26,
                    landing: 25,
                },
                SensorCalibration::Standard { distance: 3 },
                SensorCalibration::Asymmetric {
                    lift: 2,
                    landing: 1,
                },
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
    }
}

#[test]
fn sensor_calibration_invalid_request_sends_nothing_and_keeps_confirmed_state() {
    for pid in EXACT_TRANSPORTS {
        let (mut device, log) = crate::dav3pro_sim::mouse(pid).unwrap();
        let before = device.read_state().unwrap();
        let caps = device.capabilities().calibration.clone();
        log.lock().unwrap().clear();
        for value in [
            SensorCalibration::Standard { distance: 0 },
            SensorCalibration::Standard { distance: 4 },
            SensorCalibration::Asymmetric {
                lift: 1,
                landing: 0,
            },
            SensorCalibration::Asymmetric {
                lift: 26,
                landing: 26,
            },
            SensorCalibration::Asymmetric {
                lift: 27,
                landing: 1,
            },
        ] {
            assert!(matches!(
                device.apply(&Setting::SensorCalibration(value)),
                Err(Error::InvalidValue(_))
            ));
        }
        assert!(sent(&log).is_empty());
        assert_eq!(device.capabilities().calibration, caps);
        assert_eq!(device.read_state().unwrap().calibration, before.calibration);
    }
}

#[test]
fn sensor_calibration_unknown_or_failed_read_clears_only_calibration_and_can_recover() {
    for pid in EXACT_TRANSPORTS {
        for (status, config) in [(2, vec![0, 4, 9, 1]), (5, vec![]), (4, vec![])] {
            let fake = readable_firmware()
                .on(0x0b, 0x83, 2, &[0, 4, 0])
                .on(0x0b, 0x8b, 2, &[0, 4, 1, 1])
                .on(0x0b, 0x8b, status, &config)
                .on(0x0b, 0x8b, 2, &[0, 4, 1, 2])
                .on(0x0b, 0x85, 2, &[0, 4, 12, 11]);
            let (mut device, log) = open(pid, &fake);
            let first = device.read_state().unwrap();
            assert_eq!(
                first.calibration,
                Some(SensorCalibration::Standard { distance: 2 })
            );
            let failed = device.read_state().unwrap();
            assert_eq!(failed.calibration, None);
            assert!(device.capabilities().calibration.is_none());
            assert_eq!(device.state.calibration, None);
            assert_eq!(failed.dpi, first.dpi);
            assert_eq!(failed.report_rate, first.report_rate);
            assert_eq!(failed.tuning, first.tuning);
            assert_eq!(failed.battery, first.battery);
            log.lock().unwrap().clear();
            assert!(matches!(
                device.apply(&Setting::SensorCalibration(SensorCalibration::Standard {
                    distance: 1
                })),
                Err(Error::Unsupported(_))
            ));
            assert!(sent(&log).is_empty());
            assert_eq!(
                device.read_state().unwrap().calibration,
                Some(SensorCalibration::Standard { distance: 3 })
            );
            assert!(device.capabilities().calibration.is_some());
        }
    }
}

#[test]
fn sensor_calibration_other_razer_models_are_unsupported_before_board_or_mode_io() {
    for model in crate::models::MODELS {
        if EXACT_TRANSPORTS.contains(&model.pid) {
            continue;
        }
        assert!(
            crate::dav3pro_calibration::calibration_caps(model.pid).is_none(),
            "{:04x}",
            model.pid
        );
        let (mut device, log) = open(model.pid, &Fake::default());
        assert!(device.capabilities().calibration.is_none());
        assert!(
            matches!(
                device.apply(&Setting::SensorCalibration(SensorCalibration::Standard {
                    distance: 2
                })),
                Err(Error::Unsupported(_))
            ),
            "{:04x}",
            model.pid
        );
        assert!(
            sent(&log).is_empty(),
            "{:04x}: unsupported calibration touched HID",
            model.pid
        );
    }
}

#[test]
fn sensor_calibration_nearby_unimplemented_ids_are_rejected_before_open_or_probe_io() {
    use gearwise_core::{transport::MockBackend, Driver};
    for pid in [0x00b5, 0x00b8, 0x00c2, 0x00c3, 0xffff] {
        assert!(crate::dav3pro_calibration::calibration_caps(pid).is_none());
        assert!(matches!(
            crate::dav3pro_calibration::validate_sensor_calibration(
                pid,
                &SensorCalibration::Standard { distance: 2 }
            ),
            Err(Error::Unsupported(_))
        ));
        let iface = HidInterfaceInfo {
            path: format!("simulation:unproved:{pid:04x}"),
            vendor_id: VID_RAZER,
            product_id: pid,
            interface_number: 0,
            usage_page: 1,
            usage: 2,
            ..Default::default()
        };
        let backend = MockBackend::new(vec![iface.clone()], |_| {
            panic!("an unimplemented exact product must not open a HID interface")
        });
        let driver = crate::RazerDriver::new();
        assert!(!driver.probe(&iface));
        assert!(matches!(
            driver.open(&iface, &backend),
            Err(Error::Unsupported(_))
        ));
        let (io, log) = Fake::default().transport();
        let mut link = Link::new(Box::new(io), 0x1f, INSTANT);
        assert!(matches!(
            crate::dav3pro_calibration::read(&mut link, pid),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            crate::dav3pro_calibration::apply(
                &mut link,
                pid,
                &SensorCalibration::Standard { distance: 2 }
            ),
            Err(Error::Unsupported(_))
        ));
        assert!(
            sent(&log).is_empty(),
            "{pid:04x}: unproved calibration touched HID"
        );
    }
}

#[test]
fn sensor_calibration_uncertain_apply_invalidates_cached_capability_only() {
    for pid in EXACT_TRANSPORTS {
        let fake = readable_firmware()
            .on(0x0b, 0x83, 2, &[0, 4, 0])
            .on(0x0b, 0x8b, 2, &[0, 4, 1, 1])
            .on(0x0b, 0x85, 2, &[0, 4, 12, 11])
            .on(0x0b, 0x03, 4, &[])
            .on(0x0b, 0x0b, 4, &[])
            .on(0x0b, 0x05, 4, &[]);
        let (mut device, log) = open(pid, &fake);
        let before = device.read_state().unwrap();
        assert!(device
            .apply(&Setting::SensorCalibration(SensorCalibration::Asymmetric {
                lift: 26,
                landing: 25
            }))
            .is_err());
        assert_eq!(device.state.calibration, None);
        assert!(device.capabilities().calibration.is_none());
        assert_eq!(device.state.dpi, before.dpi);
        assert_eq!(device.state.report_rate, before.report_rate);
        assert_eq!(device.state.tuning, before.tuning);
        assert_eq!(device.state.battery, before.battery);
        log.lock().unwrap().clear();
        assert!(matches!(
            device.apply(&Setting::SensorCalibration(SensorCalibration::Standard {
                distance: 1
            })),
            Err(Error::Unsupported(_))
        ));
        assert!(
            sent(&log).is_empty(),
            "a fresh confirmed read is needed after failure"
        );
    }
}

#[test]
fn sensor_calibration_simulated_transports_keep_separate_state_and_exact_get_records() {
    let (mut wired, wired_log) = crate::dav3pro_sim::mouse(EXACT_TRANSPORTS[0]).unwrap();
    let (mut receiver, receiver_log) = crate::dav3pro_sim::mouse(EXACT_TRANSPORTS[1]).unwrap();
    wired_log.lock().unwrap().clear();
    receiver_log.lock().unwrap().clear();
    for (device, log) in [(&mut wired, &wired_log), (&mut receiver, &receiver_log)] {
        assert_eq!(
            device.read_state().unwrap().calibration,
            Some(SensorCalibration::Standard { distance: 2 })
        );
        assert!(device.capabilities().calibration.is_some());
        let gets = sent(log)
            .into_iter()
            .filter(|p| p[7] == 0x0b)
            .collect::<Vec<_>>();
        assert_eq!(
            gets.iter()
                .map(|p| (p[6], p[8], p[9..11].to_vec()))
                .collect::<Vec<_>>(),
            [
                (3, 0x83, vec![0, 4]),
                (4, 0x8b, vec![0, 4]),
                (4, 0x85, vec![0, 4])
            ]
        );
    }
    let receiver_before = receiver.read_state().unwrap();
    wired
        .apply(&Setting::SensorCalibration(SensorCalibration::Asymmetric {
            lift: 26,
            landing: 25,
        }))
        .unwrap();
    assert_eq!(receiver.read_state().unwrap(), receiver_before);
    let wired_before = wired.read_state().unwrap();
    receiver
        .apply(&Setting::SensorCalibration(SensorCalibration::Standard {
            distance: 3,
        }))
        .unwrap();
    assert_eq!(wired.read_state().unwrap(), wired_before);
    assert_eq!(
        receiver.read_state().unwrap().calibration,
        Some(SensorCalibration::Standard { distance: 3 })
    );
    assert_eq!(
        wired_before.calibration,
        Some(SensorCalibration::Asymmetric {
            lift: 26,
            landing: 25
        })
    );
}
