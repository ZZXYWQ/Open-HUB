//! One effect clock drives either MagDock, selected OpenRGB devices, or both.
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
type R<T> = Result<T, String>;
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub kind: String,
    pub style: u8,
    pub color: [u8; 3],
    pub palette: Vec<[u8; 3]>,
    pub brightness: f32,
    pub speed: f32,
    pub gain: f32,
    pub dock: bool,
    pub rgb: Vec<crate::rgb::Target>,
}
impl Config {
    pub fn validate(&self) -> R<()> {
        if ![
            "static", "rainbow", "gradient", "breathe", "comet", "twinkle", "music", "screen",
        ]
        .contains(&self.kind.as_str())
            || self.style > 5
            || self.palette.len() < 2
            || self.palette.len() > 8
            || self.rgb.len() > 64
            || !self.brightness.is_finite()
            || !(0.0..=1.0).contains(&self.brightness)
            || !self.speed.is_finite()
            || !(0.1..=5.0).contains(&self.speed)
            || !self.gain.is_finite()
            || !(0.5..=8.0).contains(&self.gain)
            || (!self.dock && self.rgb.is_empty())
        {
            return Err("请选择灯光目标，并检查效果参数".into());
        }
        Ok(())
    }
}
#[derive(Clone, Default, Serialize)]
pub struct Status {
    pub running: bool,
    pub kind: String,
    pub level: f32,
    pub pixels: [[u8; 3]; 12],
    pub error: Option<String>,
    pub config: Option<Config>,
}
struct Worker {
    stop: Arc<AtomicBool>,
    handle: std::thread::JoinHandle<()>,
    // Bound to this worker, including while it is starting. The status DTO is
    // published later and may still describe the preceding effect session.
    owns_dock: bool,
}
fn stop_worker(worker: &mut Option<Worker>) -> R<()> {
    if let Some(w) = worker.take() {
        w.stop.store(true, Ordering::SeqCst);
        w.handle.join().map_err(|_| "灯光线程异常退出")?;
    }
    Ok(())
}
#[derive(Default)]
pub struct Engine {
    routing: Mutex<()>,
    worker: Mutex<Option<Worker>>,
    state: Arc<Mutex<Status>>,
}
impl Engine {
    /// One command owns the complete stop/restore/mutate/start handoff. Worker
    /// methods do not take this lock themselves, so a routed handoff never
    /// recursively locks it while stopping or starting either output worker.
    pub fn route<T>(&self, action: impl FnOnce() -> R<T>) -> R<T> {
        let _routing = self.routing.lock().map_err(|_| "灯光输出协调锁定失败")?;
        action()
    }
    pub fn status(&self) -> R<Status> {
        self.state
            .lock()
            .map(|s| s.clone())
            .map_err(|_| "灯光状态锁定失败".into())
    }
    pub fn stop(&self) -> R<Status> {
        let mut worker = self.worker.lock().map_err(|_| "灯光线程锁定失败")?;
        stop_worker(&mut worker)?;
        self.status()
    }
    /// Release an effect session only if that session controls the dock.
    /// A dock-only edit must leave an independent PC RGB session running.
    pub fn controls_dock(&self) -> R<bool> {
        Ok(self.worker.lock().map_err(|_| "灯光线程锁定失败")?.as_ref().is_some_and(|w| w.owns_dock && !w.handle.is_finished()))
    }
    pub fn stop_for_dock(&self) -> R<Status> {
        let mut worker = self.worker.lock().map_err(|_| "灯光线程锁定失败")?;
        if worker.as_ref().is_some_and(|w| w.owns_dock) {
            stop_worker(&mut worker)?;
        }
        self.status()
    }
    pub fn start_selected(&self, config: Config, dock: crate::magdock::Controller, expected: Option<&str>) -> R<Status> {
        config.validate()?;
        self.route(|| {
            if config.dock {dock.check_connection(expected.unwrap_or(""))?; dock.stop_selected(expected.unwrap_or(""))?;}
            self.start(config, dock, expected)
        })
    }
    pub fn start(&self, config: Config, dock: crate::magdock::Controller, expected: Option<&str>) -> R<Status> {
        config.validate()?;
        if config.dock {dock.check_connection(expected.unwrap_or(""))?;}
        let dock_connection = expected.unwrap_or("").to_owned();
        self.stop()?;
        let mut worker = self.worker.lock().map_err(|_| "灯光线程锁定失败")?;
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let state = self.state.clone();
        let owns_dock = config.dock;
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let handle = std::thread::spawn(move || {
            let mut rgb: Option<crate::rgb::Client> = None;
            let mut devices = Vec::new();
            let mut dock_before = None;
            let result = (|| -> R<()> {
                let meter = if config.kind == "music" {
                    Some(crate::audio_meter::Meter::open()?)
                } else {
                    None
                };
                if !config.rgb.is_empty() {
                    let mut c = crate::rgb::Client::connect()?;
                    devices = c.select(&config.rgb)?;
                    let dir = std::path::PathBuf::from(
                        std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA 未设置")?,
                    )
                    .join("Open HUB/backups");
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|e| e.to_string())?
                        .as_nanos();
                    std::fs::write(
                        dir.join(format!("rgb-{now}.json")),
                        serde_json::to_vec_pretty(&devices).map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?;
                    rgb = Some(c);
                }
                if config.dock {
                    dock_before = Some(dock.stream_begin(&dock_connection)?);
                }
                if let Some(c) = rgb.as_mut() {
                    c.begin(&devices)?;
                }
                *state.lock().map_err(|_| "灯光状态锁定失败")? = Status {
                    running: true,
                    kind: config.kind.clone(),
                    config: Some(config.clone()),
                    ..Default::default()
                };
                let _ = tx.send(Ok(()));
                let clock = Instant::now();
                let mut level = 0.0f32;
                let mut last = [[0.0f32; 3]; 12];
                while !signal.load(Ordering::SeqCst) {
                    if let Some(m) = &meter {
                        level = (m.peak()?.sqrt() * config.gain)
                            .clamp(0.0, 1.0)
                            .max(level * 0.78);
                    }
                    let pixels = if config.kind == "screen" {
                        let current = crate::screen::sample()?;
                        let mut out = [[0; 3]; 12];
                        for i in 0..12 {
                            for c in 0..3 {
                                last[i][c] = last[i][c] * 0.55 + current[i][c] as f32 * 0.45;
                                out[i][c] = (last[i][c] * config.brightness).round() as u8;
                            }
                        }
                        out
                    } else {
                        render(&config, clock.elapsed().as_secs_f32(), level)
                    };
                    if let Some(lease) = dock_before.as_ref() {
                        dock.stream_frame(lease, &pixels)?;
                    }
                    if let Some(c) = rgb.as_mut() {
                        c.frame(&devices, &pixels)?;
                    }
                    if let Ok(mut s) = state.lock() {
                        s.level = level;
                        s.pixels = pixels;
                    }
                    std::thread::sleep(Duration::from_millis(if config.kind == "screen" {
                        100
                    } else {
                        50
                    }));
                }
                Ok(())
            })();
            let mut errors = Vec::new();
            if let Err(e) = result {
                let _ = tx.send(Err(e.clone()));
                errors.push(e);
            }
            if let Some(before) = dock_before {
                if let Err(e) = dock.stream_restore(&before) {
                    errors.push(format!("底座恢复：{e}"));
                }
            }
            if let Some(mut c) = rgb {
                if let Err(e) = c.restore(&devices) {
                    errors.push(format!("电脑灯光恢复：{e}"));
                }
            }
            if let Ok(mut s) = state.lock() {
                s.running = false;
                s.level = 0.0;
                s.error = if errors.is_empty() {
                    None
                } else {
                    Some(errors.join("；"))
                };
            }
        });
        *worker = Some(Worker { stop, handle, owns_dock });
        drop(worker);
        match rx.recv_timeout(Duration::from_secs(12)) {
            Ok(Ok(())) => self.status(),
            Ok(Err(e)) => {
                let _ = self.stop();
                Err(e)
            }
            Err(e) => {
                let _ = self.stop();
                Err(format!("灯光启动超时：{e}"))
            }
        }
    }
}

