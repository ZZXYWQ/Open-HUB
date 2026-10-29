//! Independently reconstructed source facts + stateful memory transport, no USB.
use super::*;
use gearwise_core::{
    model::{DpiIndicatorMode, DpiIndicatorState, IndicatorLevel},
    transport::MockRecord,
};
fn value() -> DpiIndicatorState {
    DpiIndicatorState {
        enabled: true,
        mode: DpiIndicatorMode::Breathing,
        brightness: IndicatorLevel::High,
        speed: IndicatorLevel::High,
    }
}
fn sets(log: &Arc<Mutex<Vec<MockRecord>>>) -> Vec<Vec<u8>> {
    log.lock()
        .unwrap()
        .iter()
        .filter_map(|r| match r {
            MockRecord::Write(p) if p[1] == 7 => Some(p.clone()),
            _ => None,
        })
        .collect()
}
#[test]
fn five_exact_models_all_36_source_combinations_roundtrip_and_preserve_every_other_byte() {
    for m in models().iter().filter(|m| m.dpi_color_stages > 0) {
        let (mut d, log) = memory_device(m.product).unwrap();
        let (mut other, _) = memory_device(m.product).unwrap();
        let peer = other.read_state().unwrap();
        let before = d.read_blob(0, 768 + 12 * 384).unwrap();
        let initial = d.read_state().unwrap();
        assert!(d.capabilities().dpi_indicator);
        for mode in [DpiIndicatorMode::Static, DpiIndicatorMode::Breathing] {
            for brightness in [
                IndicatorLevel::Low,
                IndicatorLevel::Medium,
                IndicatorLevel::High,
            ] {
                for speed in [
                    IndicatorLevel::Low,
                    IndicatorLevel::Medium,
                    IndicatorLevel::High,
                ] {
                    for enabled in [false, true] {
                        let desired = DpiIndicatorState {
                            mode,
                            brightness,
                            speed,
                            enabled,
                        };
                        log.lock().unwrap().clear();
                        d.apply(&Setting::DpiIndicator(desired)).unwrap();
                        assert_eq!(d.read_state().unwrap().dpi_indicator, Some(desired));
                        assert!(sets(&log).iter().all(|p| p[3..6] == [0, 76, 8]));
                    }
                }
            }
        }
        let after = d.read_blob(0, 768 + 12 * 384).unwrap();
        assert_eq!(&after[..76], &before[..76]);
        assert_eq!(&after[84..], &before[84..]);
        let state = d.read_state().unwrap();
        assert_eq!(state.dpi, initial.dpi);
        assert_eq!(state.buttons, initial.buttons);
        assert_eq!(state.dpi_colors, initial.dpi_colors);
        assert_eq!(other.read_state().unwrap(), peer);
        let info = HidInterfaceInfo {
            vendor_id: m.vendor,
            product_id: m.product,
            usage_page: 0xff02,
            usage: 2,
            ..Default::default()
        };
        let mut reopened = Atk::open(d.io, &info).unwrap();
        assert_eq!(
            reopened.read_state().unwrap().dpi_indicator,
            state.dpi_indicator
        );
    }
}
#[test]
fn full_row_source_golden_and_noop_no_set() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    log.lock().unwrap().clear();
    d.apply(&Setting::DpiIndicator(value())).unwrap();
    assert_eq!(
        sets(&log),
        vec![vec![
            8, 7, 0, 0, 76, 8, 2, 83, 255, 86, 5, 80, 1, 84, 0, 0, 158
        ]]
    );
    log.lock().unwrap().clear();
    d.apply(&Setting::DpiIndicator(value())).unwrap();
    assert!(sets(&log).is_empty());
}
#[test]
fn invalid_checksums_unknown_mode_levels_and_enabled_are_never_normalized() {
    for (offset, bad, crc_bad) in [
        (0, 9, false),
        (2, 77, false),
        (4, 2, false),
        (6, 2, false),
        (2, 128, true),
    ] {
        let (mut d, log) = memory_device(0x1017).unwrap();
        let mut row = d.read(76, 8).unwrap();
        row[offset..offset + 2].copy_from_slice(&pair(bad));
        if crc_bad {
            row[offset + 1] ^= 1;
        }
        d.io.write(&frame(7, 76, 8, &row).unwrap()).unwrap();
        let state = d.read_state().unwrap();
        assert!(state.dpi_indicator.is_none());
        log.lock().unwrap().clear();
        assert!(d.apply(&Setting::DpiIndicator(value())).is_err());
        assert!(sets(&log).is_empty());
        assert_eq!(d.read(76, 8).unwrap(), row);
    }
}
#[test]
fn unsupported_indicator_models_reject_before_io() {
    for product in [0x1115, 0x10c9, 0x103f, 0x104b, 0xf507, 0xf508] {
        let (mut d, log) = memory_device(product).unwrap();
        assert!(!d.caps.dpi_indicator);
        log.lock().unwrap().clear();
        assert!(d.apply(&Setting::DpiIndicator(value())).is_err());
        assert!(log.lock().unwrap().is_empty());
    }
}
struct Fault {
    inner: Box<dyn HidTransport>,
    identity: bool,
    short: bool,
    wrote: bool,
    used: bool,
}
impl HidTransport for Fault {
    fn write(&mut self, p: &[u8]) -> Result<usize> {
        let n = self.inner.write(p)?;
        if p.len() == 17 && p[1] == 7 {
            self.wrote = true;
            if self.short && !self.used {
                self.used = true;
                return Ok(16);
            }
        }
        Ok(n)
    }
    fn read_timeout(&mut self, p: &mut [u8], ms: i32) -> Result<usize> {
        let n = self.inner.read_timeout(p, ms)?;
        if n == 17 {
            if self.identity && p[1] == 0x10 {
                p[7] = 255;
                p[16] = 0x55u8.wrapping_sub(sum(&p[..16]));
            } else if !self.identity
                && !self.short
                && self.wrote
                && !self.used
                && p[1] == 8
                && p[4] == 76
            {
                self.used = true;
                p[6] ^= 1;
                p[16] = 0x55u8.wrapping_sub(sum(&p[..16]));
            }
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
fn identity_change_stops_indicator_before_set() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    d.io = Box::new(Fault {
        inner: d.io,
        identity: true,
        short: false,
        wrote: false,
        used: false,
    });
    log.lock().unwrap().clear();
    assert!(d.apply(&Setting::DpiIndicator(value())).is_err());
    assert!(sets(&log).is_empty());
}
#[test]
fn damaged_readback_restores_only_original_eight_bytes() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    let before = d.read(76, 8).unwrap();
    d.io = Box::new(Fault {
        inner: d.io,
        identity: false,
        short: false,
        wrote: false,
        used: false,
    });
    log.lock().unwrap().clear();
    assert!(d.apply(&Setting::DpiIndicator(value())).is_err());
    assert_eq!(d.read(76, 8).unwrap(), before);
    let writes = sets(&log);
    assert_eq!(writes.len(), 2);
    assert_eq!(&writes[1][6..14], before.as_slice());
}
#[test]
fn short_write_restores_original_only_after_complete_lighting_state_check() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    let before = d.read_blob(0, 768 + 12 * 384).unwrap();
    d.io = Box::new(Fault {
        inner: d.io,
        identity: false,
        short: true,
        wrote: false,
        used: false,
    });
    log.lock().unwrap().clear();
    let error = d.apply(&Setting::DpiIndicator(value())).unwrap_err().to_string();
    assert!(error.contains("original lighting rows restored and verified"));
    let writes = sets(&log);
    assert_eq!(writes.len(), 2);
    assert_eq!(&writes[1][6..14], &before[76..84]);
    assert_eq!(d.read_blob(0, before.len()).unwrap(), before);
}
#[test]
fn legacy_lighting_off_static_keeps_original_levels() {
    let (mut d, _) = memory_device(0x1017).unwrap();
    d.apply(&Setting::DpiIndicator(value())).unwrap();
    for effect in [
        LightingEffect::Off,
        LightingEffect::Static {
            color: Rgb {
                r: 17,
                g: 34,
                b: 51,
            },
        },
    ] {
        d.apply(&Setting::Lighting {
            zone: 0,
            state: ZoneState {
                effect,
                brightness: 100,
            },
        })
        .unwrap();
        let state = d.read_state().unwrap().dpi_indicator.unwrap();
        assert_eq!(state.brightness, IndicatorLevel::High);
        assert_eq!(state.speed, IndicatorLevel::High);
        assert_eq!(state.enabled, effect != LightingEffect::Off);
    }
}

