//! Sensor calibration levels are native model levels, never inferred millimetres.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SensorCalibration {
    Standard { distance: u8 },
    Asymmetric { lift: u8, landing: u8 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensorCalibrationCaps {
    pub standard_levels: Vec<u8>,
    pub lift_levels: Vec<u8>,
    pub landing_levels: Vec<u8>,
}

impl SensorCalibration {
    pub fn validate(&self, caps: &SensorCalibrationCaps) -> Result<()> {
        let valid = match self {
            Self::Standard { distance } => caps.standard_levels.contains(distance),
            Self::Asymmetric { lift, landing } => {
                landing < lift && caps.lift_levels.contains(lift)
                    && caps.landing_levels.contains(landing)
            }
        };
        if valid { Ok(()) } else {
            Err(Error::invalid("Sensor calibration level/mode is not supported by this exact model"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calibration_is_capability_bounded_and_asymmetric_order_is_strict() {
        let caps = SensorCalibrationCaps {
            standard_levels: vec![1, 2, 3],
            lift_levels: (2..=26).collect(),
            landing_levels: (1..=25).collect(),
        };
        for distance in 0..=255 {
            assert_eq!(SensorCalibration::Standard { distance }.validate(&caps).is_ok(),
                (1..=3).contains(&distance));
        }
        for lift in 0..=27 {
            for landing in 0..=27 {
                assert_eq!(SensorCalibration::Asymmetric { lift, landing }.validate(&caps).is_ok(),
                    (2..=26).contains(&lift) && (1..=25).contains(&landing) && landing < lift);
            }
        }
        assert!(SensorCalibration::Standard { distance: 2 }.validate(&Default::default()).is_err());
    }
    #[test]
    fn calibration_rejects_unrecognised_fields_and_modes() {
        for raw in [r#"{"Standard":{"distance":2,"mm":2}}"#,
            r#"{"Asymmetric":{"lift":13,"landing":12,"raw":[4,0]}}"#,
            r#"{"Raw":[0,4,1]}"#, r#"{"Standard":{"distance":256}}"#] {
            assert!(serde_json::from_str::<SensorCalibration>(raw).is_err());
        }
    }
    #[test]
    fn optional_calibration_preserves_old_device_json_and_isolates_state() {
        let old_caps=serde_json::to_value(crate::Capabilities::default()).unwrap();
        let old_state=serde_json::to_value(crate::DeviceState::default()).unwrap();
        assert!(old_caps.get("calibration").is_none());
        assert!(old_state.get("calibration").is_none());
        assert!(serde_json::from_value::<crate::Capabilities>(old_caps).unwrap().calibration.is_none());
        let mut state:crate::DeviceState=serde_json::from_value(old_state).unwrap();
        state.report_rate=Some(500);
        let value=SensorCalibration::Asymmetric { lift: 13, landing: 12 };
        let setting=crate::Setting::SensorCalibration(value);
        let saved=serde_json::to_value(&setting).unwrap();
        assert_eq!(saved,serde_json::json!({"SensorCalibration":{"Asymmetric":{"lift":13,"landing":12}}}));
        assert_eq!(serde_json::from_value::<crate::Setting>(saved).unwrap(),setting);
        setting.apply_to(&mut state);
        assert_eq!(state.calibration,Some(value));
        assert_eq!(state.report_rate,Some(500));
        assert!(state.buttons.is_empty()&&state.lighting.is_empty());
    }
}
