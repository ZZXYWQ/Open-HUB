//! MagDock RGB protocol, implemented from the public M HUB command layout.
use hidapi::{HidApi, HidDevice};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
type R<T> = Result<T, String>;
// windows-native HIDAPI2.6.7 configures64 input buffers; allow that queue plus
// a pending read and an empty probe, while keeping a finite flood bound.
const MAX_STALE_REPORTS: usize = 128;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
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
    pub connection_key: String,
    pub light: Light,
    pub firmware: String,
    pub music_running: bool,
    pub audio_level: f32,
    pub music_error: Option<String>,
}
trait PeakMeter {
    fn peak(&self) -> R<f32>;
}
impl PeakMeter for crate::audio_meter::Meter {
    fn peak(&self) -> R<f32> {
        crate::audio_meter::Meter::peak(self)
    }
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
    fn backup(&self, light: &Light) -> R<std::path::PathBuf> {
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
        let (light, firmware, _) = self.read_with_report()?;
        Ok((light, firmware))
    }
    fn read_with_report(&self) -> R<(Light, String, Vec<u8>)> {
        let mut b = [0u8; 65];
        let mut drained = false;
        for _ in 0..=MAX_STALE_REPORTS {
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
                return Ok((state.0, state.1, b[..n].to_vec()));
            }
        }
        Err("底座读取超时，请关闭 M HUB 后重试".into())
    }
    fn apply(&self, l: &Light) -> R<(Light, String)> {
        let (light, firmware, _) = self.apply_with_report(l)?;
        Ok((light, firmware))
    }
    fn apply_verified(&self, l: &Light, before: &Light) -> R<(Light, String)> {
        let (light, firmware, _) = self.apply_with_report_from(l, Some(before))?;
        Ok((light, firmware))
    }
    fn apply_with_report(&self, l: &Light) -> R<(Light, String, Vec<u8>)> {
        self.apply_with_report_from(l, None)
    }
    fn apply_with_report_from(
        &self,
        l: &Light,
        before: Option<&Light>,
    ) -> R<(Light, String, Vec<u8>)> {
        l.validate()?;
        let mut expected = l.clone();
        // Observed MagDock firmware0.3 keeps its base mode when wire mode5
        // enables host synchronization. Other revisions retain exact matching.
        if l.mode == 5 && l.sync {
            let (current, firmware) = self.read()?;
            if before.is_some_and(|expected| expected != &current) {
                return Err("底座状态在同步写入前已改变，停止写入".into());
            }
            if firmware == "0.3" {
                expected.mode = current.mode;
            }
        }
        self.send(39, &l.args())?;
        self.io.settle(Duration::from_millis(60));
        let (got, fw, raw) = self.read_with_report()?;
        if got != expected {
            return Err(format!("底座设置核对失败：{got:?}"));
        }
        Ok((got, fw, raw))
    }
}
#[derive(Default)]
struct Inner {
    session: Mutex<Option<Session>>,
    running: AtomicBool,
    music_connection: Mutex<Option<String>>,
    level: AtomicU32,
    music_frames: AtomicU32,
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
fn backup(l: &Light) -> R<std::path::PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA 未设置")?;
    let dir = std::path::PathBuf::from(base).join("Open HUB/backups");
    backup_to(&dir, l)
}
fn backup_to(dir: &std::path::Path, light: &Light) -> R<std::path::PathBuf> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tick = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let bytes = serde_json::to_vec_pretty(light).map_err(|e| e.to_string())?;
    let path = dir.join(format!("magdock-{tick}.json"));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(path)
}

