//! Scripted devices for the unit tests: small firmware models on top of
//! [`MockTransport`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use gearwise_core::error::Result;
use gearwise_core::transport::{
    HidInterfaceInfo, HidTransport, MockOp, MockRecord, MockReply, MockTransport, SharedTransport,
    share,
};

use crate::bragi::{Link, prop};
use crate::bragi_dev::BragiDevice;
use crate::models::{self, Proto};
use crate::nxp::Nxp;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap()
}

/// Interface record as the backend would report it for the control collection.
pub(crate) fn iface(pid: u16, path: &str) -> HidInterfaceInfo {
    HidInterfaceInfo {
        path: path.into(),
        vendor_id: 0x1b1c,
        product_id: pid,
        interface_number: 1,
        usage_page: models::find(pid).map_or(0, |m| m.control_endpoint().0),
        usage: 1,
        serial: Some(format!("SN{pid:04X}")),
        ..Default::default()
    }
}

/// Assert that an output report is `expected` after the report id, zero padded.
pub(crate) fn assert_packet(report: &[u8], expected: &[u8]) {
    assert_eq!(report[0], 0, "report id");
    assert_eq!(&report[1..1 + expected.len()], expected);
    assert!(
        report[1 + expected.len()..].iter().all(|b| *b == 0),
        "padding is not zero: {report:02x?}"
    );
}