#[test]
fn oem_off_with_zero_mode_is_retained_without_guessing_or_unsafe_enable() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    let row = [0, 0, 128, 213, 3, 82, 0, 85];
    d.io.write(&frame(7, 76, 8, &row).unwrap()).unwrap();
    let mut original = d.read_state().unwrap().dpi_indicator.unwrap();
    assert!(!original.enabled);
    assert_eq!(original.mode, DpiIndicatorMode::Unspecified);
    log.lock().unwrap().clear();
    d.apply(&Setting::DpiIndicator(original)).unwrap();
    assert!(sets(&log).is_empty());
    original.enabled = true;
    log.lock().unwrap().clear();
    assert!(d.apply(&Setting::DpiIndicator(original)).is_err());
    assert!(log.lock().unwrap().is_empty());
    original.enabled = false;
    original.brightness = IndicatorLevel::High;
    d.apply(&Setting::DpiIndicator(original)).unwrap();
    assert_eq!(d.read(76, 8).unwrap()[0], 0);
    assert_eq!(d.read_state().unwrap().dpi_indicator, Some(original));
    original.mode = DpiIndicatorMode::Breathing;
    original.enabled = true;
    d.apply(&Setting::DpiIndicator(original)).unwrap();
    assert_eq!(d.read_state().unwrap().dpi_indicator, Some(original));
}

#[test]
fn source_off_mode_zero_requires_disabled_flag_and_exact_zero_reserved_mode_bytes() {
    let row = [0, 0, 128, 213, 3, 82, 0, 85];
    let v = indicator::decode(&row).unwrap();
    assert_eq!(indicator::encode(&v), row);
    let mut enabled = row;
    enabled[6..8].copy_from_slice(&pair(1));
    assert!(indicator::decode(&enabled).is_err());
    let mut unknown = row;
    unknown[..2].copy_from_slice(&pair(0));
    assert!(indicator::decode(&unknown).is_err());
}
