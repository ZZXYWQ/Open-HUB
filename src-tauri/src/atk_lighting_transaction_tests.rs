//! Production COMPX reports over isolated memory devices; no USB or input.
use super::macro_transaction_tests::instrument;
use super::*;
use gearwise_core::model::{DpiIndicatorMode, DpiIndicatorState, IndicatorLevel};
fn static_setting() -> Setting {
    Setting::Lighting {
        zone: 0,
        state: ZoneState {
            effect: LightingEffect::Static {
                color: Rgb {
                    r: 19,
                    g: 61,
                    b: 97,
                },
            },
            brightness: 100,
        },
    }
}
fn fixture(product: u16) -> Atk {
    let (mut d, _) = memory_device(product).unwrap();
    d.apply(&Setting::Lighting {
        zone: 0,
        state: ZoneState {
            effect: LightingEffect::Off,
            brightness: 100,
        },
    })
    .unwrap();
    d
}
fn specs() -> impl Iterator<Item = &'static Model> {
    models().iter().filter(|m| m.indicator)
}
#[test]
fn lighting_transaction_success_changes_only_active_color_and_mode_and_not_another_device() {
    for spec in specs() {
        let mut d = fixture(spec.product);
        let (mut peer, _) = memory_device(spec.product).unwrap();
        let peer_before = peer.read_blob(0, 768 + 12 * 384).unwrap();
        let before = d.read_blob(0, 768 + 12 * 384).unwrap();
        let active = checked_pair(&before[4..6]).unwrap() as usize;
        let (mut d, count) = instrument(d, vec![], None);
        d.apply(&static_setting()).unwrap();
        assert_eq!(count.lock().unwrap().sets, 2);
        let after = d.read_blob(0, before.len()).unwrap();
        for at in 0..before.len() {
            if !(44 + active * 4..48 + active * 4).contains(&at)
                && !(76..78).contains(&at)
                && !(82..84).contains(&at)
            {
                assert_eq!(after[at], before[at], "{} byte {at}", spec.name);
            }
        }
        assert_eq!(peer.read_blob(0, peer_before.len()).unwrap(), peer_before);
        d.apply(&static_setting()).unwrap();
        assert_eq!(count.lock().unwrap().sets, 2);
        assert_eq!(
            d.read_state().unwrap().lighting[&0].effect,
            LightingEffect::Static {
                color: Rgb {
                    r: 19,
                    g: 61,
                    b: 97
                }
            }
        );
    }
}
#[test]
fn lighting_transaction_every_color_and_mode_failure_restores_both_rows() {
    for spec in specs() {
        for nth in 1..=2 {
            for after_effect in [false, true] {
                let mut d = fixture(spec.product);
                let before = d.read_blob(0, 768 + 12 * 384).unwrap();
                let (mut d, _) = instrument(d, vec![(nth, after_effect)], None);
                let error = d.apply(&static_setting()).unwrap_err().to_string();
                assert!(
                    error.contains("original lighting rows restored and verified"),
                    "{} {nth} {after_effect}: {error}",
                    spec.name
                );
                assert_eq!(d.read_blob(0, before.len()).unwrap(), before);
            }
        }
    }
}
#[test]
fn lighting_transaction_foreign_palette_mode_and_selector_changes_block_all_recovery_writes() {
    for spec in specs() {
        for (address, mut bytes) in [
            (44, vec![0xa5]),
            (48, vec![0xa5]),
            (76, vec![0xa5]),
            (4, vec![1, 84]),
        ] {
            let mut d = fixture(spec.product);
            if address == 4 {
                let active = checked_pair(&d.read(4, 2).unwrap()).unwrap();
                bytes = pair(if active == 0 { 1 } else { 0 }).to_vec();
            }
            assert_ne!(
                d.read(address, bytes.len()).unwrap(),
                bytes,
                "fault must change the initial state"
            );
            let (mut d, count) = instrument(d, vec![], Some((1, address, bytes.clone())));
            let error = d.apply(&static_setting()).unwrap_err().to_string();
            assert!(
                error.contains("lighting state uncertain"),
                "{} {address}: {error}",
                spec.name
            );
            assert_eq!(count.lock().unwrap().sets, 1);
            assert_eq!(d.read(address, bytes.len()).unwrap(), bytes);
        }
    }
}
#[test]
fn lighting_transaction_failed_recovery_is_uncertain_and_not_retried() {
    for spec in specs() {
        let d = fixture(spec.product);
        let (mut d, count) = instrument(d, vec![(2, true), (3, false)], None);
        let error = d.apply(&static_setting()).unwrap_err().to_string();
        assert!(error.contains("lighting state uncertain"));
        assert!(!error.contains("restored and verified"));
        assert_eq!(count.lock().unwrap().sets, 3);
    }
}
#[test]
fn lighting_transaction_palette_and_indicator_single_row_faults_preserve_other_colors() {
    for spec in models().iter().filter(|m| m.dpi_color_stages > 0) {
        let settings = [
            Setting::DpiColor {
                stage: 7,
                color: Rgb {
                    r: 91,
                    g: 92,
                    b: 93,
                },
            },
            Setting::DpiIndicator(DpiIndicatorState {
                enabled: true,
                mode: DpiIndicatorMode::Breathing,
                brightness: IndicatorLevel::High,
                speed: IndicatorLevel::High,
            }),
        ];
        for setting in settings {
            for after_effect in [false, true] {
                let (mut d, _) = memory_device(spec.product).unwrap();
                let before = d.read_blob(0, 768 + 12 * 384).unwrap();
                let (mut d, count) = instrument(d, vec![(1, after_effect)], None);
                let error = d.apply(&setting).unwrap_err().to_string();
                assert!(
                    error.contains("restored and verified"),
                    "{}: {error}",
                    spec.name
                );
                assert_eq!(d.read_blob(0, before.len()).unwrap(), before);
                assert_eq!(count.lock().unwrap().sets, if after_effect { 2 } else { 1 });
            }
        }
    }
}
#[test]
fn lighting_transaction_unknown_active_color_is_retained_without_any_set() {
    for spec in specs() {
        let mut d = fixture(spec.product);
        let active = checked_pair(&d.read(4, 2).unwrap()).unwrap();
        d.write_verified(44 + u16::from(active) * 4, &[1, 2, 3, 4])
            .unwrap();
        let (mut d, count) = instrument(d, vec![], None);
        assert!(d
            .apply(&static_setting())
            .unwrap_err()
            .to_string()
            .contains("unknown row retained"));
        assert_eq!(count.lock().unwrap().sets, 0);
    }
}
