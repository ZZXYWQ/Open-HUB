//! Connection-owned original Rival3 playback engine and persisted loop schema.
//! Explicit Open HUB loop timelines; not automatic GG playback equivalence.
use super::Service;
use gearwise_steelseries::{
    rival3,
    rival3_sequence::{Cursor, Sequence},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
type R<T> = Result<T, String>;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub id: String,
    pub connection: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub step_ms: u32,
    pub colors: [[u8; 3]; 4],
    pub zones: [Option<Sequence>; 4],
}
impl Config {
    pub fn validate(&self) -> R<()> {
        if !(33..=1000).contains(&self.step_ms) || !self.zones.iter().any(Option::is_some) {
            return Err("动态灯光需要33–1000ms帧间隔及至少一个明确灯区序列".into());
        }
        for s in self.zones.iter().flatten() {
            if s.repeat != 255 || s.frames.is_empty() || s.frames.len() > 4096 {
                return Err(
                    "当前播放层只接受1–4096帧的显式循环序列；GG有限播放尚未完整接入".into(),
                );
            }
        }
        Ok(())
    }
    pub(crate) fn palette(&self) -> rival3::Profile {
        let mut p = rival3::Profile::default();
        for (z, [r, g, b]) in p.zones.iter_mut().zip(self.colors) {
            z.color = gearwise_core::model::Rgb { r, g, b };
        }
        p
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub target: Target,
    pub playback: String,
    pub phase: String,
    pub frames: u64,
    pub writes: u64,
    pub colors: [[u8; 3]; 4],
    pub error: Option<String>,
}
#[derive(Clone, Copy)]
pub enum Control {
    Pause,
    Resume,
    Restart,
}
enum Message {
    Stop,
    Control(Control, mpsc::SyncSender<()>),
}
struct Worker {
    target: Target,
    tx: mpsc::Sender<Message>,
    handle: thread::JoinHandle<()>,
    status: Arc<Mutex<Status>>,
}
#[derive(Default)]
pub struct Player {
    operations: Mutex<()>,
    workers: Mutex<HashMap<String, Worker>>,
}
fn device<'a>(
    service: &'a mut Option<gearwise_core::DeviceManager>,
    target: &Target,
) -> R<&'a mut Box<dyn gearwise_core::Device>> {
    let d = service
        .as_mut()
        .and_then(|m| m.device_mut(&target.id))
        .ok_or("灯光目标已断开")?;
    if (d.descriptor().vendor_id, d.descriptor().product_id) != (0x1038, 0x1824) {
        return Err("当前播放层仅核对原版有线Rival3 1038:1824".into());
    }
    if d.connection_key() != Some(target.connection.as_str()) || d.lighting_revision().is_none() {
        return Err("灯光目标连接已变化或没有可核对的写入归属".into());
    }
    Ok(d)
}
impl Player {
    pub fn start(&self, service: Service, target: Target, config: Config) -> R<Status> {
        config.validate()?;
        if target.id.is_empty()
            || target.id.len() > 4096
            || target.connection.is_empty()
            || target.connection.len() > 4096
        {
            return Err("灯光目标标识无效".into());
        }
        let cursors = config
            .zones
            .iter()
            .map(|s| {
                s.clone()
                    .map(|s| Cursor::new(s, config.step_ms))
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let _op = self.operations.lock().map_err(|_| "灯光操作锁异常")?;
        {
            let mut manager = service.0.lock().map_err(|_| "设备锁异常")?;
            device(&mut manager, &target)?;
        }
        self.stop_id(&target.id)?;
        let palette = config.palette();
        let revision = {
            let mut manager = service.0.lock().map_err(|_| "设备锁异常")?;
            let d = device(&mut manager, &target)?;
            d.apply(&rival3::setting(&palette))
                .map_err(|e| format!("灯光初始化未完成，未启动播放：{e}"))?;
            let revision = d.lighting_revision().ok_or("灯光归属缺失")?;
            // A former static session must not authorize flashing a moving frame.
            service
                .3
                .lock()
                .map_err(|_| "会话方案锁异常")?
                .remove(&target.id);
            revision
        };
        let initial = Status {
            target: target.clone(),
            playback: crate::profiles::new_id(),
            phase: "running".into(),
            frames: 0,
            writes: 0,
            colors: config.colors,
            error: None,
        };
        let status = Arc::new(Mutex::new(initial.clone()));
        let state = status.clone();
        let bound = target.clone();
        let (tx, rx) = mpsc::channel();
        let manager = service.0.clone();
        // Read/attach_session holds manager before consulting this registry.
        // Never retain the registry lock while initializing or joining device IO.
        let mut workers = self.workers.lock().map_err(|_| "灯光线程锁异常")?;
        let handle = thread::Builder::new()
            .name("openhub-rival3-lighting".into())
            .spawn(move || {
                run(
                    manager, bound, config, palette, revision, cursors, rx, state,
                )
            })
            .map_err(|e| format!("灯光初始色已写入，但播放线程未启动：{e}"))?;
        workers.insert(
            target.id.clone(),
            Worker {
                target,
                tx,
                handle,
                status,
            },
        );
        Ok(initial)
    }
    pub(crate) fn find_status(&self, target: &Target) -> R<Option<Status>> {
        let workers = self.workers.lock().map_err(|_| "灯光线程锁异常")?;
        match workers.get(&target.id) {
            None => Ok(None),
            Some(w) if w.target != *target => Err("灯光连接已变化".into()),
            Some(w) => Ok(Some(w.status.lock().map_err(|_| "灯光状态锁异常")?.clone())),
        }
    }
    pub(crate) fn stop_device_id(&self, id: &str) -> R<()> {
        let _op = self.operations.lock().map_err(|_| "灯光操作锁异常")?;
        self.stop_id(id)?;
        Ok(())
    }
    pub fn status(&self, target: &Target) -> R<Status> {
        let workers = self.workers.lock().map_err(|_| "灯光线程锁异常")?;
        let w = workers
            .get(&target.id)
            .filter(|w| w.target == *target)
            .ok_or("没有此连接的灯光播放")?;
        let mut s = w.status.lock().map_err(|_| "灯光状态锁异常")?.clone();
        if w.handle.is_finished() && matches!(s.phase.as_str(), "running" | "paused") {
            s.phase = "failed".into();
            s.error = Some("灯光线程异常结束".into());
        }
        Ok(s)
    }
    pub fn control(&self, target: &Target, control: Control) -> R<Status> {
        let _op = self.operations.lock().map_err(|_| "灯光操作锁异常")?;
        let (tx, rx) = mpsc::sync_channel(1);
        {
            let workers = self.workers.lock().map_err(|_| "灯光线程锁异常")?;
            let w = workers
                .get(&target.id)
                .filter(|w| w.target == *target)
                .ok_or("灯光连接不匹配")?;
            w.tx.send(Message::Control(control, tx))
                .map_err(|_| "灯光线程已结束")?;
        }
        rx.recv_timeout(Duration::from_secs(2))
            .map_err(|_| "灯光控制尚未确认；请检查播放状态")?;
        self.status(target)
    }
    pub fn stop(&self, target: &Target) -> R<Status> {
        let _op = self.operations.lock().map_err(|_| "灯光操作锁异常")?;
        {
            let workers = self.workers.lock().map_err(|_| "灯光线程锁异常")?;
            if !workers.get(&target.id).is_some_and(|w| w.target == *target) {
                return Err("灯光连接不匹配，未停止其他连接".into());
            }
        }
        self.stop_id(&target.id)?.ok_or("没有灯光线程".into())
    }
    fn stop_id(&self, id: &str) -> R<Option<Status>> {
        let worker = self
            .workers
            .lock()
            .map_err(|_| "灯光线程锁异常")?
            .remove(id);
        let Some(w) = worker else { return Ok(None) };
        let _ = w.tx.send(Message::Stop);
        w.handle.join().map_err(|_| "灯光线程异常结束")?;
        let result = w.status.lock().map_err(|_| "灯光状态锁异常")?.clone();
        Ok(Some(result))
    }
    pub fn stop_all(&self) -> R<()> {
        let _op = self.operations.lock().map_err(|_| "灯光操作锁异常")?;
        let workers = self
            .workers
            .lock()
            .map_err(|_| "灯光线程锁异常")?
            .drain()
            .map(|(_, w)| w)
            .collect::<Vec<_>>();
        for w in &workers {
            let _ = w.tx.send(Message::Stop);
        }
        let mut failed = false;
        for w in workers {
            failed |= w.handle.join().is_err();
        }
        if failed {
            Err("有灯光线程异常结束".into())
        } else {
            Ok(())
        }
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        let workers = self
            .workers
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .drain()
            .map(|(_, w)| w)
            .collect::<Vec<_>>();
        for w in &workers {
            let _ = w.tx.send(Message::Stop);
        }
        for w in workers {
            let _ = w.handle.join();
        }
    }
}
fn run(
    manager: Arc<Mutex<Option<gearwise_core::DeviceManager>>>,
    target: Target,
    config: Config,
    mut palette: rival3::Profile,
    mut revision: u64,
    mut cursors: Vec<Option<Cursor>>,
    rx: mpsc::Receiver<Message>,
    status: Arc<Mutex<Status>>,
) {
    let epoch = Instant::now();
    let step = Duration::from_millis(u64::from(config.step_ms));
    let mut due = epoch + step;
    let mut last_slot = 0;
    let mut paused = false;
    let result = (|| -> R<()> {
        loop {
            let wait = if paused {
                Duration::from_secs(3600)
            } else {
                due.saturating_duration_since(Instant::now())
            };
            match rx.recv_timeout(wait) {
                Ok(Message::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                Ok(Message::Control(control, ack)) => {
                    match control {
                        Control::Pause => {
                            paused = true;
                            for c in cursors.iter_mut().flatten() {
                                c.stop();
                            }
                        }
                        Control::Resume => {
                            paused = false;
                            for c in cursors.iter_mut().flatten() {
                                c.resume();
                            }
                        }
                        Control::Restart => {
                            paused = false;
                            for c in cursors.iter_mut().flatten() {
                                c.start();
                            }
                        }
                    }
                    last_slot = epoch.elapsed().as_millis() / u128::from(config.step_ms);
                    due = Instant::now() + step;
                    status.lock().map_err(|_| "灯光状态锁异常")?.phase =
                        if paused { "paused" } else { "running" }.into();
                    let _ = ack.send(());
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if paused {
                continue;
            }
            let elapsed = epoch.elapsed().as_millis();
            let slot = elapsed / u128::from(config.step_ms);
            let pending = slot.saturating_sub(last_slot).min(u128::from(u32::MAX)) as u32;
            last_slot = slot;
            due = epoch
                + Duration::from_millis(
                    ((slot + 1) * u128::from(config.step_ms)).min(u128::from(u64::MAX)) as u64,
                );
            let mut next = palette.clone();
            for (i, c) in cursors.iter_mut().enumerate() {
                if let Some(c) = c {
                    if let Some([r, g, b]) = c
                        .tick(elapsed.min(u128::from(u64::MAX)) as u64, pending)
                        .color
                    {
                        next.zones[i].color = gearwise_core::model::Rgb { r, g, b };
                    }
                }
            }
            let wrote = {
                let mut manager = manager.lock().map_err(|_| "设备锁异常")?;
                let d = device(&mut manager, &target)?;
                if d.lighting_revision() != Some(revision) {
                    return Err("此连接的灯光写入归属已改变，旧播放已停止".into());
                }
                if next != palette {
                    d.apply(&rival3::setting(&next))
                        .map_err(|e| format!("灯光帧写入失败，未重试：{e}"))?;
                    revision = d.lighting_revision().ok_or("灯光归属缺失")?;
                    true
                } else {
                    false
                }
            };
            palette = next;
            let mut s = status.lock().map_err(|_| "灯光状态锁异常")?;
            s.frames += 1;
            s.writes += u64::from(wrote);
            s.colors = palette
                .zones
                .clone()
                .map(|z| [z.color.r, z.color.g, z.color.b]);
        }
    })();
    if let Ok(mut s) = status.lock() {
        match result {
            Ok(()) => s.phase = "stopped".into(),
            Err(e) => {
                s.phase = "failed".into();
                s.error = Some(e);
            }
        }
    }
}

pub const SCHEMA: &str = "openhub-rival3-loop/1";
pub(crate) fn from_plan(plan: &super::Plan) -> R<Option<Config>> {
    let loops = plan
        .settings
        .iter()
        .filter_map(|s| {
            if let gearwise_core::Setting::VendorProfile { schema, data } = s {
                (schema == SCHEMA).then_some(data)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if loops.is_empty() {
        return Ok(None);
    }
    if (plan.vendor, plan.product) != (0x1038, 0x1824)
        || loops.len() != 1
        || plan
            .settings
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    gearwise_core::Setting::VendorProfile { .. }
                        | gearwise_core::Setting::Lighting { .. }
                )
            })
            .count()
            != 1
    {
        return Err("动态灯光方案只适用于原版Rival3 1824，不能与其他灯效重复".into());
    }
    let config: Config = serde_json::from_value(loops[0].clone())
        .map_err(|e| format!("动态灯光方案格式无效：{e}"))?;
    config.validate()?;
    Ok(Some(config))
}
pub(crate) fn native_plan(plan: &super::Plan, config: &Config) -> super::Plan {
    let mut p = plan.clone();
    for s in &mut p.settings {
        if matches!(s,gearwise_core::Setting::VendorProfile{schema,..}if schema==SCHEMA) {
            *s = rival3::setting(&config.palette());
        }
    }
    p
}

pub(crate) fn simulate_frames(
    plan: &super::Plan,
    device: &mut dyn gearwise_core::Device,
) -> R<Option<serde_json::Value>> {
    let Some(config) = from_plan(plan)? else {
        return Ok(None);
    };
    let mut cursors = config
        .zones
        .iter()
        .map(|s| {
            s.clone()
                .map(|s| Cursor::new(s, config.step_ms))
                .transpose()
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut palette = config.palette();
    let mut frames = Vec::new();
    for tick in 1..=12u64 {
        for (i, c) in cursors.iter_mut().enumerate() {
            if let Some(c) = c {
                if let Some([r, g, b]) = c.tick(tick * u64::from(config.step_ms), 1).color {
                    palette.zones[i].color = gearwise_core::model::Rgb { r, g, b };
                }
            }
        }
        device
            .apply(&rival3::setting(&palette))
            .map_err(|e| e.to_string())?;
        frames.push(
            palette
                .zones
                .clone()
                .map(|z| [z.color.r, z.color.g, z.color.b]),
        );
    }
    Ok(Some(
        serde_json::json!({"step_ms":config.step_ms,"frames":frames,"real_time":false,"hardware_tested":false}),
    ))
}

#[cfg(test)]
mod tests {
    use super::super::MockLog;
    use super::*;
    use gearwise_core::{
        transport::{HidInterfaceInfo, HidTransport, MockBackend, MockReply, MockTransport},
        Device, DeviceManager, Driver,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Fault {
        inner: MockTransport,
        count: Arc<AtomicUsize>,
        fail: usize,
        probe: Option<std::sync::Weak<Player>>,
    }
    impl HidTransport for Fault {
        fn write(&mut self, data: &[u8]) -> gearwise_core::Result<usize> {
            if let Some(player) = self.probe.as_ref().and_then(std::sync::Weak::upgrade) {
                assert!(
                    std::thread::current().name() == Some("openhub-rival3-lighting")
                        || player.workers.try_lock().is_ok(),
                    "native IO must not hold worker registry"
                );
            }
            let n = self.inner.write(data)?;
            let count = self.count.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(n - usize::from(count == self.fail))
        }
        fn send_feature_report(&mut self, _: &[u8]) -> gearwise_core::Result<()> {
            panic!("no feature IO")
        }
        fn read_timeout(&mut self, _: &mut [u8], _: i32) -> gearwise_core::Result<usize> {
            panic!("no physical read")
        }
        fn get_feature_report(&mut self, _: &mut [u8]) -> gearwise_core::Result<usize> {
            panic!("no physical read")
        }
    }
    fn opened(pid: u16, unit: &str, fail: usize) -> (Box<dyn Device>, MockLog, Arc<AtomicUsize>) {
        opened_with_probe(pid, unit, fail, None)
    }
    fn opened_with_probe(
        pid: u16,
        unit: &str,
        fail: usize,
        probe: Option<std::sync::Weak<Player>>,
    ) -> (Box<dyn Device>, MockLog, Arc<AtomicUsize>) {
        let info = HidInterfaceInfo {
            path: format!("simulation:player:{unit}"),
            vendor_id: 0x1038,
            product_id: pid,
            interface_number: 3,
            usage_page: 0xffc0,
            usage: 1,
            serial: Some(unit.into()),
            ..Default::default()
        };
        let inner = MockTransport::new(|_| MockReply::none());
        let log = inner.log();
        let count = Arc::new(AtomicUsize::new(0));
        let slot = Mutex::new(Some(Fault {
            inner,
            count: count.clone(),
            fail,
            probe,
        }));
        let backend = MockBackend::new(vec![info.clone()], move |_| {
            Ok(Box::new(slot.lock().unwrap().take().unwrap()))
        });
        let d = gearwise_steelseries::SteelSeriesDriver::with_gap(Duration::ZERO)
            .open(&info, &backend)
            .unwrap()
            .remove(0);
        (d, log, count)
    }
    fn manager(devices: Vec<Box<dyn Device>>) -> DeviceManager {
        let mut m = DeviceManager::new(
            Box::new(MockBackend::new(vec![], |_| panic!("no HID"))),
            vec![],
        );
        m.add_virtual(devices);
        m
    }
    fn target(d: &dyn Device) -> Target {
        Target {
            id: d.descriptor().id.clone(),
            connection: d.connection_key().unwrap().into(),
        }
    }
    fn fixture(pid: u16, fail: usize) -> (Service, Target, MockLog, Arc<AtomicUsize>) {
        let (d, log, count) = opened(pid, "A", fail);
        let t = target(d.as_ref());
        let service = Service::default();
        *service.0.lock().unwrap() = Some(manager(vec![d]));
        (service, t, log, count)
    }
    fn config() -> Config {
        Config {
            step_ms: 33,
            colors: [[0, 0, 0], [11, 22, 33], [44, 55, 66], [77, 88, 99]],
            zones: [
                Some(Sequence {
                    repeat: 255,
                    frames: vec![[255, 0, 0], [0, 255, 0], [0, 0, 255]],
                }),
                None,
                None,
                None,
            ],
        }
    }
    fn wait(player: &Player, t: &Target, f: impl Fn(&Status) -> bool) -> Status {
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            let s = player.status(t).unwrap();
            if f(&s) {
                return s;
            }
            assert!(Instant::now() < end, "status {s:?}");
            thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn rival3_player_runs_real_driver_packets_and_joined_stop_leaves_no_flash_or_later_writes() {
        let (service, t, log, count) = fixture(0x1824, usize::MAX);
        let p = Player::default();
        p.start(service.clone(), t.clone(), config()).unwrap();
        wait(&p, &t, |s| s.writes >= 2);
        let stopped = p.stop(&t).unwrap();
        assert_eq!(stopped.phase, "stopped");
        let done = count.load(Ordering::SeqCst);
        thread::sleep(Duration::from_millis(80));
        assert_eq!(count.load(Ordering::SeqCst), done);
        let packets = log.lock().unwrap();
        assert_eq!(
            packets[0],
            gearwise_core::transport::MockRecord::Write(vec![0, 5, 0, 1, 0, 0, 0, 100])
        );
        let batch = packets
            .iter()
            .find_map(|r| {
                if let gearwise_core::transport::MockRecord::Write(v) = r {
                    (v[1] == 10).then_some(v)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(batch.len(), 33);
        assert_eq!(
            &batch[..16],
            &[0, 10, 0, 15, 255, 0, 0, 11, 22, 33, 44, 55, 66, 77, 88, 99]
        );
        assert!(packets
            .iter()
            .all(|r| !matches!(r,gearwise_core::transport::MockRecord::Write(v)if v[1]==9)));
    }
    #[test]
    fn rival3_player_manual_identical_write_revokes_old_writer_before_another_frame() {
        let (service, t, _log, count) = fixture(0x1824, usize::MAX);
        let p = Player::default();
        p.start(service.clone(), t.clone(), config()).unwrap();
        p.control(&t, Control::Pause).unwrap();
        let done = {
            let mut m = service.0.lock().unwrap();
            let d = device(&mut m, &t).unwrap();
            let data = d.read_state().unwrap().vendor_profile.unwrap();
            d.apply(&gearwise_core::Setting::VendorProfile {
                schema: rival3::SCHEMA.into(),
                data,
            })
            .unwrap();
            count.load(Ordering::SeqCst)
        };
        p.control(&t, Control::Resume).unwrap();
        let stopped = wait(&p, &t, |s| s.phase == "failed");
        assert!(stopped.error.unwrap().contains("归属"));
        assert_eq!(count.load(Ordering::SeqCst), done);
        p.stop(&t).unwrap();
    }
    #[test]
    fn rival3_player_reopened_same_id_never_receives_old_frames() {
        let (service, t, _log, _count) = fixture(0x1824, usize::MAX);
        let p = Player::default();
        p.start(service.clone(), t.clone(), config()).unwrap();
        p.control(&t, Control::Pause).unwrap();
        let (d, replacement_log, _) = opened(0x1824, "A", usize::MAX);
        assert_eq!(d.descriptor().id, t.id);
        assert_ne!(d.connection_key(), Some(t.connection.as_str()));
        *service.0.lock().unwrap() = Some(manager(vec![d]));
        p.control(&t, Control::Resume).unwrap();
        assert!(wait(&p, &t, |s| s.phase == "failed")
            .error
            .unwrap()
            .contains("连接"));
        assert!(replacement_log.lock().unwrap().is_empty());
        p.stop(&t).unwrap();
    }
    #[test]
    fn rival3_player_identical_devices_pause_resume_restart_and_stop_independently() {
        let (a, _, ca) = opened(0x1824, "A", usize::MAX);
        let (b, _, cb) = opened(0x1824, "B", usize::MAX);
        let ta = target(a.as_ref());
        let tb = target(b.as_ref());
        let service = Service::default();
        *service.0.lock().unwrap() = Some(manager(vec![a, b]));
        let p = Player::default();
        p.start(service.clone(), ta.clone(), config()).unwrap();
        p.start(service.clone(), tb.clone(), config()).unwrap();
        wait(&p, &ta, |s| s.writes > 0);
        wait(&p, &tb, |s| s.writes > 0);
        assert_eq!(p.control(&ta, Control::Pause).unwrap().phase, "paused");
        let ac = ca.load(Ordering::SeqCst);
        let bw = p.status(&tb).unwrap().writes;
        wait(&p, &tb, |s| s.writes > bw);
        assert_eq!(ca.load(Ordering::SeqCst), ac);
        p.control(&ta, Control::Restart).unwrap();
        wait(&p, &ta, |s| s.writes > 1);
        p.stop(&ta).unwrap();
        let ac = ca.load(Ordering::SeqCst);
        let bw = p.status(&tb).unwrap().writes;
        wait(&p, &tb, |s| s.writes > bw);
        assert_eq!(ca.load(Ordering::SeqCst), ac);
        p.stop_all().unwrap();
        let bc = cb.load(Ordering::SeqCst);
        thread::sleep(Duration::from_millis(60));
        assert_eq!(cb.load(Ordering::SeqCst), bc);
    }
    #[test]
    fn rival3_player_first_frame_short_write_stops_once_and_invalidates_palette() {
        let (service, t, _log, count) = fixture(0x1824, 6);
        let p = Player::default();
        p.start(service.clone(), t.clone(), config()).unwrap();
        let status = wait(&p, &t, |s| s.phase == "failed");
        assert!(status.error.unwrap().contains("未重试"));
        assert_eq!(count.load(Ordering::SeqCst), 6);
        {
            let mut m = service.0.lock().unwrap();
            assert!(device(&mut m, &t)
                .unwrap()
                .read_state()
                .unwrap()
                .vendor_profile
                .is_none());
        }
        p.stop(&t).unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 6);
    }
    #[test]
    fn rival3_player_wrong_firmware_bad_source_and_invalid_config_never_write_or_cancel_live_target(
    ) {
        let (service, t, log, _) = fixture(0x184c, usize::MAX);
        let p = Player::default();
        assert!(p.start(service, t, config()).is_err());
        assert!(log.lock().unwrap().is_empty());
        let (service, t, log, _) = fixture(0x1824, usize::MAX);
        let mut bad = config();
        bad.step_ms = 1;
        assert!(p.start(service.clone(), t.clone(), bad).is_err());
        assert!(log.lock().unwrap().is_empty());
        p.start(service.clone(), t.clone(), config()).unwrap();
        let stale = Target {
            connection: "stale".into(),
            ..t.clone()
        };
        assert!(p.start(service, stale.clone(), config()).is_err());
        assert!(p.stop(&stale).is_err());
        wait(&p, &t, |s| s.writes > 0);
        p.stop(&t).unwrap();
    }
    #[test]
    fn rival3_player_revokes_old_flash_permission_and_drop_joins_the_worker() {
        let (service, t, _log, count) = fixture(0x1824, usize::MAX);
        let out = {
            let mut manager = service.0.lock().unwrap();
            super::super::view(device(&mut manager, &t).unwrap().as_mut()).unwrap()
        };
        let plan = super::super::template_plan(&out).unwrap();
        service.remember_session(&out, &plan, false).unwrap();
        assert!(service.3.lock().unwrap().contains_key(&t.id));
        let p = Player::default();
        p.start(service.clone(), t.clone(), config()).unwrap();
        assert!(!service.3.lock().unwrap().contains_key(&t.id));
        wait(&p, &t, |s| s.writes > 0);
        drop(p);
        let done = count.load(Ordering::SeqCst);
        thread::sleep(Duration::from_millis(70));
        assert_eq!(count.load(Ordering::SeqCst), done);
    }
    struct Silent;
    impl gearwise_input::InputSink for Silent {
        fn key_down(&mut self, _: u8) -> gearwise_core::Result<()> {
            panic!("no OS input")
        }
        fn key_up(&mut self, _: u8) -> gearwise_core::Result<()> {
            panic!("no OS input")
        }
        fn mouse_down(
            &mut self,
            _: gearwise_core::model::MouseButton,
        ) -> gearwise_core::Result<()> {
            panic!("no OS input")
        }
        fn mouse_up(&mut self, _: gearwise_core::model::MouseButton) -> gearwise_core::Result<()> {
            panic!("no OS input")
        }
        fn scroll(&mut self, _: i32, _: i32) -> gearwise_core::Result<()> {
            panic!("no OS input")
        }
        fn media(&mut self, _: u16) -> gearwise_core::Result<()> {
            panic!("no OS input")
        }
    }
    struct Root(std::path::PathBuf);
    impl Root {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("openhub-loop-{}", crate::profiles::new_id())))
        }
    }
    impl Drop for Root {
        fn drop(&mut self) {
            for name in ["backups", "recovery"] {
                let dir = self.0.join(name);
                if let Ok(items) = std::fs::read_dir(&dir) {
                    for item in items {
                        let p = item.unwrap().path();
                        assert!(p.is_file());
                        std::fs::remove_file(p).unwrap();
                    }
                    std::fs::remove_dir(dir).unwrap();
                }
            }
            if self.0.exists() {
                std::fs::remove_dir(&self.0).unwrap();
            }
        }
    }
    fn runtime() -> crate::macro_runtime::Runtime {
        crate::macro_runtime::Runtime::simulation(Arc::new(Mutex::new(Silent)))
    }
    fn full_plan(service: &Service, t: &Target) -> super::super::Plan {
        let out = {
            let mut manager = service.0.lock().unwrap();
            super::super::view(device(&mut manager, t).unwrap().as_mut()).unwrap()
        };
        let mut p = super::super::template_plan(&out).unwrap();
        for s in &mut p.settings {
            if matches!(s, gearwise_core::Setting::VendorProfile { .. }) {
                *s = gearwise_core::Setting::VendorProfile {
                    schema: SCHEMA.into(),
                    data: serde_json::to_value(config()).unwrap(),
                };
            }
        }
        p
    }
    #[test]
    fn rival3_loop_native_initialization_releases_registry_before_device_io() {
        let service = Service::default();
        let (d, _, _) = opened_with_probe(
            0x1824,
            "lock-probe",
            usize::MAX,
            Some(Arc::downgrade(&service.4)),
        );
        let t = target(d.as_ref());
        *service.0.lock().unwrap() = Some(manager(vec![d]));
        let rt = runtime();
        let root = Root::new();
        let plan = full_plan(&service, &t);
        service
            .apply_context(&t.id, &plan, &rt, &root.0, None, None, Some(&t.connection))
            .unwrap();
        wait(&service.4, &t, |s| s.writes > 0);
        service.read(&t.id, &rt).unwrap();
        service.shutdown_lighting().unwrap();
        service.stop_software(&rt).unwrap();
    }
    #[test]
    fn rival3_loop_profile_applies_preserves_desired_session_and_rejects_flash() {
        let (service, t, log, count) = fixture(0x1824, usize::MAX);
        let rt = runtime();
        let root = Root::new();
        let plan = full_plan(&service, &t);
        super::super::validate(&plan).unwrap();
        assert!(service
            .apply_context(&t.id, &plan, &rt, &root.0, None, None, None)
            .is_err());
        assert!(log.lock().unwrap().is_empty());
        let out = service
            .apply_context(&t.id, &plan, &rt, &root.0, None, None, Some(&t.connection))
            .unwrap();
        assert_eq!(out.session.unwrap().plan, plan);
        let state = service
            .lighting_status(&t.id, &t.connection)
            .unwrap()
            .unwrap();
        wait(&service.4, &t, |s| s.writes > 0);
        let paused = service
            .lighting_control(&t.id, &t.connection, &state.playback, "pause")
            .unwrap();
        assert_eq!(paused.phase, "paused");
        let read = service.read(&t.id, &rt).unwrap();
        let session = read.session.unwrap();
        assert_eq!(session.software_state, "paused");
        assert_eq!(session.plan, plan);
        assert!(service
            .save_native_hardware_at(&t.id, &plan, &t.connection, &root.0)
            .unwrap_err()
            .contains("动态灯光"));
        service
            .lighting_control(&t.id, &t.connection, &state.playback, "resume")
            .unwrap();
        let descriptor = service.peers().into_iter().find(|d| d.id == t.id).unwrap();
        let identity = crate::profile_routing::serial_key(
            descriptor.vendor_id,
            descriptor.product_id,
            descriptor.serial.as_deref(),
        );
        let activated = service
            .apply_at_identity(&t.id, &plan, &rt, &root.0, None, Some(&identity))
            .unwrap();
        assert_eq!(activated.session.unwrap().plan, plan);
        assert_ne!(service.4.status(&t).unwrap().playback, state.playback);
        service.shutdown_lighting().unwrap();
        let done = count.load(Ordering::SeqCst);
        assert!(service
            .apply_context(&t.id, &plan, &rt, &root.0, None, None, Some(&t.connection))
            .is_err());
        assert_eq!(count.load(Ordering::SeqCst), done);
        service.stop_software(&rt).unwrap();
        assert!(log
            .lock()
            .unwrap()
            .iter()
            .all(|r| !matches!(r,gearwise_core::transport::MockRecord::Write(b)if b[1]==9)));
    }
    #[test]
    fn rival3_loop_new_plan_rejects_old_playback_and_stale_connection_without_canceling_current() {
        let (service, t, _log, count) = fixture(0x1824, usize::MAX);
        let rt = runtime();
        let root = Root::new();
        let plan = full_plan(&service, &t);
        service
            .apply_context(&t.id, &plan, &rt, &root.0, None, None, Some(&t.connection))
            .unwrap();
        let old = service
            .lighting_status(&t.id, &t.connection)
            .unwrap()
            .unwrap();
        service
            .apply_context(&t.id, &plan, &rt, &root.0, None, None, Some(&t.connection))
            .unwrap();
        let current = service
            .lighting_status(&t.id, &t.connection)
            .unwrap()
            .unwrap();
        assert_ne!(old.playback, current.playback);
        assert!(service
            .lighting_control(&t.id, &t.connection, &old.playback, "pause")
            .unwrap_err()
            .contains("方案已变化"));
        assert!(service
            .apply_context(
                &t.id,
                &plan,
                &rt,
                &root.0,
                None,
                None,
                Some("stale-connection")
            )
            .is_err());
        assert_eq!(
            service
                .lighting_status(&t.id, &t.connection)
                .unwrap()
                .unwrap()
                .playback,
            current.playback
        );
        let mut native = rival3::Profile::default();
        native.zones[0].color = gearwise_core::model::Rgb { r: 9, g: 8, b: 7 };
        let static_plan = super::super::Plan {
            settings: vec![rival3::setting(&native)],
            ..plan
        };
        service
            .apply_context(
                &t.id,
                &static_plan,
                &rt,
                &root.0,
                None,
                None,
                Some(&t.connection),
            )
            .unwrap();
        assert!(service
            .lighting_status(&t.id, &t.connection)
            .unwrap()
            .is_none());
        let done = count.load(Ordering::SeqCst);
        thread::sleep(Duration::from_millis(70));
        assert_eq!(count.load(Ordering::SeqCst), done);
        service.stop_software(&rt).unwrap();
    }
    #[test]
    fn rival3_loop_malformed_foreign_mixed_and_finite_plans_fail_before_any_packets() {
        let (service, t, log, _) = fixture(0x1824, usize::MAX);
        let base = full_plan(&service, &t);
        for case in 0..8 {
            let mut p = base.clone();
            let data = p
                .settings
                .iter_mut()
                .find_map(|s| {
                    if let gearwise_core::Setting::VendorProfile { data, .. } = s {
                        Some(data)
                    } else {
                        None
                    }
                })
                .unwrap();
            match case {
                0 => data["step_ms"] = 1.into(),
                1 => data["zones"][0]["repeat"] = 2.into(),
                2 => data["colors"] = serde_json::json!([]),
                3 => data["zones"][0]["data"] = serde_json::json!([]),
                4 => data["unknown"] = 1.into(),
                5 => p.product = 0x184c,
                6 => p
                    .settings
                    .push(rival3::setting(&rival3::Profile::default())),
                _ => p.settings.push(gearwise_core::Setting::Lighting {
                    zone: 0,
                    state: gearwise_core::model::ZoneState {
                        effect: gearwise_core::model::LightingEffect::Off,
                        brightness: 100,
                    },
                }),
            }
            assert!(super::super::validate(&p).is_err(), "case {case}");
        }
        assert!(log.lock().unwrap().is_empty());
        let r = super::super::simulate(&base).unwrap();
        assert_eq!(r["animation"]["frames"].as_array().unwrap().len(), 12);
        assert_eq!(r["animation"]["real_time"], false);
    }
    #[test]
    #[ignore = "requires browser-edited official GG Rival3 dynamic profile and preview"]
    fn rival3_gg_dynamic_browser_profile_preserves_1212_frames_and_reaches_scoped_worker() {
        let profiles = super::super::roundtrip_steelseries_browser_profiles(
            "OPEN_HUB_GG_RIVAL3_DYNAMIC_UI_PROFILES",
        );
        assert_eq!(profiles.len(), 1);
        let preview: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var_os("OPEN_HUB_GG_RIVAL3_DYNAMIC_PREVIEW").unwrap())
                .unwrap(),
        )
        .unwrap();
        let mut original: super::super::Plan =
            serde_json::from_value(preview["candidates"][0]["profile"]["multi"].clone()).unwrap();
        let before = from_plan(&original).unwrap().unwrap();
        assert_eq!(before.step_ms, 33);
        assert_eq!(
            before
                .zones
                .iter()
                .flatten()
                .map(|s| s.frames.len())
                .collect::<Vec<_>>(),
            vec![303; 4]
        );
        for setting in &mut original.settings {
            if let gearwise_core::Setting::VendorProfile { schema, data } = setting {
                if schema == SCHEMA {
                    data["zones"][3]["data"][302] = serde_json::json!([18, 52, 86]);
                    data["colors"][1] = serde_json::json!([171, 205, 239]);
                }
            }
        }
        let plan = profiles[0].multi.as_ref().unwrap();
        assert_eq!(plan, &original);
        let simulation = super::super::simulate(plan).unwrap();
        assert_eq!(
            simulation["animation"]["frames"].as_array().unwrap().len(),
            12
        );
        let (service, t, log, count) = fixture(0x1824, usize::MAX);
        let rt = runtime();
        let root = Root::new();
        service
            .apply_context(&t.id, plan, &rt, &root.0, None, None, Some(&t.connection))
            .unwrap();
        wait(&service.4, &t, |s| s.writes > 0);
        let state = service.4.status(&t).unwrap();
        service
            .lighting_control(&t.id, &t.connection, &state.playback, "pause")
            .unwrap();
        let read = service.read(&t.id, &rt).unwrap();
        assert_eq!(read.session.unwrap().plan, *plan);
        assert!(service
            .save_native_hardware_at(&t.id, plan, &t.connection, &root.0)
            .is_err());
        service
            .lighting_control(&t.id, &t.connection, &state.playback, "stop")
            .unwrap();
        let done = count.load(Ordering::SeqCst);
        thread::sleep(Duration::from_millis(70));
        assert_eq!(count.load(Ordering::SeqCst), done);
        service.shutdown_lighting().unwrap();
        service.stop_software(&rt).unwrap();
        assert!(log
            .lock()
            .unwrap()
            .iter()
            .all(|r| !matches!(r,gearwise_core::transport::MockRecord::Write(b)if b[1]==9)));
    }
    #[test]
    #[ignore = "requires saved Rival3 loop browser profile"]
    fn rival3_loop_browser_profile_store_import_and_live_worker_replay() {
        let profiles = super::super::roundtrip_steelseries_browser_profiles(
            "OPEN_HUB_RIVAL3_LOOP_UI_PROFILES",
        );
        assert_eq!(profiles.len(), 1);
        let (service, t, _log, _) = fixture(0x1824, usize::MAX);
        let rt = runtime();
        let root = Root::new();
        let p = profiles[0].multi.as_ref().unwrap();
        assert!(from_plan(p).unwrap().is_some());
        assert_eq!(from_plan(p).unwrap().unwrap().step_ms, 77);
        assert_eq!(
            super::super::simulate(p).unwrap()["animation"]["frames"]
                .as_array()
                .unwrap()
                .len(),
            12
        );
        service
            .apply_context(&t.id, p, &rt, &root.0, None, None, Some(&t.connection))
            .unwrap();
        wait(&service.4, &t, |s| s.writes > 0);
        let out = service.read(&t.id, &rt).unwrap();
        assert_eq!(out.session.unwrap().plan, *p);
        service.shutdown_lighting().unwrap();
        service.stop_software(&rt).unwrap();
    }
}
