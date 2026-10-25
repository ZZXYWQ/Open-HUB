//! In-memory firmware endpoints. No HID enumeration, filesystem or input injection.
use super::*;
use std::collections::VecDeque;
#[derive(Clone)]
pub(super) struct Memory {
    pub profile: u8,
    pub config: Vec<u8>,
    pub keys: Vec<u8>,
    pub light: Vec<u8>,
    pub slots: BTreeMap<u8, Vec<u8>>,
    pub selector: u8,
    pub pending: Vec<u8>,
    pub replies: VecDeque<Vec<u8>>,
    pub fail_reads: usize,
    pub fail_read_after_write: bool,
    pub corrupt: bool,
    pub m: &'static Model,
}
pub(super) struct Transport {
    pub memory: Arc<Mutex<Memory>>,
    pub log: MockLog,
}
impl HidTransport for Transport {
    fn write(&mut self, p: &[u8]) -> Result<usize> {
        self.log
            .lock()
            .unwrap()
            .push(transport::MockRecord::Write(p.to_vec()));
        let mut m = self.memory.lock().unwrap();
        if m.m.brand != "asus" || p.len() != 64 {
            return Err(invalid("simulation output"));
        }
        let cmd = u16::from_le_bytes([p[0], p[1]]);
        let mut response = vec![0; 64];
        response[0] = 0x12;
        response[1] = 0xaa;
        match cmd {
            0x0012 => {
                response[10] = m.profile;
                response[11] = m.config[63] + 1;
            }
            0x0412 => response = m.config.clone(),
            0x0512 => response = m.keys.clone(),
            0x0312 => response = m.light.clone(),
            0x3151 => {
                let field = p[2] as usize;
                if field < 2 {
                    m.config[4 + field * 2..6 + field * 2].copy_from_slice(&p[4..6]);
                } else if field == 2 {
                    m.config[8] = p[4];
                }
            }
            0x2151 => {
                let id = [0xf0, 0xf1, 0xf2, 0xe6]
                    .iter()
                    .position(|n| *n == p[4])
                    .ok_or(invalid("simulation ASUS button source"))?;
                let slot = [0, 1, 2, 5][id];
                m.keys[4 + 2 * slot..6 + 2 * slot].copy_from_slice(&p[6..8]);
            }
            0x2851 => m.light[4..9].copy_from_slice(&p[4..9]),
            0x0350 => {}
            _ => return Err(invalid("simulation ASUS command")),
        };
        m.replies.push_back(response);
        Ok(p.len())
    }
    fn read_timeout(&mut self, b: &mut [u8], _: i32) -> Result<usize> {
        let mut m = self.memory.lock().unwrap();
        if m.fail_reads > 0 {
            m.fail_reads -= 1;
            return Err(Error::Timeout);
        }
        if let Some(p) = m.replies.pop_front() {
            let n = p.len().min(b.len());
            b[..n].copy_from_slice(&p[..n]);
            Ok(n)
        } else {
            Ok(0)
        }
    }
    fn send_feature_report(&mut self, p: &[u8]) -> Result<()> {
        self.log
            .lock()
            .unwrap()
            .push(transport::MockRecord::SetFeature(p.to_vec()));
        let mut m = self.memory.lock().unwrap();
        match m.m.brand.as_str() {
            "gskill" => match p[0] {
                12 => {
                    m.pending = p.to_vec();
                    if p[2] == 12 || p[2] == 11 {
                        m.selector = p[3];
                    }
                }
                5 => m.config = p.to_vec(),
                4 => {
                    let slot = m.selector;
                    m.slots.insert(slot, p.to_vec());
                }
                _ => return Err(invalid("simulation G.Skill report")),
            },
            "marsgaming" => {
                if p[0] == 2 {
                    m.pending = p.to_vec();
                    if p[1] == 2 && p[2] & 0xf8 == 0x48 {
                        m.config[63] = p[8]
                    }
                    if p[1] == 2 && p[2] == 0xf1 {
                        m.light = p.to_vec()
                    }
                } else if p[0] == 3 {
                    m.config = p.to_vec()
                } else if p[0] == 4 {
                    m.keys = p.to_vec()
                } else {
                    return Err(invalid("simulation MM4 report"));
                }
            }
            _ => match p[0] {
                4 => {
                    m.selector = p[2];
                }
                6 => m.config = p.to_vec(),
                7 => m.keys = p.to_vec(),
                8 | 9 => {
                    let slot = if m.m.brand == "roccat" { p[4] } else { p[3] };
                    m.slots.insert(slot, p.to_vec());
                }
                _ => return Err(invalid("simulation feature report")),
            },
        }
        let settings_write = match m.m.brand.as_str() {
            "gskill" => p[0] == 5 || p[0] == 4,
            "marsgaming" => p[0] != 2 || p[1] == 2,
            _ => p[0] != 4,
        };
        if settings_write && m.fail_read_after_write {
            m.fail_read_after_write = false;
            m.fail_reads = 1;
        }
        Ok(())
    }
    fn get_feature_report(&mut self, b: &mut [u8]) -> Result<usize> {
        let id = b[0];
        self.log
            .lock()
            .unwrap()
            .push(transport::MockRecord::GetFeature(id));
        let mut m = self.memory.lock().unwrap();
        if m.fail_reads > 0 {
            m.fail_reads -= 1;
            return Err(Error::Timeout);
        }
        let mut p = match m.m.brand.as_str() {
            "gskill" => match id {
                0 => {
                    let mut r = vec![0; 9];
                    r[1] = 0xb0;
                    r[2] = m.pending[2];
                    r[3] = if m.pending[2] == 7 { m.profile } else { 0 };
                    r
                }
                5 => m.config.clone(),
                4 => {
                    let mut r = m
                        .slots
                        .get(&m.selector)
                        .cloned()
                        .unwrap_or_else(|| blank_macro(m.m, m.profile, m.selector % 10));
                    r[0] = 0xb0;
                    r[1] = 4;
                    r
                }
                _ => return Err(invalid("simulation G.Skill read")),
            },
            "marsgaming" => {
                let cmd = m.pending[2];
                let index = m.pending[3];
                let size = m.pending[4];
                let mut r = match cmd {
                    0x4f => m.config.clone(),
                    0x90 => m.keys.clone(),
                    0xf1 => m.light.clone(),
                    0x43 => {
                        let mut r = vec![0; 16];
                        r[8] = m.profile;
                        r
                    }
                    x if x & 0xf8 == 0x48 => {
                        let mut r = vec![0; 16];
                        r[8] = m.config[63];
                        r
                    }
                    _ => return Err(invalid("simulation MM4 read")),
                };
                r[0] = id;
                r[1] = 3;
                r[2] = cmd;
                r[3] = index;
                r[4] = size;
                r
            }
            _ => match id {
                4 => vec![4, 1, 0],
                5 => vec![5, 3, m.profile],
                6 => m.config.clone(),
                7 => m.keys.clone(),
                8 | 9 => m
                    .slots
                    .get(&m.selector)
                    .cloned()
                    .unwrap_or_else(|| blank_macro(m.m, m.profile, m.selector)),
                _ => return Err(invalid("simulation read id")),
            },
        };
        if m.corrupt {
            let n = p.len() - 1;
            p[n] ^= 1;
        }
        let n = p.len().min(b.len());
        b[..n].copy_from_slice(&p[..n]);
        Ok(n)
    }
}
fn blank_macro(m: &Model, profile: u8, index: u8) -> Vec<u8> {
    let mut p = vec![
        0;
        if m.brand == "gskill" {
            2052
        } else if m.brand == "roccat" {
            2082
        } else {
            130
        }
    ];
    match m.brand.as_str() {
        "gskill" => {
            p[0] = 4;
            p[2] = profile * 10 + index;
            protocol::gskill_checksum(&mut p);
        }
        "roccat" => {
            p[0] = 8;
            p[1] = 0x22;
            p[2] = 8;
            p[3] = profile;
            p[4] = index;
            protocol::checksum(&mut p);
        }
        _ => {
            p[0] = 9;
            p[1] = 0x82;
            p[2] = profile;
            p[3] = index;
        }
    }
    p
}
pub(super) fn memory(m: &'static Model) -> Memory {
    let mut config = vec![
        0;
        match m.brand.as_str() {
            "gskill" => 644,
            "asus" | "marsgaming" => 64,
            "roccat" => 31,
            _ => 40,
        }
    ];
    let mut keys = vec![
        0;
        match m.brand.as_str() {
            "gskill" => 644,
            "asus" => 64,
            "marsgaming" => 1024,
            "roccat" => 59,
            _ => 50,
        }
    ];
    let mut light = vec![];
    match m.brand.as_str() {
        "asus" => {
            config[0] = 0x12;
            config[1] = 0xaa;
            config[4..8].copy_from_slice(&[15, 0, 31, 0]);
            config[8] = 3;
            config[63] = 0;
            keys[0] = 0x12;
            keys[1] = 0xaa;
            light = vec![0; 64];
            light[..2].copy_from_slice(&[0x12, 0xaa]);
            light[4..9].copy_from_slice(&[0, 4, 80, 160, 240]);
        }
        "gskill" => {
            config[0] = 5;
            config[1] = 5;
            config[4] = 0;
            config[7] = 0x20;
            config[8..12].copy_from_slice(&[16, 16, 32, 32]);
            for (n, v) in config[18..303].iter_mut().enumerate() {
                *v = (n % 251) as u8;
            }
            config[388..400].copy_from_slice(&[77, 0, 88, 0, 45, 0, 55, 0, 56, 0, 48, 0]);
            keys = config.clone();
        }
        "marsgaming" => {
            config[..10].copy_from_slice(&[3, 3, 0x4f, 0, 0x2a, 0, 0, 0, 2, 0]);
            for (i, dpi) in [16u16, 32].iter().enumerate() {
                config[10 + 8 * i] = 1;
                config[11 + 8 * i..13 + 8 * i].copy_from_slice(&dpi.to_le_bytes());
                config[13 + 8 * i..15 + 8 * i].copy_from_slice(&dpi.to_le_bytes());
            }
            config[63] = 1;
            keys[..9].copy_from_slice(&[4, 3, 0x90, 0, 0x4d, 0, 0, 0, 19]);
            light = vec![0; 16];
            light[..13].copy_from_slice(&[4, 3, 0xf1, 0, 6, 0, 0, 0, 100, 150, 200, 3, 0]);
        }
        _ => {
            config[0] = 6;
            config[1] = config.len() as u8;
            keys[0] = 7;
            keys[1] = keys.len() as u8;
            let (mask, x, y, active, rate) = if m.brand == "roccat" {
                (6, 7, 13, 12, 19)
            } else {
                (5, 6, 12, 18, 26)
            };
            config[mask] = 3;
            config[x] = 16;
            config[x + 1] = 32;
            config[y] = 16;
            config[y + 1] = 32;
            config[active] = 0;
            config[rate] = 3;
        }
    }
    for id in 1..=m.buttons {
        let action = if id <= 5 && !(m.brand == "asus" && id == 4) {
            default_action(id)
        } else {
            ButtonAction::Dpi(DpiFunction::Cycle)
        };
        let v = encode_action(m, id, &action).unwrap();
        protocol::replace_row(m, &mut keys, id, &v).unwrap();
    }
    if m.brand == "gskill" {
        config = keys.clone();
        protocol::gskill_checksum(&mut config);
        keys = config.clone();
    }
    if m.brand == "roccat" {
        protocol::checksum(&mut keys);
    }
    Memory {
        profile: 0,
        config,
        keys,
        light,
        slots: BTreeMap::new(),
        selector: 0,
        pending: vec![0; 9],
        replies: VecDeque::new(),
        fail_reads: 0,
        fail_read_after_write: false,
        corrupt: false,
        m,
    }
}
pub(super) fn open(v: u16, p: u16) -> Result<(Wired, MockLog, Arc<Mutex<Memory>>)> {
    let m = model(v, p).ok_or(invalid("simulation exact model"))?;
    let memory = Arc::new(Mutex::new(memory(m)));
    let log = Arc::new(Mutex::new(vec![]));
    let io = Transport {
        memory: memory.clone(),
        log: log.clone(),
    };
    let i = HidInterfaceInfo {
        path: format!("simulation:{}:{p:04x}", m.brand),
        vendor_id: v,
        product_id: p,
        interface_number: 2,
        usage_page: 0xff00,
        usage: 1,
        bus: BusKind::Usb,
        ..Default::default()
    };
    Ok((Wired::open(Box::new(io), &i, false)?, log, memory))
}
pub(super) fn device(v: u16, p: u16) -> Result<(Box<dyn Device>, MockLog)> {
    let (d, log, _) = open(v, p)?;
    Ok((Box::new(d), log))
}
