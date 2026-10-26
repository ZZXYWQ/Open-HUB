//! MagDock RGB protocol, implemented from the public M HUB command layout.
use hidapi::{HidApi, HidDevice};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
type R<T> = Result<T, String>;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Light {
    pub enabled: bool,
    pub mode: u8,
    pub mode_count: u8,
    pub speed: u8,
    pub brightness: u8,
    pub sync: bool,
    pub color: [u8; 3],
    pub direction: u8,
}
impl Light {
    pub fn validate(&self) -> R<()> {
        if self.mode > 5
            || self.mode_count > 8
            || self.mode_count == 0
            || self.speed > 4
            || self.brightness > 4
            || self.direction > 1
        {
            return Err("底座灯光参数超出范围".into());
        }
        Ok(())
    }
    fn args(&self) -> [u8; 10] {
        [
            self.enabled as u8,
            self.mode,
            self.mode_count,
            self.speed,
            self.brightness,
            self.sync as u8,
            self.color[0],
            self.color[1],
            self.color[2],
            self.direction,
        ]
    }
}
#[derive(Serialize)]
pub struct View {
    pub light: Light,
    pub firmware: String,
    pub music_running: bool,
    pub audio_level: f32,
    pub music_error: Option<String>,
}
trait DockIo: Send {
    fn write(&self, bytes: &[u8]) -> R<usize>;
    fn read_timeout(&self, bytes: &mut [u8], timeout: i32) -> R<usize>;
    fn settle(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
impl DockIo for HidDevice {
    fn write(&self, bytes: &[u8]) -> R<usize> {
        HidDevice::write(self, bytes).map_err(|e| e.to_string())
    }
    fn read_timeout(&self, bytes: &mut [u8], timeout: i32) -> R<usize> {
        HidDevice::read_timeout(self, bytes, timeout).map_err(|e| e.to_string())
    }
}
struct Session {
    io: Box<dyn DockIo>,
    connection: String,
    #[cfg(test)]
    backup_dir: Option<std::path::PathBuf>,
}
/// A streaming worker can only frame/restore the opened connection it began on.
/// This generation is not a persistent hardware serial or paired mouse identity.
pub struct StreamLease {
    connection: String,
    before: Light,
    applied: Light,
}
fn packet(cmd: u8, args: &[u8]) -> R<[u8; 65]> {
    if args.len() > 57 {
        return Err("底座数据包过长".into());
    }
    let mut b = [0u8; 65];
    b[1] = 170;
    b[2] = cmd;
    b[6] = args.len() as u8;
    b[7..7 + args.len()].copy_from_slice(args);
    Ok(b)
}
fn parse_reply(bytes: &[u8]) -> R<Option<(Light, String)>> {
    let r = if bytes.first() == Some(&0) {
        &bytes[1..]
    } else {
        bytes
    };
    if r.len() < 2 || r[..2] != [170, 7] {
        return Ok(None);
    }
    if r.len() < 6 || r[2] != 2 || r[3] != 0 || r[4] != 0 {
        return Err("底座返回无效的读取响应头".into());
    }
    let declared = r[5] as usize;
    if !(30..=58).contains(&declared) || r.len() < 6 + declared {
        return Err("底座读取响应不完整".into());
    }
    let p = &r[6..6 + declared];
    if p[0] > 1 || p[5] > 1 || p[1] > 5 {
        return Err("底座返回未知灯效状态".into());
    }
    let light = Light {
        enabled: p[0] == 1,
        mode: p[1],
        mode_count: p[2],
        speed: p[3],
        brightness: p[4],
        sync: p[5] == 1,
        color: [p[6], p[7], p[8]],
        direction: p[29],
    };
    light.validate()?;
    Ok(Some((light, format!("{}.{}", p[13], p[12]))))
}
impl Session {
    fn open() -> R<Self> {
        let api = HidApi::new().map_err(|e| e.to_string())?;
        let mut matches = api.device_list().filter(|d| {
            d.vendor_id() == 0x3837
                && d.product_id() == 0x1012
                && d.usage_page() == 0xff00
                && d.usage() == 1
                && d.product_string() == Some("MCHOSE MagDock")
        });
        let d = matches.next().ok_or("未连接 MCHOSE MagDock 底座")?;
        if matches.next().is_some() {
            return Err("检测到多个 MagDock，请仅连接要控制的底座后重新识别".into());
        }
        Ok(Self {
            io: Box::new(api.open_path(d.path()).map_err(|e| e.to_string())?),
            connection: format!("magdock:{}", crate::profiles::new_id()),
            #[cfg(test)]
            backup_dir: None,
        })
    }
    fn backup(&self, light: &Light) -> R<()> {
        #[cfg(test)]
        if let Some(dir) = &self.backup_dir {
            return backup_to(dir, light);
        }
        backup(light)
    }
    fn send(&self, cmd: u8, args: &[u8]) -> R<()> {
        let b = packet(cmd, args)?;
        let n = self.io.write(&b)?;
        if n != 65 {
            return Err("底座写入不完整".into());
        }
        Ok(())
    }
    fn read(&self) -> R<(Light, String)> {
        let mut b = [0u8; 65];
        let mut drained = false;
        for _ in 0..32 {
            if self.io.read_timeout(&mut b, 0)? == 0 {
                drained = true;
                break;
            }
        }
        if !drained {
            return Err("底座持续收到旧报文，请关闭其他控制程序后重试".into());
        }
        self.send(7, &[])?;
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            let n = self.io.read_timeout(&mut b, 200)?;
            if n > b.len() {
                return Err("底座报文长度错误".into());
            }
            if let Some(state) = parse_reply(&b[..n])? {
                return Ok(state);
            }
        }
        Err("底座读取超时，请关闭 M HUB 后重试".into())
    }
    fn apply(&self, l: &Light) -> R<(Light, String)> {
        l.validate()?;
        self.send(39, &l.args())?;
        self.io.settle(Duration::from_millis(60));
        let (got, fw) = self.read()?;
        if &got != l {
            return Err(format!("底座设置核对失败：{got:?}"));
        }
        Ok((got, fw))
    }
}
#[derive(Default)]
struct Inner {
    session: Mutex<Option<Session>>,
    running: AtomicBool,
    level: AtomicU32,
    error: Mutex<Option<String>>,
}
impl Inner {
    fn access<T>(&self, f: impl FnOnce(&Session) -> R<T>) -> R<T> {
        let mut s = self.session.lock().map_err(|_| "底座接口锁定失败")?;
        if s.is_none() {
            *s = Some(Session::open()?)
        }
        let result = f(s.as_ref().unwrap());
        if result.is_err() {
            *s = None
        }
        result
    }
    fn access_bound<T>(&self, connection: &str, f: impl FnOnce(&Session) -> R<T>) -> R<T> {
        let mut session = self.session.lock().map_err(|_| "底座接口锁定失败")?;
        let current = session
            .as_ref()
            .filter(|s| !connection.is_empty() && s.connection == connection)
            .ok_or("底座连接已改变，旧灯效任务已停止；请重新识别底座")?;
        let result = f(current);
        if result.is_err() {
            *session = None;
        }
        result
    }
}
struct Worker {
    stop: Arc<AtomicBool>,
    handle: std::thread::JoinHandle<()>,
}
#[derive(Clone, Default)]
pub struct Controller {
    inner: Arc<Inner>,
    worker: Arc<Mutex<Option<Worker>>>,
}
fn backup(l: &Light) -> R<()> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA 未设置")?;
    let dir = std::path::PathBuf::from(base).join("Open HUB/backups");
    backup_to(&dir, l)
}
fn backup_to(dir: &std::path::Path, light: &Light) -> R<()> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tick = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let bytes = serde_json::to_vec_pretty(light).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(format!("magdock-{tick}.json")))
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}

