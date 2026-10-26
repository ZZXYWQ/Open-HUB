//! Real public wire observations replayed through original encoders and Link.
//! This project has no physical DeathAdder V3 Pro; no model/button support claim.
use super::*;
use gearwise_core::{
    model::{DpiStage, DpiState},
    transport::{HidInterfaceInfo, MockOp, MockReply, MockTransport},
    Setting,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};
fn bytes(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len(), 180);
    let mut data = vec![0];
    for i in (0..hex.len()).step_by(2) {
        data.push(u8::from_str_radix(&hex[i..i + 2], 16).unwrap());
    }
    data
}
#[test]
fn twelve_public_wired_and_receiver_current_dpi_and_poll_roundtrips_match_complete_reports_and_native_transport(
) {
    let mut count = 0;
    for line in include_str!("testdata/deathadder-v3-pro-captured-roundtrips.tsv")
        .lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        let fields = line.split('|').collect::<Vec<_>>();
        assert_eq!(fields.len(), 7);
        let pid = u16::from_str_radix(fields[0], 16).unwrap();
        let model = models::find(pid).unwrap();
        assert_eq!(model.dpi.unwrap().max, 30000);
        let value = fields[2].parse::<u16>().unwrap();
        let want = bytes(fields[3]);
        let ack = bytes(fields[4]);
        let query = bytes(fields[5]);
        let reply = bytes(fields[6]);
        let (request, getter) = if fields[1] == "dpi" {
            (
                commands::set_dpi(DpiStage::uniform(value)),
                commands::get_dpi_variable(),
            )
        } else {
            (
                commands::set_polling(PollProto::Legacy, value)
                    .unwrap()
                    .remove(0),
                commands::get_polling(PollProto::Legacy),
            )
        };
        let q = Arc::new(Mutex::new(VecDeque::from([(want, ack), (query, reply)])));
        let sent = q.clone();
        let io = MockTransport::new(move |op| {
            let MockOp::SetFeature(data) = op else {
                panic!("only feature transport expected")
            };
            let (expected, response) = sent
                .lock()
                .unwrap()
                .pop_front()
                .expect("no command retry or extra request");
            assert_eq!(data, expected);
            MockReply::feature(response)
        });
        let mut link = link::Link::new(
            Box::new(io),
            model.transaction_id,
            Timing::new(Duration::ZERO, Duration::ZERO, 1),
        );
        link.strict_model_replies();
        link.exec(request).unwrap();
        let read = link.exec(getter).unwrap();
        if fields[1] == "dpi" {
            assert_eq!(commands::parse_dpi(&read), DpiStage::uniform(value));
        } else {
            assert_eq!(
                commands::parse_polling(PollProto::Legacy, &read),
                Some(value)
            );
        }
        assert!(q.lock().unwrap().is_empty());
        count += 1;
    }
    assert_eq!(count, 12);
}
#[test]
fn official_30k_limit_rejects_over_range_on_each_exact_transport_before_any_write() {
    for pid in [0x00b6, 0x00b7] {
        let fake = testutil::Fake::default();
        let (io, log) = fake.transport();
        let model = models::find(pid).unwrap();
        let link = link::Link::new(Box::new(io), model.transaction_id, testutil::INSTANT);
        let mut device = RazerDevice::open(model, link, &HidInterfaceInfo::default()).unwrap();
        assert_eq!(device.capabilities().dpi.as_ref().unwrap().max, 30000);
        assert!(device.capabilities().lighting.is_empty());
        log.lock().unwrap().clear();
        assert!(device
            .apply(&Setting::Dpi(DpiState {
                stages: vec![DpiStage::uniform(30001)],
                active: 0
            }))
            .is_err());
        assert!(log.lock().unwrap().is_empty());
    }
}

#[test]
fn exact_model_explicit_stage_refusal_uses_captured_current_selector_and_does_not_write_unverified_table(
) {
    for pid in [0x00b6, 0x00b7] {
        let fake = testutil::Fake::default()
            .on(4, 0x86, 5, &[])
            .on(4, 0x85, 2, &[1, 6, 0x40, 6, 0x40, 0, 0])
            .on(0, 0x85, 2, &[1]);
        let (io, log) = fake.transport();
        let model = models::find(pid).unwrap();
        let link = link::Link::new(Box::new(io), model.transaction_id, testutil::INSTANT);
        let mut d = RazerDevice::open(model, link, &HidInterfaceInfo::default()).unwrap();
        log.lock().unwrap().clear();
        assert_eq!(
            d.read_state().unwrap().dpi.unwrap(),
            DpiState {
                stages: vec![DpiStage::uniform(1600)],
                active: 0
            }
        );
        assert_eq!(d.capabilities().dpi.as_ref().unwrap().max_stages, 1);
        let sent = testutil::sent(&log);
        assert!(sent.iter().any(|b| b[7..9] == [4, 0x85] && b[9] == 1));
        assert_eq!(sent.iter().filter(|b| b[7..9] == [4, 0x86]).count(), 1);
        log.lock().unwrap().clear();
        assert!(d
            .apply(&Setting::Dpi(DpiState {
                stages: vec![DpiStage::uniform(800); 2],
                active: 0
            }))
            .is_err());
        assert!(log.lock().unwrap().is_empty());
        d.apply(&Setting::Dpi(DpiState {
            stages: vec![DpiStage::uniform(800)],
            active: 0,
        }))
        .unwrap();
        let sent = testutil::sent(&log);
        assert_eq!(sent.len(), 1);
        assert_eq!(&sent[0][7..16], &[4, 5, 1, 3, 0x20, 3, 0x20, 0, 0]);
        log.lock().unwrap().clear();
        d.read_state().unwrap();
        assert!(!testutil::sent(&log).iter().any(|b| b[7..9] == [4, 0x86]));
    }
}
#[test]
fn malformed_stage_success_does_not_enable_current_fallback_or_change_other_model_behavior() {
    for pid in [0x00b6, 0x00b7, 0x007a] {
        let fake = if pid == 0x007a {
            testutil::Fake::default().on(4, 0x86, 5, &[])
        } else {
            testutil::Fake::default().on(4, 0x86, 2, &[])
        };
        let (io, log) = fake.transport();
        let model = models::find(pid).unwrap();
        let link = link::Link::new(Box::new(io), model.transaction_id, testutil::INSTANT);
        let mut d = RazerDevice::open(model, link, &HidInterfaceInfo::default()).unwrap();
        log.lock().unwrap().clear();
        let _ = d.read_state();
        assert!(!testutil::sent(&log).iter().any(|b| b[7..9] == [4, 0x85]));
        assert_eq!(d.capabilities().dpi.as_ref().unwrap().max_stages, 5);
    }
}
