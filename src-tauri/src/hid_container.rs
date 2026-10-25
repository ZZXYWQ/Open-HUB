//! Read-only Windows physical-container lookup for an already enumerated HID
//! interface. Never parse symbolic path fragments to guess sibling devices.
pub fn id(path: &str) -> Option<[u8; 16]> {
    if path.is_empty() || path.len() > 4096 || path.contains('\0') {
        return None;
    }
    platform(path)
}
#[cfg(not(windows))]
fn platform(_: &str) -> Option<[u8; 16]> {
    None
}
#[cfg(windows)]
fn platform(path: &str) -> Option<[u8; 16]> {
    use windows_sys::Win32::{
        Devices::{
            DeviceAndDriverInstallation::*,
            Properties::{DEVPKEY_Device_ContainerId, DEVPROP_TYPE_GUID},
        },
        Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER},
    };
    struct Set(HDEVINFO);
    impl Drop for Set {
        fn drop(&mut self) {
            unsafe {
                SetupDiDestroyDeviceInfoList(self.0);
            }
        }
    }
    // SetupAPI handles and structures remain local; no device I/O is opened.
    unsafe {
        let handle = SetupDiCreateDeviceInfoList(std::ptr::null(), std::ptr::null_mut());
        if handle == -1 {
            return None;
        }
        let set = Set(handle);
        let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
        let mut interface = SP_DEVICE_INTERFACE_DATA {
            cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        if SetupDiOpenDeviceInterfaceW(set.0, wide.as_ptr(), 0, &mut interface) == 0 {
            return None;
        }
        let mut device = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        let mut required = 0;
        // Microsoft documents that this populates DeviceInfoData while
        // returning ERROR_INSUFFICIENT_BUFFER when detail data is omitted.
        let result = SetupDiGetDeviceInterfaceDetailW(
            set.0,
            &interface,
            std::ptr::null_mut(),
            0,
            &mut required,
            &mut device,
        );
        if result == 0 && GetLastError() != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
        let mut property_type = 0;
        let mut bytes = [0u8; 16];
        required = 0;
        if SetupDiGetDevicePropertyW(
            set.0,
            &device,
            &DEVPKEY_Device_ContainerId,
            &mut property_type,
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            &mut required,
            0,
        ) == 0
        {
            return None;
        }
        if property_type != DEVPROP_TYPE_GUID
            || required != 16
            || bytes == [0; 16]
            || bytes == [0xff; 16]
        {
            return None;
        }
        Some(bytes)
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn invalid_paths_do_not_reach_windows_lookup() {
        for p in ["".to_string(), "a\0b".to_string(), "x".repeat(4097)] {
            assert_eq!(super::id(&p), None);
        }
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "reads local MCHOSE enumeration metadata only; explicit machine check"]
    fn local_mchose_container_metadata_is_available_without_opening_hid_io() {
        let api = hidapi::HidApi::new().unwrap();
        let mut groups = std::collections::BTreeMap::new();
        let mut total = 0;
        for device in api
            .device_list()
            .filter(|i| i.vendor_id() == crate::mchose::VID)
        {
            let path = device.path().to_str().unwrap();
            let container =
                super::id(path).expect("MCHOSE interface container metadata unavailable");
            *groups.entry(container).or_insert(0usize) += 1;
            total += 1;
        }
        assert!(
            total >= 2,
            "connect the known MCHOSE receiver before this explicit metadata test"
        );
        assert!(groups.values().any(|count| *count >= 2));
        println!("read-only MCHOSE metadata: {total} interfaces, {} nonzero containers; no HID handles opened",groups.len());
    }
}
