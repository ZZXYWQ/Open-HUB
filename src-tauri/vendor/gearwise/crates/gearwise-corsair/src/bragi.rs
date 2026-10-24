//! Bragi protocol client: framing, properties, handles and bulk writes.
//!
//! Every output report is `[0x00 report id][0x08 | sub-device][command]
//! [arguments...]`, zero padded to the model's packet size. The device echoes
//! `[0x08 | sub-device][command][status][data...]`. Status 0 is success.

use gearwise_core::error::{Error, Result};
use gearwise_core::transport::SharedTransport;

/// First byte of every packet; the low three bits select a receiver sub-device.
const MAGIC: u8 = 0x08;

mod cmd {
    pub const SET: u8 = 0x01;
    pub const GET: u8 = 0x02;
    pub const CLOSE: u8 = 0x05;
    pub const WRITE: u8 = 0x06;
    pub const CONTINUE: u8 = 0x07;
    pub const OPEN: u8 = 0x0d;
    pub const PING: u8 = 0x12;
}

/// Property ids for get/set.
pub(crate) mod prop {
    pub const POLL_RATE: u8 = 0x01;
    pub const BRIGHTNESS: u8 = 0x02;
    pub const MODE: u8 = 0x03;
    pub const LIFT_OFF: u8 = 0x06;
    pub const ANGLE_SNAP: u8 = 0x07;
    pub const BATTERY_LEVEL: u8 = 0x0f;
    pub const BATTERY_STATUS: u8 = 0x10;
    pub const PID: u8 = 0x12;
    pub const FIRMWARE: u8 = 0x13;
    pub const DPI: u8 = 0x20;
    pub const DPI_X: u8 = 0x21;
    pub const DPI_Y: u8 = 0x22;
    /// Receiver only: bit n set when sub-device n is connected.
    pub const SUBDEVICES: u8 = 0x36;
    pub const BRIGHTNESS_COARSE: u8 = 0x44;
    pub const MAX_POLL_RATE: u8 = 0x96;
}

pub(crate) const MODE_HARDWARE: u32 = 1;
pub(crate) const MODE_SOFTWARE: u32 = 2;

/// Lighting resource, RRR..GGG..BBB planes.
pub(crate) const RES_LIGHTING: u16 = 0x01;
/// Alternative lighting resource, `12 00` header and RGB triplets.
pub(crate) const RES_LIGHTING_ALT: u16 = 0x22;
pub(crate) const LIGHTING_HANDLE: u8 = 0;

/// Handle-open status: handle already in use.
pub(crate) const STATUS_BUSY: u8 = 0x03;
/// Handle-open status: invalid value / resource not supported on this device.
pub(crate) const STATUS_INVALID: u8 = 0x01;
pub(crate) const STATUS_NO_RESOURCE: u8 = 0x06;
const STATUS_UNSUPPORTED: u8 = 0x05;

const REPLY_TIMEOUT_MS: i32 = 250;
/// Continuation packets are not acknowledged by every model.
const OPTIONAL_REPLY_MS: i32 = 15;
/// Unrelated reports tolerated while waiting for a reply.
const MAX_SKIPPED: usize = 16;

/// Packet header of a first write packet: magic, command, handle, u32 length.
const WRITE_HEADER: usize = 7;
/// Packet header of a continuation: magic, command, handle.
const CONTINUE_HEADER: usize = 3;

/// A conversation with one Bragi device (or a receiver itself with `id` 0).
#[derive(Clone)]
pub(crate) struct Link {
    transport: SharedTransport,
    /// Receiver sub-device index (1..=7), 0 for the device behind the interface.
    id: u8,
    packet: usize,
}

impl Link {
    pub(crate) fn new(transport: SharedTransport, id: u8, packet: usize) -> Self {
        Self {
            transport,
            id,
            packet,
        }
    }

    /// The same transport addressed to receiver sub-device `id`.
    pub(crate) fn sub_device(&self, id: u8) -> Self {
        Self {
            transport: self.transport.clone(),
            id,
            packet: self.packet,
        }
    }

    fn magic(&self) -> u8 {
        MAGIC | self.id
    }

    fn frame(&self, body: &[u8]) -> Vec<u8> {
        debug_assert!(body.len() < self.packet);
        let mut out = vec![0u8; 1 + self.packet];
        out[1] = self.magic();
        out[2..2 + body.len()].copy_from_slice(body);
        out
    }

