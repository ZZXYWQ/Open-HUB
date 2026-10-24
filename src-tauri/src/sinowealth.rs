//! SinoWealth wired protocol, independently implemented from libratbag's MIT driver.
//! Exact VID/PID + firmware allowlist; never used as a fallback for an unknown mouse.
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    ffi::CString,
    path::PathBuf,
    sync::Mutex,
};
type R<T> = Result<T, String>;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Model {
    pub vendor: u16,
    pub product: u16,
    pub firmware: String,
    pub name: String,
    pub brand: String,
    pub buttons: usize,
    pub profiles: u8,
    pub sensors: Vec<u8>,
}
pub fn models() -> Vec<Model> {
    [
        (
            0x258a,
            0x0036,
            "V103",
            "Glorious Model O/O-",
            "Glorious",
            6,
            1,
            vec![6],
        ),
        (
            0x258a,
            0x0033,
            "V102",
            "Glorious Model D",
            "Glorious",
            6,
            1,
            vec![6],
        ),
        (
            0x3794,
            0xa000,
            "1009",
            "Glorious Model O Eternal",
            "Glorious",
            6,
            1,
            vec![6],
        ),
        (
            0x258a,
            0x0027,
            "V102",
            "Glorious Model O (old firmware)",
            "Glorious",
            6,
            1,
            vec![6],
        ),
        (
            0x258a,
            0x0027,
            "V161",
            "G-Wolves Hati HT-M Wired",
            "G-Wolves",
            6,
            1,
            vec![6, 15],
        ),
        (
            0x258a,
            0x0027,
            "3106",
            "DreamMachines DM5 Blink",
            "DreamMachines",
            8,
            1,
            vec![15],
        ),
        (
            0x258a,
            0x0027,
            "3110",
            "Genesis Xenon 770",
            "Genesis",
            14,
            1,
            vec![14],
        ),
        (
            0x258a,
            0x0029,
            "V127",
            "Machenike M620",
            "Machenike",
            6,
            1,
            vec![15],
        ),
        (
            0x258a,
            0x0029,
            "V287",
            "Mad Dog GM905",
            "Mad Dog",
            14,
            1,
            vec![15],
        ),
        (
            0x258a,
            0x0051,
            "2642",
            "T-Dagger Imperial T-TGM310",
            "T-Dagger",
            8,
            2,
            vec![8],
        ),
        (
            0x258a,
            0x1007,
            "9677",
            "Marvo Scorpion G961",
            "Marvo",
            6,
            1,
            vec![14],
        ),
    ]
    .into_iter()
    .map(
        |(vendor, product, fw, name, brand, buttons, profiles, sensors)| Model {
            vendor,
            product,
            firmware: fw.into(),
            name: name.into(),
            brand: brand.into(),
            buttons,
            profiles,
            sensors,
        },
    )
    .collect()
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub kind: String,
    pub code: u8,
    pub down: bool,
    pub delay: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Macro {
    pub slot: u8,
    pub repeat: u8,
    pub events: Vec<Event>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    pub model: Model,
    pub profile: u8,
    pub dpi: Vec<[u16; 2]>,
    pub active: usize,
    pub rate: u32,
    pub buttons: Vec<[u8; 4]>,
    #[serde(default)]
    pub macros: Vec<Macro>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Peer {
    pub id: String,
    pub model: Model,
    pub serial: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct View {
    pub peer: Peer,
    pub plan: Plan,
    pub free_macro_slots: Vec<u8>,
    pub macro_readback: bool,
    pub backup: Option<String>,
}
fn max_dpi(sensor: u8) -> R<u16> {
    match sensor {
        6 => Ok(12000),
        8 => Ok(7200),
        14 => Ok(10200),
        15 => Ok(16000),
        _ => Err("未识别的传感器".into()),
    }
}
fn standard(m: &Model) -> R<Model> {
    models()
        .into_iter()
        .find(|x| x == m)
        .ok_or_else(|| "型号、固件或能力记录不在协议白名单".into())
}
pub fn validate(p: &Plan) -> R<()> {
    standard(&p.model)?;
    if p.profile >= p.model.profiles
        || p.dpi.is_empty()
        || p.dpi.len() > 8
        || p.active >= p.dpi.len()
        || ![125, 250, 500, 1000].contains(&p.rate)
        || p.buttons.len() != p.model.buttons
    {
        return Err("配置档位、按键数或回报率无效".into());
    }
    let max = p
        .model
        .sensors
        .iter()
        .map(|s| max_dpi(*s))
        .collect::<R<Vec<_>>>()?
        .into_iter()
        .max()
        .unwrap();
    for xy in &p.dpi {
        for n in xy {
            if *n < 100 || *n > max || n % 100 != 0 {
                return Err(format!("DPI 必须为100的倍数，范围100–{max}"));
            }
        }
    }
    if !p.buttons.iter().any(|b| b == &[0x11, 1, 0, 0]) {
        return Err("必须保留至少一个左键".into());
    }
    for b in &p.buttons {
        match b[0] {
            0x11 if [1, 2, 4, 8, 16].contains(&b[1]) && b[2..] == [0, 0] => {}
            0x12 if [1, 255].contains(&b[1]) && b[2..] == [0, 0] => {}
            0x21 if b[1] <= 15 && b[2] > 0 && b[2] <= 231 && b[3] == 0 => {}
            0x41 if b[1] <= 2 && b[2..] == [0, 0] => {}
            0x50 if [1, 4, 7].contains(&b[1]) && b[2..] == [0, 0] => {}
            0x22 => {}
            0x31 if b[1] <= 5 && b[2] > 0 => {}
            0x42 if b[1] > 0 && b[2..] == [0, 0] => {}
            0x70 if b[1] < (p.model.buttons * p.model.profiles as usize) as u8
                && [1, 2, 4].contains(&b[2])
                && b[3] <= 100 => {}
            0 if b[1..] == [0, 0, 0] => {}
            _ => return Err("按键动作无效或尚未实现，不发送未知命令".into()),
        }
    }
    let mut slots = HashSet::new();
    if p.macros.len() > p.model.buttons * p.model.profiles as usize {
        return Err("宏数量过多".into());
    }
    for m in &p.macros {
        if !slots.insert(m.slot)
            || m.slot as usize >= p.model.buttons * p.model.profiles as usize
            || m.repeat == 0
            || m.repeat > 100
            || m.events.is_empty()
            || m.events.len() > 168
        {
            return Err("宏索引、次数或事件数无效".into());
        }
        let mut held = HashSet::new();
        for e in &m.events {
            if !["key", "mouse"].contains(&e.kind.as_str())
                || e.code == 0
                || e.delay == 0
                || e.delay > 255
                || e.kind == "mouse" && ![1, 2, 4].contains(&e.code)
                || e.kind == "key" && e.code > 231
            {
                return Err("宏事件或延迟无效，延迟范围1–255ms".into());
            }
            let k = (e.kind.clone(), e.code);
            if e.down {
                if !held.insert(k) {
                    return Err("宏包含重复按下".into());
                }
            } else if !held.remove(&k) {
                return Err("宏释放了未按下的键".into());
            }
        }
        if !held.is_empty() {
            return Err("宏有未释放的按键".into());
        }
    }
    for m in &p.macros {
        if !p.buttons.iter().any(|b| *b == [0x70, m.slot, 1, m.repeat]) {
            return Err("宏必须绑定为有限次数播放的按键".into());
        }
    }
    Ok(())
}
pub trait Link {
    fn set(&mut self, b: &[u8]) -> R<()>;
    fn get(&mut self, id: u8, size: usize) -> R<Vec<u8>>;
}
struct Usb(hidapi::HidDevice);
impl Link for Usb {
    fn set(&mut self, b: &[u8]) -> R<()> {
        self.0.send_feature_report(b).map_err(|e| e.to_string())
    }
    fn get(&mut self, id: u8, n: usize) -> R<Vec<u8>> {
        let mut b = vec![0; n];
        b[0] = id;
        let got = self
            .0
            .get_feature_report(&mut b)
            .map_err(|e| e.to_string())?;
        b.truncate(got);
        Ok(b)
    }
}
fn cmd<T: Link>(io: &mut T, c: u8) -> R<Vec<u8>> {
    io.set(&[5, c, 0, 0, 0, 0])?;
    let b = io.get(5, 6)?;
    if b.len() != 6 || b[0] != 5 || b[1] != c {
        return Err("命令响应身份或长度不符".into());
    }
    Ok(b)
}
fn raw<T: Link>(io: &mut T, c: u8, report: u8) -> R<Vec<u8>> {
    io.set(&[5, c, 0, 0, 0, 0])?;
    let mut b = io.get(report, 520)?;
    let n = b.len();
    if n < 8 || b[0] != report || b[1] != c {
        return Err("配置响应与请求不符".into());
    }
    if c & 15 == 1 && !(123..=167).contains(&n) || c & 15 == 2 && n != 88 {
        return Err(format!("不支持的配置报文长度 {n}"));
    }
    b.resize(520, 0);
    b[3] = (n - 8) as u8;
    Ok(b)
}
#[derive(Clone, Serialize, Deserialize)]
struct Snapshot {
    config: Vec<u8>,
    buttons: Vec<u8>,
    all_buttons: Vec<Vec<u8>>,
    report: u8,
    sensor: u8,
}
fn snapshot<T: Link>(io: &mut T, m: &Model, profile: u8, report: u8) -> R<Snapshot> {
    let config = raw(io, 0x11 + profile * 16, report)?;
    let sensor = config[9];
    if !m.sensors.contains(&sensor) {
        return Err("传感器与型号白名单不符".into());
    }
    let mut all_buttons = Vec::new();
    for i in 0..m.profiles {
        all_buttons.push(raw(io, 0x12 + i * 16, report)?)
    }
    let buttons = all_buttons[profile as usize].clone();
    Ok(Snapshot {
        config,
        buttons,
        all_buttons,
        report,
        sensor,
    })
}
fn decode(s: &Snapshot, m: &Model, profile: u8) -> R<Plan> {
    let c = &s.config;
    let mut dpi = Vec::new();
    let independent = c[10] & 0x80 != 0;
    let convert =
        |n: u8| -> u16 { (n as u16 + if [6, 14].contains(&s.sensor) { 1 } else { 0 }) * 100 };
    for i in 0..8 {
        if c[12] & (1 << i) != 0 {
            continue;
        }
        let x = convert(c[13 + if independent { i * 2 } else { i }]);
        let y = if independent {
            convert(c[14 + i * 2])
        } else {
            x
        };
        dpi.push([x, y]);
    }
    let active = (c[11] >> 4).checked_sub(1).ok_or("DPI 当前档位无效")? as usize;
    let rate = *[(1, 125), (2, 250), (3, 500), (4, 1000)]
        .iter()
        .find(|(n, _)| *n == c[10] & 15)
        .ok_or("回报率无效")?;
    let buttons = (0..m.buttons)
        .map(|i| s.buttons[8 + i * 4..12 + i * 4].try_into().unwrap())
        .collect();
    let p = Plan {
        model: m.clone(),
        profile,
        dpi,
        active,
        rate: rate.1,
        buttons,
        macros: vec![],
    };
    standard(m)?;
    if p.dpi.is_empty()
        || p.dpi.len() > 8
        || p.active >= p.dpi.len()
        || p.dpi
            .iter()
            .flatten()
            .any(|v| *v < 100 || *v > max_dpi(s.sensor).unwrap_or(0))
    {
        return Err("读取的DPI布局无效".into());
    }
    // Reading must remain possible when the existing assignment lacks a left button.
    // Full write validation is enforced separately, so the user can repair it.
    Ok(p)
}
fn encode(s: &Snapshot, p: &Plan) -> R<(Vec<u8>, Vec<u8>)> {
    validate(p)?;
    if !p.model.sensors.contains(&s.sensor) {
        return Err("传感器发生变化".into());
    }
    let mut c = s.config.clone();
    let mut b = s.buttons.clone();
    let independent = p.dpi.iter().any(|d| d[0] != d[1]);
    c[10] = (c[10] & 0x70)
        | if independent { 0x80 } else { 0 }
        | match p.rate {
            125 => 1,
            250 => 2,
            500 => 3,
            1000 => 4,
            _ => unreachable!(),
        };
    c[11] = p.dpi.len() as u8 | ((p.active as u8 + 1) << 4);
    c[12] = !((1u16 << p.dpi.len()) - 1) as u8;
    let convert = |n: u16| -> R<u8> {
        if n > max_dpi(s.sensor)? {
            return Err("DPI 超出实际传感器能力".into());
        }
        Ok((n / 100 - if [6, 14].contains(&s.sensor) { 1 } else { 0 }) as u8)
    };
    for (i, xy) in p.dpi.iter().enumerate() {
        if independent {
            c[13 + i * 2] = convert(xy[0])?;
            c[14 + i * 2] = convert(xy[1])?
        } else {
            c[13 + i] = convert(xy[0])?
        }
    }
    for (i, a) in p.buttons.iter().enumerate() {
        b[8 + i * 4..12 + i * 4].copy_from_slice(a)
    }
    Ok((c, b))
}
fn macro_packet(report: u8, m: &Macro) -> Vec<u8> {
    let mut b = vec![0; 520];
    b[0] = report;
    b[1] = 0x30;
    b[2] = 2;
    b[8] = m.slot;
    b[10] = m.events.len() as u8;
    for (i, e) in m.events.iter().enumerate() {
        b[11 + i * 3] = match (e.kind.as_str(), e.down) {
            ("key", true) => 0x50,
            ("key", false) => 0xd0,
            ("mouse", true) => 0x10,
            _ => 0x90,
        };
        b[12 + i * 3] = e.delay as u8;
        b[13 + i * 3] = e.code;
    }
    b
}
fn referenced(s: &Snapshot, count: usize) -> HashSet<u8> {
    s.all_buttons
        .iter()
        .flat_map(|b| {
            (0..count).filter_map(move |i| {
                let o = 8 + i * 4;
                (b[o] == 0x70).then_some(b[o + 1])
            })
        })
        .collect()
}
fn apply<T: Link>(
    io: &mut T,
    s: &Snapshot,
    p: &Plan,
    known: &[Macro],
    backup: impl FnOnce() -> R<()>,
) -> R<()> {
    let (c, b) = encode(s, p)?;
    for(i,b)in p.buttons.iter().enumerate(){if b[0]==0x70&&!p.macros.iter().any(|m|m.slot==b[1])&&s.buttons[8+i*4..12+i*4]!=b[..]{return Err("配置只引用宏槽、未携带宏内容，不能迁移或新绑定未知宏；请重新创建宏。".into())}}
    let refs = referenced(s, p.model.buttons);
    let mut changed = Vec::new();
    for m in &p.macros {
        if refs.contains(&m.slot) && !known.iter().any(|k| k.slot == m.slot) {
            return Err("该宏槽已有无法读回的原厂宏，不允许覆盖；请选择空闲槽".into());
        }
        changed.push(m)
    }
    backup()?;
    let result: R<()> = (|| {
        for m in &changed {
            io.set(&macro_packet(s.report, m))?
        }
        io.set(&c)?;
        io.set(&b)?;
        let after = snapshot(io, &p.model, p.profile, s.report)?;
        let check = decode(&after, &p.model, p.profile)?;
        if check.dpi != p.dpi
            || check.active != p.active
            || check.rate != p.rate
            || check.buttons != p.buttons
        {
            return Err("配置写入后读回不一致".into());
        }
        Ok(())
    })();
    if let Err(e) = result {
        let mut errors = Vec::new();
        for old in known
            .iter()
            .filter(|old| changed.iter().any(|m| m.slot == old.slot))
        {
            if let Err(e) = io.set(&macro_packet(s.report, old)) {
                errors.push(e)
            }
        }
        if let Err(e) = io.set(&s.config) {
            errors.push(e)
        }
        if let Err(e) = io.set(&s.buttons) {
            errors.push(e)
        }
        let original = decode(s, &p.model, p.profile);
        let restored = snapshot(io, &p.model, p.profile, s.report)
            .and_then(|s| decode(&s, &p.model, p.profile));
        match (original, restored) {
            (Ok(a), Ok(b))
                if a.dpi == b.dpi
                    && a.active == b.active
                    && a.rate == b.rate
                    && a.buttons == b.buttons => {}
            _ => errors.push("回滚读回核对失败".into()),
        }
        return Err(format!(
            "{e}；已尝试回滚 DPI/按键。{}",
            if errors.is_empty() {
                "新宏数据可能留在未绑定槽位；宏不支持硬件读回。".into()
            } else {
                format!("回滚失败：{}", errors.join("；"))
            }
        ));
    }
    Ok(())
}
fn select_profile<T: Link>(io: &mut T, profile: u8, count: u8) -> R<()> {
    if profile >= count {
        return Err("板载组越界".into());
    }
    let old = cmd(io, 2)?[2];
    if old == profile + 1 {
        return Ok(());
    }
    let result = (|| -> R<()> {
        io.set(&[5, 2, profile + 1, 0, 0, 0])?;
        if cmd(io, 2)?[2] != profile + 1 {
            return Err("活动板载组读回不一致".into());
        }
        Ok(())
    })();
    if let Err(e) = result {
        let restored = io.set(&[5, 2, old, 0, 0, 0]).and_then(|_| {
            if cmd(io, 2)?[2] == old {
                Ok(())
            } else {
                Err("恢复活动组读回失败".into())
            }
        });
        return Err(format!(
            "板载参数已更新，但切组失败：{e}；恢复原活动组：{restored:?}"
        ));
    }
    Ok(())
}
#[derive(Default)]
pub struct Service(Mutex<HashMap<String, Peer>>);
fn identity(api: &hidapi::HidApi, peer: &Peer) -> R<(Usb, u8)> {
    let path = peer.id.strip_prefix("sino:").ok_or("设备标识无效")?;
    let info = api
        .device_list()
        .find(|d| {
            d.path().to_string_lossy() == path
                && d.vendor_id() == peer.model.vendor
                && d.product_id() == peer.model.product
                && d.usage_page() == 1
                && d.usage() == 6
        })
        .ok_or("鼠标接口已断开或被替换")?;
    if info.serial_number().unwrap_or("") != peer.serial {
        return Err("设备序列发生变化".into());
    }
    let mut io = Usb(api
        .open_path(&CString::new(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?);
    let fw = cmd(&mut io, 1)?;
    if fw[2..6] != peer.model.firmware.as_bytes()[..] {
        return Err("固件身份发生变化".into());
    }
    for report in [4, 6] {
        if let Ok(s) = snapshot(&mut io, &peer.model, 0, report) {
            decode(&s, &peer.model, 0)?;
            return Ok((io, report));
        }
    }
    Err("未找到符合白名单布局的配置报告".into())
}
fn root() -> R<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA 未设置")?)
            .join("Open HUB"),
    )
}
fn key(peer: &Peer) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    peer.id.hash(&mut h);
    peer.serial.hash(&mut h);
    peer.model.firmware.hash(&mut h);
    format!("{:016x}", h.finish())
}
fn journal(peer: &Peer) -> R<PathBuf> {
    Ok(root()?
        .join("sinowealth-macros")
        .join(format!("{}.json", key(peer))))
}
fn known(peer: &Peer) -> Vec<Macro> {
    journal(peer)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
impl Service {
    pub fn apply_generic(&self, id: &str, profile: &crate::profiles::Profile) -> R<View> {
        if profile.target_vendor.is_some()
            || profile.target_model.is_some()
            || profile.ghub_id.is_some()
            || profile.mchose_board.is_some()
            || profile.sino.is_some()
        {
            return Err("此配置需要完整的型号与固件方案".into());
        }
        let mut p = self.read(id, None)?.plan;
        if !profile.levels.is_empty() {
            let max = p
                .model
                .sensors
                .iter()
                .filter_map(|s| max_dpi(*s).ok())
                .min()
                .unwrap_or(2000);
            let snap = |n: u16| ((n as u32 + 50) / 100 * 100).clamp(100, max as u32) as u16;
            p.dpi = profile
                .levels
                .iter()
                .take(8)
                .map(|d| [snap(d.x), snap(d.y.unwrap_or(d.x))])
                .collect();
            p.active = profile.active_level.min(p.dpi.len() - 1);
        }
        if let Some(rate) = profile.report_rate_hz {
            p.rate = [125u32, 250, 500, 1000]
                .into_iter()
                .min_by_key(|v| v.abs_diff(rate))
                .unwrap();
        }
        p.macros.clear();
        self.write(id, p)
    }
    pub fn activate(&self, id: &str, plan: Plan) -> R<View> {
        let profile = plan.profile;
        let view = self.write(id, plan)?;
        let peers = self.0.lock().map_err(|_| "设备锁失败")?;
        let peer = peers.get(id).ok_or("设备已断开")?;
        let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
        let (mut io, _) = identity(&api, peer)?;
        select_profile(&mut io, profile, peer.model.profiles)?;
        Ok(view)
    }
    pub fn scan(&self) -> R<Vec<Peer>> {
        let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
        let ms = models();
        let mut found = HashMap::new();
        for d in api.device_list().filter(|d| {
            d.usage_page() == 1
                && d.usage() == 6
                && ms
                    .iter()
                    .any(|m| m.vendor == d.vendor_id() && m.product == d.product_id())
        }) {
            let Ok(handle) = api.open_path(d.path()) else {
                continue;
            };
            let mut io = Usb(handle);
            let Ok(fw) = cmd(&mut io, 1) else { continue };
            let Some(model) = ms
                .iter()
                .find(|m| {
                    m.vendor == d.vendor_id()
                        && m.product == d.product_id()
                        && m.firmware.as_bytes() == &fw[2..6]
                })
                .cloned()
            else {
                continue;
            };
            let peer = Peer {
                id: format!("sino:{}", d.path().to_string_lossy()),
                model,
                serial: d.serial_number().unwrap_or("").into(),
            };
            if identity(&api, &peer).is_ok() {
                found.insert(peer.id.clone(), peer);
            }
        }
        let out = found.values().cloned().collect();
        *self.0.lock().map_err(|_| "设备锁失败")? = found;
        Ok(out)
    }
    pub fn peers(&self) -> Vec<Peer> {
        self.0
            .lock()
            .map(|g| g.values().cloned().collect())
            .unwrap_or_default()
    }
    pub fn read(&self, id: &str, profile: Option<u8>) -> R<View> {
        let peers = self.0.lock().map_err(|_| "设备锁失败")?;
        let peer = peers.get(id).ok_or("未识别的鼠标")?.clone();
        let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
        let (mut io, report) = identity(&api, &peer)?;
        let active = cmd(&mut io, 2)?[2].checked_sub(1).ok_or("板载组无效")?;
        let profile = profile.unwrap_or(active);
        if profile >= peer.model.profiles {
            return Err("板载组越界".into());
        }
        let s = snapshot(&mut io, &peer.model, profile, report)?;
        let mut plan = decode(&s, &peer.model, profile)?;
        let used = referenced(&s, peer.model.buttons);
        plan.macros = known(&peer)
            .into_iter()
            .filter_map(|mut m| {
                let binding = plan.buttons.iter().find(|b| {
                    b[0] == 0x70 && b[1] == m.slot && b[2] == 1 && b[3] > 0 && b[3] <= 100
                })?;
                m.repeat = binding[3];
                Some(m)
            })
            .collect();
        let free_macro_slots = (0..(peer.model.buttons * peer.model.profiles as usize) as u8)
            .filter(|s| !used.contains(s))
            .collect();
        Ok(View {
            peer,
            plan,
            free_macro_slots,
            macro_readback: false,
            backup: None,
        })
    }
    pub fn write(&self, id: &str, plan: Plan) -> R<View> {
        validate(&plan)?;
        {
            let peers = self.0.lock().map_err(|_| "设备锁失败")?;
            let peer = peers.get(id).ok_or("未识别的鼠标")?.clone();
            if peer.model != plan.model {
                return Err("配置品牌、型号或固件不匹配".into());
            }
            let api = hidapi::HidApi::new().map_err(|e| e.to_string())?;
            let (mut io, report) = identity(&api, &peer)?;
            let s = snapshot(&mut io, &peer.model, plan.profile, report)?;
            let old = known(&peer);
            let base = root()?.join("backups");
            std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
            let backup = base.join(format!(
                "sinowealth-{}-{}.json",
                key(&peer),
                crate::profiles::new_id()
            ));
            let journal = journal(&peer)?;
            std::fs::create_dir_all(journal.parent().unwrap()).map_err(|e| e.to_string())?;
            // Prepare the new macro journal before the first device write. Publish only after readback succeeds.
            let mut next = old.clone();
            for m in &plan.macros {
                next.retain(|o| o.slot != m.slot);
                next.push(m.clone());
            }
            let tmp = journal.with_extension("json.pending");
            std::fs::write(&tmp, serde_json::to_vec(&next).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            apply(&mut io, &s, &plan, &old, || {
                std::fs::write(&backup,serde_json::to_vec_pretty(&serde_json::json!({"peer":peer,"snapshot":s,"known_macros":old,"unreadable_original_macros":true})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())
            })?;
            std::fs::rename(&tmp, &journal)
                .map_err(|e| format!("硬件已写入，但宏记录保存失败：{e}"))?;
            drop(io);
            drop(peers);
            let mut v = self.read(id, Some(plan.profile))?;
            v.backup = Some(backup.to_string_lossy().into_owned());
            return Ok(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    struct Sim {
        config: Vec<u8>,
        buttons: Vec<u8>,
        pending: u8,
        writes: Vec<Vec<u8>>,
        fail_write: Option<usize>,
        bad_readback: bool,
        backup: std::rc::Rc<Cell<bool>>,
    }
    impl Sim {
        fn new(m: &Model) -> Self {
            let mut c = vec![0xa5; 137];
            c[0] = 4;
            c[1] = 0x11;
            c[3] = 0;
            c[9] = m.sensors[0];
            c[10] = 4;
            c[11] = 0x22;
            c[12] = 0xfc;
            c[13] = 7;
            c[14] = 15;
            let mut b = vec![0; 88];
            b[0] = 4;
            b[1] = 0x12;
            let defaults = [
                [0x11, 1, 0, 0],
                [0x11, 2, 0, 0],
                [0x11, 4, 0, 0],
                [0x11, 8, 0, 0],
                [0x11, 16, 0, 0],
                [0x41, 0, 0, 0],
            ];
            for i in 0..m.buttons {
                b[8 + i * 4..12 + i * 4].copy_from_slice(&defaults[i % 6]);
            }
            Self {
                config: c,
                buttons: b,
                pending: 0,
                writes: vec![],
                fail_write: None,
                bad_readback: false,
                backup: std::rc::Rc::new(Cell::new(false)),
            }
        }
    }
    impl Link for Sim {
        fn set(&mut self, b: &[u8]) -> R<()> {
            if b.len() == 6 {
                self.pending = b[1];
                return Ok(());
            }
            assert!(self.backup.get(), "write happened before backup");
            self.writes.push(b.to_vec());
            if self.fail_write == Some(self.writes.len()) {
                return Err("simulated write error".into());
            }
            if b[1] & 15 == 1 {
                let n = b[3] as usize + 8;
                self.config = b[..n].to_vec()
            } else if b[1] & 15 == 2 {
                self.buttons = b[..88].to_vec()
            }
            Ok(())
        }
        fn get(&mut self, id: u8, _: usize) -> R<Vec<u8>> {
            if id == 5 {
                return Ok(vec![5, self.pending, 1, 0, 0, 0]);
            }
            let mut b = if self.pending & 15 == 1 {
                self.config.clone()
            } else {
                self.buttons.clone()
            };
            if self.bad_readback && self.writes.len() >= 2 && self.pending & 15 == 1 {
                b[13] = 0;
            }
            b[1] = self.pending;
            Ok(b)
        }
    }
    fn setup(index: usize) -> (Sim, Snapshot, Plan) {
        let m = models()[index].clone();
        let mut sim = Sim::new(&m);
        if [8, 15].contains(&m.sensors[0]) {
            sim.config[13] = 8;
            sim.config[14] = 16;
        }
        let s = snapshot(&mut sim, &m, 0, 4).unwrap();
        let p = decode(&s, &m, 0).unwrap();
        (sim, s, p)
    }
    fn m(slot: u8) -> Macro {
        Macro {
            slot,
            repeat: 1,
            events: vec![
                Event {
                    kind: "key".into(),
                    code: 4,
                    down: true,
                    delay: 20,
                },
                Event {
                    kind: "key".into(),
                    code: 4,
                    down: false,
                    delay: 1,
                },
            ],
        }
    }
    #[test]
    fn all_allowlisted_models_decode_and_roundtrip() {
        for i in 0..models().len() {
            let (mut io, s, mut p) = setup(i);
            p.dpi = vec![[400, 400], [800, 1200], [3200, 3200]];
            p.active = 2;
            p.rate = 500;
            let backup = io.backup.clone();
            apply(&mut io, &s, &p, &[], || {
                backup.set(true);
                Ok(())
            })
            .unwrap();
            let after = snapshot(&mut io, &p.model, 0, 4).unwrap();
            let r = decode(&after, &p.model, 0).unwrap();
            assert_eq!(r.dpi, p.dpi);
            assert_eq!(r.active, 2);
            assert_eq!(r.rate, 500);
            assert_eq!(after.config[80], 0xa5);
        }
    }
    #[test]
    fn raw_command_and_truncated_reports_are_rejected() {
        let (mut io, _, _) = setup(0);
        io.config.truncate(122);
        assert!(snapshot(&mut io, &models()[0], 0, 4).is_err());
    }
    #[test]
    fn wrong_firmware_and_mutated_capabilities_are_rejected() {
        let (_, _, mut p) = setup(0);
        p.model.firmware = "V999".into();
        assert!(validate(&p).is_err());
        p.model = models()[0].clone();
        p.model.buttons = 20;
        assert!(validate(&p).is_err());
    }
    #[test]
    fn all_sensor_dpi_encodings() {
        for i in [0, 4, 5, 6, 9, 10] {
            let (_, s, mut p) = setup(i);
            p.dpi = vec![[100, 100], [1600, 2000]];
            p.active = 1;
            let (c, b) = encode(&s, &p).unwrap();
            let t = Snapshot {
                config: c,
                buttons: b,
                ..s
            };
            assert_eq!(decode(&t, &p.model, 0).unwrap().dpi, p.dpi);
        }
    }
    #[test]
    fn invalid_dpi_never_writes() {
        let (mut io, s, mut p) = setup(0);
        p.dpi[0][0] = 123;
        assert!(apply(&mut io, &s, &p, &[], || Ok(())).is_err());
        assert!(io.writes.is_empty());
    }
    #[test]
    fn removing_last_left_button_is_rejected() {
        let (_, _, mut p) = setup(0);
        p.buttons.fill([0x11, 2, 0, 0]);
        assert!(validate(&p).is_err());
    }
    #[test]
    fn macro_packets_match_documented_offsets() {
        let b = macro_packet(4, &m(3));
        assert_eq!(b.len(), 520);
        assert_eq!(&b[..3], &[4, 0x30, 2]);
        assert_eq!(b[8], 3);
        assert_eq!(b[10], 2);
        assert_eq!(&b[11..17], &[0x50, 20, 4, 0xd0, 1, 4]);
    }
    #[test]
    fn stuck_keys_and_excess_events_are_rejected() {
        let (_, _, mut p) = setup(0);
        p.buttons[5] = [0x70, 5, 1, 1];
        p.macros = vec![m(5)];
        p.macros[0].events.pop();
        assert!(validate(&p).is_err());
        p.macros[0].events = vec![m(5).events[0].clone(); 169];
        assert!(validate(&p).is_err());
    }
    #[test]
    fn occupied_unreadable_macro_never_overwritten() {
        let (mut io, mut s, mut p) = setup(0);
        s.all_buttons[0][28..32].copy_from_slice(&[0x70, 5, 1, 1]);
        p.buttons[5] = [0x70, 5, 1, 1];
        p.macros = vec![m(5)];
        assert!(apply(&mut io, &s, &p, &[], || Ok(())).is_err());
        assert!(io.writes.is_empty());
    }
    #[test]
    fn macro_write_precedes_binding_and_has_backup() {
        let (mut io, s, mut p) = setup(0);
        p.buttons[5] = [0x70, 5, 1, 1];
        p.macros = vec![m(5)];
        let backup = io.backup.clone();
        apply(&mut io, &s, &p, &[], || {
            backup.set(true);
            Ok(())
        })
        .unwrap();
        assert_eq!(io.writes[0][1], 0x30);
        assert_eq!(io.writes[1][1], 0x11);
        assert_eq!(io.writes[2][1], 0x12);
    }
    #[test]
    fn failure_rolls_back_config_and_buttons() {
        let (mut io, s, mut p) = setup(0);
        let before = decode(&s, &p.model, 0).unwrap();
        p.dpi[0] = [400, 400];
        io.fail_write = Some(2);
        let backup = io.backup.clone();
        assert!(apply(&mut io, &s, &p, &[], || {
            backup.set(true);
            Ok(())
        })
        .is_err());
        let after = snapshot(&mut io, &p.model, 0, 4).unwrap();
        assert_eq!(decode(&after, &p.model, 0).unwrap().dpi, before.dpi);
        assert_eq!(io.writes.len(), 4);
    }
    #[test]
    fn failed_backup_prevents_every_write() {
        let (mut io, s, p) = setup(0);
        assert!(apply(&mut io, &s, &p, &[], || Err("backup unavailable".into())).is_err());
        assert!(io.writes.is_empty());
    }
    #[test]
    fn bad_readback_does_not_report_success() {
        let (mut io, s, p) = setup(0);
        io.bad_readback = true;
        let backup = io.backup.clone();
        assert!(apply(&mut io, &s, &p, &[], || {
            backup.set(true);
            Ok(())
        })
        .is_err());
        assert_eq!(io.writes.len(), 4);
    }
}
// Explicit offline simulator. It implements Link and never constructs HidApi/Usb.
pub fn simulate(plan: &Plan) -> R<serde_json::Value> {
    validate(plan)?;
    struct Bus {
        config: Vec<u8>,
        buttons: Vec<u8>,
        pending: u8,
        packets: Vec<Vec<u8>>,
    }
    impl Link for Bus {
        fn set(&mut self, b: &[u8]) -> R<()> {
            if b.len() == 6 {
                self.pending = b[1];
                return Ok(());
            }
            self.packets.push(b.to_vec());
            if b[1] & 15 == 1 {
                self.config = b[..b[3] as usize + 8].to_vec()
            } else if b[1] & 15 == 2 {
                self.buttons = b[..88].to_vec()
            }
            Ok(())
        }
        fn get(&mut self, id: u8, _: usize) -> R<Vec<u8>> {
            let mut b = if self.pending & 15 == 1 {
                self.config.clone()
            } else {
                self.buttons.clone()
            };
            b[0] = id;
            b[1] = self.pending;
            Ok(b)
        }
    }
    let mut config = vec![0; 131];
    config[0] = 4;
    config[1] = 0x11 + plan.profile * 16;
    config[9] = plan.model.sensors[0];
    config[10] = 4;
    config[11] = 0x11;
    config[12] = 0xfe;
    config[13] = if [6, 14].contains(&config[9]) { 7 } else { 8 };
    let mut buttons = vec![0; 88];
    buttons[0] = 4;
    buttons[1] = 0x12 + plan.profile * 16;
    for i in 0..plan.model.buttons {
        buttons[8 + i * 4..12 + i * 4].copy_from_slice(if i == 0 {
            &[17, 1, 0, 0]
        } else {
            &[80, 1, 0, 0]
        })
    }
    let mut io = Bus {
        config,
        buttons,
        pending: 0,
        packets: vec![],
    };
    let before = snapshot(&mut io, &plan.model, plan.profile, 4)?;
    let backed = std::cell::Cell::new(false);
    apply(&mut io, &before, plan, &[], || {
        backed.set(true);
        Ok(())
    })?;
    Ok(
        serde_json::json!({"simulated":true,"hardware_tested":false,"model":plan.model.name,"firmware":plan.model.firmware,"backup_before_write":backed.get(),"dpi_button_readback":true,"macro_content_readback":false,"packet_count":io.packets.len(),"packet_commands":io.packets.iter().map(|b|b[1]).collect::<Vec<_>>(),"note":"合成设备模拟；不验证USB接口、真实固件行为或宏的物理执行。"}),
    )
}
#[cfg(test)]
mod integration_tests {
    use super::*;
    fn plan(m: Model) -> Plan {
        let buttons = (0..m.buttons)
            .map(|i| if i == 0 { [17, 1, 0, 0] } else { [80, 1, 0, 0] })
            .collect();
        Plan {
            model: m,
            profile: 0,
            dpi: vec![[800, 800], [1600, 1600]],
            active: 1,
            rate: 1000,
            buttons,
            macros: vec![],
        }
    }
    #[test]
    fn simulation_passes_all_11_exact_models_without_usb() {
        assert_eq!(models().len(), 11);
        for m in models() {
            let mut p = plan(m);
            p.buttons[5] = [0x70, 0, 1, 1];
            p.macros = vec![Macro {
                slot: 0,
                repeat: 1,
                events: vec![
                    Event {
                        kind: "key".into(),
                        code: 4,
                        down: true,
                        delay: 20,
                    },
                    Event {
                        kind: "key".into(),
                        code: 4,
                        down: false,
                        delay: 1,
                    },
                ],
            }];
            let v = simulate(&p).unwrap();
            assert_eq!(v["hardware_tested"], false);
            assert_eq!(v["packet_count"], 3);
            assert_eq!(v["packet_commands"], serde_json::json!([48, 17, 18]));
        }
    }
    #[test]
    fn allowlist_never_contains_logitech_or_mchose() {
        assert!(models()
            .iter()
            .all(|m| ![0x046d, 0x3837].contains(&m.vendor)));
    }
    #[test]
    fn complete_profile_roundtrip_keeps_buttons_macros_and_firmware() {
        let mut p = crate::profiles::Profile::desktop();
        let mut native = plan(models()[0].clone());
        native.buttons[5] = [0x70, 0, 1, 2];
        native.macros = vec![Macro {
            slot: 0,
            repeat: 2,
            events: vec![
                Event {
                    kind: "mouse".into(),
                    code: 1,
                    down: true,
                    delay: 20,
                },
                Event {
                    kind: "mouse".into(),
                    code: 1,
                    down: false,
                    delay: 1,
                },
            ],
        }];
        p.id = "native-roundtrip".into();
        p.target_vendor = Some(native.model.vendor);
        p.target_model = Some(native.model.product);
        p.levels = native
            .dpi
            .iter()
            .map(|d| crate::profiles::Level {
                x: d[0],
                y: Some(d[1]),
            })
            .collect();
        p.active_level = native.active;
        p.report_rate_hz = Some(native.rate);
        p.sino = Some(native.clone());
        crate::profile_transfer::validate(&p).unwrap();
        let copy: crate::profiles::Profile =
            serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert_eq!(copy.sino.unwrap(), native);
        p.target_vendor = Some(0x3837);
        assert!(crate::profile_transfer::validate(&p).is_err());
    }
    struct Groups {
        active: u8,
        bad: bool,
        commands: Vec<Vec<u8>>,
    }
    impl Link for Groups {
        fn set(&mut self, b: &[u8]) -> R<()> {
            self.commands.push(b.to_vec());
            if b[2] != 0 {
                self.active = b[2]
            }
            Ok(())
        }
        fn get(&mut self, _: u8, _: usize) -> R<Vec<u8>> {
            let group = if self.bad && self.active == 2 {
                1
            } else {
                self.active
            };
            Ok(vec![5, 2, group, 0, 0, 0])
        }
    }
    #[test]
    fn activation_selects_requested_group_and_checks_readback() {
        let mut io = Groups {
            active: 1,
            bad: false,
            commands: vec![],
        };
        select_profile(&mut io, 1, 2).unwrap();
        assert_eq!(io.active, 2);
        assert!(select_profile(&mut io, 2, 2).is_err());
    }
    #[test]
    fn activation_failure_restores_original_group() {
        let mut io = Groups {
            active: 1,
            bad: true,
            commands: vec![],
        };
        assert!(select_profile(&mut io, 1, 2).is_err());
        assert_eq!(io.active, 1);
    }
}

#[cfg(test)]mod golden_tests{
 use super::*;
 #[test]fn dpi_polling_and_shortcut_bytes_match_libratbag_layout(){let m=models()[0].clone();let mut config=vec![0;520];config[0]=4;config[1]=17;config[3]=129;config[9]=6;config[10]=0x74;let mut buttons=vec![0;520];buttons[0]=4;buttons[1]=18;buttons[3]=80;let s=Snapshot{config,buttons,all_buttons:vec![],report:4,sensor:6};let mut bindings=vec![[80,1,0,0];6];bindings[0]=[17,1,0,0];bindings[5]=[33,1,4,0];let p=Plan{model:m,profile:0,dpi:vec![[400,600],[1600,1600]],active:1,rate:500,buttons:bindings,macros:vec![]};let(c,b)=encode(&s,&p).unwrap();assert_eq!(&c[..4],&[4,17,0,129]);assert_eq!(&c[9..17],&[6,0xf3,0x22,0xfc,3,5,15,15]);assert_eq!(&b[..4],&[4,18,0,80]);assert_eq!(&b[28..32],&[33,1,4,0]);}
 #[test]fn config_long_macro_id_is_preserved(){let m=Macro{slot:0,repeat:1,events:vec![Event{kind:"mouse".into(),code:1,down:true,delay:50},Event{kind:"mouse".into(),code:1,down:false,delay:1}]};let b=macro_packet(6,&m);assert_eq!(&b[..3],&[6,48,2]);assert_eq!(&b[8..17],&[0,0,2,16,50,1,144,1,1]);}
}

#[cfg(test)]mod unknown_macro_tests{use super::*;struct NeverWrite;impl Link for NeverWrite{fn set(&mut self,_:&[u8])->R<()>{panic!("unknown macro reference must be rejected before USB write")}fn get(&mut self,_:u8,_:usize)->R<Vec<u8>>{panic!("no USB operation expected")}}
#[test]fn imported_reference_without_macro_content_cannot_bind_another_device_slot(){let model=models()[0].clone();let mut config=vec![0;520];config[0]=4;config[1]=17;config[3]=123;config[9]=6;let mut buttons=vec![0;520];buttons[0]=4;buttons[1]=18;buttons[3]=80;buttons[8..12].copy_from_slice(&[17,1,0,0]);let s=Snapshot{config,buttons:buttons.clone(),all_buttons:vec![buttons],report:4,sensor:6};let mut bindings=vec![[80,1,0,0];6];bindings[0]=[17,1,0,0];bindings[5]=[112,4,1,1];let plan=Plan{model,profile:0,dpi:vec![[800,800]],active:0,rate:1000,buttons:bindings,macros:vec![]};assert!(apply(&mut NeverWrite,&s,&plan,&[],||panic!("must reject before backup callback")).is_err());}}