/// Writes recorded by a transport log.
fn writes_of(log: &Mutex<Vec<MockRecord>>) -> Vec<Vec<u8>> {
    lock(log)
        .iter()
        .filter_map(|r| match r {
            MockRecord::Write(d) => Some(d.clone()),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Bragi
// ---------------------------------------------------------------------------

#[derive(Default)]
struct BragiState {
    /// Property values as the bytes following the status byte.
    props: HashMap<u8, Vec<u8>>,
    /// Properties answered with an error status (and not stored).
    failing: HashMap<u8, u8>,
    /// Properties that are stored but answered with an error status.
    quirky: HashMap<u8, u8>,
    /// Resources the firmware refuses to open, with the status it returns.
    refused: HashMap<u16, u8>,
    open: HashMap<u8, u16>,
    /// Data written per handle: announced length and bytes so far.
    written: HashMap<u8, (usize, Vec<u8>)>,
    noise: Vec<Vec<u8>>,
    muted: bool,
}

pub(crate) struct FakeBragi {
    state: Arc<Mutex<BragiState>>,
    log: Arc<Mutex<Vec<MockRecord>>>,
    transport: SharedTransport,
    packet: usize,
}

impl FakeBragi {
    pub(crate) fn new(packet: usize) -> Self {
        let state = Arc::new(Mutex::new(BragiState::default()));
        let st = Arc::clone(&state);
        let transport = MockTransport::new(move |op| match op {
            MockOp::Write(d) => bragi_respond(&mut lock(&st), d, packet),
            MockOp::SetFeature(_) => MockReply::none(),
        });
        let log = transport.log();
        Self {
            state,
            log,
            transport: share(Box::new(transport)),
            packet,
        }
    }

    pub(crate) fn link(&self, id: u8) -> Link {
        Link::new(self.transport.clone(), id, self.packet)
    }

    /// Factory for a [`gearwise_core::transport::MockBackend`].
    pub(crate) fn transport_factory(
        &self,
    ) -> impl Fn() -> Box<dyn HidTransport> + Send + Sync + 'static {
        let t = self.transport.clone();
        move || Box::new(Shared(t.clone()))
    }

    pub(crate) fn set_prop(&self, property: u8, value: u32) {
        lock(&self.state)
            .props
            .insert(property, value.to_le_bytes().to_vec());
    }

    pub(crate) fn set_raw(&self, property: u8, bytes: &[u8]) {
        lock(&self.state).props.insert(property, bytes.to_vec());
    }

    pub(crate) fn prop(&self, property: u8) -> Option<u32> {
        let props = &lock(&self.state).props;
        let mut v = [0u8; 4];
        let bytes = props.get(&property)?;
        let n = bytes.len().min(4);
        v[..n].copy_from_slice(&bytes[..n]);
        Some(u32::from_le_bytes(v))
    }

    pub(crate) fn fail_prop(&self, property: u8, status: u8) {
        lock(&self.state).failing.insert(property, status);
    }

    pub(crate) fn quirk_prop(&self, property: u8, status: u8) {
        lock(&self.state).quirky.insert(property, status);
    }

    pub(crate) fn refuse_resource(&self, resource: u16, status: u8) {
        lock(&self.state).refused.insert(resource, status);
    }

    pub(crate) fn mute(&self) {
        lock(&self.state).muted = true;
    }

    pub(crate) fn queue_noise(&self, report: Vec<u8>) {
        lock(&self.state).noise.push(report);
    }

    /// Every output report written so far (including the leading report id).
    pub(crate) fn writes(&self) -> Vec<Vec<u8>> {
        writes_of(&self.log)
    }

    /// Bulk data the device received for `handle`.
    pub(crate) fn written(&self, handle: u8) -> Vec<u8> {
        lock(&self.state)
            .written
            .get(&handle)
            .map(|(_, d)| d.clone())
            .unwrap_or_default()
    }

    pub(crate) fn handle_open(&self, handle: u8) -> Option<u16> {
        lock(&self.state).open.get(&handle).copied()
    }

    fn default_prop(&self, property: u8, value: u32) {
        lock(&self.state)
            .props
            .entry(property)
            .or_insert_with(|| value.to_le_bytes().to_vec());
    }

    /// Give the fake the properties a real unit of `pid` answers (unless the
    /// test set them already), then open it.
    pub(crate) fn device(&self, pid: u16) -> BragiDevice {
        let model = models::find(pid).expect("known model");
        let Proto::Bragi(spec) = &model.proto else {
            panic!("not a Bragi device")
        };
        lock(&self.state)
            .props
            .entry(prop::FIRMWARE)
            .or_insert_with(|| vec![1, 2, 3, 0]);
        self.default_prop(prop::MODE, 1);
        self.default_prop(prop::POLL_RATE, 4);
        if let Some(d) = &spec.dpi {
            if d.separate_xy {
                self.default_prop(prop::DPI_X, 800);
                self.default_prop(prop::DPI_Y, 800);
            } else {
                self.default_prop(prop::DPI, 800);
            }
        }
        if spec.battery {
            self.default_prop(prop::BATTERY_LEVEL, 750);
            self.default_prop(prop::BATTERY_STATUS, 2);
        }
        if spec.angle_snap {
            self.default_prop(prop::ANGLE_SNAP, 0);
        }
        if let Some((_, value)) = spec.lift.first() {
            self.default_prop(prop::LIFT_OFF, u32::from(*value));
        }
        BragiDevice::open(
            self.link(0),
            None,
            model,
            spec,
            &iface(pid, "/dev/hidraw-test"),
        )
        .expect("device opens")
    }
}

fn bragi_respond(st: &mut BragiState, d: &[u8], packet: usize) -> MockReply {
    if st.muted {
        return MockReply::none();
    }
    let (magic, cmd, args) = (d[1], d[2], &d[3..]);
    let mut status = 0u8;
    let mut data: Vec<u8> = Vec::new();
    match cmd {
        // set property: [prop, 0, v0, v1, v2]
        0x01 => {
            let p = args[0];
            if let Some(s) = st.failing.get(&p) {
                status = *s;
            } else if let Some(value) = st.props.get_mut(&p) {
                *value = vec![args[2], args[3], args[4], 0];
                status = st.quirky.get(&p).copied().unwrap_or(0);
            } else {
                status = 5;
            }
        }
        // get property
        0x02 => {
            let p = args[0];
            match (st.failing.get(&p), st.props.get(&p)) {
                (Some(s), _) => status = *s,
                (None, Some(v)) => data = v.clone(),
                (None, None) => status = 5,
            }
        }
        // close handle: [count, handle]
        0x05 => {
            st.open.remove(&args[1]);
        }
        // write: [handle, len u32, data...]
        0x06 => {
            let handle = args[0];
            if st.open.contains_key(&handle) {
                let len = u32::from_le_bytes([args[1], args[2], args[3], args[4]]) as usize;
                let take = len.min(packet - 7);
                st.written.insert(handle, (len, args[5..5 + take].to_vec()));
            } else {
                status = 3;
            }
        }
        // continue write: [handle, data...]
        0x07 => {
            if let Some((len, bytes)) = st.written.get_mut(&args[0]) {
                let take = (*len - bytes.len()).min(packet - 3);
                bytes.extend_from_slice(&args[1..1 + take]);
            }
        }
        // open handle: [handle, resource lo, hi]
        0x0d => {
            let resource = u16::from_le_bytes([args[1], args[2]]);
            match st.refused.get(&resource) {
                Some(s) => status = *s,
                None if st.open.contains_key(&args[0]) => status = 3,
                None => {
                    st.open.insert(args[0], resource);
                }
            }
        }
        _ => {}
    }
    let mut reply = vec![0u8; packet];
    reply[0] = magic;
    reply[1] = cmd;
    reply[2] = status;
    let n = data.len().min(packet - 3);
    reply[3..3 + n].copy_from_slice(&data[..n]);
    let mut reports = std::mem::take(&mut st.noise);
    reports.push(reply);
    MockReply {
        input_reports: reports,
        feature: None,
    }
}

// ---------------------------------------------------------------------------
// NXP
// ---------------------------------------------------------------------------

#[derive(Default)]
struct NxpState {
    /// Read replies keyed by the first four request bytes; value is the
    /// reply bytes from index 4 on.
    replies: HashMap<[u8; 4], Vec<u8>>,
    events: Vec<Vec<u8>>,
}

pub(crate) struct FakeNxp {
    state: Arc<Mutex<NxpState>>,
    log: Arc<Mutex<Vec<MockRecord>>>,
    transport: SharedTransport,
}

impl FakeNxp {
    pub(crate) fn new() -> Self {
        let state = Arc::new(Mutex::new(NxpState::default()));
        let st = Arc::clone(&state);
        let transport = MockTransport::new(move |op| match op {
            MockOp::Write(d) if d[1] == 0x0e => {
                let mut st = lock(&st);
                let key = [d[1], d[2], d[3], d[4]];
                let Some(rest) = st.replies.get(&key).cloned() else {
                    return MockReply::none();
                };
                let mut reply = vec![0u8; 64];
                reply[..4].copy_from_slice(&key);
                reply[4..4 + rest.len()].copy_from_slice(&rest);
                let mut reports = std::mem::take(&mut st.events);
                reports.push(reply);
                MockReply {
                    input_reports: reports,
                    feature: None,
                }
            }
            _ => MockReply::none(),
        });
        let log = transport.log();
        Self {
            state,
            log,
            transport: share(Box::new(transport)),
        }
    }

    pub(crate) fn nxp(&self) -> Nxp {
        Nxp::new(self.transport())
    }

    pub(crate) fn transport(&self) -> Box<dyn HidTransport> {
        Box::new(Shared(self.transport.clone()))
    }

    /// Factory for a [`gearwise_core::transport::MockBackend`].
    pub(crate) fn transport_factory(
        &self,
    ) -> impl Fn() -> Box<dyn HidTransport> + Send + Sync + 'static {
        let t = self.transport.clone();
        move || Box::new(Shared(t.clone()))
    }

    /// Answer reads of `request` (first four bytes) with `data` after the echo.
    pub(crate) fn reply(&self, request: &[u8], data: &[u8]) {
        let key = [request[0], request[1], request[2], request[3]];
        lock(&self.state).replies.insert(key, data.to_vec());
    }

    pub(crate) fn queue_event(&self, report: Vec<u8>) {
        lock(&self.state).events.push(report);
    }

    pub(crate) fn writes(&self) -> Vec<Vec<u8>> {
        writes_of(&self.log)
    }

    /// Identification reply of a mouse: firmware, polling interval, zone mask.
    pub(crate) fn ident_for_mouse(&self, firmware: u16, poll_ms: u8, zones: u8) {
        self.ident(firmware, poll_ms, 0xc1, zones, 0x01);
    }

    pub(crate) fn ident_for_keyboard(&self, firmware: u16, poll_ms: u8) {
        self.ident(firmware, poll_ms, 0xc0, 0x00, 0x00);
    }

    fn ident(&self, firmware: u16, poll_ms: u8, kind: u8, zones: u8, byte24: u8) {
        let mut r = vec![0u8; 60];
        r[0..4].copy_from_slice(&[1, 1, 0, 1]);
        r[4..6].copy_from_slice(&firmware.to_le_bytes());
        r[6..8].copy_from_slice(&[4, 0]);
        r[8..12].copy_from_slice(&[0x1c, 0x1b, 0x2e, 0x1b]);
        r[12] = poll_ms;
        // Indexes 21.. of the packet, i.e. 17.. after the 4-byte echo.
        r[17] = kind;
        r[18] = 0xff;
        r[19] = 0x40;
        r[20] = byte24;
        r[21] = zones;
        self.reply(&[0x0e, 0x01, 0x00, 0x00], &r);
    }

    /// Stored mouse profile: enabled mask, current stage, lift, snap and the
    /// (hardware stage, x, y) table.
    pub(crate) fn mouse_profile(
        &self,
        mask: u8,
        current: u8,
        lift: u8,
        snap: u8,
        stages: &[(u8, u16, u16)],
    ) {
        self.reply(&[0x0e, 0x13, 0x05, 1], &[mask]);
        self.reply(&[0x0e, 0x13, 0x02, 1], &[current]);
        self.reply(&[0x0e, 0x13, 0x03, 1], &[lift]);
        self.reply(&[0x0e, 0x13, 0x04, 1], &[snap]);
        for (h, x, y) in stages {
            let mut d = vec![0u8];
            d.extend_from_slice(&x.to_le_bytes());
            d.extend_from_slice(&y.to_le_bytes());
            self.reply(&[0x0e, 0x13, 0xd0 | h, 1], &d);
        }
    }
}

/// A transport handle that forwards to a shared one, so a test can keep
/// talking to the fake after handing a copy to a device.
struct Shared(SharedTransport);

impl HidTransport for Shared {
    fn write(&mut self, data: &[u8]) -> Result<usize> {
        lock(&self.0).write(data)
    }
    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> Result<usize> {
        lock(&self.0).read_timeout(buf, timeout_ms)
    }
    fn send_feature_report(&mut self, data: &[u8]) -> Result<()> {
        lock(&self.0).send_feature_report(data)
    }
    fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize> {
        lock(&self.0).get_feature_report(buf)
    }
}
impl FakeNxp { pub(crate) fn operation_log(&self)->Arc<Mutex<Vec<MockRecord>>>{self.log.clone()} }