    /// Next report that answers `command` from this device, or `None` when
    /// the device stays silent for `timeout_ms`.
    fn read_reply(
        &self,
        t: &mut dyn gearwise_core::HidTransport,
        command: u8,
        timeout_ms: i32,
    ) -> Result<Option<Vec<u8>>> {
        let mut buf = vec![0u8; self.packet];
        for _ in 0..MAX_SKIPPED {
            let n = t.read_timeout(&mut buf, timeout_ms)?;
            if n == 0 {
                return Ok(None);
            }
            if n >= 3 && buf[0] & 0x07 == self.id && buf[1] == command {
                return Ok(Some(buf[..n].to_vec()));
            }
        }
        Ok(None)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Box<dyn gearwise_core::HidTransport>>> {
        self.transport
            .lock()
            .map_err(|_| Error::transport("transport lock poisoned"))
    }

    /// Discard reports left over from earlier exchanges.
    fn drain(t: &mut dyn gearwise_core::HidTransport) -> Result<()> {
        let mut buf = [0u8; 1024];
        for _ in 0..MAX_SKIPPED {
            if t.read_timeout(&mut buf, 0)? == 0 {
                break;
            }
        }
        Ok(())
    }

    /// Send a command and return the full reply: `[magic, command, status, data...]`.
    fn exchange(&self, body: &[u8]) -> Result<Vec<u8>> {
        let mut guard = self.lock()?;
        let t: &mut dyn gearwise_core::HidTransport = &mut **guard;
        Self::drain(t)?;
        t.write(&self.frame(body))?;
        self.read_reply(t, body[0], REPLY_TIMEOUT_MS)?
            .ok_or(Error::Timeout)
    }

    /// Send a command whose acknowledgement is optional. A reply carrying an
    /// error status is still reported.
    fn send(&self, body: &[u8]) -> Result<()> {
        let mut guard = self.lock()?;
        let t: &mut dyn gearwise_core::HidTransport = &mut **guard;
        t.write(&self.frame(body))?;
        match self.read_reply(t, body[0], OPTIONAL_REPLY_MS)? {
            Some(reply) => check(reply[2], "write"),
            None => Ok(()),
        }
    }

    /// Reply data of a property read (the bytes after the status).
    pub(crate) fn get_raw(&self, property: u8) -> Result<Vec<u8>> {
        let reply = self.exchange(&[cmd::GET, property, 0])?;
        check(reply[2], "get property")?;
        Ok(reply[3..].to_vec())
    }

    /// Property value as a little-endian integer (up to 32 bits).
    pub(crate) fn get(&self, property: u8) -> Result<u32> {
        let data = self.get_raw(property)?;
        let mut v = [0u8; 4];
        let n = data.len().min(4);
        v[..n].copy_from_slice(&data[..n]);
        Ok(u32::from_le_bytes(v))
    }

    /// Write a property and return the device's status byte.
    pub(crate) fn set_status(&self, property: u8, value: u32) -> Result<u8> {
        let v = value.to_le_bytes();
        let reply = self.exchange(&[cmd::SET, property, 0, v[0], v[1], v[2]])?;
        Ok(reply[2])
    }

    pub(crate) fn set(&self, property: u8, value: u32) -> Result<()> {
        check(self.set_status(property, value)?, "set property")
    }

    /// Open `resource` on `handle`; returns the status for the caller to judge.
    pub(crate) fn open(&self, handle: u8, resource: u16) -> Result<u8> {
        let r = resource.to_le_bytes();
        Ok(self.exchange(&[cmd::OPEN, handle, r[0], r[1]])?[2])
    }

    pub(crate) fn close(&self, handle: u8) -> Result<()> {
        // Argument 1 is the number of handles being closed.
        check(self.exchange(&[cmd::CLOSE, 1, handle])?[2], "close handle")
    }

    /// Write `data` to an open handle, split over as many packets as needed.
    pub(crate) fn write(&self, handle: u8, data: &[u8]) -> Result<()> {
        let first = data.len().min(self.packet - WRITE_HEADER);
        let mut body = vec![cmd::WRITE, handle];
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(&data[..first]);
        check(self.exchange(&body)?[2], "write")?;
        for chunk in data[first..].chunks(self.packet - CONTINUE_HEADER) {
            let mut body = vec![cmd::CONTINUE, handle];
            body.extend_from_slice(chunk);
            self.send(&body)?;
        }
        Ok(())
    }

    /// Keep-alive: devices in software mode fall back to hardware mode when
    /// the host goes quiet.
    pub(crate) fn ping(&self) -> Result<()> {
        check(self.exchange(&[cmd::PING])?[2], "ping")
    }
}

/// Turn a status byte into a result.
pub(crate) fn check(status: u8, what: &str) -> Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(status_error(status, what))
    }
}

