//! Bind live MCHOSE UI requests to a manager transport generation, not a path.
//! This is connection ownership, not proof of a receiver's paired mouse serial.
use crate::{
    device::Device,
    hidpp::{Error, Result},
    mchose,
    mchose_config::HardwareState,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub device_key: String,
    pub connection_key: String,
}

fn changed() -> Error {
    Error::Other("迈从设备连接已改变，请返回设备列表重新识别并读取配置".into())
}

pub fn read_context(device: &Device, expected_connection: &str) -> Result<Target> {
    if device.vendor_id != mchose::VID
        || device.product_id != mchose::RECEIVER_PID
        || !device.model_ids.contains(&mchose::MODEL_PID)
        || device.profile_key.is_empty()
        || expected_connection.is_empty()
        || device.connection_key != expected_connection
    {
        return Err(changed());
    }
    Ok(Target {
        device_key: device.profile_key.clone(),
        connection_key: device.connection_key.clone(),
    })
}

impl Target {
    pub fn verify(&self, device: &Device) -> Result<()> {
        let current = read_context(device, &self.connection_key)?;
        if current != *self {
            return Err(changed());
        }
        Ok(())
    }
}

pub fn with_target<R>(
    device: &Device,
    transport: &mchose::Transport,
    target: &Target,
    f: impl FnOnce(&mchose::Transport) -> Result<R>,
) -> Result<R> {
    target.verify(device)?;
    f(transport)
}

#[derive(Serialize)]
pub struct View {
    #[serde(flatten)]
    pub state: HardwareState,
    pub target: Target,
}

pub fn read_hardware(
    device: &Device,
    transport: &mchose::Transport,
    expected_connection: &str,
) -> Result<View> {
    let target = read_context(device, expected_connection)?;
    Ok(View {
        state: transport.hardware_state()?,
        target,
    })
}

/// All native profile shapes share this guard before the dispatcher does I/O.
/// Generic DPI and legacy board profiles must not bypass the captured connection.
pub fn check_profile_connection(device: &Device, expected: Option<&str>) -> Result<()> {
    if device.vendor_id == mchose::VID {
        read_context(device, expected.unwrap_or(""))?;
    }
    Ok(())
}

// Direct generic IPC has no read context. Saved profiles are checked separately
// by check_profile_connection in the native activation dispatcher.
pub fn reject_unbound_write(device: &Device) -> Result<()> {
    if device.vendor_id == mchose::VID {
        return Err(changed());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn simulated_device(id: &str) -> Device {
    Device {
        vendor_id: mchose::VID,
        id: id.into(),
        profile_key: format!("session:{id}"),
        connection_key: format!("connection:{id}:1"),
        path: std::ffi::CString::new("synthetic-path").unwrap(),
        index: crate::hidpp::DIRECT,
        product_id: mchose::RECEIVER_PID,
        product: "MCHOSE A7 V3 Ultra+".into(),
        name: "MCHOSE A7 V3 Ultra+".into(),
        firmware_name: "synthetic".into(),
        model_ids: vec![mchose::MODEL_PID],
        kind: "mouse",
        protocol: "synthetic MCHOSE".into(),
        features: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn reused_path_owner_or_connection_never_retargets_pending_requests() {
        let old = simulated_device("mchose-A");
        let target = read_context(&old, &old.connection_key).unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = requests.clone();
        let directory = std::env::temp_dir().join(format!(
            "open-hub-mchose-target-{}",
            crate::profiles::new_id()
        ));
        let transport = mchose::Transport::memory(
            move |_, _| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(vec![])
            },
            directory.clone(),
        );
        for variant in 0..7 {
            let mut current = simulated_device("mchose-A");
            match variant {
                0 => current.connection_key.push_str(":reconnected"),
                1 => current.profile_key.push_str(":new-owner"),
                2 => current.profile_key.clear(),
                3 => current.connection_key.clear(),
                4 => current.vendor_id = 0x046d,
                5 => current.product_id = 0x1012,
                _ => current.model_ids = vec![0x4033],
            }
            for _operation in ["read macro", "apply", "export", "restore"] {
                assert!(
                    with_target(&current, &transport, &target, |_| -> Result<()> {
                        panic!("stale operation ran")
                    })
                    .is_err()
                );
            }
            if variant != 1 {
                assert!(read_hardware(&current, &transport, &old.connection_key).is_err());
            }
        }
        // The owner change above is detectable only by a previously read Target;
        // a fresh discovery bound to the live generation is allowed to read it.
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        assert!(!directory.exists());
    }

    #[test]
    fn discovery_rejects_missing_connection_and_context_does_not_use_ui_aliases() {
        let device = simulated_device("mchose-A");
        assert!(read_context(&device, "").is_err());
        let target = read_context(&device, &device.connection_key).unwrap();
        assert_eq!(target.device_key, "session:mchose-A");
        target.verify(&device).unwrap();
        let alias = Target {
            device_key: "durable-ui-alias".into(),
            ..target
        };
        assert!(alias.verify(&device).is_err());
    }

    #[test]
    fn generic_manual_ipc_cannot_bypass_mchose_context_other_brands_unchanged() {
        let mut device = simulated_device("mchose-A");
        assert!(reject_unbound_write(&device).is_err());
        for vendor in [0x046d, 0x1532, 0x1b1c, 0x1038] {
            device.vendor_id = vendor;
            reject_unbound_write(&device).unwrap();
        }
    }
}
