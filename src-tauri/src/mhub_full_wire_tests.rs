// Full known-field conversion replay over the existing production restore path.
// This fixture starts with no existing macros. Production import is not enabled:
// its future commit path must consume macro_writes rather than reupload all slots.
use super::*;
use crate::{
    hidpp::{Error, Result},
    mchose::Transport,
};
use std::sync::{Arc, Mutex};

struct Mouse {
    state: native::HardwareState,
    other_keys: [Vec<Vec<u8>>; 2],
    macros: BTreeMap<u8, Vec<u8>>,
    staged: BTreeMap<u8, Vec<u8>>,
    names: BTreeMap<u8, Vec<u8>>,
    requests: Vec<(u16, Vec<u8>)>,
}
impl Mouse {
    fn new() -> Self {
        let mut state = baseline().state;
        state.adaptive.clear();
        Self {
            other_keys: [state.buttons.clone(), state.buttons.clone()],
            state,
            macros: BTreeMap::new(),
            staged: BTreeMap::new(),
            names: BTreeMap::new(),
            requests: vec![],
        }
    }
    fn request(&mut self, cmd: u16, args: &[u8]) -> Result<Vec<u8>> {
        self.requests.push((cmd, args.to_vec()));
        Ok(match cmd {
            0x0900 => {
                let mut r = vec![0; 16];
                r[..4].copy_from_slice(&[0x37, 0x38, 0x26, 0x40]);
                r[10] = 2;
                r[12] = 80;
                r
            }
            2 => self.state.raw_settings.clone(),
            3 => {
                if args[1] == 0 {
                    self.state.raw_x.clone()
                } else {
                    self.state.raw_y.clone()
                }
            }
            1 => {
                if args[0] == 0 {
                    self.state.raw_buttons.clone()
                } else {
                    self.other_keys[args[0] as usize - 1].concat()
                }
            }
            9 => vec![self.state.lod],
            0x000a | 0x010a | 0x010b => panic!("A7 has no adaptive rate in this OEM build"),
            0x0901 => vec![0, if args[0] == 0 { 23 } else { 6 }],
            0x0905 => {
                let mut b = [0; 60];
                let name = self.state.names[args[0] as usize].as_bytes();
                b[..name.len()].copy_from_slice(name);
                b[args[1] as usize..args[1] as usize + args[2] as usize].to_vec()
            }
            0x090c => match self.macros.get(&args[0]) {
                None => vec![0],
                Some(raw) => {
                    let at = u16::from_le_bytes([args[1], args[2]]) as usize;
                    let n = args[3] as usize;
                    [vec![args[3]], raw[at..at + n].to_vec()].concat()
                }
            },
            0x090b => {
                let slot = args[0];
                let at = u16::from_le_bytes([args[1], args[2]]) as usize;
                let n = (args[3] >> 2) as usize;
                assert_eq!(n, args.len() - 4);
                assert!(n <= 19);
                if args[3] & 3 == 1 {
                    assert_eq!(at, 0);
                    self.staged.insert(slot, vec![]);
                }
                let data = self.staged.get_mut(&slot).unwrap();
                assert_eq!(data.len(), at);
                data.extend_from_slice(&args[4..]);
                if args[3] & 3 == 2 {
                    let raw = self.staged.remove(&slot).unwrap();
                    assert_eq!(
                        raw.len(),
                        64 + u16::from_le_bytes([raw[0], raw[1]]) as usize
                    );
                    self.macros.insert(slot, raw);
                }
                vec![]
            }
            0x0103 => {
                assert_eq!(args[0], 0);
                let table = [
                    vec![0, args[1], args[3], args[4], args[2]],
                    args[5..].to_vec(),
                ]
                .concat();
                if args[1] == 0 {
                    self.state.raw_x = table
                } else {
                    self.state.raw_y = table
                }
                vec![]
            }
            0x0102 => {
                assert_eq!(args[0], 0);
                assert_eq!(args.len(), 19);
                self.state.raw_settings = args.to_vec();
                vec![]
            }
            0x0101 => {
                assert_eq!(args[0], 0);
                assert_eq!(args[2], 11);
                self.state.buttons[args[1] as usize] = args[3..].to_vec();
                self.state.raw_buttons = self.state.buttons.concat();
                vec![]
            }
            0x0109 => {
                assert_eq!(args[0], 0);
                self.state.lod = args[1];
                vec![]
            }
            0x0910 => {
                let p = args[0];
                let at = args[1] as usize;
                let n = (args[2] >> 2) as usize;
                assert_eq!(n, args.len() - 3);
                if args[2] & 1 != 0 {
                    self.names.insert(p, vec![0; 60]);
                }
                self.names.get_mut(&p).unwrap()[at..at + n].copy_from_slice(&args[3..]);
                if args[2] & 2 != 0 {
                    self.state.names[p as usize] =
                        String::from_utf8(self.names.remove(&p).unwrap())
                            .unwrap()
                            .trim_end_matches('\0')
                            .into();
                }
                vec![]
            }
            _ => {
                return Err(Error::Other(format!(
                    "unexpected isolated M HUB replay command {cmd:04x}"
                )))
            }
        })
    }
}
struct Rig {
    t: Transport,
    mouse: Arc<Mutex<Mouse>>,
    directory: std::path::PathBuf,
}
impl Rig {
    fn new() -> Self {
        let mouse = Arc::new(Mutex::new(Mouse::new()));
        let source = mouse.clone();
        let directory =
            std::env::temp_dir().join(format!("open-hub-mhub-wire-{}", crate::profiles::new_id()));
        let t = Transport::memory(
            move |cmd, args| source.lock().unwrap().request(cmd, args),
            directory.clone(),
        );
        Self {
            t,
            mouse,
            directory,
        }
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        if let Ok(files) = std::fs::read_dir(&self.directory) {
            for file in files.flatten() {
                let _ = std::fs::remove_file(file.path());
            }
            let _ = std::fs::remove_dir(&self.directory);
        }
    }
}
#[test]
fn converted_known_template_replays_through_production_restore_codec_on_one_target() {
    let plan = plan();
    let a = Rig::new();
    let b = Rig::new();
    let before = a.t.full_snapshot().unwrap();
    let all = boards(&before);
    let compiled = compile(&plan, &before, &all).unwrap();
    let expected = compiled.snapshot.state.clone();
    let macro_bytes = compiled.macro_writes[0].raw.clone();
    let device = crate::mchose_target::simulated_device("mchose-A");
    let target = crate::mchose_target::read_context(&device, &device.connection_key).unwrap();
    let actual = crate::mchose_target::with_target(&device, &a.t, &target, |t| {
        t.restore_snapshot(compiled.snapshot)
    })
    .unwrap();
    assert!(actual == expected);
    let mouse = a.mouse.lock().unwrap();
    assert_eq!(mouse.macros.get(&0), Some(&macro_bytes));
    assert_eq!(
        mouse.other_keys,
        [before.state.buttons.clone(), before.state.buttons.clone()]
    );
    let settings = mouse
        .requests
        .iter()
        .filter(|(cmd, _)| *cmd == 0x0102)
        .collect::<Vec<_>>();
    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].1, expected.raw_settings);
    let chunks = mouse
        .requests
        .iter()
        .filter(|(cmd, _)| *cmd == 0x090b)
        .map(|(_, args)| args)
        .collect::<Vec<_>>();
    assert_eq!(
        chunks
            .iter()
            .map(|a| u16::from_le_bytes([a[1], a[2]]))
            .collect::<Vec<_>>(),
        [0, 19, 38, 57, 76, 95]
    );
    assert_eq!(chunks.last().unwrap()[3], 6);
    assert!(mouse
        .requests
        .iter()
        .filter(|(cmd, _)| [0x0101, 0x0102, 0x0103, 0x0109, 0x0910].contains(cmd))
        .all(|(_, args)| args[0] == 0));
    assert!(b.mouse.lock().unwrap().requests.is_empty());
    drop(mouse);
    let files = std::fs::read_dir(&a.directory).unwrap().collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    let backup: Snapshot =
        serde_json::from_slice(&std::fs::read(files[0].as_ref().unwrap().path()).unwrap()).unwrap();
    assert!(backup.state == before.state);
    assert!(backup.macros.is_empty());
}
#[test]
fn wrong_target_cannot_replay_a_compiled_template_or_create_backup() {
    let a = Rig::new();
    let device = crate::mchose_target::simulated_device("mchose-A");
    let target = crate::mchose_target::read_context(&device, &device.connection_key).unwrap();
    let other = crate::mchose_target::simulated_device("mchose-B");
    let before = baseline();
    let compiled = compile(&plan(), &before, &boards(&before)).unwrap();
    assert!(
        crate::mchose_target::with_target(&other, &a.t, &target, |t| t
            .restore_snapshot(compiled.snapshot))
        .is_err()
    );
    assert!(a.mouse.lock().unwrap().requests.is_empty());
    assert!(!a.directory.exists());
}
