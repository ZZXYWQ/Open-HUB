//! Model-specific, optional mouse performance controls. No vendor defaults are guessed.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MouseTuning {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debounce_ms: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion_sync: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle_snapping: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ripple_control: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub led_idle_seconds: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensor_sleep_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensor_sleep_seconds: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lift_off_microns: Option<u16>,
    /// Whole-device wireless idle timeout, distinct from sensor-only sleep.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_sleep_seconds: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub low_power_percent: Option<u8>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TuningCaps {
    pub debounce_ms: Vec<u8>,
    pub motion_sync: bool,
    pub angle_snapping: bool,
    pub ripple_control: bool,
    pub led_idle_seconds: Vec<u16>,
    pub sensor_sleep_enabled: bool,
    pub sensor_sleep_seconds: Vec<u16>,
    pub lift_off_microns: Vec<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub device_sleep_seconds: Vec<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub low_power_percent: Vec<u8>,
}
impl MouseTuning {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
    pub fn validate(&self, c: &TuningCaps) -> Result<()> {
        if self.is_empty() {
            return Err(Error::invalid("Empty mouse performance patch"));
        }
        if self
            .debounce_ms
            .is_some_and(|v| !c.debounce_ms.contains(&v))
            || self
                .lift_off_microns
                .is_some_and(|v| !c.lift_off_microns.contains(&v))
            || self
                .led_idle_seconds
                .is_some_and(|v| !c.led_idle_seconds.contains(&v))
            || self
                .sensor_sleep_seconds
                .is_some_and(|v| !c.sensor_sleep_seconds.contains(&v))
            || self.device_sleep_seconds.is_some_and(|v|!c.device_sleep_seconds.contains(&v))
            || self.low_power_percent.is_some_and(|v|!c.low_power_percent.contains(&v))
            || self.motion_sync.is_some() && !c.motion_sync
            || self.angle_snapping.is_some() && !c.angle_snapping
            || self.ripple_control.is_some() && !c.ripple_control
            || self.sensor_sleep_enabled.is_some() && !c.sensor_sleep_enabled
        {
            return Err(Error::invalid(
                "Mouse performance value/feature is not supported by this exact model",
            ));
        }
        Ok(())
    }
    /// Refresh selected fields without expanding a previously saved patch.
    pub fn refresh_selected(&mut self,live:&Self){
        macro_rules! fields{($($k:ident),*)=>{$(if self.$k.is_some()&&live.$k.is_some(){self.$k=live.$k;})*}}fields!(debounce_ms,motion_sync,angle_snapping,ripple_control,led_idle_seconds,sensor_sleep_enabled,sensor_sleep_seconds,lift_off_microns,device_sleep_seconds,low_power_percent);
    }
    pub fn merge(&self, old: &mut Self) {
        macro_rules! fields{($($k:ident),*)=>{$(if self.$k.is_some(){old.$k=self.$k;})*}}
        fields!(
            debounce_ms,
            motion_sync,
            angle_snapping,
            ripple_control,
            led_idle_seconds,
            sensor_sleep_enabled,
            sensor_sleep_seconds,
            lift_off_microns,
            device_sleep_seconds,
            low_power_percent
        );
    }
}
