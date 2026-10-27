use super::*;
use std::collections::VecDeque;
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../tests/fixtures/yjx-sc620/configuration.json"
    ))
    .unwrap()
}
fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|v| u8::from_str_radix(v, 16).unwrap())
        .collect()
}
fn response() -> Vec<u8> {
    hex(fixture()["before_response_hex"].as_str().unwrap())
}
fn image() -> ConfigImage {
    ConfigImage::from_response(2, &response()).unwrap()
}
#[test]
fn reconstructed_oem_read_vectors_include_windows_report_id_and_checksum() {
    for v in fixture()["read_headers"].as_array().unwrap() {
        let page = v["page"].as_u64().unwrap() as u8;
        let q = match v["query"].as_str().unwrap() {
            "Identity" => Read::Identity,
            "Bindings" => Read::Bindings { page },
            _ => Read::Config { page },
        };
        let report = q.report().unwrap();
        let expected = hex(v["hex"].as_str().unwrap());
        assert_eq!(&report[..9], expected);
        assert!(report[9..].iter().all(|v| *v == 0));
    }
    for page in 6..=255 {
        assert!(Read::Bindings { page }.report().is_err());
        assert!(Read::Config { page }.report().is_err());
    }
}
#[test]
fn diagnostic_command_set_cannot_generate_destructive_keymap_or_flash_commands() {
    let qs = std::iter::once(Read::Identity)
        .chain((0..6).flat_map(|page| [Read::Bindings { page }, Read::Config { page }]));
    for q in qs {
        let r = q.report().unwrap();
        assert!(matches!(r[2], 3 | 8 | 14));
        assert!(r[9..].iter().all(|v| *v == 0));
    }
}
#[test]
fn config_decodes_actual_offsets_and_keeps_opaque_values() {
    let a = image();
    assert_eq!(a.page(), 2);
    assert_eq!(a.active(), 2);
    assert_eq!(a.dpi(), [800, 1200, 1600, 3200, 5000, 12000]);
    assert_eq!(
        a.colors(),
        [
            [1, 2, 3],
            [16, 32, 48],
            [255, 0, 127],
            [0, 255, 128],
            [18, 52, 86],
            [171, 205, 239]
        ]
    );
    assert_eq!(
        &a.bytes()[35..],
        [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 7, 3, 0xb5, 12, 10, 9]
    );
}
#[test]
fn complete_output_matches_reconstructed_oem_vector_and_preserves_untouched_fields() {
    let f = fixture();
    let p = &f["patch"];
    let changes = ConfigPatch {
        light: None,
        active: Some(p["active"].as_u64().unwrap() as u8),
        dpi: Some(serde_json::from_value(p["dpi"].clone()).unwrap()),
        colors: Some(serde_json::from_value(p["colors"].clone()).unwrap()),
        ..ConfigPatch::default()
    };
    let before = image();
    let after = before.patch(&changes).unwrap();
    assert_eq!(
        after.compile_output().to_vec(),
        hex(f["expected_windows_output_hex"].as_str().unwrap())
    );
    assert_eq!(&before.bytes()[..4], &after.bytes()[..4]);
    assert_eq!(&before.bytes()[35..], &after.bytes()[35..]);
    assert_eq!(before, image());
}
#[test]
fn single_selection_changes_only_selected_bytes_no_rate_sensor_or_lighting_defaults() {
    let a = image();
    for p in [
        ConfigPatch {
            active: Some(5),
            ..Default::default()
        },
        ConfigPatch {
            dpi: Some([50, 100, 800, 10000, 20000, 26000]),
            ..Default::default()
        },
        ConfigPatch {
            colors: Some([[7, 8, 9]; 6]),
            ..Default::default()
        },
    ] {
        let b = a.patch(&p).unwrap();
        for i in 0..46 {
            if !(p.active.is_some() && i == 4
                || p.dpi.is_some() && (5..17).contains(&i)
                || p.colors.is_some() && (17..35).contains(&i))
            {
                assert_eq!(a.bytes()[i], b.bytes()[i], "changed unrelated byte {i}");
            }
        }
    }
}
#[test]
fn malformed_or_different_firmware_images_are_not_replaced_with_examples() {
    let good = response();
    for len in 0..64 {
        assert!(ConfigImage::from_response(2, &good[..len]).is_err());
    }
    let mut long = good.clone();
    long.push(0);
    assert!(ConfigImage::from_response(2, &long).is_err());
    for (at, value) in [
        (0, 0x55),
        (1, 0x0f),
        (2, 0xae),
        (4, 45),
        (4, 57),
        (5, 1),
        (6, 1),
        (7, 3),
        (8, 3),
        (11, 5),
        (12, 0),
        (12, 7),
    ] {
        let mut b = good.clone();
        b[at] = value;
        assert!(
            ConfigImage::from_response(2, &b).is_err(),
            "accepted invalid {at}:{value}"
        );
    }
    for dpi in [0u16, 49, 26001, 65535] {
        let mut b = good.clone();
        b[13..15].copy_from_slice(&dpi.to_le_bytes());
        assert!(ConfigImage::from_response(2, &b).is_err());
    }
}
#[test]
fn patches_reject_invalid_values_before_compilation_without_clamping() {
    let a = image();
    assert!(a.patch(&ConfigPatch::default()).is_err());
    for active in 6..=255 {
        assert!(a
            .patch(&ConfigPatch {
                active: Some(active),
                ..Default::default()
            })
            .is_err());
    }
    for i in 0..6 {
        for bad in [0, 49, 26001, u16::MAX] {
            let mut dpi = a.dpi();
            dpi[i] = bad;
            assert!(a
                .patch(&ConfigPatch {
                    dpi: Some(dpi),
                    ..Default::default()
                })
                .is_err());
        }
    }
    let dpi = [50, 51, 52, 25998, 25999, 26000];
    assert_eq!(
        a.patch(&ConfigPatch {
            dpi: Some(dpi),
            ..Default::default()
        })
        .unwrap()
        .dpi(),
        dpi
    );
}
struct Io {
    before: VecDeque<Vec<u8>>,
    after: VecDeque<Vec<u8>>,
    writes: Vec<Vec<u8>>,
    short_write: bool,
    read_calls: usize,
    stuck: bool,
    overreport: bool,
}
impl Io {
    fn new(reports: Vec<Vec<u8>>) -> Self {
        Self {
            before: VecDeque::new(),
            after: reports.into(),
            writes: vec![],
            short_write: false,
            read_calls: 0,
            stuck: false,
            overreport: false,
        }
    }
}
impl HidTransport for Io {
    fn write(&mut self, data: &[u8]) -> Result<usize> {
        self.writes.push(data.to_vec());
        Ok(if self.short_write {
            data.len() - 1
        } else {
            data.len()
        })
    }
    fn read_timeout(&mut self, out: &mut [u8], _: i32) -> Result<usize> {
        self.read_calls += 1;
        if self.stuck {
            return Ok(64);
        }
        if self.overreport && !self.writes.is_empty() {
            return Ok(out.len() + 1);
        }
        let q = if self.writes.is_empty() {
            &mut self.before
        } else {
            &mut self.after
        };
        if let Some(r) = q.pop_front() {
            out[..r.len()].copy_from_slice(&r);
            Ok(r.len())
        } else {
            Ok(0)
        }
    }
    fn send_feature_report(&mut self, _: &[u8]) -> Result<()> {
        panic!("feature report used for interrupt-only protocol")
    }
    fn get_feature_report(&mut self, _: &mut [u8]) -> Result<usize> {
        panic!("feature report used for interrupt-only protocol")
    }
}
#[test]
fn actual_query_drains_old_responses_ignores_events_and_uses_one_interrupt_write() {
    let mut event = vec![0; 64];
    event[0] = 0xaa;
    event[1] = 0xfa;
    event[2] = 0xae;
    let mut io = Io::new(vec![event, response()]);
    io.before.push_back(response());
    assert_eq!(
        query(&mut io, Read::Config { page: 2 }).unwrap(),
        response()
    );
    assert_eq!(
        io.writes,
        vec![Read::Config { page: 2 }.report().unwrap().to_vec()]
    );
}
#[test]
fn query_refuses_busy_input_and_invalid_page_without_sending_anything() {
    let mut io = Io::new(vec![response()]);
    io.stuck = true;
    assert!(query(&mut io, Read::Config { page: 2 }).is_err());
    assert!(io.writes.is_empty());
    assert_eq!(io.read_calls, 32);
    let mut io = Io::new(vec![]);
    assert!(query(&mut io, Read::Config { page: 6 }).is_err());
    assert_eq!(io.read_calls, 0);
    assert!(io.writes.is_empty());
}
#[test]
fn short_writes_missing_or_corrupt_replies_never_resend_query() {
    let mut io = Io::new(vec![response()]);
    io.short_write = true;
    assert!(query(&mut io, Read::Config { page: 2 }).is_err());
    assert_eq!(io.writes.len(), 1);
    assert_eq!(io.read_calls, 1);
    for bad in [
        vec![],
        response()[..20].to_vec(),
        {
            let mut b = response();
            b[7] = 3;
            b
        },
        {
            let mut b = response();
            b.push(0);
            b
        },
    ] {
        let mut io = Io::new(if bad.is_empty() { vec![] } else { vec![bad] });
        assert!(query(&mut io, Read::Config { page: 2 }).is_err());
        assert_eq!(io.writes.len(), 1);
        assert!(io.read_calls <= 33);
    }
    let mut io = Io::new(vec![]);
    io.overreport = true;
    assert!(query(&mut io, Read::Config { page: 2 }).is_err());
    assert_eq!(io.writes.len(), 1);
}
#[test]
fn transmit_checksum_wraps_and_includes_last_payload_byte_but_not_header_commands() {
    let a = image();
    let before = a.compile_output();
    let mut next = a.clone();
    next.bytes[45] = 250;
    let after = next.compile_output();
    assert_eq!(after[4].wrapping_sub(before[4]), 250u8.wrapping_sub(9));
    assert_eq!(&after[..4], &before[..4]);
}

#[test]
fn seven_oem_lighting_modes_change_only_the_confirmed_byte_and_unknown_modes_are_preserved() {
    let a = image();
    assert_eq!(a.light(), None);
    for (code, mode) in [
        LightMode::Wave,
        LightMode::Neon,
        LightMode::CyclingFlash,
        LightMode::YoyoBall,
        LightMode::SingleFlash,
        LightMode::BreathingLoop,
        LightMode::Off,
    ]
    .into_iter()
    .enumerate()
    {
        let next = a
            .patch(&ConfigPatch {
                light: Some(mode),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(next.light(), Some(mode));
        assert_eq!(next.bytes()[1], code as u8);
        for i in 0..46 {
            if i != 1 {
                assert_eq!(next.bytes()[i], a.bytes()[i]);
            }
        }
        let packet = next.compile_output();
        assert_eq!(packet[10], code as u8);
        assert_eq!(
            packet[4],
            packet[5..].iter().fold(0u8, |s, b| s.wrapping_add(*b))
        );
    }
    assert!(serde_json::from_str::<LightMode>("7").is_err());
    assert!(serde_json::from_str::<LightMode>(r#""MadeUpEffect""#).is_err());
    assert_eq!(a.light(), None);
}