impl Controller {
    pub fn view(&self) -> R<View> {
        let (l, firmware) = self.inner.access(|s| s.read())?;
        Ok(View {
            light: l,
            firmware,
            music_running: self.inner.running.load(Ordering::SeqCst),
            audio_level: f32::from_bits(self.inner.level.load(Ordering::SeqCst)),
            music_error: self.inner.error.lock().ok().and_then(|e| e.clone()),
        })
    }
    pub fn stop(&self) -> R<()> {
        let mut w = self.worker.lock().map_err(|_| "音乐线程锁定失败")?;
        if let Some(w) = w.take() {
            w.stop.store(true, Ordering::SeqCst);
            w.handle.join().map_err(|_| "音乐线程异常退出")?;
        }
        Ok(())
    }
    pub fn apply(&self, l: Light) -> R<View> {
        l.validate()?;
        self.stop()?;
        self.inner.access(|s| {
            let (before, _) = s.read()?;
            s.backup(&before)?;
            s.apply(&l)
                .map_err(|e| format!("{e}；可能部分生效，已保留写入前备份；未自动覆盖设备状态"))?;
            Ok(())
        })?;
        self.view()
    }
    pub fn reset(&self) -> R<View> {
        self.stop()?;
        self.inner.access(|s| {
            let (before, _) = s.read()?;
            s.backup(&before)?;
            s.send(42, &[])?;
            std::thread::sleep(Duration::from_millis(100));
            s.read()?;
            Ok(())
        })?;
        self.view()
    }
    pub fn music(&self, mut l: Light, gain: f32) -> R<View> {
        l.validate()?;
        if !gain.is_finite() || !(0.5..=8.0).contains(&gain) {
            return Err("律动灵敏度应为 0.5–8".into());
        }
        self.stop()?;
        let mut worker = self.worker.lock().map_err(|_| "音乐线程锁定失败")?;
        let (connection, before) = self.inner.access(|s| {
            let before = s.read()?.0;
            s.backup(&before)?;
            Ok((s.connection.clone(), before))
        })?;
        l.mode = 5;
        l.sync = true;
        l.enabled = true;
        let inner = self.inner.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let handle = std::thread::spawn(move || {
            let result = (|| -> R<()> {
                let meter = crate::audio_meter::Meter::open()?;
                inner.access_bound(&connection, |s| {
                    if s.read()?.0 != before {
                        return Err("底座状态在律动启动前已改变，请重新读取".into());
                    }
                    s.apply(&l)
                })?;
                inner.running.store(true, Ordering::SeqCst);
                if let Ok(mut e) = inner.error.lock() {
                    *e = None
                }
                let _ = tx.send(Ok(()));
                let mut envelope = 0.0f32;
                while !signal.load(Ordering::SeqCst) {
                    let peak = meter.peak()?;
                    envelope = (peak.sqrt() * gain).clamp(0.0, 1.0).max(envelope * 0.78);
                    inner.level.store(envelope.to_bits(), Ordering::SeqCst);
                    let colors = music_colors(envelope, l.color, l.brightness);
                    inner.access_bound(&connection, |s| s.send(41, &colors))?;
                    std::thread::sleep(Duration::from_millis(50));
                }
                Ok(())
            })();
            if let Err(ref e) = result {
                let _ = tx.send(Err(e.clone()));
                if let Ok(mut error) = inner.error.lock() {
                    *error = Some(e.clone())
                }
            }
            let restored = restore_light(&inner, &connection, &l, &before);
            if let Err(e) = restored {
                if let Ok(mut error) = inner.error.lock() {
                    *error = Some(format!("停止律动后恢复灯效失败：{e}"))
                }
            }
            inner.running.store(false, Ordering::SeqCst);
            inner.level.store(0, Ordering::SeqCst);
        });
        *worker = Some(Worker { stop, handle });
        drop(worker);
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => self.view(),
            Ok(Err(e)) => {
                self.stop()?;
                Err(e)
            }
            Err(e) => {
                self.stop()?;
                Err(format!("音乐律动启动失败：{e}"))
            }
        }
    }
    pub fn stream_begin(&self) -> R<StreamLease> {
        self.stop()?;
        self.inner.access(|s| {
            let before = s.read()?.0;
            s.backup(&before)?;
            if s.read()?.0 != before {
                return Err("底座状态在备份期间已改变，请重新读取".into());
            }
            let mut applied = before.clone();
            applied.enabled = true;
            applied.mode = 5;
            applied.sync = true;
            s.apply(&applied)
                .map_err(|e| format!("{e}；已保留底座备份，未自动覆盖恢复"))?;
            Ok(StreamLease {
                connection: s.connection.clone(),
                before,
                applied,
            })
        })
    }
    pub fn stream_frame(&self, lease: &StreamLease, pixels: &[[u8; 3]; 12]) -> R<()> {
        let colors: Vec<u8> = pixels.iter().flatten().copied().collect();
        self.inner
            .access_bound(&lease.connection, |s| s.send(41, &colors))
    }
    pub fn stream_restore(&self, lease: &StreamLease) -> R<()> {
        restore_light(
            &self.inner,
            &lease.connection,
            &lease.applied,
            &lease.before,
        )
    }
    pub fn self_test(&self) -> R<serde_json::Value> {
        self.stop()?;
        self.inner.access(|s| {
            let before = s.read()?.0;
            s.backup(&before)?;
            let mut checks = Vec::new();
            for mode in 0..=5 {
                let mut light = before.clone();
                light.enabled = true;
                light.mode = mode;
                light.sync = mode == 5;
                light.color = [32, 180, 160];
                light.brightness = mode % 5;
                light.speed = mode % 5;
                light.direction = mode % 2;
                s.apply(&light)?;
                checks.push(format!("mode {mode}: write/read verified"));
            }
            let mut off = before.clone();
            off.enabled = false;
            s.apply(&off)?;
            checks.push("switch off: verified".into());
            if s.read()?.0 != off {
                return Err("底座状态已被其他程序改变，不覆盖恢复".into());
            }
            let restored = s.apply(&before)?.0 == before;
            Ok(serde_json::json!({"checks":checks,"before":before,"restored":restored}))
        })
    }
}
fn restore_light(inner: &Inner, connection: &str, expected: &Light, before: &Light) -> R<()> {
    inner.access_bound(connection, |s| {
        let current = s.read()?.0;
        if &current == before {
            return Ok(());
        }
        if &current != expected {
            return Err("底座灯效已被其他程序改变，不覆盖恢复".into());
        }
        s.apply(before)?;
        Ok(())
    })
}

