//! MCHOSE A7 V3 Ultra+ protocol adapter, independently implemented from
//! the publicly served M HUB packet layout. This is NOT Logitech HID++.
//! Only the exact USB/model pair verified on the user's hardware is enabled.
use crate::device::{Battery, Choice, Dpi, ReportRate};
use crate::hidpp::{Error, Event, EventSink, Result, DIRECT};
use hidapi::{HidApi, HidDevice};
use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const VID: u16 = 0x3837;
pub const RECEIVER_PID: u16 = 0x1014;
pub const MODEL_PID: u16 = 0x4026;
pub const USAGE_PAGE: u16 = 0xFF01;
pub fn known_interface(
    vendor: u16,
    product: u16,
    page: u16,
    usage: u16,
    name: Option<&str>,
) -> bool {
    vendor == VID
        && product == RECEIVER_PID
        && page == USAGE_PAGE
        && usage == 1
        && name == Some("MCHOSE A7 V3 Ultra+")
}
const REPORT: u8 = 0x4D;
const IDENTITY_GROUPS: usize = 4;
const IDENTITY_ATTEMPTS_PER_GROUP: usize = 5;
const IDENTITY_RETRY_DELAY: Duration = Duration::from_millis(30);
const TIMEOUT: Duration = Duration::from_millis(2000);
const RATES: &[(u8, u32)] = &[
    (0, 125),
    (2, 500),
    (3, 1000),
    (4, 2000),
    (5, 4000),
    (6, 8000),
];

fn invalid(s: impl Into<String>) -> Error {
    Error::Other(s.into())
}

pub fn frame(command: u16, args: &[u8]) -> Result<[u8; 64]> {
    if args.len() > 55 {
        return Err(invalid("MCHOSE payload is too long"));
    }
    let mut b = [0u8; 64];
    b[0] = REPORT;
    b[1] = 1;
    b[2] = 1;
    b[3] = args.len() as u8;
    b[4..6].copy_from_slice(&command.to_le_bytes());
    b[8..8 + args.len()].copy_from_slice(args);
    b[8 + args.len()] = b[2..8 + args.len()].iter().fold(0, |sum, v| sum ^ v);
    Ok(b)
}

fn decode(b: &[u8]) -> Result<(u16, Vec<u8>)> {
    if b.len() < 9 || b[0] != REPORT || b[1] != 1 {
        return Err(invalid("Invalid MCHOSE report header"));
    }
    let n = usize::from(b[3]);
    if n > 55 || b.len() < 9 + n {
        return Err(invalid("Truncated MCHOSE reply"));
    }
    // Receiver firmware V6 omits the checksum for its duplicated 4-byte
    // version reply. Accept only that exact read-only response shape.
    let receiver_version_without_checksum =
        b[4..6] == [1, 9] && n == 4 && b[8..10] == b[10..12] && b[12] == 0;
    if b[2] & 1 != 0
        && b[2..8 + n].iter().fold(0, |sum, v| sum ^ v) != b[8 + n]
        && !receiver_version_without_checksum
    {
        return Err(invalid("MCHOSE reply checksum mismatch"));
    }
    Ok((u16::from_le_bytes([b[4], b[5]]), b[8..8 + n].to_vec()))
}

/// The exact captured identity rejection is never a successful payload. Its
/// omitted checksum must not relax integrity checks for settings or writes.
fn response(b: &[u8]) -> Result<(u16, Result<Vec<u8>>)> {
    if b.len() == 64
        && b[..9] == [0x4d, 1, 1, 1, 0, 9, 0, 0, 0xff]
        && b[9..].iter().all(|v| *v == 0)
    {
        return Ok((0x0900, Err(Error::MchoseIdentityUnavailable)));
    }
    decode(b).map(|(command, data)| {
        let reply = if command == 0x0900 && data == [0xff] {
            Err(Error::MchoseIdentityUnavailable)
        } else {
            Ok(data)
        };
        (command, reply)
    })
}

