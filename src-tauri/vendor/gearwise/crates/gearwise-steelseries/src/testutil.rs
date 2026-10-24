//! Helpers for driving a device over a scripted transport.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use gearwise_core::transport::{
    HidInterfaceInfo, MockBackend, MockOp, MockRecord, MockReply, MockTransport,
};
use gearwise_core::{Device, Driver};

use crate::SteelSeriesDriver;
use crate::models;

pub type Log = Arc<Mutex<Vec<MockRecord>>>;

/// The control interface of `pid`, as the backend would list it.
pub fn iface(pid: u16) -> HidInterfaceInfo {
    let interface = models::find(pid).map_or(0, |m| m.spec.control.interface);
    HidInterfaceInfo {
        path: format!("hid-{pid:04x}"),
        vendor_id: 0x1038,
        product_id: pid,
        interface_number: interface,
        usage_page: 0xffc0,
        usage: 1,
        serial: Some(format!("SN{pid:04x}")),
        ..Default::default()
    }
}

/// Open `pid` over a transport whose firmware is `responder`.
pub fn open_with(
    pid: u16,
    responder: impl FnMut(MockOp<'_>) -> MockReply + Send + 'static,
) -> (Box<dyn Device>, Log) {
    let transport = MockTransport::new(responder);
    let log = transport.log();
    let slot = Mutex::new(Some(transport));
    let info = iface(pid);
    let backend = MockBackend::new(vec![info.clone()], move |_| {
        Ok(Box::new(slot.lock().unwrap().take().expect("opened once")))
    });
    let mut devices = SteelSeriesDriver::with_gap(Duration::ZERO)
        .open(&info, &backend)
        .expect("model is in the table");
    (devices.remove(0), log)
}

/// A firmware that ignores everything.
pub fn silent(pid: u16) -> (Box<dyn Device>, Log) {
    open_with(pid, |_| MockReply::none())
}

/// An output report as recorded: report id 0, then `payload`.
pub fn output(payload: &[u8]) -> MockRecord {
    MockRecord::Write([&[0][..], payload].concat())
}

/// A feature report as recorded: report id 0, then `payload`.
pub fn feature(payload: &[u8]) -> MockRecord {
    MockRecord::SetFeature([&[0][..], payload].concat())
}

/// Take everything recorded so far.
pub fn drain(log: &Log) -> Vec<MockRecord> {
    std::mem::take(&mut *log.lock().unwrap())
}

/// Bytes from a hex dump such as `"5b 00 e8 03"`.
pub fn hex(dump: &str) -> Vec<u8> {
    dump.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).expect("hex byte"))
        .collect()
}
