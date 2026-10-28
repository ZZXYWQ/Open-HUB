//! OEM logical fields: Fu mode/level enums, rd RGB objects, Bu/Uu seconds.
//! The same exact driver capabilities validate each selected category.
use super::*;
use gearwise_core::{
    model::{DpiIndicatorMode, DpiIndicatorState, IndicatorLevel, Rgb},
    tuning::MouseTuning,
};
fn boolean(value: &Value, name: &str) -> Result<bool> {
    Ok(integer(value, name, 1)? == 1)
}
pub(super) fn tuning(mouse: &Value, m: &atk::Model) -> Result<Setting> {
    let c = atk::tuning_capabilities(m.vendor, m.product).ok_or("此型号没有已适配性能参数")?;
    let t = MouseTuning {
        debounce_ms: Some(integer(&mouse["keyDebounceTime"], "消抖", 255)? as u8),
        motion_sync: Some(boolean(&mouse["moveSync"], "Motion Sync")?),
        angle_snapping: if c.angle_snapping {
            Some(boolean(&mouse["lineCorrection"], "直线修正")?)
        } else {
            None
        },
        ripple_control: Some(boolean(&mouse["rippleCorrection"], "波纹修正")?),
        led_idle_seconds: Some(integer(&mouse["sleepTime"], "闲置熄灯秒数", 65535)? as u16),
        sensor_sleep_enabled: Some(boolean(&mouse["sensorSleepEnabled"], "传感器休眠开关")?),
        sensor_sleep_seconds: Some(
            integer(&mouse["sensorSleepTime"], "传感器休眠秒数", 65535)? as u16
        ),
        lift_off_microns: if c.lift_off_microns.is_empty() {
            None
        } else {
            Some(
                match integer(&mouse["silentHeight"], "原厂抬升高度编码", 3)? {
                    1 => 1000,
                    2 => 2000,
                    3 => 700,
                    _ => return Err("原厂抬升高度编码无效".into()),
                },
            )
        },
        ..Default::default()
    };
    atk::validate_tuning(m.vendor, m.product, &t).map_err(|e| e.to_string())?;
    Ok(Setting::Tuning(t))
}
pub(super) fn lighting(root: &Value, m: &atk::Model) -> Result<Vec<Setting>> {
    let count = atk::palette_stages(m.vendor, m.product);
    if count == 0 {
        return Err("此型号没有已适配的 DPI 灯效与色表文件转换，未推断其他灯区".into());
    }
    let light = &root["deviceConfig"]["lightConfig"];
    let enabled = boolean(&light["enabled"], "DPI 灯效开关")?;
    let mode = match (integer(&light["mode"], "灯效模式", 255)?, enabled) {
        (0, false) => DpiIndicatorMode::Unspecified,
        (9, true) => DpiIndicatorMode::Static,
        (8, true) => DpiIndicatorMode::Breathing,
        _ => return Err("原厂灯效模式或开关不一致，未替换为默认灯效".into()),
    };
    fn level(value: &Value, name: &str) -> Result<IndicatorLevel> {
        Ok(match integer(value, name, 2)? {
            0 => IndicatorLevel::Low,
            1 => IndicatorLevel::Medium,
            _ => IndicatorLevel::High,
        })
    }
    let indicator = DpiIndicatorState {
        enabled,
        mode,
        brightness: level(&light["brightness"], "灯光亮度枚举")?,
        speed: level(&light["speed"], "灯效速度枚举")?,
    };
    atk::validate_indicator_state(m.vendor, m.product, &indicator).map_err(|e| e.to_string())?;
    let list = root["deviceConfig"]["mouseConfig"]["dpiList"]
        .as_array()
        .filter(|a| a.len() == usize::from(count))
        .ok_or("需要完整的原厂 DPI 色表（含未启用档位）")?;
    let mut settings = vec![];
    for (stage, item) in list.iter().enumerate() {
        let color = &item["color"];
        if !color.is_object() {
            return Err("DPI 颜色必须是原厂 RGB 对象".into());
        }
        settings.push(Setting::DpiColor {
            stage: stage as u8,
            color: Rgb {
                r: integer(&color["r"], "红色", 255)? as u8,
                g: integer(&color["g"], "绿色", 255)? as u8,
                b: integer(&color["b"], "蓝色", 255)? as u8,
            },
        });
    }
    // Complete palette first, then enable the requested indicator mode.
    settings.push(Setting::DpiIndicator(indicator));
    Ok(settings)
}
