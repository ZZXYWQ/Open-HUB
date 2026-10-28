//! Stateful protocol simulation, not USB captures or physical-device acceptance.
use super::*;
use gearwise_core::transport::MockRecord;

fn rgb(stage: u8) -> Rgb {
    Rgb {
        r: 17 + stage,
        g: 34 + stage,
        b: 51 + stage,
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
fn all_five_models_read_and_restore_eight_colors_without_touching_other_state() {
    let eligible = models()
        .iter()
        .filter(|m| m.dpi_color_stages > 0)
        .collect::<Vec<_>>();
    assert_eq!(eligible.len(), 6);
    for m in eligible {
        let (mut d, log) = memory_device(m.product).unwrap();
        let (mut other, _) = memory_device(m.product).unwrap();
        let peer = other.read_state().unwrap();
        let mac = gearwise_core::profile::Macro {
            id: "retained".into(),
            name: "Existing macro".into(),
            mode: gearwise_core::profile::MacroMode::Once,
            steps: vec![
                gearwise_core::profile::MacroStep::KeyDown(7),
                gearwise_core::profile::MacroStep::Delay(77),
                gearwise_core::profile::MacroStep::KeyUp(7),
            ],
        };
        d.set_macro_definitions(&[mac.clone()]).unwrap();
        d.apply(&Setting::Button {
            id: 5,
            action: ButtonAction::Macro(mac.id),
        })
        .unwrap();
        let before = d.read_blob(0, 768 + 12 * 384).unwrap();
        let initial = d.read_state().unwrap();
        assert_eq!(initial.dpi_colors.len(), 8);
        assert_eq!(d.caps.dpi_color_stages, 8);
        log.lock().unwrap().clear();
        for stage in 0..8 {
            d.apply(&Setting::DpiColor {
                stage,
                color: rgb(stage),
            })
            .unwrap();
        }
        let after = d.read_blob(0, 768 + 12 * 384).unwrap();
        assert_eq!(&before[..44], &after[..44]);
        assert_eq!(&before[76..], &after[76..]);
        assert_eq!(sets(&log).len(), 8);
        let state = d.read_state().unwrap();
        for stage in 0..8 {
            assert_eq!(state.dpi_colors[&stage], rgb(stage));
        }
        assert_eq!(state.dpi, initial.dpi);
        assert_eq!(state.report_rate, initial.report_rate);
        assert_eq!(state.buttons, initial.buttons);
        assert_eq!(state.tuning, initial.tuning);
        assert_eq!(other.read_state().unwrap(), peer);
        let info = HidInterfaceInfo {
            vendor_id: m.vendor,
            product_id: m.product,
            usage_page: 0xff02,
            usage: 2,
            ..Default::default()
        };
        let mut reopened = Atk::open(d.io, &info).unwrap();
        assert_eq!(reopened.read_state().unwrap().dpi_colors, state.dpi_colors);
    }
}
#[test]
fn reconstructed_source_golden_reports_match_and_noop_skips_set() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    log.lock().unwrap().clear();
    let s = Setting::DpiColor {
        stage: 0,
        color: rgb(0),
    };
    d.apply(&s).unwrap();
    assert_eq!(
        sets(&log),
        vec![vec![
            8, 7, 0, 0, 44, 4, 17, 34, 51, 239, 0, 0, 0, 0, 0, 0, 193
        ]]
    );
    let get = vec![8, 8, 0, 0, 44, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 21];
    assert!(log
        .lock()
        .unwrap()
        .iter()
        .any(|r| matches!(r,MockRecord::Write(p)if *p==get)));
    log.lock().unwrap().clear();
    d.apply(&s).unwrap();
    assert!(sets(&log).is_empty());
}
#[test]
fn inactive_stage_crc_failure_is_unknown_and_rejected_without_set() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    let bad = [1, 2, 3, 4];
    d.io.write(&frame(7, 72, 4, &bad).unwrap()).unwrap();
    let state = d.read_state().unwrap();
    assert_eq!(state.dpi_colors.len(), 7);
    assert!(!state.dpi_colors.contains_key(&7));
    log.lock().unwrap().clear();
    assert!(d
        .apply(&Setting::DpiColor {
            stage: 7,
            color: rgb(7)
        })
        .is_err());
    assert!(sets(&log).is_empty());
    assert_eq!(d.read(72, 4).unwrap(), bad);
}
#[test]
fn exact_model_and_stage_limits_reject_before_io() {
    for product in [0x1017, 0x1115, 0x10c9, 0x103f, 0x104b, 0xf507, 0xf508] {
        let (mut d, log) = memory_device(product).unwrap();
        for stage in if product == 0x1017 {
            vec![8, 255]
        } else {
            vec![0, 7]
        } {
            log.lock().unwrap().clear();
            assert!(d
                .apply(&Setting::DpiColor {
                    stage,
                    color: rgb(0)
                })
                .is_err());
            assert!(log.lock().unwrap().is_empty());
        }
    }
}
#[test]
fn color_table_edit_preserves_off_and_unknown_global_modes() {
    for (mode, enabled) in [(1, 0), (2, 1), (9, 1)] {
        let (mut d, _) = memory_device(0x1017).unwrap();
        let mut row = d.read(76, 8).unwrap();
        row[..2].copy_from_slice(&pair(mode));
        row[6..8].copy_from_slice(&pair(enabled));
        d.io.write(&frame(7, 76, 8, &row).unwrap()).unwrap();
        d.apply(&Setting::DpiColor {
            stage: 7,
            color: rgb(7),
        })
        .unwrap();
        assert_eq!(d.read(76, 8).unwrap(), row);
        let state = d.read_state().unwrap();
        assert_eq!(state.dpi_colors[&7], rgb(7));
        if enabled == 0 {
            assert_eq!(state.lighting[&0].effect, LightingEffect::Off);
        } else {
            assert!(state.lighting.is_empty());
        }
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
                && p[4] == 44
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
fn changed_model_blocks_palette_writes() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    d.io = Box::new(Fault {
        inner: d.io,
        identity: true,
        short: false,
        wrote: false,
        used: false,
    });
    log.lock().unwrap().clear();
    assert!(d
        .apply(&Setting::DpiColor {
            stage: 0,
            color: rgb(0)
        })
        .is_err());
    assert!(sets(&log).is_empty());
}
#[test]
fn bad_readback_attempts_only_original_row_restore() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    let original = d.read(44, 4).unwrap();
    d.io = Box::new(Fault {
        inner: d.io,
        identity: false,
        short: false,
        wrote: false,
        used: false,
    });
    log.lock().unwrap().clear();
    assert!(d
        .apply(&Setting::DpiColor {
            stage: 0,
            color: rgb(0)
        })
        .is_err());
    let writes = sets(&log);
    assert_eq!(writes.len(), 2);
    assert_eq!(&writes[1][6..10], &original);
    assert_eq!(d.read(44, 4).unwrap(), original);
}
#[test]
fn uncertain_short_write_is_error_without_automatic_retry() {
    let (mut d, log) = memory_device(0x1017).unwrap();
    d.io = Box::new(Fault {
        inner: d.io,
        identity: false,
        short: true,
        wrote: false,
        used: false,
    });
    log.lock().unwrap().clear();
    assert!(d
        .apply(&Setting::DpiColor {
            stage: 0,
            color: rgb(0)
        })
        .is_err());
    assert_eq!(sets(&log).len(), 1);
}
