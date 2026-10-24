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
struct Session {
    hid: HidDevice,
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
impl Session {
    fn open() -> R<Self> {
        let api = HidApi::new().map_err(|e| e.to_string())?;
        let d = api
            .device_list()
            .find(|d| {
                d.vendor_id() == 0x3837
                    && d.product_id() == 0x1012
                    && d.usage_page() == 0xff00
                    && d.usage() == 1
                    && d.product_string() == Some("MCHOSE MagDock")
            })
            .ok_or("未连接 MCHOSE MagDock 底座")?;
        Ok(Self {
            hid: api.open_path(d.path()).map_err(|e| e.to_string())?,
        })
    }
    fn send(&self, cmd: u8, args: &[u8]) -> R<()> {
        let b = packet(cmd, args)?;
        let n = self.hid.write(&b).map_err(|e| e.to_string())?;
        if n != 65 {
            return Err("底座写入不完整".into());
        }
        Ok(())
    }
    fn read(&self) -> R<(Light, String)> {
        let mut b = [0u8; 65];
        while self
            .hid
            .read_timeout(&mut b, 0)
            .map_err(|e| e.to_string())?
            > 0
        {}
        self.send(7, &[])?;
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            let n = self
                .hid
                .read_timeout(&mut b, 200)
                .map_err(|e| e.to_string())?;
            let r = if n > 0 && b[0] == 0 {
                &b[1..n]
            } else {
                &b[..n]
            };
            if r.len() >= 36 && r[0] == 170 && r[1] == 7 && r[2] == 2 && r[5] >= 30 {
                let p = &r[6..];
                let light = Light {
                    enabled: p[0] == 1,
                    mode: if p[5] == 1 { 5 } else { p[1] },
                    mode_count: p[2],
                    speed: p[3],
                    brightness: p[4],
                    sync: p[5] == 1,
                    color: [p[6], p[7], p[8]],
                    direction: p[29],
                };
                light.validate()?;
                return Ok((light, format!("{}.{}", p[13], p[12])));
            }
        }
        Err("底座读取超时，请关闭 M HUB 后重试".into())
    }
    fn apply(&self, l: &Light) -> R<(Light, String)> {
        l.validate()?;
        self.send(39, &l.args())?;
        std::thread::sleep(Duration::from_millis(60));
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
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tick = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    std::fs::write(
        dir.join(format!("magdock-{tick}.json")),
        serde_json::to_vec_pretty(l).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
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
            backup(&before)?;
            if let Err(e) = s.apply(&l) {
                let recovery = s.apply(&before);
                return Err(format!("{e}；恢复结果：{recovery:?}"));
            }
            Ok(())
        })?;
        self.view()
    }
    pub fn reset(&self) -> R<View> {
        self.stop()?;
        self.inner.access(|s| {
            let (before, _) = s.read()?;
            backup(&before)?;
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
        let (before, _) = self.inner.access(|s| s.read())?;
        backup(&before)?;
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
                inner.access(|s| s.apply(&l))?;
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
                    inner.access(|s| s.send(41, &colors))?;
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
            let restored = inner.access(|s| s.apply(&before));
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
    pub fn stream_begin(&self) -> R<Light> {
        self.stop()?;
        let before = self.view()?.light;
        backup(&before)?;
        let mut live = before.clone();
        live.enabled = true;
        live.mode = 5;
        live.sync = true;
        if let Err(e)=self.inner.access(|s| s.apply(&live)){let restored=self.inner.access(|s|s.apply(&before));return Err(format!("{e}；底座恢复：{restored:?}"));}
        Ok(before)
    }
    pub fn stream_frame(&self, pixels: &[[u8; 3]; 12]) -> R<()> {
        let colors: Vec<u8> = pixels.iter().flatten().copied().collect();
        self.inner.access(|s| s.send(41, &colors))
    }
    pub fn stream_restore(&self, before: &Light) -> R<()> {
        self.inner.access(|s| s.apply(before))?;
        Ok(())
    }
    pub fn self_test(&self) -> R<serde_json::Value> {
        self.stop()?;
        let before = self.view()?.light;
        let result = (|| -> R<serde_json::Value> {
            let mut checks = Vec::new();
            for mode in 0..=5 {
                let mut l = before.clone();
                l.enabled = true;
                l.mode = mode;
                l.sync = mode == 5;
                l.color = [32, 180, 160];
                l.brightness = mode % 5;
                l.speed = mode % 5;
                l.direction = mode % 2;
                self.apply(l)?;
                checks.push(format!("mode {mode}: write/read verified"));
            }
            let mut off = before.clone();
            off.enabled = false;
            self.apply(off)?;
            checks.push("switch off: verified".into());
            Ok(serde_json::json!({"checks":checks,"before":before}))
        })();
        let restored = self.apply(before.clone());
        if let Err(e) = restored {
            return Err(format!("测试结束恢复失败：{e}"));
        }
        let mut report = result?;
        report["restored"] = serde_json::json!(self.view()?.light == before);
        Ok(report)
    }
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