/// The error for a non-zero status byte.
pub(crate) fn status_error(status: u8, what: &str) -> Error {
    let reason = match status {
        STATUS_UNSUPPORTED => return Error::Unsupported("not implemented by firmware"),
        1 => "invalid value",
        2 => "packet too long",
        3 => "operation failed",
        6 => "handle not open",
        9 => "read only",
        _ => "error",
    };
    Error::protocol(format!("{what}: {reason} (status 0x{status:02x})"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeBragi;

    #[test]
    fn property_set_and_get_frames() {
        let fake = FakeBragi::new(64);
        fake.set_prop(prop::POLL_RATE, 0);
        let link = fake.link(0);
        link.set(prop::POLL_RATE, 3).unwrap();
        assert_eq!(
            fake.writes()[0][..8],
            [0x00, 0x08, 0x01, 0x01, 0x00, 0x03, 0x00, 0x00]
        );
        assert_eq!(fake.writes()[0].len(), 65);
        assert_eq!(link.get(prop::POLL_RATE).unwrap(), 3);
        assert_eq!(fake.writes()[1][..5], [0x00, 0x08, 0x02, 0x01, 0x00]);
    }

    #[test]
    fn sub_device_ids_go_into_the_magic_byte() {
        let fake = FakeBragi::new(64);
        fake.set_prop(prop::PID, 0x1b4c);
        let link = fake.link(0).sub_device(2);
        assert_eq!(link.get(prop::PID).unwrap(), 0x1b4c);
        assert_eq!(fake.writes()[0][1], 0x0a);
    }

    #[test]
    fn error_statuses_are_reported() {
        let fake = FakeBragi::new(64);
        fake.fail_prop(prop::DPI_X, 1);
        fake.fail_prop(prop::DPI_Y, STATUS_UNSUPPORTED);
        let link = fake.link(0);
        assert!(
            matches!(link.set(prop::DPI_X, 99_999), Err(Error::Protocol(m)) if m.contains("invalid value"))
        );
        assert!(matches!(link.get(prop::DPI_Y), Err(Error::Unsupported(_))));
        assert_eq!(link.set_status(prop::DPI_X, 1).unwrap(), 1);
    }

    #[test]
    fn silent_device_times_out() {
        let fake = FakeBragi::new(64);
        fake.mute();
        assert!(matches!(fake.link(0).get(prop::PID), Err(Error::Timeout)));
    }

    #[test]
    fn unrelated_reports_are_skipped() {
        let fake = FakeBragi::new(64);
        fake.set_prop(prop::PID, 0x1b93);
        // A report for another sub-device and one for another command precede the real reply.
        fake.queue_noise(vec![0x0b, 0x02, 0x00, 0x34, 0x12]);
        fake.queue_noise(vec![0x08, 0x01, 0x00]);
        let link = fake.link(0);
        assert_eq!(link.get(prop::PID).unwrap(), 0x1b93);
    }

    #[test]
    fn write_splits_into_a_header_packet_and_continuations() {
        let fake = FakeBragi::new(64);
        let link = fake.link(0);
        let data: Vec<u8> = (0..130u32).map(|i| i as u8).collect();
        assert!(
            matches!(link.write(0, &data), Err(Error::Protocol(m)) if m.contains("operation failed")),
            "handle not opened yet"
        );
        link.open(0, RES_LIGHTING).unwrap();
        let first = fake.writes().len();
        link.write(0, &data).unwrap();
        let w = fake.writes()[first..].to_vec();
        assert_eq!(w.len(), 3);
        // 57 bytes in the first packet, 61 in each continuation.
        assert_eq!(w[0][..8], [0x00, 0x08, 0x06, 0x00, 130, 0, 0, 0]);
        assert_eq!(w[0][8..65], data[..57]);
        assert_eq!(w[1][..4], [0x00, 0x08, 0x07, 0x00]);
        assert_eq!(w[1][4..65], data[57..118]);
        assert_eq!(w[2][..4], [0x00, 0x08, 0x07, 0x00]);
        assert_eq!(w[2][4..16], data[118..]);
        assert!(w[2][16..].iter().all(|b| *b == 0));
        assert_eq!(fake.written(0), data);
    }

    #[test]
    fn handle_open_and_close_frames() {
        let fake = FakeBragi::new(64);
        let link = fake.link(0);
        assert_eq!(link.open(0, RES_LIGHTING).unwrap(), 0);
        assert_eq!(fake.writes()[0][..6], [0x00, 0x08, 0x0d, 0x00, 0x01, 0x00]);
        link.close(0).unwrap();
        assert_eq!(fake.writes()[1][..5], [0x00, 0x08, 0x05, 0x01, 0x00]);
    }
}