/// Mirror the pinned BR(5) / hgt(4) read-only retry bound, but only for the
/// explicit unavailable identity response. Empty battery data, unknown models,
/// checksum errors, disconnects and timeouts never become retryable writes.
fn identity_with_retry(
    mut query: impl FnMut() -> Result<Vec<u8>>,
    mut wait: impl FnMut(Duration),
) -> Result<Vec<u8>> {
    for group in 0..IDENTITY_GROUPS {
        for attempt in 0..IDENTITY_ATTEMPTS_PER_GROUP {
            match query() {
                Err(Error::MchoseIdentityUnavailable)
                    if group + 1 < IDENTITY_GROUPS || attempt + 1 < IDENTITY_ATTEMPTS_PER_GROUP =>
                {
                    if attempt + 1 < IDENTITY_ATTEMPTS_PER_GROUP {
                        wait(IDENTITY_RETRY_DELAY);
                    }
                }
                result => return result,
            }
        }
    }
    unreachable!("final unavailable reply returns its error")
}
/// A partial/failed USB write can have uncertain effects. Invalidate this
/// transport so later requests cannot reuse its pending response/connection.
fn finish_output(
    shared: &Shared,
    sent: hidapi::HidResult<usize>,
    expected: usize,
    events: &EventSink,
) -> Result<()> {
    let result=match sent {
        Ok(n) if n==expected=>Ok(()),
        Ok(n)=>Err(invalid(format!("MCHOSE incomplete output report: {n}/{expected}; connection closed, no automatic retry"))),
        Err(e)=>Err(e.into()),
    };
    if result.is_err() {
        let was_alive = shared.alive.swap(false, Ordering::SeqCst);
        if let Ok(mut pending) = shared.pending.lock() {
            pending.command = None;
            pending.reply = None;
        }
        shared.ready.notify_all();
        if was_alive {
            events(Event::Gone);
        }
    }
    result
}

fn complete_reply(shared: &Shared, command: u16, reply: Result<Vec<u8>>) -> bool {
    if let Ok(mut pending) = shared.pending.lock() {
        if pending.command == Some(command) {
            pending.command = None;
            pending.reply = Some(reply);
            shared.ready.notify_all();
            return true;
        }
    }
    false
}

#[derive(Default)]
struct Pending {
    command: Option<u16>,
    reply: Option<Result<Vec<u8>>>,
}
struct Shared {
    pending: Mutex<Pending>,
    ready: Condvar,
    alive: AtomicBool,
}
enum Writer {
    Hid(HidDevice),
    #[cfg(test)]
    Memory {
        request: Box<dyn Fn(u16, &[u8]) -> Result<Vec<u8>> + Send + Sync>,
        backup_directory: std::path::PathBuf,
    },
}
pub struct Transport {
    writer: Writer,
    shared: Arc<Shared>,
    transaction: Mutex<()>,
    events: EventSink,
}

