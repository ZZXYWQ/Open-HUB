//! [`HidTransport`] over an open hidapi device.

use gearwise_core::{HidTransport, Result};
use hidapi::{HidDevice, HidResult};

use crate::error_map::map_error;

/// One opened HID interface.
///
/// The device is closed when the transport is dropped.
pub struct HidapiTransport {
    dev: HidDevice,
    path: String,
}

impl HidapiTransport {
    pub(crate) fn new(dev: HidDevice, path: String) -> Self {
        Self { dev, path }
    }

    /// Path of the interface this transport is bound to.
    pub fn path(&self) -> &str {
        &self.path
    }

    fn check<T>(&self, res: HidResult<T>) -> Result<T> {
        res.map_err(|e| {
            let err = map_error(&e, &self.path);
            log::debug!("hid i/o on {} failed: {e} -> {err}", self.path);
            err
        })
    }
}

impl std::fmt::Debug for HidapiTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HidapiTransport")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl HidTransport for HidapiTransport {
    fn write(&mut self, data: &[u8]) -> Result<usize> {
        let n = self.check(self.dev.write(data))?;
        // hidapi-rs' Windows backend reports 0 when the overlapped write
        // completed synchronously. A zero-byte result for a non-empty report
        // is never a meaningful short write, so report it as fully written.
        Ok(if n == 0 { data.len() } else { n })
    }

    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> Result<usize> {
        self.check(self.dev.read_timeout(buf, timeout_ms))
    }

    fn send_feature_report(&mut self, data: &[u8]) -> Result<()> {
        self.check(self.dev.send_feature_report(data))
    }

    fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.check(self.dev.get_feature_report(buf))
    }
}
