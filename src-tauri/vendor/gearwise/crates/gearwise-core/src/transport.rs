//! HID transport abstraction.
//!
//! Drivers never talk to hidapi directly: they get a [`HidTransport`] from a
//! [`HidBackend`]. The real backend lives in `gearwise-hid`; tests use
//! [`MockBackend`] / [`MockTransport`] to script device replies byte for byte.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::error::{Error, Result};

/// One HID interface (a collection on Windows, a hidraw node on Linux) as
/// reported by enumeration. A single physical device usually exposes several.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HidInterfaceInfo {
    /// Backend-specific path used to open the interface.
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    /// USB interface number, `-1` when unknown (e.g. Bluetooth).
    pub interface_number: i32,
    pub usage_page: u16,
    pub usage: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial: Option<String>,
    pub bus: BusKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BusKind {
    #[default]
    Usb,
    Bluetooth,
    Unknown,
}

/// An open HID interface.
///
/// Report buffers follow hidapi conventions: byte 0 is the report ID (use 0
/// for devices without numbered reports), the payload follows.
pub trait HidTransport: Send {
    /// Send an output report. Returns bytes written.
    fn write(&mut self, data: &[u8]) -> Result<usize>;

    /// Read one input report, waiting at most `timeout_ms` (`-1` blocks).
    /// Returns 0 on timeout.
    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> Result<usize>;

    /// Send a feature report; `data[0]` is the report ID.
    fn send_feature_report(&mut self, data: &[u8]) -> Result<()>;

    /// Fetch a feature report; set `buf[0]` to the report ID before calling.
    fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize>;
}

/// Enumerates and opens HID interfaces.
pub trait HidBackend: Send + Sync {
    fn enumerate(&self) -> Result<Vec<HidInterfaceInfo>>;
    fn open(&self, path: &str) -> Result<Box<dyn HidTransport>>;
}

/// A transport shared between several logical devices — e.g. one Logitech
/// receiver node serving up to six paired mice and keyboards.
pub type SharedTransport = Arc<Mutex<Box<dyn HidTransport>>>;

pub fn share(t: Box<dyn HidTransport>) -> SharedTransport {
    Arc::new(Mutex::new(t))
}

// ---------------------------------------------------------------------------
// Mock transport for driver tests
// ---------------------------------------------------------------------------

/// What the driver did to the mock, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MockRecord {
    Write(Vec<u8>),
    SetFeature(Vec<u8>),
    GetFeature(u8),
}

