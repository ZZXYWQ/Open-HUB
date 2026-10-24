//! Request/response exchange over a HID feature-report pipe.
//!
//! Razer devices answer a SET_FEATURE with a status that must be fetched with
//! GET_FEATURE after a short settle time. Slow devices (wireless receivers)
//! answer "busy" until they have relayed the command, so the exchange is
//! retried a bounded number of times.

use std::thread;
use std::time::Duration;

use gearwise_core::transport::HidTransport;
use gearwise_core::{Error, Result};

use crate::report::{Report, Status, WIRE_LEN};

/// How long a device needs before its reply is ready, as observed by the
/// openrazer kernel driver (0.6 ms, 31 ms and 59.9 ms respectively).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settle {
    /// Wired devices answer almost immediately.
    Fast,
    /// Newer wireless-capable mice (HyperSpeed generation).
    Medium,
    /// First-generation wireless-capable mice.
    Slow,
}

/// Timing of one exchange.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// Delay between sending a command and reading its reply.
    pub settle: Duration,
    /// Extra delay before re-sending after a busy or mismatched reply.
    pub retry_gap: Duration,
    /// Total tries, including the first.
    pub attempts: u8,
}

impl Timing {
    pub const fn new(settle: Duration, retry_gap: Duration, attempts: u8) -> Self {
        Self {
            settle,
            retry_gap,
            attempts,
        }
    }

    /// Timing for a device class. The worst case stays near 100 ms: fewer
    /// attempts are made when each one has to wait longer.
    pub const fn for_settle(s: Settle) -> Self {
        let gap = Duration::from_millis(5);
        match s {
            Settle::Fast => Self::new(Duration::from_millis(2), gap, 5),
            Settle::Medium => Self::new(Duration::from_millis(31), gap, 3),
            Settle::Slow => Self::new(Duration::from_millis(60), gap, 2),
        }
    }
}

/// A transport plus the per-model transaction id and timing.
pub struct Link {
    io: Box<dyn HidTransport>,
    transaction_id: u8,
    timing: Timing,
}

impl Link {
    pub fn new(io: Box<dyn HidTransport>, transaction_id: u8, timing: Timing) -> Self {
        Self {
            io,
            transaction_id,
            timing,
        }
    }

    /// Send `req` and return the device's successful reply.
    ///
    /// Busy/not-yet-started replies and replies to some other command are
    /// retried; the device's own error codes map to [`Error`] variants:
    /// failure -> `Protocol`, not supported -> `Unsupported`, timeout ->
    /// `Timeout`.
    pub fn exec(&mut self, req: Report) -> Result<Report> {
        self.exec_with_id(req, self.transaction_id)
    }

    /// The exact Viper V3 Pro button command uses a different transaction id.
    pub(crate) fn exec_with_id(&mut self, mut req: Report, transaction_id: u8) -> Result<Report> {
        req.transaction_id = transaction_id;
        let wire = req.to_wire();
        let mut last = Error::Timeout;

        for attempt in 0..self.timing.attempts.max(1) {
            if attempt > 0 {
                thread::sleep(self.timing.retry_gap);
            }
            self.io.send_feature_report(&wire)?;
            thread::sleep(self.timing.settle);

            let mut buf = [0u8; WIRE_LEN];
            let n = self.io.get_feature_report(&mut buf)?;
            let resp = Report::from_wire(&buf[..n])?;

            if (resp.class, resp.command) != (req.class, req.command) {
                log::debug!(
                    "razer: reply {:02x}:{:02x} does not answer {:02x}:{:02x}",
                    resp.class,
                    resp.command,
                    req.class,
                    req.command
                );
                last = Error::protocol(format!(
                    "reply to {:02x}:{:02x} was for {:02x}:{:02x}",
                    req.class, req.command, resp.class, resp.command
                ));
                continue;
            }
            match resp.status() {
                Status::Success => return Ok(resp),
                Status::New | Status::Busy => {
                    log::debug!(
                        "razer: {:02x}:{:02x} still busy (try {})",
                        req.class,
                        req.command,
                        attempt + 1
                    );
                    last = Error::Timeout;
                }
                Status::Failure => {
                    return Err(Error::protocol(format!(
                        "command {:02x}:{:02x} failed",
                        req.class, req.command
                    )));
                }
                Status::NotSupported => {
                    return Err(Error::Unsupported(
                        "command rejected by the device firmware",
                    ));
                }
                Status::Timeout => return Err(Error::Timeout),
                Status::Unknown(s) => {
                    return Err(Error::protocol(format!(
                        "command {:02x}:{:02x}: unknown status {s:#04x}",
                        req.class, req.command
                    )));
                }
            }
        }
        Err(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{Fake, INSTANT, sent};

    fn dpi_request() -> Report {
        Report::new(0x04, 0x85, 0x07, &[0])
    }

    fn link(
        fake: &Fake,
        tid: u8,
    ) -> (
        Link,
        std::sync::Arc<std::sync::Mutex<Vec<gearwise_core::transport::MockRecord>>>,
    ) {
        let (io, log) = fake.transport();
        (Link::new(Box::new(io), tid, INSTANT), log)
    }

    #[test]
    fn busy_then_success_is_retried() {
        let fake =
            Fake::default()
                .on(0x04, 0x85, 1, &[])
                .on(0x04, 0x85, 2, &[0, 0x03, 0x20, 0x03, 0x20]);
        let (mut l, log) = link(&fake, 0x1f);
        let resp = l.exec(dpi_request()).unwrap();
        assert_eq!(&resp.args[1..5], &[0x03, 0x20, 0x03, 0x20]);
        let reqs = sent(&log);
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0], reqs[1], "the same request is re-sent");
        assert_eq!(reqs[0][2], 0x1f, "model transaction id is stamped");
    }

