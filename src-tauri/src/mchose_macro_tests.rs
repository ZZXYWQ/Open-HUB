//! Original production codec and stateful USB request replay; no HID/OS input.
use super::*;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

fn sequence() -> Vec<MacroEvent> {
    vec![
        MacroEvent {
            kind: "key".into(),
            code: 4,
            down: true,
            delay_ms: 0,
        },
        MacroEvent {
            kind: "delay".into(),
            code: 0,
            down: false,
            delay_ms: 37,
        },
        MacroEvent {
            kind: "key".into(),
            code: 4,
            down: false,
            delay_ms: 0,
        },
    ]
}
fn canonical() -> MouseMacro {
    macro_record(2, "A", 1, &sequence(), None).unwrap()
}
fn opaque() -> MouseMacro {
    let mut bytes = canonical().raw;
    bytes[2..4].copy_from_slice(&[0xa5, 0x17]);
    bytes[5..8].copy_from_slice(&[0x91, 0x82, 0x73]);
    bytes[64] = 29; // A non-time word is retained, not reinterpreted as timing.
    bytes[66] |= 0x80; // An unusual terminal flag is also opaque content.
    decode_macro(2, bytes).unwrap()
}
fn action(m: &MouseMacro, raw: Option<Vec<u8>>) -> Action {
    Action::Macro {
        slot: m.slot,
        name: m.name.clone(),
        mode: m.mode,
        events: m.events.clone(),
        raw,
    }
}

