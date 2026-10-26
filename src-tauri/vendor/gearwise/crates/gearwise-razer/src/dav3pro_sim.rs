//! Synthetic firmware for exact B6/B7 power and current-DPI integration tests.
//! Stage-table refusal is constructed, not claimed as captured behavior.
use super::*;
use gearwise_core::{
    model::DpiStage,
    transport::{HidInterfaceInfo, MockOp, MockRecord, MockReply, MockTransport},
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
pub fn mouse(pid: u16) -> Result<(Box<dyn Device>, Arc<Mutex<Vec<MockRecord>>>)> {
    if !crate::power::supported(pid) {
        return Err(Error::Unsupported("DeathAdder V3 Pro simulation model"));
    }
    let model = models::find(pid).unwrap();
    let mut dpi = DpiStage::uniform(1600);
    let mut rate = 1;
    let mut sleep = 300u16;
    let mut low = 77u8;
    let io = MockTransport::new(move |op| {
        let MockOp::SetFeature(bytes) = op else {
            return MockReply::none();
        };
        let req = report::Report::from_wire(bytes).unwrap();
        let mut r = req.clone();
        r.status = 2;
        r.args = [0; 80];
        match (req.class, req.command) {
            (0, 0x82) => r.args[..19].copy_from_slice(b"SIMULATED-DAV3PRO-A"),
            (0, 0x81) => r.args[..2].copy_from_slice(&[2, 1]),
            (4, 0x86) => r.status = 5,
            (4, 0x85) if req.args[0] == 1 => {
                let (x, y) = (dpi.x.to_be_bytes(), dpi.y.to_be_bytes());
                r.args[..7].copy_from_slice(&[1, x[0], x[1], y[0], y[1], 0, 0]);
            }
            (4, 5) => {
                dpi = DpiStage {
                    x: u16::from_be_bytes([req.args[1], req.args[2]]),
                    y: u16::from_be_bytes([req.args[3], req.args[4]]),
                };
            }
            (0, 0x85) => r.args[0] = rate,
            (0, 5) => rate = req.args[0],
            (7, 0x83) => r.args[..2].copy_from_slice(&sleep.to_be_bytes()),
            (7, 3) => sleep = u16::from_be_bytes([req.args[0], req.args[1]]),
            (7, 0x81) => r.args[..2].copy_from_slice(&[low, 0]),
            (7, 1) => low = req.args[0],
            (7, 0x80) => r.args[..2].copy_from_slice(&[0, 128]),
            (7, 0x84) => r.args[1] = u8::from(pid == 0x00b6),
            _ => r.status = 5,
        }
        MockReply::feature(r.to_wire().to_vec())
    });
    let log = io.log();
    let link = link::Link::new(
        Box::new(io),
        model.transaction_id,
        Timing::new(Duration::ZERO, Duration::ZERO, 1),
    );
    let iface = HidInterfaceInfo {
        path: format!("simulation:dav3pro:{pid:04x}"),
        vendor_id: 0x1532,
        product_id: pid,
        interface_number: 0,
        usage_page: 1,
        usage: 2,
        ..Default::default()
    };
    Ok((Box::new(RazerDevice::open(model, link, &iface)?), log))
}