impl Controller {
    fn make_view(&self, connection: String, light: Light, firmware: String) -> View {
        let owns_music = self
            .inner
            .music_connection
            .lock()
            .ok()
            .is_some_and(|c| c.as_deref() == Some(connection.as_str()));
        View {
            connection_key: connection,
            light,
            firmware,
            music_running: owns_music && self.inner.running.load(Ordering::SeqCst),
            audio_level: if owns_music {
                f32::from_bits(self.inner.level.load(Ordering::SeqCst))
            } else {
                0.0
            },
            music_error: if owns_music {
                self.inner.error.lock().ok().and_then(|e| e.clone())
            } else {
                None
            },
        }
    }
    pub fn view(&self) -> R<View> {
        let (connection, (light, firmware)) = self
            .inner
            .access(|s| Ok((s.connection.clone(), s.read()?)))?;
        Ok(self.make_view(connection, light, firmware))
    }
    pub fn view_selected(&self, expected: &str) -> R<View> {
        let (light, firmware) = self.inner.access_bound(expected, |s| s.read())?;
        Ok(self.make_view(expected.into(), light, firmware))
    }
    pub fn check_connection(&self, expected: &str) -> R<()> {
        self.inner.access_bound(expected, |_| Ok(()))
    }
    /// Validate the caller's preview before stopping any shared dock effect.
    pub fn with_selected_connection<T>(
        &self,
        engine: &crate::effects::Engine,
        expected: &str,
        action: impl FnOnce() -> R<T>,
    ) -> R<T> {
        engine.route(|| {
            self.check_connection(expected)?;
            engine.stop_for_dock()?;
            action()
        })
    }
    pub fn stop_selected(&self, expected: &str) -> R<View> {
        self.check_connection(expected)?;
        self.stop()?;
        self.view_selected(expected)
    }
    pub fn stop(&self) -> R<()> {
        let mut w = self.worker.lock().map_err(|_| "音乐线程锁定失败")?;
        if let Some(w) = w.take() {
            w.stop.store(true, Ordering::SeqCst);
            w.handle.join().map_err(|_| "音乐线程异常退出")?;
        }
        Ok(())
    }
    pub fn apply(&self, l: Light, expected: &str) -> R<View> {
        self.apply_reviewed(l, expected, None)
    }
    pub(crate) fn apply_reviewed(
        &self,
        l: Light,
        expected: &str,
        reviewed: Option<(&Light, &str)>,
    ) -> R<View> {
        l.validate()?;
        self.check_connection(expected)?;
        self.stop()?;
        let (light, firmware) = self.inner.access_bound(expected, |s| {
            let (before, firmware) = s.read()?;
            if reviewed.is_some_and(|(state, fw)| state != &before || fw != firmware) {
                return Err("底座状态在预览后已改变，请重新选择备份".into());
            }
            s.backup(&before)?;
            if s.read()?.0 != before {
                return Err("底座状态在备份期间已改变，请重新读取".into());
            }
            s.apply_verified(&l, &before)
                .map_err(|e| format!("{e}；可能部分生效，已保留写入前备份；未自动覆盖设备状态"))
        })?;
        Ok(self.make_view(expected.into(), light, firmware))
    }
    pub fn reset(&self, expected: &str) -> R<View> {
        self.check_connection(expected)?;
        self.stop()?;
        let (light, firmware) = self.inner.access_bound(expected, |s| {
            let (before, _) = s.read()?;
            s.backup(&before)?;
            if s.read()?.0 != before {
                return Err("底座状态在备份期间已改变，请重新读取".into());
            }
            s.send(42, &[])?;
            s.io.settle(Duration::from_millis(100));
            s.read()
        })?;
        Ok(self.make_view(expected.into(), light, firmware))
    }
    pub fn music(&self, l: Light, gain: f32, expected: &str) -> R<View> {
        self.music_with_meter(l, gain, expected, crate::audio_meter::Meter::open)
    }
    fn music_with_meter<M: PeakMeter + 'static>(
        &self,
        mut l: Light,
        gain: f32,
        expected: &str,
        open: impl FnOnce() -> R<M> + Send + 'static,
    ) -> R<View> {
        l.validate()?;
        if !gain.is_finite() || !(0.5..=8.0).contains(&gain) {
            return Err("律动灵敏度应为 0.5–8".into());
        }
        self.check_connection(expected)?;
        self.stop()?;
        let mut worker = self.worker.lock().map_err(|_| "音乐线程锁定失败")?;
        self.inner.music_frames.store(0, Ordering::SeqCst);
        let (connection, before) = self.inner.access_bound(expected, |s| {
            let before = s.read()?.0;
            s.backup(&before)?;
            Ok((s.connection.clone(), before))
        })?;
        l.mode = 5;
        l.sync = true;
        l.enabled = true;
        *self
            .inner
            .music_connection
            .lock()
            .map_err(|_| "底座律动连接锁定失败")? = Some(connection.clone());
        let inner = self.inner.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let handle = std::thread::spawn(move || {
            let mut applied = None;
            let result = (|| -> R<()> {
                let meter = open()?;
                let (observed, _) = inner.access_bound(&connection, |s| {
                    if s.read()?.0 != before {
                        return Err("底座状态在律动启动前已改变，请重新读取".into());
                    }
                    s.apply_verified(&l, &before)
                })?;
                applied = Some(observed);
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
                    inner.music_frames.fetch_add(1, Ordering::SeqCst);
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
            let restored = restore_light(
                &inner,
                &connection,
                applied.as_ref().unwrap_or(&before),
                &before,
            );
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
            Ok(Ok(())) => self.view_selected(expected),
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
    pub fn stream_begin(&self, expected: &str) -> R<StreamLease> {
        self.check_connection(expected)?;
        self.stop()?;
        self.inner.access_bound(expected, |s| {
            let before = s.read()?.0;
            s.backup(&before)?;
            if s.read()?.0 != before {
                return Err("底座状态在备份期间已改变，请重新读取".into());
            }
            let mut applied = before.clone();
            applied.enabled = true;
            applied.mode = 5;
            applied.sync = true;
            let (applied, _) = s
                .apply(&applied)
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
    /// Always join/restore after the diagnostic, including failed status reads.
    pub fn music_test(&self) -> R<serde_json::Value> {
        self.music_test_with(Duration::from_secs(3), crate::audio_meter::Meter::open)
    }
    fn music_test_with<M: PeakMeter + 'static>(
        &self,
        duration: Duration,
        open: impl FnOnce() -> R<M> + Send + 'static,
    ) -> R<serde_json::Value> {
        let before = self.view()?;
        let backup = self
            .inner
            .access_bound(&before.connection_key, |s| s.backup(&before.light))?;
        let mut report =
            serde_json::json!({"before":before,"backup":backup.to_string_lossy(),"restored":false});
        let result = (|| -> R<View> {
            self.music_with_meter(before.light.clone(), 2.0, &before.connection_key, open)?;
            let first_frame_deadline = Instant::now() + Duration::from_secs(1);
            while self.inner.music_frames.load(Ordering::SeqCst) == 0
                && self.inner.running.load(Ordering::SeqCst)
                && Instant::now() < first_frame_deadline
            {
                std::thread::sleep(Duration::from_millis(2));
            }
            if self.inner.music_frames.load(Ordering::SeqCst) == 0 {
                return Err("音乐测试未发送颜色帧".into());
            }
            std::thread::sleep(duration);
            let during = self.view_selected(&before.connection_key)?;
            if !during.music_running || during.music_error.is_some() {
                return Err(during
                    .music_error
                    .unwrap_or("音乐线程在测试期间退出".into()));
            }
            Ok(during)
        })();
        let cleanup = self.stop();
        let after = self.view_selected(&before.connection_key);
        report["frames_sent"] = self.inner.music_frames.load(Ordering::SeqCst).into();
        let mut errors = vec![];
        match result {
            Ok(v) => report["during"] = serde_json::to_value(v).unwrap(),
            Err(e) => errors.push(e),
        }
        if let Err(e) = cleanup {
            errors.push(e);
        }
        match after {
            Ok(v) => {
                if v.light == before.light {
                    report["restored"] = true.into();
                } else {
                    errors.push("音频测试后底座设置未恢复".into());
                }
                report["after"] = serde_json::to_value(v).unwrap();
            }
            Err(e) => errors.push(e),
        }
        if errors.is_empty() {
            Ok(report)
        } else {
            report["errors"] = serde_json::to_value(errors).unwrap();
            Err(serde_json::to_string_pretty(&report).unwrap())
        }
    }
    pub fn self_test(&self) -> R<serde_json::Value> {
        self.light_diagnostic(false)
    }
    /// Experimental characterization records byte9 instead of certifying it as
    /// an independent sleep flag. All final declared bytes must still restore.
    pub fn characterize_light(&self) -> R<serde_json::Value> {
        self.light_diagnostic(true)
    }
    fn light_diagnostic(&self, opaque_byte9: bool) -> R<serde_json::Value> {
        self.stop()?;
        let mut report = serde_json::json!({"format":"open-hub-magdock-native-roundtrip/1","steps":[],"restored":false,"pixel_framebuffer_readback":false,"opaque_byte9_characterization":opaque_byte9,"opaque_byte9_semantics_confirmed":false});
        let result = self.inner.access(|s| {
            let (before, firmware, original_raw) = s.read_with_report()?;
            let backup = s.backup(&before)?;
            report["connection_key"] = s.connection.clone().into();
            report["backup"] = backup.to_string_lossy().into_owned().into();
            report["before"] = serde_json::to_value(&before).unwrap();
            report["firmware"] = firmware.clone().into();
            report["before_response"] = serde_json::to_value(&original_raw).unwrap();
            let protected = protected_light_response(&original_raw, opaque_byte9)?;
            let mut expected = before.clone();
            for mode in 0..=5 {
                require_diagnostic_state(s, &expected, &firmware, &protected, opaque_byte9)?;
                let mut desired = before.clone();
                desired.enabled = true; desired.mode = mode; desired.sync = mode == 5;
                desired.color = [32,180,160]; desired.brightness = mode % 5;
                desired.speed = mode % 5; desired.direction = mode % 2;
                let (observed, fw, raw) = s.apply_with_report_from(&desired,Some(&expected))?;
                report["steps"].as_array_mut().unwrap().push(serde_json::json!({"kind":"mode","mode":mode,"requested":desired,"observed":observed,"response":raw}));
                if fw != firmware || protected_light_response(&raw, opaque_byte9)? != protected {
                    return Err("底座受保护字段在测试中改变，停止写入".into());
                }
                expected = observed;
                if opaque_byte9 && firmware=="0.3" && mode<5 {
                    let base=expected.clone(); let mut host=base.clone(); host.mode=5;host.sync=true;
                    let (entered,fw,raw)=s.apply_with_report_from(&host,Some(&base))?;
                    report["steps"].as_array_mut().unwrap().push(serde_json::json!({"kind":"host_transition","base_mode":mode,"requested":host,"observed":entered,"response":raw}));
                    if entered.mode!=mode||fw!=firmware||protected_light_response(&raw,opaque_byte9)?!=protected{return Err("底座音乐同步未保留当前基础模式".into());}
                    require_diagnostic_state(s,&entered,&firmware,&protected,opaque_byte9)?;
                    let (resumed,fw,raw)=s.apply_with_report(&base)?;
                    report["steps"].as_array_mut().unwrap().push(serde_json::json!({"kind":"base_resume","base_mode":mode,"requested":base,"observed":resumed,"response":raw}));
                    if fw!=firmware||protected_light_response(&raw,opaque_byte9)?!=protected{return Err("底座退出音乐同步后受保护字段改变".into());}
                    expected=resumed;
                }
                s.io.settle(Duration::from_millis(250));
            }
            for color in [[40u8,0,0],[0,40,0],[0,0,40]] {
                require_diagnostic_state(s, &expected, &firmware, &protected, opaque_byte9)?;
                let payload:Vec<u8> = color.repeat(12); s.send(41, &payload)?;
                report["steps"].as_array_mut().unwrap().push(serde_json::json!({"kind":"stream","command":41,"pixels":vec![color;12],"transport_write_complete":true,"pixel_readback":false}));
                s.io.settle(Duration::from_millis(150));
            }
            require_diagnostic_state(s, &expected, &firmware, &protected, opaque_byte9)?;
            let mut off = before.clone(); off.enabled = false;
            let (observed, fw, raw) = s.apply_with_report(&off)?;
            report["steps"].as_array_mut().unwrap().push(serde_json::json!({"kind":"off","requested":off,"observed":observed,"response":raw}));
            if fw != firmware || protected_light_response(&raw, opaque_byte9)? != protected {
                return Err("底座受保护字段在关闭测试中改变".into());
            }
            require_diagnostic_state(s, &observed, &firmware, &protected, opaque_byte9)?;
            let (after, fw, raw) = s.apply_with_report_from(&before,Some(&observed))?;
            report["after"] = serde_json::to_value(&after).unwrap();
            report["after_response"] = serde_json::to_value(&raw).unwrap();
            if fw != firmware || protected_light_response(&raw, opaque_byte9)? != protected {
                return Err("底座恢复后受保护字段不同".into());
            }
            let strip = |r:Vec<u8>| if r.first()==Some(&0) {r[1..].to_vec()} else {r};
            let original = strip(original_raw); let current = strip(raw);
            if current[6..36] != original[6..36] {return Err("恢复后完整30字节设置载荷不同".into());}
            report["full_declared_payload_restored"] = true.into();
            report["restored"] = true.into(); report["protected_fields_preserved"] = true.into();
            Ok(())
        });
        if let Err(error) = result {
            report["error"] = error.into();
            return Err(serde_json::to_string_pretty(&report).unwrap());
        }
        Ok(report)
    }
}
fn protected_light_response(raw: &[u8], opaque_byte9: bool) -> R<Vec<u8>> {
    let r = if raw.first() == Some(&0) {
        &raw[1..]
    } else {
        raw
    };
    if parse_reply(raw)?.is_none() {
        return Err("无效的底座读取响应".into());
    }
    Ok(r[6 + if opaque_byte9 { 10 } else { 9 }..6 + 29].to_vec())
}
fn require_diagnostic_state(
    s: &Session,
    expected: &Light,
    firmware: &str,
    protected: &[u8],
    opaque_byte9: bool,
) -> R<()> {
    let (current, fw, raw) = s.read_with_report()?;
    if &current != expected
        || fw != firmware
        || protected_light_response(&raw, opaque_byte9)? != protected
    {
        return Err("底座状态在测试步骤之间改变，停止写入".into());
    }
    Ok(())
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
        s.apply_verified(before, &current)?;
        Ok(())
    })
}

/// Diagnostic read only: open one exact MagDock interface and issue GET7.
/// Raw response is opt-in; normal views do not expose or persist it.
pub fn readonly_report() -> R<serde_json::Value> {
    let session = Session::open()?;
    let (light, firmware, response) = session.read_with_report()?;
    Ok(serde_json::json!({
        "format":"open-hub-magdock-readonly-report/1", "requested_command":7,
        "settings_commands_sent":false, "native_request_bytes_including_id":65,
        "response_bytes":response, "light":light, "firmware":firmware,
    }))
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
