//! A scripted Razer firmware for tests.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gearwise_core::transport::{MockOp, MockRecord, MockReply, MockTransport};

use crate::link::Timing;
use crate::report::{MAX_ARGS, Report};

/// No sleeping, five tries.
pub const INSTANT: Timing = Timing::new(Duration::ZERO, Duration::ZERO, 5);

type Script = HashMap<(u8, u8), VecDeque<(u8, Vec<u8>)>>;

/// Answers each (class, command) from a queue of (status, arguments); the
/// last entry repeats. Commands without a script succeed with empty arguments.
#[derive(Clone, Default)]
pub struct Fake {
    script: Arc<Mutex<Script>>,
}

impl Fake {
    pub fn on(self, class: u8, command: u8, status: u8, args: &[u8]) -> Self {
        self.script
            .lock()
            .unwrap()
            .entry((class, command))
            .or_default()
            .push_back((status, args.to_vec()));
        self
    }

    /// A transport playing this firmware, and its operation log.
    pub fn transport(&self) -> (MockTransport, Arc<Mutex<Vec<MockRecord>>>) {
        let script = Arc::clone(&self.script);
        let t = MockTransport::new(move |op| match op {
            MockOp::SetFeature(data) => {
                let req = Report::from_wire(data).expect("driver sent a malformed report");
                let (status, args) = {
                    let mut s = script.lock().unwrap();
                    match s.get_mut(&(req.class, req.command)) {
                        Some(q) if q.len() > 1 => q.pop_front().unwrap(),
                        Some(q) => q.front().cloned().unwrap(),
                        None => (2, vec![]),
                    }
                };
                let mut resp = req;
                resp.status = status;
                resp.args = [0; MAX_ARGS];
                resp.args[..args.len()].copy_from_slice(&args);
                MockReply::feature(resp.to_wire().to_vec())
            }
            MockOp::Write(_) => MockReply::none(),
        });
        let log = t.log();
        (t, log)
    }
}

/// The requests (without the leading report id) the driver sent, in order.
pub fn sent(log: &Arc<Mutex<Vec<MockRecord>>>) -> Vec<Vec<u8>> {
    log.lock()
        .unwrap()
        .iter()
        .filter_map(|r| match r {
            MockRecord::SetFeature(d) => Some(d.clone()),
            _ => None,
        })
        .collect()
}