fn music_colors(level: f32, color: [u8; 3], brightness: u8) -> [u8; 36] {
    let groups: [&[usize]; 7] = [&[5], &[4, 6], &[3, 7], &[2, 8], &[1, 9], &[0, 10], &[11]];
    let mut out = [0; 36];
    let count = (level * 7.0).ceil() as usize;
    let scale = (0.3 + brightness as f32 * 0.175) * 0.8;
    for g in groups.iter().take(count) {
        for i in *g {
            for c in 0..3 {
                out[i * 3 + c] = (color[c] as f32 * scale).round() as u8;
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_packet() {
        let p = packet(39, &[1, 0, 6, 2, 4, 0, 1, 2, 3, 0]).unwrap();
        assert_eq!(&p[..7], &[0, 170, 39, 0, 0, 0, 10]);
        assert_eq!(p.len(), 65);
        assert!(packet(41, &[0; 58]).is_err())
    }
    #[test]
    fn light_bounds() {
        let mut l = Light {
            enabled: true,
            mode: 5,
            mode_count: 6,
            speed: 4,
            brightness: 4,
            sync: true,
            color: [0, 128, 255],
            direction: 1,
        };
        assert!(l.validate().is_ok());
        l.mode = 6;
        assert!(l.validate().is_err())
    }
    #[test]
    fn pixels_follow_level() {
        assert_eq!(music_colors(0.0, [255; 3], 4), [0; 36]);
        assert_eq!(music_colors(1.0, [255; 3], 4), [204; 36]);
        assert_eq!(
            music_colors(0.1, [255; 3], 0)
                .iter()
                .filter(|n| **n > 0)
                .count(),
            3
        );
    }
}

#[cfg(test)]
#[path = "magdock_protocol_tests.rs"]
mod protocol_tests;
