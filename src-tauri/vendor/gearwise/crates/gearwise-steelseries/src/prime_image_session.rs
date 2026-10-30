//! Scoped repeat-equal macro-image collection over the pinned output reader.
//! Caller metadata and observations are not physical identity, atomicity,
//! continuous idle-input or write-authorization proof. No service is enabled.
use crate::{
    prime_feature_read::Read,
    prime_macro_storage::{Image, OBSERVED_BYTES},
    prime_output_read::Reader,
};
use gearwise_core::{
    Error, Result,
    transport::{BusKind, HidInterfaceInfo, HidTransport},
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    path: String,
    serial: Option<String>,
    interface: i32,
    connection: String,
    operation: String,
}
impl Scope {
    pub fn new(info: &HidInterfaceInfo, connection: &str, operation: &str) -> Result<Self> {
        if info.vendor_id != 0x1038
            || info.product_id != 0x182e
            || !matches!(info.interface_number, -1 | 0)
            || info.usage_page != 0xffc0
            || info.usage != 1
            || info.bus != BusKind::Usb
            || info.path.is_empty()
            || info.path.len() > 4096
            || connection.is_empty()
            || connection.len() > 256
            || operation.is_empty()
            || operation.len() > 256
            || info.serial.as_ref().is_some_and(|s| s.len() > 256)
        {
            return Err(Error::invalid("Prime read scope/interface/operation"));
        }
        Ok(Self {
            path: info.path.clone(),
            serial: info.serial.clone(),
            interface: info.interface_number,
            connection: connection.into(),
            operation: operation.into(),
        })
    }
    pub fn connection(&self) -> &str {
        &self.connection
    }
    pub fn operation(&self) -> &str {
        &self.operation
    }
}
pub struct Status {
    pub scope: Scope,
    pub cancelled: bool,
}
fn bounded_output(frame: &[u8]) -> bool {
    if frame.len() != 65 || frame[0] != 0 {
        return false;
    }
    if frame[1] == 0x90 {
        return frame[2..].iter().all(|b| *b == 0);
    }
    if frame[1..3] != [0x83, 0] {
        return false;
    }
    let offset = usize::from(u16::from_le_bytes([frame[3], frame[4]]));
    let length = usize::from(u16::from_le_bytes([frame[5], frame[6]]));
    if offset == 0x92a && length == 32 {
        return frame[39..].iter().all(|b| *b == 0);
    }
    if length == 0 || length > 64 || frame[7..].iter().any(|b| *b != 0) {
        return false;
    }
    let address = 0x20000bd8 + offset;
    [
        (0x20001cb6, 30),
        (0x20001cd4, 10),
        (0x20001c00, 180),
        (0x20001e73, OBSERVED_BYTES),
    ]
    .iter()
    .any(|(base, len)| address >= *base && address + length <= base + len)
}
struct Scoped<T, F> {
    io: T,
    expected: Scope,
    check: F,
}
impl<T, F: FnMut() -> Result<Status>> Scoped<T, F> {
    fn verify(&mut self) -> Result<()> {
        let status = (self.check)()?;
        if status.cancelled || status.scope != self.expected {
            return Err(Error::protocol(
                "Prime read scope changed or operation cancelled",
            ));
        }
        Ok(())
    }
}
impl<T: HidTransport, F: FnMut() -> Result<Status> + Send> HidTransport for Scoped<T, F> {
    fn write(&mut self, frame: &[u8]) -> Result<usize> {
        self.verify()?;
        if !bounded_output(frame) {
            return Err(Error::invalid(
                "Prime session forbids this output command/range",
            ));
        }
        let result = self.io.write(frame);
        self.verify()?;
        result
    }
    fn read_timeout(&mut self, buf: &mut [u8], timeout: i32) -> Result<usize> {
        self.verify()?;
        let result = self.io.read_timeout(buf, timeout);
        self.verify()?;
        result
    }
    fn send_feature_report(&mut self, _: &[u8]) -> Result<()> {
        self.verify()?;
        Err(Error::Unsupported(
            "feature writes in read-only Prime session",
        ))
    }
    fn get_feature_report(&mut self, _: &mut [u8]) -> Result<usize> {
        self.verify()?;
        Err(Error::Unsupported(
            "unverified feature cache in Prime image session",
        ))
    }
}
#[derive(PartialEq, Eq)]
struct Pass {
    version: Vec<u8>,
    physical: Vec<u8>,
    wheel: Vec<u8>,
    contexts: Vec<u8>,
    memory: Vec<u8>,
}
pub struct ObservedImage {
    scope: Scope,
    image: Image,
    context_observations: usize,
}
impl ObservedImage {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn image(&self) -> &Image {
        &self.image
    }
    pub fn equal_passes(&self) -> usize {
        2
    }
    pub fn inactive_context_observations(&self) -> usize {
        self.context_observations
    }
    pub fn physical_identity_verified(&self) -> bool {
        false
    }
    pub fn atomic_image_verified(&self) -> bool {
        false
    }
}
pub struct Session<T: HidTransport, F: FnMut() -> Result<Status> + Send> {
    reader: Reader<Scoped<T, F>>,
    scope: Scope,
}
impl<T: HidTransport, F: FnMut() -> Result<Status> + Send> Session<T, F> {
    pub fn new(io: T, seed: [u8; 16], scope: Scope, check: F) -> Result<Self> {
        let mut transport = Scoped {
            io,
            expected: scope.clone(),
            check,
        };
        transport.verify()?;
        Ok(Self {
            reader: Reader::new(transport, seed)?,
            scope,
        })
    }
    fn read(&mut self, read: &Read) -> Result<Vec<u8>> {
        Ok(self.reader.read(read)?.bytes().to_vec())
    }
    fn contexts(&mut self) -> Result<Vec<u8>> {
        let bytes = self.read(&Read::contexts())?;
        if bytes.len() != 180
            || bytes
                .chunks_exact(60)
                .any(|c| c[13] != 255 || c[20..60].iter().any(|b| *b != 0))
        {
            return Err(Error::protocol(
                "Prime macro contexts are active or not reset",
            ));
        }
        Ok(bytes)
    }
    fn pass(&mut self) -> Result<Pass> {
        self.reader.transport_mut().verify()?;
        let version = self.read(&Read::version())?;
        let contexts = self.contexts()?;
        let physical = self.read(&Read::physical())?;
        let wheel = self.read(&Read::wheel())?;
        let mut memory = Vec::with_capacity(OBSERVED_BYTES);
        for offset in (0..OBSERVED_BYTES).step_by(512) {
            memory.extend(self.read(&Read::macro_chunk(
                offset,
                (OBSERVED_BYTES - offset).min(512),
            )?)?);
        }
        if self.read(&Read::physical())? != physical
            || self.read(&Read::wheel())? != wheel
            || self.contexts()? != contexts
        {
            return Err(Error::protocol(
                "Prime key tables or macro contexts changed during read",
            ));
        }
        self.reader.transport_mut().verify()?;
        Ok(Pass {
            version,
            physical,
            wheel,
            contexts,
            memory,
        })
    }
    /// Two equal bracketed observations, not a firmware freeze or input lock.
    /// Caller must revalidate this operation before publishing or later use.
    pub fn collect(&mut self) -> Result<ObservedImage> {
        let first = self.pass()?;
        let second = self.pass()?;
        if first != second {
            return Err(Error::protocol("Prime macro image observations differ"));
        }
        let image = Image::new(
            self.scope.connection(),
            &second.memory,
            &second.physical,
            &second.wheel,
        )?;
        self.reader.transport_mut().verify()?;
        Ok(ObservedImage {
            scope: self.scope.clone(),
            image,
            context_observations: 4,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        prime_macro::event::{Event, Page},
        prime_macro_block::binding,
        prime_macro_once,
    };
    use gearwise_core::transport::{MockOp, MockReply, MockTransport};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    fn info() -> HidInterfaceInfo {
        HidInterfaceInfo {
            path: "mock-prime-a".into(),
            vendor_id: 0x1038,
            product_id: 0x182e,
            interface_number: 0,
            usage_page: 0xffc0,
            usage: 1,
            serial: Some("mock-a".into()),
            ..Default::default()
        }
    }
    fn scope() -> Scope {
        Scope::new(&info(), "mock-open-1", "mock-read-1").unwrap()
    }
    fn valid_memory() -> Vec<u8> {
        let mut memory = vec![0x4a; 0x2800];
        let mut p = [0x51, 1, 0, 0, 0].repeat(6);
        p[25..].copy_from_slice(&[0x30, 0, 0, 0, 0]);
        p[5..10].copy_from_slice(&binding(64).unwrap());
        memory[0x10de..0x10de + 30].copy_from_slice(&p);
        memory[0x10fc..0x10fc + 10].copy_from_slice(&[0x31, 0, 0, 0, 0, 0x32, 0, 0, 0, 0]);
        for index in 0..3 {
            let mut c = [0; 60];
            c[13] = 255;
            memory[0x1028 + index * 60..0x1028 + (index + 1) * 60].copy_from_slice(&c);
        }
        let events = [
            Event::Key {
                usage: 4,
                page: Page::Keyboard,
                down: true,
            },
            Event::Key {
                usage: 4,
                page: Page::Keyboard,
                down: false,
            },
        ];
        let block = prime_macro_once::compile(&events)
            .unwrap()
            .encode()
            .unwrap();
        memory[0x129b + 64..0x129b + 64 + block.len()].copy_from_slice(&block);
        memory
    }
    fn backend(memory: Arc<Mutex<Vec<u8>>>, mutate: u8, cancel: Arc<AtomicBool>) -> MockTransport {
        let mut changed = false;
        MockTransport::new(move |op| {
            let MockOp::Write(frame) = op else {
                panic!("feature query")
            };
            let mut reply = vec![0; 64];
            if frame[1] == 0x90 {
                reply[..14].copy_from_slice(b"1.0.15+4244c08");
            } else if &frame[1..7] == [0x83, 0, 0x2a, 9, 32, 0] {
                reply[..32].copy_from_slice(&frame[7..39]);
            } else {
                let offset = usize::from(u16::from_le_bytes([frame[3], frame[4]]));
                let n = usize::from(u16::from_le_bytes([frame[5], frame[6]]));
                let mut m = memory.lock().unwrap();
                reply[..n].copy_from_slice(&m[offset..offset + n]);
                if !changed
                    && ((mutate == 1 && offset == 0x10de)
                        || (mutate == 2 && offset == 0x129b + 5056))
                {
                    m[if mutate == 1 { 0x10df } else { 0x129b }] ^= 1;
                    changed = true;
                }
                if mutate == 3 && offset == 0x129b {
                    cancel.store(true, Ordering::SeqCst);
                }
            }
            MockReply::input(reply)
        })
    }
    #[test]
    fn two_equal_passes_return_complete_owned_macro_image_but_no_hardware_authority() {
        let memory = Arc::new(Mutex::new(valid_memory()));
        let original = memory.lock().unwrap().clone();
        let expected = scope();
        let check_scope = expected.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut session = Session::new(backend(memory, 0, cancel), [7; 16], expected, move || {
            Ok(Status {
                scope: check_scope.clone(),
                cancelled: false,
            })
        })
        .unwrap();
        session.reader.omit_test_gap();
        let observed = session.collect().unwrap();
        assert_eq!(
            observed.image().memory(),
            &original[0x129b..0x129b + OBSERVED_BYTES]
        );
        assert_eq!(observed.equal_passes(), 2);
        assert_eq!(observed.inactive_context_observations(), 4);
        assert_eq!(observed.image().references().len(), 1);
        assert!(!observed.physical_identity_verified());
        assert!(!observed.atomic_image_verified());
    }
    #[test]
    fn changes_in_tables_or_between_library_passes_never_publish_an_image() {
        for mutate in [1, 2] {
            let memory = Arc::new(Mutex::new(valid_memory()));
            let expected = scope();
            let check = expected.clone();
            let mut session = Session::new(
                backend(memory, mutate, Arc::new(AtomicBool::new(false))),
                [9; 16],
                expected,
                move || {
                    Ok(Status {
                        scope: check.clone(),
                        cancelled: false,
                    })
                },
            )
            .unwrap();
            session.reader.omit_test_gap();
            assert!(session.collect().is_err());
        }
    }
    #[test]
    fn cancellation_during_io_rejects_the_reply_before_it_is_used() {
        let memory = Arc::new(Mutex::new(valid_memory()));
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let check_flag = flag.clone();
        let expected = scope();
        let current = expected.clone();
        let io = backend(memory, 3, cancelled);
        let log = io.log();
        let mut session = Session::new(io, [3; 16], expected, move || {
            Ok(Status {
                scope: current.clone(),
                cancelled: check_flag.load(Ordering::SeqCst),
            })
        })
        .unwrap();
        session.reader.omit_test_gap();
        assert!(session.collect().is_err());
        assert!(flag.load(Ordering::SeqCst));
        assert!(!log.lock().unwrap().is_empty());
    }
    #[test]
    fn active_or_stale_held_contexts_are_refused_before_library_collection() {
        for stale in [false, true] {
            let mut m = valid_memory();
            m[0x1028 + if stale { 20 } else { 13 }] = 1;
            let memory = Arc::new(Mutex::new(m));
            let expected = scope();
            let current = expected.clone();
            let mut session = Session::new(
                backend(memory, 0, Arc::new(AtomicBool::new(false))),
                [5; 16],
                expected,
                move || {
                    Ok(Status {
                        scope: current.clone(),
                        cancelled: false,
                    })
                },
            )
            .unwrap();
            session.reader.omit_test_gap();
            assert!(session.collect().is_err());
        }
    }
    #[test]
    fn wrong_interface_scope_connection_operation_or_cancel_prevents_any_io() {
        for field in 0..5 {
            let mut i = info();
            match field {
                0 => i.vendor_id = 1,
                1 => i.product_id = 0x182f,
                2 => i.interface_number = 1,
                3 => i.usage_page = 0xffc1,
                _ => i.bus = BusKind::Bluetooth,
            }
            assert!(Scope::new(&i, "c", "o").is_err());
        }
        let mut unknown = info();
        unknown.interface_number = -1;
        assert!(Scope::new(&unknown, "c", "o").is_ok());
        for changed in 0..4 {
            let expected = scope();
            let mut current = expected.clone();
            match changed {
                0 => current.path.push('b'),
                1 => current.connection.push('2'),
                2 => current.operation.push('2'),
                _ => {}
            }
            let io = backend(
                Arc::new(Mutex::new(valid_memory())),
                0,
                Arc::new(AtomicBool::new(false)),
            );
            let log = io.log();
            assert!(
                Session::new(io, [6; 16], expected, move || Ok(Status {
                    scope: current.clone(),
                    cancelled: changed == 3
                }))
                .is_err()
            );
            assert!(log.lock().unwrap().is_empty());
        }
    }
    #[test]
    fn read_transport_rejects_every_write_direction_and_unapproved_range() {
        let expected = scope();
        let current = expected.clone();
        let io = backend(
            Arc::new(Mutex::new(valid_memory())),
            0,
            Arc::new(AtomicBool::new(false)),
        );
        let log = io.log();
        let mut transport = Scoped {
            io,
            expected,
            check: move || {
                Ok(Status {
                    scope: current.clone(),
                    cancelled: false,
                })
            },
        };
        for opcode in [0x03, 0x5e, 0x5b, 0x59, 0x62] {
            let mut frame = vec![0; 65];
            frame[1] = opcode;
            assert!(transport.write(&frame).is_err());
        }
        let mut frame = Read::physical().output_report().unwrap();
        frame[3..5].copy_from_slice(&1u16.to_le_bytes());
        assert!(transport.write(&frame).is_err());
        assert!(transport.send_feature_report(&[0; 643]).is_err());
        assert!(transport.get_feature_report(&mut [0; 643]).is_err());
        assert!(log.lock().unwrap().is_empty());
    }
    #[test]
    fn cancelling_another_same_model_session_cannot_cancel_or_supply_this_image() {
        let expected_a = scope();
        let current_a = expected_a.clone();
        let mut other_info = info();
        other_info.path = "mock-prime-b".into();
        other_info.serial = Some("mock-b".into());
        let expected_b = Scope::new(&other_info, "mock-open-2", "mock-read-2").unwrap();
        let current_b = expected_b.clone();
        let cancelled_b = Arc::new(AtomicBool::new(false));
        let status_b = cancelled_b.clone();
        let memory_a = valid_memory();
        let mut memory_b = valid_memory();
        memory_b[0x129b + 3070] ^= 1;
        let io_a = backend(
            Arc::new(Mutex::new(memory_a.clone())),
            0,
            Arc::new(AtomicBool::new(false)),
        );
        let io_b = backend(
            Arc::new(Mutex::new(memory_b)),
            0,
            Arc::new(AtomicBool::new(false)),
        );
        let log_b = io_b.log();
        let mut a = Session::new(io_a, [12; 16], expected_a, move || {
            Ok(Status {
                scope: current_a.clone(),
                cancelled: false,
            })
        })
        .unwrap();
        let mut b = Session::new(io_b, [13; 16], expected_b, move || {
            Ok(Status {
                scope: current_b.clone(),
                cancelled: status_b.load(Ordering::SeqCst),
            })
        })
        .unwrap();
        a.reader.omit_test_gap();
        b.reader.omit_test_gap();
        cancelled_b.store(true, Ordering::SeqCst);
        assert!(b.collect().is_err());
        assert!(log_b.lock().unwrap().is_empty());
        let observed = a.collect().unwrap();
        assert_eq!(
            observed.image().memory(),
            &memory_a[0x129b..0x129b + OBSERVED_BYTES]
        );
        assert_eq!(observed.scope().connection(), "mock-open-1");
    }
}