    #[test]
    fn a_reply_that_never_leaves_busy_times_out_after_the_attempt_limit() {
        let fake = Fake::default().on(0x04, 0x85, 1, &[]);
        let (mut l, log) = link(&fake, 0x1f);
        assert!(matches!(l.exec(dpi_request()), Err(Error::Timeout)));
        assert_eq!(sent(&log).len(), 5);
    }

    #[test]
    fn status_new_counts_as_not_ready() {
        let fake = Fake::default()
            .on(0x04, 0x85, 0, &[])
            .on(0x04, 0x85, 2, &[]);
        let (mut l, log) = link(&fake, 0x1f);
        assert!(l.exec(dpi_request()).is_ok());
        assert_eq!(sent(&log).len(), 2);
    }

    #[test]
    fn device_error_codes_map_to_error_variants_without_retrying() {
        for (status, check) in [
            (
                3u8,
                (|e: &Error| matches!(e, Error::Protocol(m) if m.contains("04:85")))
                    as fn(&Error) -> bool,
            ),
            (4, |e| matches!(e, Error::Timeout)),
            (5, |e| matches!(e, Error::Unsupported(_))),
            (9, |e| matches!(e, Error::Protocol(m) if m.contains("0x09"))),
        ] {
            let fake = Fake::default().on(0x04, 0x85, status, &[]);
            let (mut l, log) = link(&fake, 0x1f);
            let err = l.exec(dpi_request()).unwrap_err();
            assert!(check(&err), "status {status}: {err:?}");
            assert_eq!(sent(&log).len(), 1, "status {status} must not be retried");
        }
    }

    #[test]
    fn a_reply_for_another_command_is_not_accepted() {
        // The scripted device answers 04:85 with a reply that claims to be for 07:80.
        let (io, _) = {
            let t = gearwise_core::transport::MockTransport::new(|op| match op {
                gearwise_core::transport::MockOp::SetFeature(d) => {
                    let mut r = Report::from_wire(d).unwrap();
                    r.status = 2;
                    r.class = 0x07;
                    r.command = 0x80;
                    gearwise_core::transport::MockReply::feature(r.to_wire().to_vec())
                }
                _ => gearwise_core::transport::MockReply::none(),
            });
            let log = t.log();
            (t, log)
        };
        let mut l = Link::new(Box::new(io), 0x1f, INSTANT);
        assert!(matches!(l.exec(dpi_request()), Err(Error::Protocol(m)) if m.contains("04:85")));
    }

    #[test]
    fn timing_stays_near_a_hundred_milliseconds_in_the_worst_case() {
        for s in [Settle::Fast, Settle::Medium, Settle::Slow] {
            let t = Timing::for_settle(s);
            let worst = t.settle * t.attempts as u32 + t.retry_gap * (t.attempts as u32 - 1);
            assert!(worst <= Duration::from_millis(130), "{s:?}: {worst:?}");
        }
    }
}