/// The driver operation a responder reacts to.
#[derive(Debug)]
pub enum MockOp<'a> {
    Write(&'a [u8]),
    SetFeature(&'a [u8]),
}

/// What the scripted device produces in reaction to an operation.
#[derive(Default, Debug)]
pub struct MockReply {
    /// Input reports queued for subsequent `read_timeout` calls.
    pub input_reports: Vec<Vec<u8>>,
    /// Feature report returned by the next `get_feature_report` for the
    /// report ID in byte 0.
    pub feature: Option<Vec<u8>>,
}

impl MockReply {
    pub fn none() -> Self {
        Self::default()
    }
    pub fn input(report: Vec<u8>) -> Self {
        Self {
            input_reports: vec![report],
            feature: None,
        }
    }
    pub fn feature(report: Vec<u8>) -> Self {
        Self {
            input_reports: vec![],
            feature: Some(report),
        }
    }
}

type Responder = Box<dyn FnMut(MockOp<'_>) -> MockReply + Send>;

/// Scriptable in-memory transport. The responder plays the device's firmware.
pub struct MockTransport {
    log: Arc<Mutex<Vec<MockRecord>>>,
    input: VecDeque<Vec<u8>>,
    features: HashMap<u8, Vec<u8>>,
    responder: Responder,
}

impl MockTransport {
    pub fn new(responder: impl FnMut(MockOp<'_>) -> MockReply + Send + 'static) -> Self {
        Self {
            log: Arc::new(Mutex::new(Vec::new())),
            input: VecDeque::new(),
            features: HashMap::new(),
            responder: Box::new(responder),
        }
    }

    /// Handle to the operation log, usable after the transport is moved into
    /// a driver.
    pub fn log(&self) -> Arc<Mutex<Vec<MockRecord>>> {
        Arc::clone(&self.log)
    }

    /// Queue an unsolicited input report (a notification from the device).
    pub fn push_input(&mut self, report: Vec<u8>) {
        self.input.push_back(report);
    }

    fn absorb(&mut self, reply: MockReply) {
        self.input.extend(reply.input_reports);
        if let Some(f) = reply.feature {
            let id = f.first().copied().unwrap_or(0);
            self.features.insert(id, f);
        }
    }
}

impl HidTransport for MockTransport {
    fn write(&mut self, data: &[u8]) -> Result<usize> {
        self.log
            .lock()
            .unwrap()
            .push(MockRecord::Write(data.to_vec()));
        let reply = (self.responder)(MockOp::Write(data));
        self.absorb(reply);
        Ok(data.len())
    }

    fn read_timeout(&mut self, buf: &mut [u8], _timeout_ms: i32) -> Result<usize> {
        match self.input.pop_front() {
            Some(r) => {
                let n = r.len().min(buf.len());
                buf[..n].copy_from_slice(&r[..n]);
                Ok(n)
            }
            None => Ok(0),
        }
    }

    fn send_feature_report(&mut self, data: &[u8]) -> Result<()> {
        self.log
            .lock()
            .unwrap()
            .push(MockRecord::SetFeature(data.to_vec()));
        let reply = (self.responder)(MockOp::SetFeature(data));
        self.absorb(reply);
        Ok(())
    }

    fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize> {
        let id = buf.first().copied().unwrap_or(0);
        self.log.lock().unwrap().push(MockRecord::GetFeature(id));
        match self.features.get(&id) {
            Some(r) => {
                let n = r.len().min(buf.len());
                buf[..n].copy_from_slice(&r[..n]);
                Ok(n)
            }
            None => Err(Error::Timeout),
        }
    }
}

/// Backend over a fixed set of interfaces, each opened through a factory that
/// builds a fresh [`MockTransport`].
pub struct MockBackend {
    interfaces: Vec<HidInterfaceInfo>,
    #[allow(clippy::type_complexity)]
    factory: Box<dyn Fn(&str) -> Result<Box<dyn HidTransport>> + Send + Sync>,
}

impl MockBackend {
    pub fn new(
        interfaces: Vec<HidInterfaceInfo>,
        factory: impl Fn(&str) -> Result<Box<dyn HidTransport>> + Send + Sync + 'static,
    ) -> Self {
        Self {
            interfaces,
            factory: Box::new(factory),
        }
    }
}

impl HidBackend for MockBackend {
    fn enumerate(&self) -> Result<Vec<HidInterfaceInfo>> {
        Ok(self.interfaces.clone())
    }
    fn open(&self, path: &str) -> Result<Box<dyn HidTransport>> {
        (self.factory)(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_echoes_writes_as_input() {
        let mut t = MockTransport::new(|op| match op {
            MockOp::Write(d) => MockReply::input(d.to_vec()),
            MockOp::SetFeature(_) => MockReply::none(),
        });
        let log = t.log();
        t.write(&[0x10, 1, 2]).unwrap();
        let mut buf = [0u8; 8];
        assert_eq!(t.read_timeout(&mut buf, 10).unwrap(), 3);
        assert_eq!(&buf[..3], &[0x10, 1, 2]);
        assert_eq!(t.read_timeout(&mut buf, 10).unwrap(), 0);
        assert_eq!(log.lock().unwrap().len(), 1);
    }

    #[test]
    fn mock_feature_round_trip() {
        let mut t = MockTransport::new(|op| match op {
            MockOp::SetFeature(d) => {
                let mut r = d.to_vec();
                r[1] = 0x02;
                MockReply::feature(r)
            }
            MockOp::Write(_) => MockReply::none(),
        });
        t.send_feature_report(&[0, 0, 9]).unwrap();
        let mut buf = [0u8; 3];
        t.get_feature_report(&mut buf).unwrap();
        assert_eq!(buf, [0, 2, 9]);
    }
}