struct Mouse {
    state: HardwareState,
    macros: BTreeMap<u8, Vec<u8>>,
    staged: BTreeMap<u8, Vec<u8>>,
    writes: Vec<Vec<u8>>,
    ignore_commit: bool,
    fail_offset: Option<usize>,
}
impl Mouse {
    fn request(&mut self, command: u16, args: &[u8]) -> Result<Vec<u8>> {
        Ok(match command {
            0x0900 => {
                let mut id = vec![0; 16];
                id[..2].copy_from_slice(&crate::mchose::VID.to_le_bytes());
                id[2..4].copy_from_slice(&crate::mchose::MODEL_PID.to_le_bytes());
                id[10] = 2;
                id
            }
            2 => self.state.raw_settings.clone(),
            3 => {
                if args[1] == 0 {
                    self.state.raw_x.clone()
                } else {
                    self.state.raw_y.clone()
                }
            }
            1 => self.state.raw_buttons.clone(),
            9 => vec![self.state.lod],
            10 => self.state.adaptive.clone(),
            0x0901 => vec![0, 23],
            0x0905 => {
                let mut bytes = [0; 60];
                let name = self.state.names[args[0] as usize].as_bytes();
                bytes[..name.len()].copy_from_slice(name);
                bytes[args[1] as usize..args[1] as usize + args[2] as usize].to_vec()
            }
            0x090c => match self.macros.get(&args[0]) {
                None => vec![0],
                Some(raw) => {
                    let offset = u16::from_le_bytes([args[1], args[2]]) as usize;
                    let len = args[3] as usize;
                    let part = raw
                        .get(offset..offset + len)
                        .ok_or_else(|| bad("synthetic read outside macro"))?;
                    [vec![len as u8], part.to_vec()].concat()
                }
            },
            0x090b => {
                let offset = u16::from_le_bytes([args[1], args[2]]) as usize;
                let count = (args[3] >> 2) as usize;
                assert_eq!(count, args.len() - 4);
                assert!(count <= 19);
                self.writes.push(args.to_vec());
                if self.fail_offset == Some(offset) {
                    return Err(bad("authored chunk failure"));
                }
                let slot = args[0];
                if args[3] & 3 == 1 {
                    assert_eq!(offset, 0);
                    self.staged.insert(slot, vec![]);
                }
                let bytes = self.staged.get_mut(&slot).unwrap();
                assert_eq!(bytes.len(), offset);
                bytes.extend_from_slice(&args[4..]);
                if args[3] & 3 == 2 {
                    let bytes = self.staged.remove(&slot).unwrap();
                    assert_eq!(
                        bytes.len(),
                        64 + u16::from_le_bytes([bytes[0], bytes[1]]) as usize
                    );
                    if !self.ignore_commit {
                        self.macros.insert(slot, bytes);
                    }
                }
                vec![]
            }
            _ => return Err(bad(format!("unexpected macro request {command:04x}"))),
        })
    }
}
struct Rig {
    transport: Transport,
    mouse: Arc<Mutex<Mouse>>,
    root: std::path::PathBuf,
}
impl Rig {
    fn new(record: MouseMacro) -> Self {
        let snapshot: Snapshot =
            serde_json::from_str(include_str!("../tests/fixtures/mchose.json")).unwrap();
        let mouse = Arc::new(Mutex::new(Mouse {
            state: snapshot.state,
            macros: [(record.slot, record.raw)].into_iter().collect(),
            staged: BTreeMap::new(),
            writes: vec![],
            ignore_commit: false,
            fail_offset: None,
        }));
        let source = mouse.clone();
        let root = std::env::temp_dir().join(format!(
            "open-hub-mchose-macro-{}",
            crate::profiles::new_id()
        ));
        let transport = Transport::memory(
            move |cmd, args| source.lock().unwrap().request(cmd, args),
            root.join("backups"),
        );
        Self {
            transport,
            mouse,
            root,
        }
    }
    fn backups(&self) -> Vec<Snapshot> {
        std::fs::read_dir(self.root.join("backups"))
            .unwrap()
            .map(|entry| {
                serde_json::from_slice(&std::fs::read(entry.unwrap().path()).unwrap()).unwrap()
            })
            .collect()
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        // Two fixed levels containing only this test's own generated files.
        for dir in [self.root.join("backups"), self.root.clone()] {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    if entry.file_type().is_ok_and(|t| t.is_file()) {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
            let _ = std::fs::remove_dir(dir);
        }
    }
}

#[test]
fn canonical_encoder_matches_independent_official_vector_and_canonical_edits() {
    let m = canonical();
    let mut expected = vec![12, 0, 0, 0, 1, 0, 0, 0, 65];
    expected.resize(64, 0);
    expected.extend_from_slice(&[0, 0, 81, 4, 37, 0, 63, 0, 0, 0, 145, 4]);
    assert_eq!(m.raw, expected);
    for mode in 0..=3 {
        let edited = macro_record(17, "新宏", mode, &sequence(), Some(&m.raw)).unwrap();
        assert_eq!(edited.mode, mode);
        assert!(edited.events == m.events);
        assert_eq!(edited.name, "新宏");
        assert_eq!(
            decode_macro(17, edited.raw.clone()).unwrap().raw,
            edited.raw
        );
    }
}

#[test]
fn opaque_native_read_save_file_export_import_write_preserves_every_byte() {
    let original = opaque();
    let a = Rig::new(original.clone());
    let b = Rig::new(canonical());
    let untouched = b.mouse.lock().unwrap().macros.clone();
    let state_before = a.mouse.lock().unwrap().state.clone();
    let read = a.transport.read_macro(2).unwrap().unwrap();
    assert!(read == original);
    a.transport
        .apply_hardware_for(0, action(&read, Some(read.raw.clone())))
        .unwrap();
    std::fs::create_dir_all(&a.root).unwrap();
    let path = a.root.join("macro.json");
    crate::mchose_macro_file(
        path.to_string_lossy().into(),
        read.slot,
        read.name.clone(),
        read.mode,
        read.events.clone(),
        Some(read.raw.clone()),
    )
    .unwrap();
    let mut imported: MouseMacro = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(imported == original);
    imported.slot = 7;
    a.transport
        .apply_hardware_for(0, action(&imported, Some(imported.raw.clone())))
        .unwrap();
    assert_eq!(
        a.transport.read_macro(7).unwrap().unwrap().raw,
        original.raw
    );
    let mouse = a.mouse.lock().unwrap();
    assert_eq!(mouse.macros[&2], original.raw);
    assert!(mouse.state == state_before);
    drop(mouse);
    assert_eq!(b.mouse.lock().unwrap().macros, untouched);
    assert!(b.mouse.lock().unwrap().writes.is_empty());
    for snapshot in a.backups() {
        assert!(snapshot.macros.iter().any(|m| m.raw == original.raw));
    }
}

#[test]
fn opaque_edits_and_unsupported_words_reject_without_native_set_or_file_overwrite() {
    let original = opaque();
    let rig = Rig::new(original.clone());
    for changed in 0..3 {
        let mut m = original.clone();
        if changed == 0 {
            m.name = "rename".into()
        } else if changed == 1 {
            m.mode = 2
        } else {
            m.events[1].delay_ms = 91
        }
        assert!(rig
            .transport
            .apply_hardware_for(0, action(&m, Some(original.raw.clone())))
            .is_err());
    }
    let mut new = canonical();
    new.events[0].delay_ms = 37;
    assert!(rig
        .transport
        .apply_hardware_for(0, action(&new, None))
        .is_err());
    new.events = sequence();
    new.events[1].code = 4;
    assert!(rig
        .transport
        .apply_hardware_for(0, action(&new, None))
        .is_err());
    assert!(rig.mouse.lock().unwrap().writes.is_empty());
    assert!(!rig.root.join("backups").exists());
    std::fs::create_dir_all(&rig.root).unwrap();
    let path = rig.root.join("original.json");
    std::fs::write(&path, b"keep original file").unwrap();
    assert!(crate::mchose_macro_file(
        path.to_string_lossy().into(),
        2,
        "changed".into(),
        original.mode,
        original.events,
        Some(original.raw)
    )
    .is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"keep original file");
}

#[test]
fn empty_baseline_cannot_bypass_referenced_macro_deletion_checks() {
    let rig = Rig::new(canonical());
    let empty = MouseMacro {
        slot: 2,
        name: String::new(),
        mode: 0,
        events: vec![],
        raw: vec![0; 64],
    };
    {
        let mut mouse = rig.mouse.lock().unwrap();
        mouse.state.buttons[4] = vec![33, 2, 0];
        mouse.state.raw_buttons = mouse.state.buttons.concat();
    }
    assert!(rig
        .transport
        .apply_hardware_for(0, action(&empty, Some(empty.raw.clone())))
        .is_err());
    assert!(rig.mouse.lock().unwrap().writes.is_empty());
    assert!(!rig.root.join("backups").exists());
}

#[test]
fn canonical_native_edit_chunks_and_receipt_are_exact() {
    let original = canonical();
    let rig = Rig::new(original.clone());
    let mut edited = original.clone();
    edited.name = "B".into();
    edited.mode = 3;
    edited.events[1].delay_ms = 81;
    let receipt = macro_record(
        2,
        &edited.name,
        edited.mode,
        &edited.events,
        Some(&original.raw),
    )
    .unwrap();
    let state = rig
        .transport
        .apply_hardware_for(0, action(&edited, Some(original.raw.clone())))
        .unwrap();
    assert_eq!(
        rig.transport.read_macro(2).unwrap().unwrap().raw,
        receipt.raw
    );
    let mouse = rig.mouse.lock().unwrap();
    assert_eq!(
        mouse.writes.iter().map(|v| v[3]).collect::<Vec<_>>(),
        [77, 76, 76, 78]
    );
    assert_eq!(
        mouse
            .writes
            .iter()
            .map(|v| u16::from_le_bytes([v[1], v[2]]))
            .collect::<Vec<_>>(),
        [0, 19, 38, 57]
    );
    drop(mouse);
    let value = serde_json::to_value(ApplyResult {
        state,
        saved_macro: Some(receipt.clone()),
    })
    .unwrap();
    assert_eq!(value["profile"], 0);
    assert_eq!(value["saved_macro"]["raw"], serde_json::json!(receipt.raw));
    assert!(value.get("state").is_none());
    assert!(serde_json::from_value::<HardwareState>(value).is_ok());
    assert_eq!(rig.backups()[0].macros[0].raw, original.raw);
}

#[test]
fn macro_chunk_failure_or_ack_without_readback_is_not_success_or_retry() {
    for failed_chunk in [false, true] {
        let original = canonical();
        let rig = Rig::new(original.clone());
        {
            let mut mouse = rig.mouse.lock().unwrap();
            if failed_chunk {
                mouse.fail_offset = Some(19)
            } else {
                mouse.ignore_commit = true
            }
        }
        let mut edited = original.clone();
        edited.events[1].delay_ms = 82;
        assert!(rig
            .transport
            .apply_hardware_for(0, action(&edited, Some(original.raw.clone())))
            .is_err());
        let mouse = rig.mouse.lock().unwrap();
        assert_eq!(mouse.writes.len(), if failed_chunk { 2 } else { 4 });
        assert_eq!(mouse.macros[&2], original.raw);
        drop(mouse);
        assert_eq!(rig.backups().len(), 1);
        assert_eq!(rig.backups()[0].macros[0].raw, original.raw);
    }
}

#[test]
#[ignore = "requires built-UI capture via OPEN_HUB_MCHOSE_MACRO_RAW_UI_CAPTURE"]
fn actual_ui_macro_save_export_import_replays_through_production_codec() {
    #[derive(Deserialize)]
    struct Request {
        id: String,
        #[serde(rename = "expectedProfile")]
        board: u8,
        action: Action,
        expected_success: bool,
        expected_raw: Option<Vec<u8>>,
    }
    #[derive(Deserialize)]
    struct ExportArgs {
        slot: u8,
        name: String,
        mode: u8,
        events: Vec<MacroEvent>,
        raw: Option<Vec<u8>>,
    }
    #[derive(Deserialize)]
    struct Export {
        args: ExportArgs,
        expected_raw: Vec<u8>,
    }
    #[derive(Deserialize)]
    struct Capture {
        actions: Vec<Request>,
        exports: Vec<Export>,
    }
    let path = std::env::var("OPEN_HUB_MCHOSE_MACRO_RAW_UI_CAPTURE").unwrap();
    let bytes = std::fs::read(path).unwrap();
    assert!(bytes.len() < 1024 * 1024);
    let capture: Capture = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(capture.actions.len(), 6);
    assert_eq!(capture.exports.len(), 2);
    let mut original = opaque();
    original.slot = 0;
    let a = Rig::new(original.clone());
    let b = Rig::new(canonical());
    let mut normal = canonical();
    normal.slot = 1;
    a.mouse.lock().unwrap().macros.insert(1, normal.raw);
    let b_before = b.mouse.lock().unwrap().macros.clone();
    let state_before = a.mouse.lock().unwrap().state.clone();
    let mut successes = 0;
    let mut failures = 0;
    for request in capture.actions {
        assert_eq!(request.id, "mchose-A");
        assert_eq!(request.board, 0);
        let (slot, record) = match &request.action {
            Action::Macro {
                slot,
                name,
                mode,
                events,
                raw,
            } => (
                *slot,
                macro_record(*slot, name, *mode, events, raw.as_deref()),
            ),
            _ => panic!("unexpected non-macro UI request"),
        };
        let writes_before = a.mouse.lock().unwrap().writes.len();
        let before = a.mouse.lock().unwrap().macros.clone();
        let result = a
            .transport
            .apply_hardware_for(request.board, request.action);
        if request.expected_success {
            successes += 1;
            assert!(result.is_ok());
            let expected = request.expected_raw.unwrap();
            assert_eq!(record.unwrap().raw, expected);
            assert_eq!(a.transport.read_macro(slot).unwrap().unwrap().raw, expected);
            assert_eq!(a.mouse.lock().unwrap().macros[&0], original.raw);
        } else {
            failures += 1;
            assert!(result.is_err());
            assert!(record.is_err());
            assert_eq!(a.mouse.lock().unwrap().writes.len(), writes_before);
            assert_eq!(a.mouse.lock().unwrap().macros, before);
        }
        assert!(a.mouse.lock().unwrap().state == state_before);
        assert_eq!(b.mouse.lock().unwrap().macros, b_before);
        assert!(b.mouse.lock().unwrap().writes.is_empty());
    }
    assert_eq!((successes, failures), (5, 1));
    for (i, file) in capture.exports.into_iter().enumerate() {
        let path = a.root.join(format!("export-{i}.json"));
        let p = file.args;
        crate::mchose_macro_file(
            path.to_string_lossy().into(),
            p.slot,
            p.name,
            p.mode,
            p.events,
            p.raw,
        )
        .unwrap();
        let record: MouseMacro = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(record.raw, file.expected_raw);
    }
}