impl Transport {
    pub fn open(api: &HidApi, path: &CStr, events: EventSink) -> Result<Self> {
        let writer = api.open_path(path)?;
        let reader = api.open_path(path)?;
        let shared = Arc::new(Shared {
            pending: Mutex::new(Pending::default()),
            ready: Condvar::new(),
            alive: AtomicBool::new(true),
        });
        let state = shared.clone();
        let path = path.to_owned();
        let publish = events.clone();
        std::thread::Builder::new()
            .name("mchose-v3-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 64];
                while state.alive.load(Ordering::SeqCst) {
                    match reader.read_timeout(&mut buf, 500) {
                        Ok(0) => {}
                        Ok(n) => {
                            if let Ok((command, reply)) = response(&buf[..n]) {
                                if command == 0x0600 {
                                    events(Event::DeviceChanged {
                                        path: path.clone(),
                                        index: DIRECT,
                                    });
                                    continue;
                                }
                                complete_reply(&state, command, reply);
                            }
                        }
                        Err(_) => {
                            let was_alive = state.alive.swap(false, Ordering::SeqCst);
                            if let Ok(mut p) = state.pending.lock() {
                                p.command = None;
                                p.reply = Some(Err(Error::Disconnected));
                            }
                            state.ready.notify_all();
                            if was_alive {
                                events(Event::Gone);
                            }
                            break;
                        }
                    }
                }
            })
            .map_err(|e| invalid(e.to_string()))?;
        Ok(Self {
            writer: Writer::Hid(writer),
            shared,
            transaction: Mutex::new(()),
            events: publish,
        })
    }
    pub fn is_alive(&self) -> bool {
        self.shared.alive.load(Ordering::SeqCst)
    }
    pub fn request(&self, command: u16, args: &[u8]) -> Result<Vec<u8>> {
        if crate::events::logging() {
            eprintln!("[mchose] request {command:04x} {args:02x?}");
        }
        let _serial = self
            .transaction
            .lock()
            .map_err(|_| invalid("MCHOSE transaction lock poisoned"))?;
        if !self.is_alive() {
            return Err(Error::Disconnected);
        }
        let packet = frame(command, args)?;
        #[cfg(test)]
        if let Writer::Memory { request, .. } = &self.writer {
            // Exercise the production packet layout in both directions while
            // replacing only physical HID I/O with a stateful native device.
            let (command, args) = decode(&packet)?;
            let response = request(command, &args)?;
            return self::response(&frame(command, &response)?).and_then(|(_, body)| body);
        }
        std::thread::sleep(Duration::from_millis(40));
        {
            let mut p = self
                .shared
                .pending
                .lock()
                .map_err(|_| invalid("MCHOSE reply lock poisoned"))?;
            p.command = Some(command);
            p.reply = None;
        }
        let sent = match &self.writer {
            Writer::Hid(writer) => writer.write(&packet),
            #[cfg(test)]
            Writer::Memory { .. } => unreachable!("memory request returned above"),
        };
        finish_output(&self.shared, sent, packet.len(), &self.events)?;
        let deadline = Instant::now() + TIMEOUT;
        let mut p = self
            .shared
            .pending
            .lock()
            .map_err(|_| invalid("MCHOSE reply lock poisoned"))?;
        loop {
            if let Some(reply) = p.reply.take() {
                return reply;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                p.command = None;
                return Err(invalid(format!("MCHOSE command {command:04x} timed out")));
            }
            p = self
                .shared
                .ready
                .wait_timeout(p, left)
                .map_err(|_| invalid("MCHOSE reply lock poisoned"))?
                .0;
        }
    }
    #[cfg(test)]
    pub(crate) fn memory(
        request: impl Fn(u16, &[u8]) -> Result<Vec<u8>> + Send + Sync + 'static,
        backup_directory: std::path::PathBuf,
    ) -> Self {
        Self {
            writer: Writer::Memory {
                request: Box::new(request),
                backup_directory,
            },
            shared: Arc::new(Shared {
                pending: Mutex::new(Pending::default()),
                ready: Condvar::new(),
                alive: AtomicBool::new(true),
            }),
            transaction: Mutex::new(()),
            events: Arc::new(|_| {}),
        }
    }
    pub(crate) fn require_board(&self, expected: u8) -> Result<()> {
        if expected > 2 || self.settings()?[0] != expected {
            return Err(invalid("板载配置已改变，请刷新重试"));
        }
        Ok(())
    }
    pub(crate) fn backup_directory(&self) -> Result<std::path::PathBuf> {
        #[cfg(test)]
        if let Writer::Memory {
            backup_directory, ..
        } = &self.writer
        {
            return Ok(backup_directory.clone());
        }
        let root = std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| invalid("LOCALAPPDATA is unavailable; refusing write without backup"))?;
        Ok(std::path::PathBuf::from(root)
            .join("Open HUB")
            .join("backups"))
    }
    pub fn identity(&self) -> Result<Vec<u8>> {
        let data = identity_with_retry(|| self.request(0x0900, &[]), std::thread::sleep)?;
        if data.len() < 14
            || u16::from_le_bytes([data[0], data[1]]) != VID
            || u16::from_le_bytes([data[2], data[3]]) != MODEL_PID
        {
            return Err(invalid(
                "This MCHOSE model has not been verified; writes are disabled",
            ));
        }
        if data[10] == 0 {
            return Err(invalid(
                "MCHOSE connection status is0; the connected mouse is not confirmed",
            ));
        }
        Ok(data)
    }
    pub fn battery(&self) -> Result<Option<Battery>> {
        let b = self.identity()?;
        if b[12] > 100 {
            return Err(invalid("Invalid MCHOSE battery level"));
        }
        Ok(Some(Battery {
            percent: Some(b[12]),
            level: None,
            status: if b[11] == 1 {
                "charging"
            } else {
                "discharging"
            },
            voltage_mv: None,
        }))
    }
    pub(crate) fn settings(&self) -> Result<Vec<u8>> {
        let b = self.request(0x0002, &[])?;
        if b.len() != 19 || b[0] > 2 || b[1] & 15 > 5 || b[2] & 15 > 5 {
            return Err(invalid("Unrecognised MCHOSE settings layout"));
        }
        Ok(b)
    }
    pub(crate) fn dpi_table(&self, profile: u8, axis: u8) -> Result<Vec<u8>> {
        let b = self.request(0x0003, &[profile, axis])?;
        if b.len() != 17
            || b[0] != profile
            || b[1] != axis
            || !(1..=6).contains(&b[2])
            || b[3] >= b[2]
            || b[4] > 1
        {
            return Err(invalid("Invalid MCHOSE DPI table"));
        }
        for i in 0..usize::from(b[2]) {
            let v = u16::from_le_bytes([b[5 + 2 * i], b[6 + 2 * i]]);
            if !(50..=50000).contains(&v) {
                return Err(invalid("Invalid MCHOSE DPI value"));
            }
        }
        Ok(b)
    }
    pub fn dpi(&self) -> Result<Option<Dpi>> {
        let cfg = self.settings()?;
        let b = self.dpi_table(cfg[0], 0)?;
        let at = 5 + usize::from(b[3]) * 2;
        let y = if b[4] == 1 {
            let y = self.dpi_table(cfg[0], 1)?;
            Some(u16::from_le_bytes([y[at], y[at + 1]]))
        } else {
            None
        };
        Ok(Some(Dpi {
            feature: 0,
            x: u16::from_le_bytes([b[at], b[at + 1]]),
            y,
            default: None,
            supported: (50..=50000).step_by(50).collect(),
            supports_y: true,
            lod: None,
            supports_lod: false,
        }))
    }
    pub fn report_rate(&self) -> Result<Option<ReportRate>> {
        let cfg = self.settings()?;
        let value = cfg[2] >> 4;
        if !RATES.iter().any(|(v, _)| *v == value) {
            return Err(invalid("Unknown MCHOSE report rate"));
        }
        Ok(Some(ReportRate {
            feature: 0,
            current: value,
            choices: RATES
                .iter()
                .map(|(v, hz)| Choice {
                    value: *v,
                    hz: *hz,
                    label: format!("{hz} Hz"),
                })
                .collect(),
        }))
    }
    fn backup_for(&self, expected_board: u8) -> Result<()> {
        let identity = self.identity()?;
        let cfg = self.settings()?;
        if cfg[0] != expected_board {
            return Err(invalid("板载配置已改变，请刷新重试"));
        }
        let x = self.dpi_table(cfg[0], 0)?;
        let y = self.dpi_table(cfg[0], 1)?;
        self.require_board(expected_board)?;
        let dir = self.backup_directory()?;
        std::fs::create_dir_all(&dir).map_err(|e| invalid(e.to_string()))?;
        let tick = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| invalid(e.to_string()))?
            .as_nanos();
        let data = serde_json::json!({"format":"mchose-v3-raw-v1","model":"A7 V3 Ultra+","identity":identity,"settings":cfg,"dpi_x":x,"dpi_y":y});
        std::fs::write(
            dir.join(format!("{tick}.json")),
            serde_json::to_vec_pretty(&data).map_err(|e| invalid(e.to_string()))?,
        )
        .map_err(|e| invalid(format!("Could not save backup: {e}")))
    }
    pub fn set_dpi(&self, x: u16, y: Option<u16>, lod: Option<u8>) -> Result<()> {
        if !(50..=50000).contains(&x)
            || x % 50 != 0
            || y.is_some_and(|v| !(50..=50000).contains(&v) || v % 50 != 0)
        {
            return Err(invalid("DPI must be 50–50000 in steps of 50"));
        }
        let cfg = self.settings()?;
        let board = cfg[0];
        let a = self.dpi_table(cfg[0], 0)?;
        let b = self.dpi_table(cfg[0], 1)?;
        let mut xs: Vec<u16> = (0..6)
            .map(|i| u16::from_le_bytes([a[5 + 2 * i], a[6 + 2 * i]]))
            .collect();
        let mut ys: Vec<u16> = (0..6)
            .map(|i| u16::from_le_bytes([b[5 + 2 * i], b[6 + 2 * i]]))
            .collect();
        let at = usize::from(a[3]);
        let y = y.unwrap_or(if a[4] != 0 { ys[at] } else { x });
        if xs[at] != x || ys[at] != y {
            xs[at] = x;
            ys[at] = y;
            self.apply_hardware_for(
                board,
                crate::mchose_config::Action::Dpi {
                    x: xs,
                    y: ys,
                    count: a[2],
                    active: a[3],
                    independent: a[4] != 0 || x != y,
                },
            )?;
        }
        if let Some(index) = lod {
            self.apply_hardware_for(board, crate::mchose_config::Action::Lod { index })?;
        }
        Ok(())
    }
    pub fn set_report_rate(&self, value: u8) -> Result<()> {
        if !RATES.iter().any(|(v, _)| *v == value) {
            return Err(invalid("Unsupported MCHOSE report rate"));
        }
        let mut cfg = self.settings()?;
        if cfg[1] >> 4 == value && cfg[2] >> 4 == value {
            return Ok(());
        }
        let board = cfg[0];
        self.backup_for(board)?;
        self.require_board(board)?;
        cfg[1] = (value << 4) | (cfg[1] & 15);
        cfg[2] = (value << 4) | (cfg[2] & 15);
        self.request(0x0102, &cfg)?;
        for _ in 0..5 {
            std::thread::sleep(Duration::from_millis(100));
            let now = self.settings()?;
            if now == cfg {
                return Ok(());
            }
        }
        Err(invalid(
            "MCHOSE report-rate read-back did not match; inspect the saved backup",
        ))
    }
}
impl Drop for Transport {
    fn drop(&mut self) {
        self.shared.alive.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
#[path = "mchose_board_tests.rs"]
mod board_tests;

#[cfg(test)]
#[path = "mchose_keybinding_tests.rs"]
mod keybinding_tests;

#[cfg(test)]
#[path = "mchose_response_tests.rs"]
mod response_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_known_receiver_version_checksum_omission() {
        let b = [0x4d, 1, 1, 4, 1, 9, 0, 0, 0, 6, 0, 6, 0];
        assert_eq!(decode(&b).unwrap().1, [0, 6, 0, 6]);
        let mut bad = b;
        bad[4] = 2;
        assert!(decode(&bad).is_err());
    }
    #[test]
    fn identity_packet_matches_official_wire_layout() {
        let b = frame(0x0900, &[]).unwrap();
        assert_eq!(&b[..9], &[0x4d, 1, 1, 0, 0, 9, 0, 0, 8]);
    }
    #[test]
    fn captured_reply_and_corruption() {
        let mut b = frame(
            0x0003,
            &[
                0, 0, 6, 3, 0, 0x90, 1, 0x20, 3, 0x40, 6, 0x80, 12, 0, 25, 0x50, 0xc3,
            ],
        )
        .unwrap();
        assert_eq!(decode(&b).unwrap().1.len(), 17);
        b[14] ^= 1;
        assert!(decode(&b).is_err());
        assert!(decode(&b[..8]).is_err());
    }
    #[test]
    fn report_rate_skips_unsupported_250hz() {
        assert!(!RATES.iter().any(|(v, _)| *v == 1));
        assert_eq!(RATES.last(), Some(&(6, 8000)));
    }
    #[test]
    fn parses_actual_a7_v3_identity_capture() {
        let raw = [
            0x4d, 1, 1, 0x10, 0, 9, 0, 0, 0x37, 0x38, 0x26, 0x40, 4, 0, 0, 0, 0, 0x10, 2, 1, 100,
            0, 8, 0xe4, 0xee,
        ];
        let (command, body) = decode(&raw).unwrap();
        assert_eq!(command, 0x0900);
        assert_eq!(u16::from_le_bytes([body[2], body[3]]), MODEL_PID);
        assert_eq!(body[12], 100);
    }
}