#[cfg(test)]
#[path = "effects_ownership_tests.rs"]
mod ownership_tests;
fn scale(c: [u8; 3], v: f32) -> [u8; 3] {
    c.map(|n| (n as f32 * v.clamp(0.0, 1.0)).round() as u8)
}
fn hsv(h: f32) -> [u8; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let x = (1.0 - (h % 2.0 - 1.0).abs()) * 255.0;
    match h as u8 {
        0 => [255, x as u8, 0],
        1 => [x as u8, 255, 0],
        2 => [0, 255, x as u8],
        3 => [0, x as u8, 255],
        4 => [x as u8, 0, 255],
        _ => [255, 0, x as u8],
    }
}
fn palette(config: &Config, p: f32) -> [u8; 3] {
    let p = p.rem_euclid(1.0) * config.palette.len() as f32;
    let i = p.floor() as usize;
    let f = p.fract();
    std::array::from_fn(|c| {
        (config.palette[i][c] as f32 * (1.0 - f)
            + config.palette[(i + 1) % config.palette.len()][c] as f32 * f)
            .round() as u8
    })
}
pub fn render(c: &Config, time: f32, level: f32) -> [[u8; 3]; 12] {
    let t = time * c.speed;
    std::array::from_fn(|i| {
        let p = i as f32 / 12.0;
        let (col, value) = match c.kind.as_str() {
            "static" => (c.color, 1.0),
            "rainbow" => (hsv(p + t * 0.12), 1.0),
            "gradient" => (palette(c, p + t * 0.12), 1.0),
            "breathe" => (c.color, (0.5 + 0.5 * (t * 2.5).sin()).powf(1.5)),
            "comet" => {
                let distance = (p - t * 0.3).rem_euclid(1.0);
                (palette(c, t * 0.1), (-distance * 9.0).exp())
            }
            "twinkle" => {
                let wave = (t * (2.0 + i as f32 * 0.17) + i as f32 * 2.41)
                    .sin()
                    .max(0.0)
                    .powi(8);
                (palette(c, p + t * 0.04), wave)
            }
            "music" => match c.style {
                0 => {
                    let distance = if i == 11 {
                        6
                    } else {
                        (i as i32 - 5).unsigned_abs() as usize
                    };
                    (
                        c.color,
                        if (level * 7.0).ceil() as usize > distance {
                            1.0
                        } else {
                            0.0
                        },
                    )
                }
                1 => (c.color, level),
                2 => (
                    palette(c, p + t * 0.1),
                    level * (0.5 + 0.5 * (p * 6.28 - t * 4.0).sin()),
                ),
                3 => (hsv(p + t * 0.1), level),
                4 => {
                    let d = (p - t * level.max(0.05) * 0.5).rem_euclid(1.0);
                    (palette(c, t * 0.1), (-d * 10.0).exp() * level)
                }
                _ => (
                    palette(c, t * 0.2),
                    if level > 0.55 { 1.0 } else { level * 0.2 },
                ),
            },
            _ => ([0; 3], 0.0),
        };
        scale(col, value * c.brightness)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config {
            kind: "static".into(),
            style: 0,
            color: [70, 200, 150],
            palette: vec![[255, 0, 0], [0, 0, 255]],
            brightness: 1.0,
            speed: 1.0,
            gain: 2.0,
            dock: true,
            rgb: vec![],
        }
    }
    #[test]
    fn all_effects_and_music_styles() {
        let mut c = config();
        for kind in [
            "static", "rainbow", "gradient", "breathe", "comet", "twinkle",
        ] {
            c.kind = kind.into();
            assert!(c.validate().is_ok());
            assert_ne!(render(&c, 1.0, 1.0), [[0; 3]; 12]);
        }
        c.kind = "music".into();
        for style in 0..6 {
            c.style = style;
            assert_eq!(render(&c, 1.0, 0.0), [[0; 3]; 12]);
            assert_ne!(render(&c, 1.0, 1.0), [[0; 3]; 12]);
        }
    }
    #[test]
    fn target_and_nan_rejected() {
        let mut c = config();
        c.dock = false;
        assert!(c.validate().is_err());
        c.rgb = vec![crate::rgb::Target {
            id: 0,
            key: "test".into(),
        }];
        assert!(c.validate().is_ok());
        c.speed = f32::NAN;
        assert!(c.validate().is_err());
    }
}
pub fn hardware_test() -> R<serde_json::Value> {
    let rgb_server=crate::rgb::Server::default();rgb_server.scan()?;
    let engine = Engine::default();
    let dock = crate::magdock::Controller::default();
    let before_view = dock.view()?;
    let connection = before_view.connection_key;
    let before = before_view.light;
    let rgb_before = crate::rgb::Client::connect()?.devices;
    let ids: Vec<crate::rgb::Target> = rgb_before
        .iter()
        .filter(|d| d.direct && d.leds > 0)
        .map(|d| crate::rgb::Target {
            id: d.id,
            key: d.key.clone(),
        })
        .collect();
    let mut checks = Vec::new();
    let mut config = Config {
        kind: "static".into(),
        style: 0,
        color: [30, 90, 70],
        palette: vec![[70, 230, 190], [100, 120, 250], [240, 100, 180]],
        brightness: 0.35,
        speed: 1.0,
        gain: 2.0,
        dock: true,
        rgb: ids.clone(),
    };
    let result = (|| -> R<()> {
        for kind in [
            "static", "rainbow", "gradient", "breathe", "comet", "twinkle", "screen", "music",
        ] {
            config.kind = kind.into();
            engine.start(config.clone(), dock.clone(), Some(&connection))?;
            std::thread::sleep(Duration::from_millis(650));
            let running = engine.status()?;
            if !running.running {
                return Err(format!("{kind} 提前停止：{:?}", running.error));
            }
            if !dock.view()?.light.sync {
                return Err("底座未进入逐灯控制模式".into());
            }
            if kind == "static" {
                let actual = crate::rgb::Client::connect()?.devices;
                let expected = scale(config.color, config.brightness);
                for id in &ids {
                    let d = actual
                        .iter()
                        .find(|d| d.id == id.id)
                        .ok_or("RGB 测试期间设备断开")?;
                    if d.colors.chunks_exact(4).any(|c| c[..3] != expected) {
                        return Err(format!("{} RGB 颜色读回不一致", d.name));
                    }
                }
            }
            let stopped = engine.stop()?;
            if let Some(e) = stopped.error {
                return Err(e);
            }
            if dock.view()?.light != before {
                return Err("底座恢复核对失败".into());
            }
            let actual = crate::rgb::Client::connect()?.devices;
            for orig in &rgb_before {
                let now = actual
                    .iter()
                    .find(|d| d.id == orig.id)
                    .ok_or("恢复后 RGB 设备缺失")?;
                if now.active != orig.active || now.colors != orig.colors {
                    return Err(format!("{} 原灯效恢复核对失败", orig.name));
                }
            }
            checks.push(format!(
                "{kind}: streamed to MagDock and {} RGB device(s); restored",
                ids.len()
            ));
        }
        config.kind = "music".into();
        for style in 1..6 {
            config.style = style;
            engine.start(config.clone(), dock.clone(), Some(&connection))?;
            std::thread::sleep(Duration::from_millis(250));
            if !engine.status()?.running {
                return Err("音乐风格异常退出".into());
            }
            let stopped = engine.stop()?;
            if let Some(e) = stopped.error {
                return Err(e);
            }
            checks.push(format!("music style {style}: running and restored"));
        }
        if !ids.is_empty() {
            config.dock = false;
            config.kind = "screen".into();
            engine.start(config.clone(), dock.clone(), Some(&connection))?;
            std::thread::sleep(Duration::from_millis(500));
            if dock.view()?.light != before {
                return Err("仅电脑同步意外修改了底座".into());
            }
            engine.stop()?;
            checks.push("PC-only screen sync: MagDock unchanged".into());
        }
        Ok(())
    })();
    let _ = engine.stop();
    if dock.view()?.light != before {
        return Err("底座未恢复到测试前状态；不向重新连接或已改变的底座自动写入".into());
    }
    result?;
    Ok(
        serde_json::json!({"checks":checks,"dock_restored":dock.view()?.light==before,"rgb_devices":rgb_before.iter().map(|d|&d.name).collect::<Vec<_>>()}),
    )
}
