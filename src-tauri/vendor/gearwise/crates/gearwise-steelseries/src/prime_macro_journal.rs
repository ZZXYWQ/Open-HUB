//! Append-only, reserve-before-send journal for the pinned Prime macro edit.
//! No HID/service is enabled. Caller observations/leases are not authentication.
use crate::{
    link::Packet,
    prime_image_session::Scope,
    prime_macro::event::Event,
    prime_macro_program::{Playback, Trigger},
    prime_macro_storage::{self, Edit, Image},
    prime_macro_transfer::CHUNK_BYTES,
};
use gearwise_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const LIMIT: u64 = 96 * 1024;
const MAX_EVENTS: usize = 64;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    version: u8,
    scope: Scope,
    memory: Vec<u8>,
    physical: Vec<u8>,
    wheel: Vec<u8>,
    control: u8,
    events: Vec<[u8; 4]>,
    #[serde(default, skip_serializing_if = "is_once")]
    playback: Playback,
    #[serde(default, skip_serializing_if = "is_press")]
    trigger: Trigger,
}
fn is_once(p: &Playback) -> bool {
    *p == Playback::Once
}
fn is_press(t: &Trigger) -> bool {
    *t == Trigger::Press
}
impl Intent {
    fn edit(&self) -> Result<Edit> {
        self.scope.validate()?;
        if self.version != 1 || self.events.len() > 254 {
            return Err(Error::invalid("Prime journal version/event limit"));
        }
        let before = Image::new(
            self.scope.connection(),
            &self.memory,
            &self.physical,
            &self.wheel,
        )?;
        let events = self
            .events
            .iter()
            .map(|e| Event::decode(e))
            .collect::<Result<Vec<_>>>()?;
        let edit = prime_macro_storage::replace_program_in_place(
            &before,
            self.control,
            &events,
            self.playback,
            self.trigger,
        )?;
        if edit.packets().is_empty() {
            return Err(Error::invalid(
                "unchanged Prime edit needs no write journal",
            ));
        }
        Ok(edit)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Entry {
    ForwardAttempt { index: u8 },
    BeginRestore { observed: Vec<u8> },
    RestoreAttempt { index: u8 },
    Committed,
    Restored,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Prepared,
    Forward,
    Restoring,
    Committed,
    Restored,
}
#[derive(Clone, Default)]
struct State {
    forward: u8,
    origin: Option<Vec<u8>>,
    restore_cursor: u8,
    restore_max: u8,
    terminal: Option<Phase>,
}
impl State {
    fn phase(&self) -> Phase {
        self.terminal.unwrap_or(if self.origin.is_some() {
            Phase::Restoring
        } else if self.forward > 0 {
            Phase::Forward
        } else {
            Phase::Prepared
        })
    }
    fn apply(&self, edit: &Edit, entry: &Entry) -> Result<Self> {
        if self.terminal.is_some() {
            return Err(Error::invalid("Prime journal is terminal"));
        }
        let mut next = self.clone();
        match entry {
            Entry::ForwardAttempt { index } => {
                if self.origin.is_some()
                    || *index != self.forward + 1
                    || usize::from(*index) > edit.packets().len()
                {
                    return Err(Error::invalid("Prime forward attempt order"));
                }
                next.forward = *index;
            }
            Entry::BeginRestore { observed } => {
                let image = image_with(edit.before(), observed)?;
                if !edit
                    .restore_with_attempts(&image, self.forward.into(), self.restore_max.into())?
                    .needed
                {
                    return Err(Error::invalid(
                        "already original Prime image needs no restoration commands",
                    ));
                }
                next.origin = Some(observed.clone());
                next.restore_cursor = 0;
            }
            Entry::RestoreAttempt { index } => {
                if self.origin.is_none()
                    || *index != self.restore_cursor + 1
                    || usize::from(*index) > edit.packets().len()
                {
                    return Err(Error::invalid("Prime restore attempt order"));
                }
                next.restore_cursor = *index;
                next.restore_max = next.restore_max.max(*index);
            }
            Entry::Committed => {
                if self.origin.is_some() || usize::from(self.forward) != edit.packets().len() {
                    return Err(Error::invalid(
                        "Prime commit without complete forward attempts",
                    ));
                }
                next.terminal = Some(Phase::Committed);
            }
            Entry::Restored => next.terminal = Some(Phase::Restored),
        }
        Ok(next)
    }
}
fn image_with(template: &Image, memory: &[u8]) -> Result<Image> {
    Image::new(
        template.scope(),
        memory,
        template.physical(),
        template.wheel(),
    )
}
fn io_error(context: &str, error: impl std::fmt::Display) -> Error {
    Error::protocol(format!("Prime journal {context}: {error}"))
}
// Canonical-payload FNV-1a64 catches accidental record corruption. This is
// an integrity checksum, not authentication against another local writer.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    payload: T,
    checksum: String,
}
fn checksum(value: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|e| io_error("checksum encoding", e))?;
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    Ok(format!("{hash:016x}"))
}
fn read_json<T: serde::de::DeserializeOwned + Serialize>(path: &Path) -> Result<T> {
    let meta = fs::symlink_metadata(path).map_err(|e| io_error("metadata", e))?;
    if !meta.is_file() || meta.len() > LIMIT {
        return Err(Error::invalid("Prime journal file type/size"));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| io_error("open", e))?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error("read", e))?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::invalid("Prime journal grew while reading"));
    }
    let envelope: Envelope<T> = serde_json::from_slice(&bytes)
        .map_err(|e| io_error("decode; preserve original file", e))?;
    if envelope.checksum != checksum(&envelope.payload)? {
        return Err(Error::invalid("Prime journal integrity checksum mismatch"));
    }
    Ok(envelope.payload)
}
fn create_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let envelope = Envelope {
        payload: value,
        checksum: checksum(value)?,
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|e| io_error("encode", e))?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::invalid("Prime journal encoded size"));
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| io_error("create new", e))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| io_error("flush; preserve uncertain record", e))
}
/// Private application-owned directory, immutable intent and numbered entries.
/// Only confirmed file sync is claimed; filesystem/directory power-loss behavior
/// needs platform validation. Invalid/truncated/gapped logs refuse all commands.
pub struct Journal {
    directory: PathBuf,
    scope: Scope,
    edit: Edit,
    state: State,
    sequence: usize,
    poisoned: bool,
}
impl Journal {
    /// The directory must not exist; no old record is replaced or deleted.
    pub fn create(
        directory: &Path,
        scope: Scope,
        before: &Image,
        control: u8,
        events: &[Event],
    ) -> Result<Self> {
        Self::create_program(
            directory,
            scope,
            before,
            control,
            events,
            Playback::Once,
            Trigger::Press,
        )
    }
    /// Retain the selected native policy exactly; never silently convert loops
    /// or release triggers to press-once during reconstruction/recovery.
    pub fn create_program(
        directory: &Path,
        scope: Scope,
        before: &Image,
        control: u8,
        events: &[Event],
        playback: Playback,
        trigger: Trigger,
    ) -> Result<Self> {
        if events.len() > 254 || before.scope() != scope.connection() {
            return Err(Error::invalid("Prime journal connection mismatch"));
        }
        let intent = Intent {
            version: 1,
            scope: scope.clone(),
            memory: before.memory().into(),
            physical: before.physical().into(),
            wheel: before.wheel().into(),
            control,
            playback,
            trigger,
            events: events
                .iter()
                .copied()
                .map(Event::encode)
                .collect::<Result<_>>()?,
        };
        let edit = intent.edit()?;
        fs::create_dir(directory).map_err(|e| io_error("create private directory", e))?;
        create_json(&directory.join("intent.json"), &intent)?;
        Ok(Self {
            directory: directory.into(),
            scope,
            edit,
            state: State::default(),
            sequence: 0,
            poisoned: false,
        })
    }
    /// Reopen requires the exact original logical lease. A reconnect must not
    /// automatically adopt the old journal, even for the same model/serial.
    pub fn open(directory: &Path, expected: &Scope) -> Result<Self> {
        expected.validate()?;
        let meta =
            fs::symlink_metadata(directory).map_err(|e| io_error("directory metadata", e))?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(Error::invalid("Prime journal directory type"));
        }
        let intent: Intent = read_json(&directory.join("intent.json"))?;
        if &intent.scope != expected {
            return Err(Error::invalid("Prime journal belongs to another lease"));
        }
        let edit = intent.edit()?;
        let mut names = Vec::new();
        for entry in fs::read_dir(directory)
            .map_err(|e| io_error("list", e))?
            .take(MAX_EVENTS + 2)
        {
            let name = entry.map_err(|e| io_error("entry", e))?.file_name();
            let name = name
                .to_str()
                .ok_or(Error::invalid("Prime journal filename"))?;
            if name != "intent.json" {
                names.push(name.to_owned());
            }
        }
        if names.len() > MAX_EVENTS {
            return Err(Error::invalid("Prime journal entry limit"));
        }
        names.sort();
        let mut state = State::default();
        for (index, name) in names.iter().enumerate() {
            if name != &format!("{:08}.json", index + 1) {
                return Err(Error::invalid("Prime journal missing/unexpected entry"));
            }
            let entry: Entry = read_json(&directory.join(name))?;
            state = state.apply(&edit, &entry)?;
        }
        Ok(Self {
            directory: directory.into(),
            scope: expected.clone(),
            edit,
            state,
            sequence: names.len(),
            poisoned: false,
        })
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn phase(&self) -> Phase {
        self.state.phase()
    }
    pub fn forward_attempts(&self) -> usize {
        self.state.forward.into()
    }
    pub fn restore_attempts(&self) -> usize {
        self.state.restore_max.into()
    }
    pub fn before(&self) -> &Image {
        self.edit.before()
    }
    pub fn after(&self) -> &Image {
        self.edit.after()
    }
    fn append(&mut self, entry: Entry) -> Result<()> {
        if self.poisoned || self.sequence >= MAX_EVENTS {
            return Err(Error::invalid("Prime journal unavailable/entry limit"));
        }
        let next = self.state.apply(&self.edit, &entry)?;
        if let Err(error) = create_json(
            &self
                .directory
                .join(format!("{:08}.json", self.sequence + 1)),
            &entry,
        ) {
            self.poisoned = true;
            return Err(error);
        }
        self.state = next;
        self.sequence += 1;
        Ok(())
    }
    /// Persist the attempted count BEFORE the caller submits this exact frame.
    /// A fresh exact prefix observation is required before the next reservation.
    /// A returned packet is not proof it was sent/accepted and may not be reused.
    pub fn reserve_forward(&mut self, current: &Image) -> Result<Packet> {
        let mut expected = self.edit.before().memory().to_vec();
        let n = usize::from(self.state.forward) * CHUNK_BYTES;
        expected[..n].copy_from_slice(&self.edit.after().memory()[..n]);
        if current != &image_with(self.edit.before(), &expected)? {
            return Err(Error::invalid(
                "Prime forward readback differs from exact prior prefix",
            ));
        }
        let packet = self
            .edit
            .packets()
            .get(self.state.forward as usize)
            .ok_or(Error::invalid("Prime forward commands exhausted"))?
            .clone();
        self.append(Entry::ForwardAttempt {
            index: self.state.forward + 1,
        })?;
        Ok(packet)
    }
    /// Explicitly start/restart recovery from an observed recognized history.
    /// Restarting is never automatic and still requires caller identity/idle I/O
    /// admission. Full tables/scope/unknown bytes must remain exact.
    pub fn begin_restore(&mut self, current: &Image) -> Result<()> {
        self.edit.restore_with_attempts(
            current,
            self.forward_attempts(),
            self.restore_attempts(),
        )?;
        self.append(Entry::BeginRestore {
            observed: current.memory().into(),
        })
    }
    pub fn reserve_restore(&mut self, current: &Image) -> Result<Packet> {
        let mut expected = self
            .state
            .origin
            .clone()
            .ok_or(Error::invalid("Prime restoration not begun"))?;
        let n = usize::from(self.state.restore_cursor) * CHUNK_BYTES;
        expected[..n].copy_from_slice(&self.edit.before().memory()[..n]);
        if current != &image_with(self.edit.before(), &expected)? {
            return Err(Error::invalid(
                "Prime restore readback differs from exact prior prefix",
            ));
        }
        let restore = self.edit.restore_with_attempts(
            &image_with(self.edit.before(), self.state.origin.as_ref().unwrap())?,
            self.forward_attempts(),
            self.restore_attempts(),
        )?;
        let packet = restore
            .packets
            .get(self.state.restore_cursor as usize)
            .ok_or(Error::invalid("Prime restore commands exhausted"))?
            .clone();
        self.append(Entry::RestoreAttempt {
            index: self.state.restore_cursor + 1,
        })?;
        Ok(packet)
    }
    pub fn verify_committed(&mut self, current: &Image) -> Result<()> {
        if current != self.edit.after() {
            return Err(Error::invalid("Prime complete forward readback mismatch"));
        }
        self.append(Entry::Committed)
    }
    pub fn verify_restored(&mut self, current: &Image) -> Result<()> {
        if current != self.edit.before() {
            return Err(Error::invalid("Prime complete original readback mismatch"));
        }
        self.append(Entry::Restored)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{prime_macro::event::Page, prime_macro_block::binding, prime_macro_once};
    use gearwise_core::transport::HidInterfaceInfo;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "open-hub-prime-journal-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            if self.0.is_dir() {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
    }
    fn scope(unit: &str, connection: &str, operation: &str) -> Scope {
        Scope::new(
            &HidInterfaceInfo {
                path: format!("mock-prime:{unit}"),
                serial: Some(unit.into()),
                vendor_id: 0x1038,
                product_id: 0x182e,
                interface_number: 0,
                usage_page: 0xffc0,
                usage: 1,
                ..Default::default()
            },
            connection,
            operation,
        )
        .unwrap()
    }
    fn events(usage: u8) -> Vec<Event> {
        vec![
            Event::Key {
                usage,
                page: Page::Keyboard,
                down: true,
            },
            Event::Delay(7),
            Event::Key {
                usage,
                page: Page::Keyboard,
                down: false,
            },
        ]
    }
    fn fixture(at: usize) -> Image {
        let mut memory = vec![0x4a; 5120];
        let mut physical = vec![0x51, 1, 0, 0, 0].repeat(6);
        physical[25..].copy_from_slice(&[0x30, 0, 0, 0, 0]);
        for (i, offset) in [at, 1200, 2048].into_iter().enumerate() {
            let mut raw = prime_macro_once::compile(&events(4))
                .unwrap()
                .encode()
                .unwrap();
            if i == 2 {
                raw[1] = 6;
                raw[8] = 0xfd;
            }
            memory[offset..offset + raw.len()].copy_from_slice(&raw);
            physical[(i + 1) * 5..(i + 2) * 5].copy_from_slice(&binding(offset).unwrap());
        }
        Image::new(
            "open-a",
            &memory,
            &physical,
            &[0x31, 0, 0, 0, 0, 0x32, 0, 0, 0, 0],
        )
        .unwrap()
    }
    fn apply(image: &Image, packet: &Packet) -> Image {
        let offset = usize::from(u16::from_le_bytes([packet.bytes[1], packet.bytes[2]])) * 4;
        let n = usize::from(u16::from_le_bytes([packet.bytes[3], packet.bytes[4]])) * 4;
        let mut memory = image.memory().to_vec();
        memory[offset..offset + n].copy_from_slice(&packet.bytes[5..5 + n]);
        image_with(image, &memory).unwrap()
    }
    fn journal(path: &Path, at: usize) -> Journal {
        Journal::create(
            path,
            scope("a", "open-a", "edit-a"),
            &fixture(at),
            2,
            &[events(6)[0], events(6)[2]],
        )
        .unwrap()
    }
    #[test]
    fn send_reservations_survive_reopen_and_exact_full_readback_finishes() {
        let temp = Temp::new();
        let mut j = journal(&temp.0, 504);
        let lease = j.scope().clone();
        let mut current = j.before().clone();
        assert!(j.verify_committed(&current).is_err());
        let first = j.reserve_forward(&current).unwrap();
        assert_eq!(j.forward_attempts(), 1);
        // Reservation is durable even though this example has not sent anything.
        drop(j);
        let mut j = Journal::open(&temp.0, &lease).unwrap();
        assert_eq!(j.forward_attempts(), 1);
        assert!(j.reserve_forward(&current).is_err());
        current = apply(&current, &first);
        let second = j.reserve_forward(&current).unwrap();
        current = apply(&current, &second);
        j.verify_committed(&current).unwrap();
        assert_eq!(
            Journal::open(&temp.0, &lease).unwrap().phase(),
            Phase::Committed
        );
        assert!(j.reserve_forward(&current).is_err());
        assert!(j.begin_restore(&current).is_err());
        assert!(temp.0.join("intent.json").is_file()); // terminal records are retained
    }
    #[test]
    fn interrupted_forward_and_restore_can_be_explicitly_restarted_from_known_history() {
        for forward_applied in 0..=2 {
            for restore_applied in 0..=1 {
                let temp = Temp::new();
                let mut j = journal(&temp.0, 504);
                let lease = j.scope().clone();
                let before = j.before().clone();
                let mut current = before.clone();
                let first = j.reserve_forward(&current).unwrap();
                if forward_applied > 0 {
                    current = apply(&current, &first);
                }
                if forward_applied == 2 {
                    let second = j.reserve_forward(&current).unwrap();
                    current = apply(&current, &second);
                }
                if current == before {
                    j.verify_restored(&current).unwrap();
                    continue;
                }
                j.begin_restore(&current).unwrap();
                let restore = j.reserve_restore(&current).unwrap();
                if restore_applied > 0 {
                    current = apply(&current, &restore);
                }
                drop(j);
                let mut j = Journal::open(&temp.0, &lease).unwrap();
                if current == before {
                    j.verify_restored(&current).unwrap();
                } else {
                    j.begin_restore(&current).unwrap();
                    for _ in 0..2 {
                        let p = j.reserve_restore(&current).unwrap();
                        current = apply(&current, &p);
                    }
                    j.verify_restored(&current).unwrap();
                }
                assert_eq!(current, before);
                assert_eq!(
                    Journal::open(&temp.0, &lease).unwrap().phase(),
                    Phase::Restored
                );
            }
        }
    }
    #[test]
    fn unknown_scope_tables_and_foreign_bytes_never_receive_a_reservation() {
        let temp = Temp::new();
        let mut j = journal(&temp.0, 64);
        let before = j.before().clone();
        let mut memory = before.memory().to_vec();
        memory[4095] ^= 1;
        let foreign = image_with(&before, &memory).unwrap();
        assert!(j.reserve_forward(&foreign).is_err());
        let mut physical = before.physical().to_vec();
        physical[0] = 0x52;
        let changed =
            Image::new(before.scope(), before.memory(), &physical, before.wheel()).unwrap();
        assert!(j.reserve_forward(&changed).is_err());
        let other =
            Image::new("open-b", before.memory(), before.physical(), before.wheel()).unwrap();
        assert!(j.reserve_forward(&other).is_err());
        j.reserve_forward(&before).unwrap();
        assert!(j.begin_restore(&foreign).is_err());
        assert!(j.begin_restore(&changed).is_err());
        for lease in [
            scope("b", "open-a", "edit-a"),
            scope("a", "open-b", "edit-a"),
            scope("a", "open-a", "edit-b"),
        ] {
            assert!(Journal::open(&temp.0, &lease).is_err());
        }
        assert_eq!(j.forward_attempts(), 1);
        assert_eq!(j.restore_attempts(), 0);
    }
    #[test]
    fn disk_failure_returns_no_packet_and_poisoned_handle_cannot_advance() {
        let temp = Temp::new();
        let mut j = journal(&temp.0, 64);
        let before = j.before().clone();
        fs::write(temp.0.join("00000001.json"), b"interrupted record").unwrap();
        assert!(j.reserve_forward(&before).is_err());
        assert_eq!(j.forward_attempts(), 0);
        // Even if an external process clears the collision, this handle stays poisoned.
        fs::remove_file(temp.0.join("00000001.json")).unwrap();
        assert!(j.reserve_forward(&before).is_err());
        assert!(!temp.0.join("00000001.json").exists());
    }
    #[test]
    fn truncated_gapped_unknown_reordered_and_oversized_logs_are_preserved_and_refused() {
        for (name, bytes) in [
            ("00000001.json", b"{".to_vec()),
            ("00000002.json", b"{}".to_vec()),
            ("extra.json", b"{}".to_vec()),
            (
                "00000001.json",
                br#"{"kind":"forward_attempt","index":2}"#.to_vec(),
            ),
            ("00000001.json", vec![b' '; LIMIT as usize + 1]),
        ] {
            let temp = Temp::new();
            let j = journal(&temp.0, 64);
            let lease = j.scope().clone();
            drop(j);
            fs::write(temp.0.join(name), &bytes).unwrap();
            assert!(Journal::open(&temp.0, &lease).is_err());
            assert_eq!(fs::read(temp.0.join(name)).unwrap(), bytes);
        }
    }
    #[test]
    fn invalid_or_noop_intent_and_existing_directory_do_not_replace_records() {
        let temp = Temp::new();
        let before = fixture(64);
        let lease = scope("a", "open-a", "edit-a");
        assert!(Journal::create(&temp.0, lease.clone(), &before, 2, &events(4)).is_err());
        assert!(!temp.0.exists());
        assert!(Journal::create(&temp.0, lease.clone(), &before, 1, &events(6)).is_err());
        assert!(!temp.0.exists());
        let j = journal(&temp.0, 64);
        let original = fs::read(temp.0.join("intent.json")).unwrap();
        assert!(Journal::create(&temp.0, lease.clone(), &before, 2, &events(6)).is_err());
        assert_eq!(fs::read(temp.0.join("intent.json")).unwrap(), original);
        drop(j);
        let mut intent: serde_json::Value = serde_json::from_slice(&original).unwrap();
        intent["payload"]["scope"]["operation"] = "".into();
        fs::write(
            temp.0.join("intent.json"),
            serde_json::to_vec(&intent).unwrap(),
        )
        .unwrap();
        assert!(Journal::open(&temp.0, &lease).is_err());
    }
    #[test]
    fn valid_json_payload_corruption_and_validly_checksummed_reordering_are_refused() {
        let temp = Temp::new();
        let j = journal(&temp.0, 64);
        let lease = j.scope().clone();
        drop(j);
        let path = temp.0.join("intent.json");
        let original = fs::read(&path).unwrap();
        let mut data: serde_json::Value = serde_json::from_slice(&original).unwrap();
        // Old macro event changes can be hidden by the desired replacement.
        // Preserve the original checksum so a damaged backup cannot be adopted.
        data["payload"]["memory"][72] = serde_json::json!(3);
        fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(Journal::open(&temp.0, &lease).is_err());
        fs::write(&path, original).unwrap();
        create_json(
            &temp.0.join("00000001.json"),
            &Entry::ForwardAttempt { index: 2 },
        )
        .unwrap();
        assert!(Journal::open(&temp.0, &lease).is_err());
    }
    #[test]
    fn playback_policies_survive_reopen_and_old_once_intents_remain_readable() {
        for mode in [
            Playback::Once,
            Playback::Counted {
                count: 3,
                interval_ticks: 7,
            },
            Playback::WhileHeld,
            Playback::Toggle,
        ] {
            let temp = Temp::new();
            let before = fixture(504);
            let lease = scope("a", "open-a", "edit-a");
            let trigger = if mode == Playback::WhileHeld {
                Trigger::Press
            } else {
                Trigger::Release
            };
            let mut j = Journal::create_program(
                &temp.0,
                lease.clone(),
                &before,
                2,
                &[events(6)[0], events(6)[2]],
                mode,
                trigger,
            )
            .unwrap();
            let desired = j.after().clone();
            let mut current = before;
            let packet = j.reserve_forward(&current).unwrap();
            current = apply(&current, &packet);
            drop(j);
            let mut j = Journal::open(&temp.0, &lease).unwrap();
            assert_eq!(j.after(), &desired);
            let packet = j.reserve_forward(&current).unwrap();
            current = apply(&current, &packet);
            j.verify_committed(&current).unwrap();
            assert_eq!(current, desired);
        }
        let temp = Temp::new();
        let j = journal(&temp.0, 64);
        let lease = j.scope().clone();
        drop(j);
        let bytes = fs::read_to_string(temp.0.join("intent.json")).unwrap();
        assert!(!bytes.contains("playback") && !bytes.contains("trigger"));
        assert_eq!(
            Journal::open(&temp.0, &lease).unwrap().phase(),
            Phase::Prepared
        );
    }
    #[test]
    fn another_mouse_journal_is_independent_and_same_bytes_do_not_transfer_ownership() {
        let a = Temp::new();
        let b = Temp::new();
        let mut ja = journal(&a.0, 64);
        let before = fixture(64);
        let mut jb =
            Journal::create(&b.0, scope("b", "open-a", "edit-a"), &before, 2, &events(6)).unwrap();
        let current = before.clone();
        ja.reserve_forward(&current).unwrap();
        assert_eq!(jb.forward_attempts(), 0);
        assert!(Journal::open(&a.0, jb.scope()).is_err());
        jb.verify_restored(&current).unwrap();
        assert_eq!(ja.phase(), Phase::Forward);
        assert_eq!(
            Journal::open(&a.0, ja.scope()).unwrap().forward_attempts(),
            1
        );
    }
}
