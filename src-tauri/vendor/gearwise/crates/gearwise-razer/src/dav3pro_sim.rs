//! Synthetic firmware for exact B6/B7 power and current/staged DPI tests.
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
    mouse_mode(pid, true)
}
pub fn current_mouse(pid: u16) -> Result<(Box<dyn Device>, Arc<Mutex<Vec<MockRecord>>>)> {
    mouse_mode(pid, false)
}
fn mouse_mode(pid: u16, staged: bool) -> Result<(Box<dyn Device>, Arc<Mutex<Vec<MockRecord>>>)> {
    if !crate::power::supported(pid) {
        return Err(Error::Unsupported("DeathAdder V3 Pro simulation model"));
    }
    let model = models::find(pid).unwrap();
    let mut dpi = DpiStage::uniform(1600);
    let mut table = gearwise_core::model::DpiState {
        stages: [400, 800, 1600, 3200, 6400].map(DpiStage::uniform).to_vec(),
        active: 2,
    };
    let mut buttons = crate::dav3pro_buttons::controls()
        .into_iter()
        .map(|(id, _)| (id, crate::dav3pro_buttons::default_block(id).unwrap()))
        .collect::<std::collections::BTreeMap<_, _>>();
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
            (4, 0x86) if staged => {
                assert_eq!(req.data_size, 38);
                assert_eq!(req.args[0], 1);
                r.args[0] = 1;
                r.args[1] = table.active + 1;
                r.args[2] = table.stages.len() as u8;
                for (i, s) in table.stages.iter().enumerate() {
                    let at = 3 + i * 7;
                    r.args[at] = i as u8 + 1;
                    r.args[at + 1..at + 3].copy_from_slice(&s.x.to_be_bytes());
                    r.args[at + 3..at + 5].copy_from_slice(&s.y.to_be_bytes());
                }
            }
            (4, 0x86) => r.status = 5,
            (4, 6) if staged => {
                assert_eq!(req.data_size, 38);
                assert_eq!(req.args[0], 1);
                let count = usize::from(req.args[2]);
                assert!((1..=5).contains(&count));
                let mut rows = vec![];
                for (i, row) in req.args[3..3 + 7 * count].chunks_exact(7).enumerate() {
                    assert_eq!(row[0], i as u8);
                    assert_eq!(&row[5..7], &[0, 0]);
                    rows.push(DpiStage {
                        x: u16::from_be_bytes([row[1], row[2]]),
                        y: u16::from_be_bytes([row[3], row[4]]),
                    });
                }
                table = gearwise_core::model::DpiState {
                    stages: rows,
                    active: req.args[1] - 1,
                };
                dpi = table.stages[usize::from(table.active)];
            }
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
            (0, 0x84) => r.args[..2].copy_from_slice(&[0, 0]),
            (2, 0x8c) if req.args[0] == 1 && req.args[2] <= 1 => {
                if let Some(block) = buttons.get(&(u16::from(req.args[1])|(u16::from(req.args[2])<<8))) {
                    r.args[..3].copy_from_slice(&req.args[..3]);
                    r.args[3..10].copy_from_slice(block);
                } else {
                    r.status = 5;
                }
            }
            (2, 0x0c) if req.args[0] == 1 && req.args[2] <= 1 => {
                if let Some(block) = buttons.get_mut(&(u16::from(req.args[1])|(u16::from(req.args[2])<<8))) {
                    block.copy_from_slice(&req.args[3..10]);
                    r.args[..10].copy_from_slice(&req.args[..10]);
                } else {
                    r.status = 5;
                }
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
