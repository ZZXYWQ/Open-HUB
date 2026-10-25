use super::*;
use gearwise_core::transport::{MockRecord, MockReply, MockTransport};
use std::sync::{Arc, Mutex};
fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
fn sample(delay: u16) -> Macro {
    Macro {
        id: "macro-a".into(),
        name: "A".into(),
        playback: Playback::Times { count: 1 },
        events: vec![
            Event {
                input: Input::Key(4),
                down: true,
                delay_ms: delay,
            },
            Event {
                input: Input::Key(4),
                down: false,
                delay_ms: 10,
            },
        ],
    }
}
fn sequences(keys: &[u8]) -> Vec<Event> {
    keys.iter()
        .flat_map(|k| {
            [
                Event {
                    input: Input::Key(*k),
                    down: true,
                    delay_ms: 10,
                },
                Event {
                    input: Input::Key(*k),
                    down: false,
                    delay_ms: 10,
                },
            ]
        })
        .collect()
}
#[test]
fn all_320_dpi_captures_match_complete_wireless_and_wired_reports() {
    let captures: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/x11/dpi-captures.json")).unwrap();
    let mut count = 0;
    for row in captures["samples"].as_array().unwrap() {
        let value = row["label"]
            .as_str()
            .unwrap()
            .strip_prefix("DPI ")
            .unwrap()
            .parse::<u16>()
            .unwrap();
        let expected = row["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        let mut d = Profile::example().dpi;
        d.stages[0] = value;
        d.opaque_14_15 = [expected[14], expected[15]];
        // Captures changed their otherwise unrelated sixth stage from 12000 to
        // 22000. Both are independent documented settings, not patched raw bytes.
        d.stages[5] = if expected[13] == 0x8d { 12000 } else { 22000 };
        assert_eq!(encode_dpi(&d, true).unwrap(), expected, "DPI {value}");
        assert_eq!(
            encode_dpi(&d, false).unwrap(),
            expected[..52],
            "wired DPI {value}"
        );
        count += 1;
    }
    assert_eq!(count, 320);
    assert_eq!(dpi_values().len(), 320);
    for invalid_dpi in [0, 49, 51, 10001, 10050, 12050, 22001, 65535] {
        let mut d = Profile::example().dpi;
        d.stages[2] = invalid_dpi;
        assert!(encode_dpi(&d, true).is_err());
    }
}
#[test]
fn capture_backed_macro_delays_all_slots_counts_mouse_and_page_boundaries() {
    let captures: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/x11/macro-captures.json")).unwrap();
    let mut verified = 0;
    for (i, row) in captures.as_array().unwrap().iter().enumerate() {
        let mut m = sample(10);
        let mut slot = 8;
        match i {
            0..=20 => {
                m.events[0].delay_ms = row["label"]
                    .as_str()
                    .unwrap()
                    .split(' ')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap()
            }
            21 => {
                m.events.push(Event {
                    input: Input::Key(4),
                    down: true,
                    delay_ms: 10,
                });
                assert!(encode_macro(8, &m).is_err());
                continue;
            }
            22 => m.events = sequences(&[4, 4]),
            23 | 24 => {
                m.events = sequences(&[4; 23]);
                if i == 24 {
                    m.events[0].delay_ms = 1271;
                }
            }
            25 => m.playback = Playback::Times { count: 2 },
            26 => m.playback = Playback::Times { count: 65 },
            27 => m.playback = Playback::Times { count: 255 },
            28..=32 => slot = [1, 2, 3, 7, 8][i - 28],
            33 | 34 => {
                m.events = vec![Event {
                    input: Input::Key(4),
                    down: i == 33,
                    delay_ms: 10,
                }];
                assert!(encode_macro(8, &m).is_err());
                continue;
            }
            35 => m.events = sequences(&[5]),
            36 => m.events = sequences(&[6]),
            37..=40 => {
                m.events = sequences(&[6, 29]);
                match i {
                    38 => m.events[0].delay_ms = 1271,
                    39 => m.events[2].delay_ms = 1271,
                    40 => m.events[1].delay_ms = 5000,
                    _ => {}
                }
            }
            41..=45 => {
                let input = Input::Mouse(
                    [
                        Mouse::Left,
                        Mouse::Right,
                        Mouse::Middle,
                        Mouse::Forward,
                        Mouse::Back,
                    ][i - 41],
                );
                for e in &mut m.events {
                    e.input = input;
                }
            }
            46..=52 => {
                for e in &mut m.events {
                    e.input = Input::Mouse(Mouse::Back);
                }
                m.events[0].delay_ms = [15, 20, 35, 55, 75, 95, 255][i - 46];
            }
            _ => panic!("unreviewed capture"),
        }
        let actual = encode_macro(slot, &m).unwrap();
        for (part, packet) in actual.iter().enumerate() {
            assert_eq!(
                *packet,
                hex(row["packets"][part].as_str().unwrap()),
                "sample {i} {} page {part}",
                row["label"]
            );
        }
        verified += 1;
    }
    assert_eq!(verified, 50);
}
#[test]
fn macro_capacity_validation_never_truncates_events_or_holds() {
    let mut m = sample(10);
    m.events = sequences(&[4; 23]);
    m.events[0].delay_ms = 1271;
    let p = encode_macro(8, &m).unwrap();
    assert_eq!(p[0][29], 47);
    m.events[2].delay_ms = 1271;
    assert!(encode_macro(8, &m).is_err());
    for delay in [0, 51001, 65535] {
        assert!(encode_macro(8, &sample(delay)).is_err());
    }
    let mut m = sample(10);
    m.events.last_mut().unwrap().delay_ms = 11;
    assert!(encode_macro(8, &m).is_err());
    let mut m = sample(51000);
    let p = encode_macro(8, &m).unwrap();
    assert_eq!(&p[0][30..36], &[1, 4, 255, 3, 129, 4]);
    m.events[0].input = Input::Key(3);
    assert!(encode_macro(8, &m).is_err());
    let mut m = sample(10);
    m.playback = Playback::Times { count: 0 };
    assert!(encode_macro(8, &m).is_err());
    for (mode, raw) in [(Playback::UntilAnyKey, 1), (Playback::WhileHeld, 2)] {
        m.playback = mode;
        let p = encode_macro(8, &m).unwrap();
        assert_eq!(p[0][4], raw);
        assert_eq!(p[0][8], 1);
    }
}
#[test]
fn light_modes_sleep_buckets_color_thresholds_and_exact_rate_packet() {
    let mut p = Profile::example();
    for (mode, raw) in [
        (Light::Off, 0),
        (Light::Static, 0x10),
        (Light::Breathing, 0x20),
        (Light::Neon, 0x30),
        (Light::ColorBreathing, 0x40),
        (Light::StaticDpi, 0x50),
        (Light::BreathingDpi, 0x60),
    ] {
        p.preferences.light = mode;
        p.preferences.color = Rgb::new(99, 100, 255);
        for minutes in [1, 16, 17, 32, 33, 48, 49, 60] {
            for speed in 1..=5 {
                p.preferences.deep_sleep_seconds = minutes * 60;
                p.preferences.light_speed = speed;
                let bytes = encode_preferences(&p.preferences, true).unwrap();
                assert_eq!(bytes.len(), 15);
                assert_eq!(bytes[3], raw);
                assert_eq!(bytes[4], ((((minutes - 1) / 16) as u8) << 4) | (6 - speed));
                assert_eq!(bytes[5], (8 + minutes * 16) as u8);
                assert_eq!(bytes[11], 2 + u8::from(mode == Light::BreathingDpi));
                assert_eq!(
                    bytes[12],
                    bytes[3..11].iter().fold(0u8, |a, b| a.wrapping_add(*b))
                );
                assert_eq!(
                    encode_preferences(&p.preferences, false).unwrap(),
                    bytes[..13]
                );
            }
        }
    }
    assert_eq!(
        compile(VID, WIRED, &p).unwrap().packets[2],
        [6, 9, 1, 1, 254, 0, 0, 0, 0]
    );
    p.report_rate = 250;
    assert_eq!(
        compile(VID, WIRELESS, &p).unwrap().packets[2],
        [6, 9, 1, 4, 251, 0, 0, 0, 0]
    );
    for (sleep, deep, debounce, speed) in [
        (0, 600, 4, 3),
        (31, 600, 4, 3),
        (1830, 600, 4, 3),
        (30, 59, 4, 3),
        (30, 3660, 4, 3),
        (30, 600, 3, 3),
        (30, 600, 51, 3),
        (30, 600, 4, 0),
        (30, 600, 4, 6),
    ] {
        let mut v = p.preferences.clone();
        v.sleep_seconds = sleep;
        v.deep_sleep_seconds = deep;
        v.debounce_ms = debounce;
        v.light_speed = speed;
        assert!(encode_preferences(&v, true).is_err());
    }
}
#[test]
fn eight_controls_are_independent_and_complete_map_is_not_reset_between_macros() {
    let mut p = Profile::example();
    p.macros = vec![sample(10)];
    p.buttons.insert(4, Action::Macro("macro-a".into()));
    p.buttons.insert(5, Action::Macro("macro-a".into()));
    p.buttons.insert(7, Action::Key { usage: 6, mods: 9 });
    let c = compile(VID, WIRED, &p).unwrap();
    assert_eq!(c.packets.len(), 10);
    assert_eq!(&c.packets[3][24..27], &[18, 0, 8]);
    assert_eq!(&c.packets[3][21..24], &[18, 0, 7]);
    assert_eq!(&c.packets[3][51..54], &[17, 9, 6]);
    assert_eq!(&c.packets[3][54..57], &[10, 0, 0]);
    assert_eq!(c.packets.iter().filter(|v| v[0] == 8).count(), 1);
    assert_eq!(c.packets[4][2], 8);
    assert_eq!(c.packets[7][2], 7);
    p.buttons.insert(6, Action::Macro("macro-a".into()));
    assert!(compile(VID, WIRED, &p).is_err());
    let mut p = Profile::example();
    p.buttons.insert(1, Action::Disabled);
    assert!(compile(VID, WIRED, &p).is_err());
    p.buttons.insert(5, Action::Left);
    compile(VID, WIRED, &p).unwrap();
    p.buttons.remove(&8);
    assert!(compile(VID, WIRED, &p).is_err());
}
struct Fault {
    calls: Arc<Mutex<Vec<Vec<u8>>>>,
    fail: usize,
}
impl HidTransport for Fault {
    fn write(&mut self, _: &[u8]) -> Result<usize> {
        panic!("no output-report fallback")
    }
    fn read_timeout(&mut self, _: &mut [u8], _: i32) -> Result<usize> {
        panic!("no invented interrupt acknowledgement")
    }
    fn get_feature_report(&mut self, _: &mut [u8]) -> Result<usize> {
        panic!("no invented hardware readback")
    }
    fn send_feature_report(&mut self, p: &[u8]) -> Result<()> {
        let mut calls = self.calls.lock().unwrap();
        calls.push(p.to_vec());
        if calls.len() == self.fail {
            Err(Error::Disconnected)
        } else {
            Ok(())
        }
    }
}
#[test]
fn entire_profile_validates_before_io_and_each_failure_invalidates_cache_without_retry() {
    for failure in 1..=7 {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let mut w = Writer::new(
            Box::new(Fault {
                calls: calls.clone(),
                fail: failure,
            }),
            VID,
            WIRED,
        )
        .unwrap();
        w.pause = false;
        let mut p = Profile::example();
        p.macros = vec![sample(10)];
        p.buttons.insert(4, Action::Macro("macro-a".into()));
        let mut invalid = p.clone();
        invalid.preferences.sleep_seconds = 0;
        assert!(w.apply(&invalid).is_err());
        assert!(calls.lock().unwrap().is_empty());
        assert!(w.cached().is_none());
        let error = w.apply(&p).unwrap_err().to_string();
        assert!(error.contains("未重试"));
        assert_eq!(calls.lock().unwrap().len(), failure);
        assert!(w.cached().is_none());
    }
}
#[test]
fn read_is_honestly_unknown_until_sent_and_two_handles_do_not_share_profiles() {
    let a = MockTransport::new(|_| MockReply::none());
    let alog = a.log();
    let b = MockTransport::new(|_| MockReply::none());
    let blog = b.log();
    let mut a = Writer::new(Box::new(a), VID, WIRED).unwrap();
    a.pause = false;
    let mut b = Writer::new(Box::new(b), VID, WIRELESS).unwrap();
    b.pause = false;
    assert!(a.cached().is_none());
    assert!(b.cached().is_none());
    assert!(alog.lock().unwrap().is_empty());
    let p = Profile::example();
    assert_eq!(a.apply(&p).unwrap(), 4);
    assert_eq!(a.cached(), Some(&p));
    assert!(b.cached().is_none());
    assert!(blog.lock().unwrap().is_empty());
    let mut q = p.clone();
    q.dpi.stages[0] = 400;
    q.preferences.light = Light::Neon;
    b.apply(&q).unwrap();
    assert_eq!(a.cached(), Some(&p));
    assert_eq!(b.cached(), Some(&q));
    let log = blog.lock().unwrap();
    assert!(log.iter().all(|v| matches!(v, MockRecord::SetFeature(_))));
    if let MockRecord::SetFeature(d) = &log[0] {
        assert_eq!(d.len(), 56);
    } else {
        panic!()
    }
    let mut bad = q.clone();
    bad.report_rate = 2000;
    assert!(b.apply(&bad).is_err());
    assert_eq!(b.cached(), Some(&q));
}
#[test]
fn wrong_model_and_unknown_fields_fail_closed() {
    for (v, p) in [(VID, 0xfa61), (0x3554, WIRED), (VID, 0x1234)] {
        assert!(compile(v, p, &Profile::example()).is_err());
    }
    let mut data = serde_json::to_value(Profile::example()).unwrap();
    data["private_vendor_bytes"] = serde_json::json!([1, 2, 3]);
    assert!(serde_json::from_value::<Profile>(data).is_err());
    let p = Profile::example();
    assert_eq!(
        p,
        serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap()
    );
}
#[test]
fn real_writer_spaces_feature_reports_by_at_least_250_ms() {
    struct Timed(Arc<Mutex<Vec<Instant>>>);
    impl HidTransport for Timed {
        fn write(&mut self, _: &[u8]) -> Result<usize> {
            unreachable!()
        }
        fn read_timeout(&mut self, _: &mut [u8], _: i32) -> Result<usize> {
            unreachable!()
        }
        fn get_feature_report(&mut self, _: &mut [u8]) -> Result<usize> {
            unreachable!()
        }
        fn send_feature_report(&mut self, _: &[u8]) -> Result<()> {
            self.0.lock().unwrap().push(Instant::now());
            Ok(())
        }
    }
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut w = Writer::new(Box::new(Timed(calls.clone())), VID, WIRED).unwrap();
    w.apply(&Profile::example()).unwrap();
    let t = calls.lock().unwrap();
    assert_eq!(t.len(), 4);
    for times in t.windows(2) {
        assert!(times[1].duration_since(times[0]) >= Duration::from_millis(250));
    }
}

fn bridge_plan(product: u16) -> crate::multibrand::Plan {
    crate::multibrand::Plan {
        vendor: VID,
        product,
        settings: vec![gearwise_core::Setting::VendorProfile {
            schema: SCHEMA.into(),
            data: serde_json::to_value(Profile::example()).unwrap(),
        }],
        macros: vec![],
        source: None,
    }
}
fn stored(plan: crate::multibrand::Plan) -> crate::profiles::Profile {
    let x = profile_from_plan(&plan).unwrap();
    let mut p = crate::profiles::Profile::desktop();
    p.id = "x11-test".into();
    p.name = "X11 native test".into();
    p.ghub_id = None;
    p.target_vendor = Some(VID);
    p.target_model = Some(plan.product);
    p.mchose_board = None;
    p.levels = x
        .dpi
        .stages
        .iter()
        .map(|v| crate::profiles::Level { x: *v, y: Some(*v) })
        .collect();
    p.active_level = x.dpi.active as usize;
    p.report_rate_hz = Some(u32::from(x.report_rate));
    p.multi = Some(plan);
    p
}
#[test]
fn complete_bridge_plan_and_summary_import_keep_exact_model_and_schema() {
    for product in [WIRED, WIRELESS] {
        let plan = bridge_plan(product);
        crate::multibrand::validate(&plan).unwrap();
        let report = crate::multibrand::simulate(&plan).unwrap();
        assert_eq!(report["packet_count"], 4);
        assert_eq!(report["hardware_tested"], false);
        assert!(report["software_macros"].as_array().unwrap().is_empty());
        let mut p = stored(plan);
        crate::profile_transfer::validate(&p).unwrap();
        assert!(!p.targets(0x046d, &[product]));
        p.active_level = 0;
        assert!(crate::profile_transfer::validate(&p).is_err());
    }
    let mut plan = bridge_plan(WIRED);
    plan.product = 0xfa61;
    assert!(crate::multibrand::validate(&plan).is_err());
    let mut plan = bridge_plan(WIRED);
    plan.settings.push(gearwise_core::Setting::ReportRate(1000));
    assert!(crate::multibrand::validate(&plan).is_err());
    let mut plan = bridge_plan(WIRED);
    if let gearwise_core::Setting::VendorProfile { schema, .. } = &mut plan.settings[0] {
        *schema = "unknown/1".into();
    }
    assert!(crate::multibrand::validate(&plan).is_err());
    let mut plan = bridge_plan(WIRED);
    plan.vendor = 0x1532;
    plan.product = 0x00ab;
    assert!(crate::multibrand::validate(&plan).is_err());
}
#[test]
fn x11_native_templates_export() {
    let views = crate::multibrand::templates()
        .unwrap()
        .into_iter()
        .filter(|v| supported(v.descriptor.vendor_id, v.descriptor.product_id))
        .collect::<Vec<_>>();
    assert_eq!(views.len(), 2);
    for v in &views {
        assert!(v.descriptor.simulated);
        assert_eq!(v.readback, "write_only_session_cached");
        assert!(v.state.vendor_profile.is_some());
        assert!(!v.hardware_tested);
    }
    if let Some(path) = std::env::var_os("OPEN_HUB_X11_FIXTURE") {
        std::fs::write(path, serde_json::to_vec_pretty(&views).unwrap()).unwrap();
    }
}
#[test]
fn attackshark_import_filters_before_preview_and_never_invents_factory_support() {
    let p = stored(bridge_plan(WIRED));
    let mut other = crate::profiles::Profile::desktop();
    other.id = "foreign".into();
    let path = std::env::temp_dir().join(format!("x11-pack-{}.json", crate::profiles::new_id()));
    std::fs::write(
        &path,
        serde_json::to_vec(
            &serde_json::json!({"format":"open-hub-profiles/1","profiles":[p,other]}),
        )
        .unwrap(),
    )
    .unwrap();
    let pack = crate::profile_transfer::preview("attackshark", Some(path.to_string_lossy().into()))
        .unwrap();
    assert_eq!(pack.profiles.len(), 1);
    assert_eq!(pack.profiles[0].target_model, Some(WIRED));
    std::fs::write(&path, b"{\"format\":\"vendor-private\",\"profiles\":[]}").unwrap();
    assert!(
        crate::profile_transfer::preview("attackshark", Some(path.to_string_lossy().into()))
            .is_err()
    );
    std::fs::remove_file(path).unwrap();
}
#[test]
#[ignore = "requires current X11 browser saved profiles"]
fn browser_x11_profiles_import_and_execute_complete_native_driver() {
    let path = std::env::var_os("OPEN_HUB_X11_UI_PROFILES").unwrap();
    let profiles: Vec<crate::profiles::Profile> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(profiles.len(), 2);
    for p in profiles {
        crate::profile_transfer::validate(&p).unwrap();
        let plan = p.multi.as_ref().unwrap();
        let x = profile_from_plan(plan).unwrap();
        assert_eq!(x.dpi.stages[0], 400);
        assert_eq!(x.preferences.light, Light::BreathingDpi);
        assert_eq!(x.preferences.light_speed, 5);
        assert_eq!(x.preferences.debounce_ms, 12);
        assert_eq!(x.preferences.deep_sleep_seconds, 2940);
        assert_eq!(x.preferences.sleep_seconds, 150);
        assert_eq!(x.macros.len(), 1);
        assert_eq!(x.macros[0].playback, Playback::WhileHeld);
        assert_eq!(x.buttons[&4], Action::Macro(x.macros[0].id.clone()));
        let report = crate::multibrand::simulate(plan).unwrap();
        assert_eq!(report["packet_count"], 7);
        assert_eq!(report["hardware_tested"], false);
        assert!(!p.targets(0x046d, &[plan.product]));
    }
}
