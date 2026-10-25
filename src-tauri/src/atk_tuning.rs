//! COMPX performance fields independently implemented from ATK HUB 3.2.27.
//! All EEPROM bytes outside the explicit patch are retained. Sensor models are not guessed.
use super::*;
use gearwise_core::tuning::{MouseTuning, TuningCaps};
fn r1_style(p: u16) -> bool {
    matches!(p, 0x1115 | 0x10c9 | 0x1017 | 0xf58f | 0xf58c)
}
pub(super) fn capabilities(m: &Model) -> Option<TuningCaps> {
    if !matches!(
        (m.vendor, m.product),
        (0x373b, 0x1115 | 0x10c9 | 0x1017 | 0x103f | 0x104b)
            | (0x3554, 0xf58f | 0xf58c | 0xf502 | 0xf506)
    ) {
        return None;
    }
    let r1 = r1_style(m.product);
    Some(TuningCaps {
        debounce_ms: if r1 {
            vec![0, 1, 2, 4, 8, 15, 20]
        } else {
            (0..=15).collect()
        },
        motion_sync: true,
        angle_snapping: r1,
        ripple_control: true,
        led_idle_seconds: if r1 {
            vec![30, 60, 120, 180, 300, 1200, 1500, 1800]
        } else {
            vec![30, 60, 120, 300, 600, 1800]
        },
        sensor_sleep_enabled: true,
        sensor_sleep_seconds: if r1 {
            vec![30, 60, 120, 180, 300, 1200, 1500, 1800]
        } else {
            vec![30, 60, 120, 300, 600, 1800]
        },
        lift_off_microns: match m.product {
            0x1017 => vec![700, 1000, 2000],
            0x1115 | 0x10c9 | 0xf58f | 0xf58c => vec![1000, 2000],
            _ => vec![],
        },
    })
}
pub(super) fn validate(m: &Model, p: &MouseTuning) -> Result<()> {
    p.validate(&capabilities(m).ok_or(Error::Unsupported("Exact COMPX mouse performance layout"))?)
}
fn value(row: &[u8], offset: usize) -> Result<u8> {
    checked_pair(
        row.get(offset..offset + 2)
            .ok_or(Error::invalid("COMPX performance row length"))?,
    )
}
fn known_bool(row: &[u8], offset: usize) -> Result<Option<bool>> {
    Ok(match value(row, offset)? {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    })
}
fn set(row: &mut [u8], offset: usize, v: u8) {
    row[offset..offset + 2].copy_from_slice(&pair(v));
}
fn lift_code(m: &Model, microns: u16) -> Result<u8> {
    match (m.product, microns) {
        (0x1017, 700) => Ok(3),
        (0x1017 | 0x1115 | 0x10c9 | 0xf58f | 0xf58c, 1000) => Ok(1),
        (0x1017 | 0x1115 | 0x10c9 | 0xf58f | 0xf58c, 2000) => Ok(2),
        _ => Err(Error::invalid("Exact COMPX lift-off encoding")),
    }
}
impl Atk {
    pub(super) fn read_tuning(&mut self) -> Result<MouseTuning> {
        let caps = capabilities(self.model).ok_or(Error::Unsupported("COMPX performance"))?;
        let mut out = MouseTuning::default();
        if let Ok(a) = self.read(169, 10) {
            if [0, 2, 4, 6, 8].iter().all(|o| value(&a, *o).is_ok()) {
                let debounce = value(&a, 0)?;
                let led = u16::from(value(&a, 4)?) * 10;
                out.debounce_ms = caps.debounce_ms.contains(&debounce).then_some(debounce);
                out.motion_sync = known_bool(&a, 2)?;
                out.angle_snapping = if caps.angle_snapping {
                    known_bool(&a, 6)?
                } else {
                    None
                };
                out.ripple_control = known_bool(&a, 8)?;
                out.led_idle_seconds = caps.led_idle_seconds.contains(&led).then_some(led);
            }
        }
        if let Ok(sensor) = self.read(181, 6) {
            if [0, 2, 4].iter().all(|o| value(&sensor, *o).is_ok()) {
                let sleep = u16::from(value(&sensor, 2)?) * 10;
                out.sensor_sleep_enabled = known_bool(&sensor, 0)?;
                out.sensor_sleep_seconds =
                    caps.sensor_sleep_seconds.contains(&sleep).then_some(sleep);
            }
        }
        if !caps.lift_off_microns.is_empty() {
            if let Ok(row) = self.read(10, 2) {
                if let Ok(code) = checked_pair(&row) {
                    out.lift_off_microns = caps
                        .lift_off_microns
                        .iter()
                        .copied()
                        .find(|v| lift_code(self.model, *v).ok() == Some(code));
                }
            }
        }
        if out.is_empty() {
            return Err(Error::invalid("No readable COMPX performance fields"));
        }
        Ok(out)
    }
    pub(super) fn apply_tuning(&mut self, p: &MouseTuning) -> Result<()> {
        validate(self.model, p)?;
        self.identify()?;
        // Read and validate every affected row before the first write.
        let mut rows = Vec::<(u16, Vec<u8>, Vec<u8>)>::new();
        if p.lift_off_microns.is_some() {
            let before = self.read(10, 2)?;
            value(&before, 0)?;
            rows.push((
                10,
                before.clone(),
                pair(lift_code(self.model, p.lift_off_microns.unwrap())?).to_vec(),
            ));
        }
        if p.debounce_ms.is_some()
            || p.motion_sync.is_some()
            || p.angle_snapping.is_some()
            || p.ripple_control.is_some()
            || p.led_idle_seconds.is_some()
        {
            let before = self.read(169, 10)?;
            for o in [0, 2, 4, 6, 8] {
                value(&before, o)?;
            }
            let mut next = before.clone();
            if let Some(v) = p.debounce_ms {
                set(&mut next, 0, v);
            }
            if let Some(v) = p.motion_sync {
                set(&mut next, 2, u8::from(v));
            }
            if let Some(v) = p.led_idle_seconds {
                set(&mut next, 4, (v / 10) as u8);
            }
            if let Some(v) = p.angle_snapping {
                set(&mut next, 6, u8::from(v));
            }
            if let Some(v) = p.ripple_control {
                set(&mut next, 8, u8::from(v));
            }
            rows.push((169, before, next));
        }
        if p.sensor_sleep_enabled.is_some() || p.sensor_sleep_seconds.is_some() {
            let before = self.read(181, 6)?;
            for o in [0, 2, 4] {
                value(&before, o)?;
            }
            let mut next = before.clone();
            if let Some(v) = p.sensor_sleep_enabled {
                set(&mut next, 0, u8::from(v));
            }
            if let Some(v) = p.sensor_sleep_seconds {
                set(&mut next, 2, (v / 10) as u8);
            }
            rows.push((181, before, next));
        }
        let mut attempted = Vec::new();
        let operation = (|| -> Result<()> {
            for (i, (address, _, next)) in rows.iter().enumerate() {
                attempted.push(i);
                self.write_verified(*address, next)?;
            }
            let actual = self.read_tuning()?;
            let mut wanted = actual.clone();
            p.merge(&mut wanted);
            if actual != wanted {
                return Err(Error::invalid(
                    "COMPX complete performance readback mismatch",
                ));
            }
            Ok(())
        })();
        if let Err(error) = operation {
            let mut restore_errors = Vec::new();
            for i in attempted.into_iter().rev() {
                let (address, before, _) = &rows[i];
                if let Err(e) = self.write_verified(*address, before) {
                    restore_errors.push(e.to_string());
                }
            }
            return Err(Error::invalid(format!(
                "{error}; performance original rows restoration: {}",
                if restore_errors.is_empty() {
                    "confirmed".into()
                } else {
                    restore_errors.join("; ")
                }
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gearwise_core::transport::MockRecord;
    fn patch(m: &Model) -> MouseTuning {
        let c = capabilities(m).unwrap();
        MouseTuning {
            debounce_ms: c.debounce_ms.last().copied(),
            motion_sync: Some(false),
            angle_snapping: c.angle_snapping.then_some(true),
            ripple_control: Some(true),
            led_idle_seconds: Some(120),
            sensor_sleep_enabled: Some(false),
            sensor_sleep_seconds: Some(300),
            lift_off_microns: c.lift_off_microns.last().copied(),
        }
    }
    fn writes(log: &Arc<Mutex<Vec<MockRecord>>>) -> Vec<Vec<u8>> {
        log.lock()
            .unwrap()
            .iter()
            .filter_map(|r| {
                if let MockRecord::Write(p) = r {
                    (p[1] == 7).then(|| p.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    #[test]
    fn exact_compx_models_performance_roundtrip_and_no_other_regions_change() {
        let mut count = 0;
        for m in models().iter().filter(|m| capabilities(m).is_some()) {
            let (mut d, l) = memory_device(m.product).unwrap();
            let before = d.read_blob(0, 169).unwrap();
            let wanted = patch(m);
            let original_mode = d.read(185, 2).unwrap();
            l.lock().unwrap().clear();
            d.apply(&Setting::Tuning(wanted.clone())).unwrap();
            let actual = d.read_state().unwrap().tuning.unwrap();
            let mut expected = actual.clone();
            wanted.merge(&mut expected);
            assert_eq!(actual, expected);
            assert_eq!(d.read(185, 2).unwrap(), original_mode);
            let after = d.read_blob(0, 169).unwrap();
            assert_eq!(before[..10], after[..10]);
            assert_eq!(before[12..], after[12..]);
            assert!(writes(&l)
                .iter()
                .all(|p| matches!(u16::from_be_bytes([p[3], p[4]]), 10 | 169 | 181)));
            count += 1;
        }
        assert_eq!(count, 9);
    }
    #[test]
    fn published_a9_row_keeps_unmodified_pairs_in_exact_patch_packet() {
        let (mut d, l) = memory_device(0x1115).unwrap();
        let captured = [2, 83, 0, 85, 12, 73, 0, 85, 0, 85];
        d.io.write(&frame(7, 169, 10, &captured).unwrap()).unwrap();
        l.lock().unwrap().clear();
        d.apply(&Setting::Tuning(MouseTuning {
            debounce_ms: Some(4),
            motion_sync: Some(true),
            ..Default::default()
        }))
        .unwrap();
        assert_eq!(
            writes(&l),
            vec![vec![
                8, 7, 0, 0, 169, 10, 4, 81, 1, 84, 12, 73, 0, 85, 0, 85, 234
            ]]
        );
        let actual = d.read_tuning().unwrap();
        assert_eq!(actual.led_idle_seconds, Some(120));
        assert_eq!(actual.ripple_control, Some(false));
    }
    #[test]
    fn invalid_values_and_other_protocols_are_refused_before_io() {
        let (mut d, l) = memory_device(0x1115).unwrap();
        for p in [
            MouseTuning::default(),
            MouseTuning {
                debounce_ms: Some(3),
                ..Default::default()
            },
            MouseTuning {
                led_idle_seconds: Some(37),
                ..Default::default()
            },
            MouseTuning {
                sensor_sleep_seconds: Some(0),
                ..Default::default()
            },
            MouseTuning {
                lift_off_microns: Some(700),
                ..Default::default()
            },
        ] {
            l.lock().unwrap().clear();
            assert!(d.apply(&Setting::Tuning(p)).is_err());
            assert!(l.lock().unwrap().is_empty());
        }
        let (mut pulse, l) = memory_device(0xf507).unwrap();
        l.lock().unwrap().clear();
        assert!(pulse
            .apply(&Setting::Tuning(MouseTuning {
                motion_sync: Some(true),
                ..Default::default()
            }))
            .is_err());
        assert!(l.lock().unwrap().is_empty());
    }
    #[test]
    fn all_touched_rows_are_validated_before_first_mutation() {
        let (mut d, l) = memory_device(0x1115).unwrap();
        let mut sensor = d.read(181, 6).unwrap();
        sensor[5] ^= 1;
        d.io.write(&frame(7, 181, 6, &sensor).unwrap()).unwrap();
        l.lock().unwrap().clear();
        assert!(d.apply(&Setting::Tuning(patch(d.model))).is_err());
        assert!(writes(&l).is_empty());
        let readable = d.read_tuning().unwrap();
        assert_eq!(readable.debounce_ms, Some(8));
        assert_eq!(readable.sensor_sleep_enabled, None);
    }
    struct Short {
        inner: Box<dyn HidTransport>,
        at: usize,
        count: usize,
    }
    impl HidTransport for Short {
        fn write(&mut self, p: &[u8]) -> Result<usize> {
            let n = self.inner.write(p)?;
            if p.len() == 17 && p[1] == 7 {
                self.count += 1;
                if self.count == self.at {
                    return Ok(16);
                }
            }
            Ok(n)
        }
        fn read_timeout(&mut self, b: &mut [u8], t: i32) -> Result<usize> {
            self.inner.read_timeout(b, t)
        }
        fn send_feature_report(&mut self, p: &[u8]) -> Result<()> {
            self.inner.send_feature_report(p)
        }
        fn get_feature_report(&mut self, b: &mut [u8]) -> Result<usize> {
            self.inner.get_feature_report(b)
        }
    }
    #[test]
    fn uncertain_writes_restore_every_original_performance_row() {
        for at in 1..=3 {
            let (mut d, _) = memory_device(0x1115).unwrap();
            let before = [
                (10, d.read(10, 2).unwrap()),
                (169, d.read(169, 10).unwrap()),
                (181, d.read(181, 6).unwrap()),
            ];
            let p = patch(d.model);
            d.io = Box::new(Short {
                inner: d.io,
                at,
                count: 0,
            });
            assert!(d.apply(&Setting::Tuning(p)).is_err());
            for (address, raw) in before {
                assert_eq!(d.read(address, raw.len()).unwrap(), raw);
            }
        }
    }
    #[test]
    fn unknown_neighbor_sensor_mode_and_angle_flags_are_not_zeroed() {
        let (mut d, _) = memory_device(0x103f).unwrap();
        let sensor = [1, 84, 6, 79, 211, 130];
        d.io.write(&frame(7, 181, 6, &sensor).unwrap()).unwrap();
        let mut advanced = d.read(169, 10).unwrap();
        advanced[6..8].copy_from_slice(&pair(211));
        d.io.write(&frame(7, 169, 10, &advanced).unwrap()).unwrap();
        let p = MouseTuning {
            motion_sync: Some(false),
            sensor_sleep_seconds: Some(300),
            ..Default::default()
        };
        d.apply(&Setting::Tuning(p)).unwrap();
        assert_eq!(d.read(185, 2).unwrap(), vec![211, 130]);
        assert_eq!(d.read(175, 2).unwrap(), vec![211, 130]);
        assert_eq!(d.read_tuning().unwrap().angle_snapping, None);
    }
    #[test]
    fn same_lift_height_has_model_specific_wire_code() {
        for (product, um, code) in [
            (0x1115, 1000, 1),
            (0x1115, 2000, 2),
            (0x1017, 700, 3),
            (0x1017, 1000, 1),
            (0x1017, 2000, 2),
        ] {
            let (mut d, _) = memory_device(product).unwrap();
            d.apply(&Setting::Tuning(MouseTuning {
                lift_off_microns: Some(um),
                ..Default::default()
            }))
            .unwrap();
            assert_eq!(d.read(10, 2).unwrap(), pair(code));
            assert_eq!(d.read_tuning().unwrap().lift_off_microns, Some(um));
        }
    }
}
