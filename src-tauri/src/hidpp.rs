//! HID++ transport over hidapi.
//!
//! Requests go out as reports:
//! `[report id, device index, feature index / sub id, (function << 4) | sw_id or address, params...]`.
//! A background reader per collection delivers each reply to the waiting
//! request and forwards unsolicited reports (HID++ 2.0 feature events,
//! HID++ 1.0 receiver notifications) as [`Event`]s, so nothing polls.
//!
//! Windows exposes short (7-byte) and long (20-byte) reports as separate
//! collections. Requests to devices use the long one; receivers also get the
//! short one for register access and connection notifications.

use std::ffi::{CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};

pub const LOGITECH_VID: u16 = 0x046D;
/// Vendor-defined usage page that carries HID++ traffic.
pub const HIDPP_USAGE_PAGE: u16 = 0xFF00;
/// Usage of the collection that carries short (7-byte) reports.
pub const HIDPP_SHORT_USAGE: u16 = 0x0001;
/// Usage of the collection that carries long (20-byte) reports.
pub const HIDPP_LONG_USAGE: u16 = 0x0002;

/// Device index for a device attached directly (cable or built-in dongle).
pub const DIRECT: u8 = 0xFF;

const REPORT_SHORT: u8 = 0x10;
const REPORT_LONG: u8 = 0x11;
const SHORT_LEN: usize = 7;
const LONG_LEN: usize = 20;
/// Payload bytes available in a long report.
pub const PAYLOAD_LEN: usize = LONG_LEN - 4;
/// Software id stamped on our requests so replies to other software
/// (such as G HUB, which may hold the same device open) are ignored.
const SW_ID: u8 = 0x0B;
/// Reader wake-up interval, used only to notice shutdown.
const READ_SLICE_MS: i32 = 500;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HID error: {0}")]
    Hid(#[from] hidapi::HidError),
    #[error("timed out waiting for the device to respond")]
    Timeout,
    #[error("the device was disconnected")]
    Disconnected,
    #[error("device rejected the request: {1} (HID++ 2.0 error {0:#04x})")]
    Hidpp20(u8, &'static str),
    #[error("receiver rejected the request (HID++ 1.0 error {0:#04x})")]
    Hidpp10(u8),
    #[error("feature {0:#06x} is not supported by this device")]
    Unsupported(u16),
    #[error("迈从接收器未返回鼠标身份（状态 0xFF）；请检查鼠标连接，原因尚未确认；未写入配置")]
    MchoseIdentityUnavailable,
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

fn hidpp20_error_name(code: u8) -> &'static str {
    match code {
        0x01 => "unknown error",
        0x02 => "invalid argument",
        0x03 => "value out of range",
        0x04 => "hardware error",
        0x05 => "internal error",
        0x06 => "invalid feature index",
        0x07 => "invalid function",
        0x08 => "device busy",
        0x09 => "unsupported",
        _ => "unrecognised error",
    }
}

/// Receives [`Event`]s from the readers; called on reader threads.
pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

/// Something a device said without being asked, or a lost connection.
/// `path` is the long collection's path, which identifies the transport.
#[derive(Debug)]
pub enum Event {
    Notification { path: CString, report: Vec<u8> },
    DeviceChanged { path: CString, index: u8 },
    Gone,
}

/// The one outstanding request: bytes 1..=3 its reply must echo.
#[derive(Default)]
struct Slot {
    key: Option<[u8; 3]>,
    reply: Option<Result<Vec<u8>>>,
}

struct Shared {
    slot: Mutex<Slot>,
    reply_ready: Condvar,
    alive: AtomicBool,
}

impl Shared {
    /// Hands `report` to the waiting request if it is its reply.
    fn deliver(&self, r: &[u8]) -> bool {
        let Ok(mut slot) = self.slot.lock() else { return false };
        let Some([device, b2, b3]) = slot.key else { return false };
        if r.len() < 4 || r[1] != device || !matches!(r[0], REPORT_SHORT | REPORT_LONG) {
            return false;
        }
        let matching = r[2] == b2 && r[3] == b3;
        let matching_error = r.len() >= 5 && matches!(r[2], 0xFF | 0x8F) && r[3] == b2 && r[4] == b3;
        if !matching && !matching_error { return false; }
        let expected_len = if r[0] == REPORT_SHORT { SHORT_LEN } else { LONG_LEN };
        let reply = if r.len() != expected_len {
            Err(Error::Other(format!("Malformed HID++ reply: report {:02X} has {} bytes, expected {expected_len}", r[0], r.len())))
        } else if matching {
            Ok(r[4..].to_vec())
        } else if r[2] == 0xFF && r[3] == b2 && r[4] == b3 {
            Err(Error::Hidpp20(r[5], hidpp20_error_name(r[5])))
        } else if r[2] == 0x8F && r[3] == b2 && r[4] == b3 {
            Err(Error::Hidpp10(r[5]))
        } else {
            return false;
        };
        slot.key = None;
        slot.reply = Some(reply);
        self.reply_ready.notify_all();
        true
    }

    fn fail(&self) {
        self.alive.store(false, Ordering::SeqCst);
        if let Ok(mut slot) = self.slot.lock() {
            if slot.key.take().is_some() {
                slot.reply = Some(Err(Error::Disconnected));
            }
        }
        self.reply_ready.notify_all();
    }
}

/// Unsolicited traffic worth forwarding: HID++ 1.0 notifications (sub id
/// below 0x80) and HID++ 2.0 feature events (software id 0). Replies meant
/// for other software carry their own non-zero software id and are dropped.
fn is_notification(r: &[u8]) -> bool {
    if r.len() < 4 || r[2] >= 0x80 {
        return false;
    }
    match r[0] {
        REPORT_SHORT => true,
        REPORT_LONG => r[3] & 0x0F == 0,
        _ => false,
    }
}

fn spawn_reader(dev: HidDevice, path: CString, shared: Arc<Shared>, events: EventSink) {
    let spawned = std::thread::Builder::new().name("hidpp-reader".into()).spawn(move || {
        let mut buf = [0u8; 64];
        while shared.alive.load(Ordering::SeqCst) {
            match dev.read_timeout(&mut buf, READ_SLICE_MS) {
                Ok(0) => {}
                Ok(n) => {
                    let report = &buf[..n];
                    if !shared.deliver(report) && is_notification(report) {
                        events(Event::Notification { path: path.clone(), report: report.to_vec() });
                    }
                }
                Err(_) => {
                    // Unplugged (or the handle broke): wake any waiter and report it once.
                    if shared.alive.load(Ordering::SeqCst) {
                        shared.fail();
                        events(Event::Gone);
                    }
                    break;
                }
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("could not start HID reader: {e}");
    }
}

/// An open HID++ device or receiver. Devices behind a receiver share the
/// receiver's transport and are addressed by device index.
pub struct Transport {
    mchose: Option<crate::mchose::Transport>,
    long: HidDevice,
    short: Option<HidDevice>,
    shared: Arc<Shared>,
}

impl Transport {
    /// Opens the long collection (and the short one, if given) for writing,
    /// plus separate read handles served by background readers.
    pub fn open(api: &HidApi, long_path: &CStr, short_path: Option<&CStr>, events: EventSink) -> Result<Self> {
        let shared = Arc::new(Shared {
            slot: Mutex::new(Slot::default()),
            reply_ready: Condvar::new(),
            alive: AtomicBool::new(true),
        });
        let long = api.open_path(long_path)?;
        spawn_reader(api.open_path(long_path)?, long_path.to_owned(), shared.clone(), events.clone());
        let short = match short_path {
            Some(path) => match (api.open_path(path), api.open_path(path)) {
                (Ok(writer), Ok(reader)) => {
                    spawn_reader(reader, long_path.to_owned(), shared.clone(), events);
                    Some(writer)
                }
                _ => None, // short reports are an optimisation; long ones still work
            },
            None => None,
        };
        Ok(Self { long, short, shared, mchose: None })
    }

    pub fn open_mchose(api: &HidApi, path: &CStr, events: EventSink) -> Result<Self> {
        let native=crate::mchose::Transport::open(api,path,events)?;
        Ok(Self { long:api.open_path(path)?,short:None,mchose:Some(native),shared:Arc::new(Shared {slot:Mutex::new(Slot::default()),reply_ready:Condvar::new(),alive:AtomicBool::new(true)}) })
    }

    pub fn mchose(&self) -> Option<&crate::mchose::Transport> { self.mchose.as_ref() }

    pub fn is_alive(&self) -> bool {
        if let Some(t)=&self.mchose {return t.is_alive()}
        self.shared.alive.load(Ordering::SeqCst)
    }

    /// Writes `packet` and waits for the reply echoing `key` (bytes 1..=3).
    fn exchange(&self, dev: &HidDevice, key: [u8; 3], packet: &[u8], timeout: Duration) -> Result<Vec<u8>> {
        if !self.is_alive() {
            return Err(Error::Disconnected);
        }
        let lock = || self.shared.slot.lock().map_err(|_| Error::Other("HID reply lock poisoned".into()));
        {
            let mut slot = lock()?;
            slot.key = Some(key);
            slot.reply = None;
        }
        match dev.write(packet) {
            Ok(n) if n != packet.len() => {
                lock()?.key = None;
                return Err(Error::Other("short HID++ write; request was not retried".into()));
            }
            Err(e) => { lock()?.key = None; return Err(e.into()); }
            _ => {}
        }
        let deadline = Instant::now() + timeout;
        let mut slot = lock()?;
        loop {
            if let Some(reply) = slot.reply.take() {
                return reply;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                slot.key = None;
                return Err(Error::Timeout);
            }
            slot = self
                .shared
                .reply_ready
                .wait_timeout(slot, left)
                .map_err(|_| Error::Other("HID reply lock poisoned".into()))?
                .0;
        }
    }

    /// Compatibility interface: valid short replies retain the historical
    /// 16-byte padding. Truncated/malformed reports are rejected before this.
    pub fn request(&self, device:u8, feature_index:u8, function:u8, params:&[u8], timeout:Duration)->Result<Vec<u8>> {
        let mut reply=self.request_unpadded(device,feature_index,function,params,timeout)?;
        reply.resize(PAYLOAD_LEN,0);
        Ok(reply)
    }
    /// Preserve the actual protocol payload (3 bytes short / 16 bytes long).
    pub fn request_unpadded(&self,device:u8,feature_index:u8,function:u8,params:&[u8],timeout:Duration)->Result<Vec<u8>> {
        if self.mchose.is_some() {return Err(Error::Other("HID++ requests cannot be sent to a MCHOSE device".into()))}
        let packet=feature_packet(device,feature_index,function,params)?;
        self.exchange(&self.long,[device,feature_index,packet[3]],&packet,timeout)
    }

    /// HID++ 1.0 register access on a receiver (sub id 0x80 set / 0x81 get),
    /// sent as a short report. Returns the 3 register bytes of the reply.
    pub fn register(&self, sub_id: u8, address: u8, params: [u8; 3], timeout: Duration) -> Result<[u8; 3]> {
        let short = self.short.as_ref().ok_or_else(|| Error::Other("no short-report collection".into()))?;
        let packet = [REPORT_SHORT, DIRECT, sub_id, address, params[0], params[1], params[2]];
        let r = self.exchange(short, [DIRECT, sub_id, address], &packet, timeout)?;
        Ok([r[0], r[1], r[2]])
    }
}

/// Compile a complete long report without silently truncating a request.
fn feature_packet(device:u8,feature_index:u8,function:u8,params:&[u8])->Result<[u8;LONG_LEN]> {
    if function>15 || params.len()>PAYLOAD_LEN {return Err(Error::Other("invalid HID++ function or payload length".into()));}
    let mut packet=[0u8;LONG_LEN];
    packet[..4].copy_from_slice(&[REPORT_LONG,device,feature_index,(function<<4)|SW_ID]);
    packet[4..4+params.len()].copy_from_slice(params);
    Ok(packet)
}

/// HID++ feature requests used by on-board operations. The production
/// implementation keeps the same device handle and existing reply routing;
/// tests can execute the complete flash transaction against simulated memory.
pub trait FeatureRequest {
    fn request(&self,device:u8,feature_index:u8,function:u8,params:&[u8],timeout:Duration)->Result<Vec<u8>>;
    fn request_unpadded(&self,device:u8,feature_index:u8,function:u8,params:&[u8],timeout:Duration)->Result<Vec<u8>> {
        self.request(device,feature_index,function,params,timeout)
    }

}
impl FeatureRequest for Transport {
    fn request(&self,device:u8,feature_index:u8,function:u8,params:&[u8],timeout:Duration)->Result<Vec<u8>>{
        Transport::request(self,device,feature_index,function,params,timeout)
    }
    fn request_unpadded(&self,device:u8,feature_index:u8,function:u8,params:&[u8],timeout:Duration)->Result<Vec<u8>> {
        Transport::request_unpadded(self,device,feature_index,function,params,timeout)
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        // Readers notice within READ_SLICE_MS and exit, closing their handles.
        self.shared.alive.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared_waiting_for(key: [u8; 3]) -> Shared {
        let shared = Shared { slot: Mutex::new(Slot::default()), reply_ready: Condvar::new(), alive: AtomicBool::new(true) };
        shared.slot.lock().unwrap().key = Some(key);
        shared
    }

    #[test]
    fn delivers_matching_reply_and_errors() {
        let s = shared_waiting_for([0x01, 0x05, 0x2B]);
        assert!(!s.deliver(&[0x11, 0x02, 0x05, 0x2B, 0, 0, 0])); // other device
        let mut response=[0u8;LONG_LEN];response[..5].copy_from_slice(&[0x11,0x01,0x05,0x2B,0xAA]);
        assert!(s.deliver(&response));
        assert_eq!(s.slot.lock().unwrap().reply.take().unwrap().unwrap()[0], 0xAA);

        let s = shared_waiting_for([0x01, 0x05, 0x2B]);
        assert!(s.deliver(&[0x10, 0x01, 0xFF, 0x05, 0x2B, 0x07, 0]));
        assert!(matches!(s.slot.lock().unwrap().reply.take(), Some(Err(Error::Hidpp20(0x07, _)))));

        let s = shared_waiting_for([0x03, 0x00, 0x1B]);
        assert!(s.deliver(&[0x10, 0x03, 0x8F, 0x00, 0x1B, 0x09, 0]));
        assert!(matches!(s.slot.lock().unwrap().reply.take(), Some(Err(Error::Hidpp10(0x09)))));
    }

    #[test]
    fn classifies_notifications() {
        assert!(is_notification(&[0x10, 0x01, 0x41, 0x04, 0x00, 0x93, 0x40])); // device connection
        assert!(is_notification(&[0x11, 0x01, 0x04, 0x00, 0x1A])); // feature event, sw id 0
        assert!(!is_notification(&[0x11, 0x01, 0x04, 0x1A, 0x1A])); // reply to other software
        assert!(!is_notification(&[0x10, 0xFF, 0x81, 0x00, 0, 0, 0])); // register reply
    }
    #[test]
    fn raw_reply_routing_never_pads_a_short_payload_or_accepts_a_truncated_long_frame() {
        let s=shared_waiting_for([1,5,0x2b]);
        assert!(s.deliver(&[REPORT_SHORT,1,5,0x2b,0xaa,0xbb,0xcc]));
        assert_eq!(s.slot.lock().unwrap().reply.take().unwrap().unwrap(),vec![0xaa,0xbb,0xcc]);
        for (tag,size) in [(REPORT_LONG,4),(REPORT_LONG,7),(REPORT_LONG,19),(REPORT_LONG,21),(REPORT_SHORT,6),(REPORT_SHORT,8)] {
            let s=shared_waiting_for([1,5,0x2b]);let mut r=vec![0;size];r[..4].copy_from_slice(&[tag,1,5,0x2b]);
            assert!(s.deliver(&r));assert!(matches!(s.slot.lock().unwrap().reply.take(),Some(Err(Error::Other(_)))));assert!(s.slot.lock().unwrap().key.is_none());
        }
    }
    #[test]
    fn unrelated_or_unknown_reports_do_not_consume_pending_strict_requests() {
        let s=shared_waiting_for([1,5,0x2b]);
        for r in [vec![0x12,1,5,0x2b,0,0,0],vec![REPORT_LONG,2,5,0x2b,0,0,0],vec![REPORT_LONG,1,6,0x2b,0,0,0],vec![REPORT_SHORT,1,0xff,6,0x2b,7,0]]{assert!(!s.deliver(&r));assert_eq!(s.slot.lock().unwrap().key,Some([1,5,0x2b]));}
    }
    #[test]
    fn complete_feature_packets_reject_overflow_instead_of_truncating_data() {
        let r=feature_packet(0xff,0x24,3,&[1,2,3]).unwrap();assert_eq!(&r[..7],&[0x11,0xff,0x24,0x3b,1,2,3]);assert!(r[7..].iter().all(|v|*v==0));
        assert!(feature_packet(1,5,16,&[]).is_err());assert!(feature_packet(1,5,1,&[0;17]).is_err());assert_eq!(feature_packet(1,5,15,&[0xa5;16]).unwrap()[19],0xa5);
    }

}
