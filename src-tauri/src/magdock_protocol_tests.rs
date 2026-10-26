// Independent authored source-layout vectors, replayed through the production
// report parser and Session I/O. This does not emulate firmware lighting output.
use super::*;
use std::collections::VecDeque;

// AA / GET7 / response2 / frame0 / total0 / payload30. Sleep fields and firmware
// occupy source offsets; direction is offset29, not the adjacent write offset9.
const REPLY: [u8; 36] = [
    170, 7, 2, 0, 0, 30, 1, 4, 5, 2, 3, 0, 17, 34, 51, 1, 45, 0, 9, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 1,
];
fn before() -> Light {
    Light {
        enabled: true,
        mode: 4,
        mode_count: 5,
        speed: 2,
        brightness: 3,
        sync: false,
        color: [17, 34, 51],
        direction: 1,
    }
}
#[derive(Default)]
struct Wire {
    body: Vec<u8>,
    replies: VecDeque<Vec<u8>>,
    writes: Vec<Vec<u8>>,
    short_write: bool,
    ignore_set: bool,
    disconnect: bool,
    flood: bool,
    reads: usize,
}
struct Memory(Arc<Mutex<Wire>>);
impl DockIo for Memory {
    fn write(&self, bytes: &[u8]) -> R<usize> {
        let mut w = self.0.lock().unwrap();
        w.writes.push(bytes.to_vec());
        if w.disconnect {
            return Err("authored unplug".into());
        }
        if w.short_write {
            return Ok(bytes.len() - 1);
        }
        assert_eq!(bytes.len(), 65);
        assert_eq!(&bytes[..2], &[0, 170]);
        match bytes[2] {
            7 => {
                let mut reply = vec![170, 7, 2, 0, 0, 30];
                reply.extend_from_slice(&w.body);
                w.replies.push_back(reply);
            }
            39 => {
                assert_eq!(bytes[6], 10);
                if !w.ignore_set {
                    w.body[..9].copy_from_slice(&bytes[7..16]);
                    w.body[29] = bytes[16];
                }
            }
            41 => assert_eq!(bytes[6], 36),
            _ => panic!("unexpected native dock command"),
        }
        Ok(bytes.len())
    }
    fn read_timeout(&self, bytes: &mut [u8], _timeout: i32) -> R<usize> {
        let mut w = self.0.lock().unwrap();
        w.reads += 1;
        if w.disconnect {
            return Err("authored unplug".into());
        }
        if w.flood {
            bytes[0] = 99;
            return Ok(1);
        }
        if let Some(reply) = w.replies.pop_front() {
            bytes[..reply.len()].copy_from_slice(&reply);
            Ok(reply.len())
        } else {
            Ok(0)
        }
    }
    fn settle(&self, _duration: Duration) {}
}
fn session(key: &str) -> (Session, Arc<Mutex<Wire>>) {
    let wire = Arc::new(Mutex::new(Wire {
        body: REPLY[6..].to_vec(),
        ..Default::default()
    }));
    (
        Session {
            io: Box::new(Memory(wire.clone())),
            connection: key.into(),
            backup_dir: Some(
                std::env::temp_dir()
                    .join(format!("open-hub-magdock-{}", crate::profiles::new_id())),
            ),
        },
        wire,
    )
}
fn controller(key: &str) -> (Controller, Arc<Mutex<Wire>>) {
    let (session, wire) = session(key);
    let controller = Controller::default();
    *controller.inner.session.lock().unwrap() = Some(session);
    (controller, wire)
}
fn lease(key: &str) -> StreamLease {
    let old = before();
    let mut applied = old.clone();
    applied.enabled = true;
    applied.mode = 5;
    applied.sync = true;
    StreamLease {
        connection: key.into(),
        before: old,
        applied,
    }
}
fn set_body(wire: &Arc<Mutex<Wire>>, light: &Light) {
    let mut w = wire.lock().unwrap();
    let args = light.args();
    w.body[..9].copy_from_slice(&args[..9]);
    w.body[29] = args[9];
}
fn commands(wire: &Arc<Mutex<Wire>>) -> Vec<u8> {
    wire.lock().unwrap().writes.iter().map(|b| b[2]).collect()
}
#[test]
fn source_response_fields_preserve_sync_mode_and_five_mode_count() {
    let (light, firmware) = parse_reply(&REPLY).unwrap().unwrap();
    assert_eq!(light, before());
    assert_eq!(firmware, "2.9");
    let mut with_id = vec![0];
    with_id.extend_from_slice(&REPLY);
    with_id.resize(65, 0);
    assert_eq!(parse_reply(&with_id).unwrap().unwrap().0, before());
    // Source stores mode and lightSyncStatus independently; reading sync must not
    // rewrite the underlying mode before a later stream restore.
    let mut sync = REPLY;
    sync[7] = 0;
    sync[11] = 1;
    let got = parse_reply(&sync).unwrap().unwrap().0;
    assert_eq!(got.mode, 0);
    assert!(got.sync);
}
#[test]
fn truncated_declared_fragmented_or_unknown_responses_are_not_accepted() {
    for n in 2..REPLY.len() {
        assert!(parse_reply(&REPLY[..n]).is_err(), "length {n}");
    }
    for (at, value) in [
        (2, 0),
        (3, 1),
        (4, 1),
        (5, 29),
        (5, 31),
        (5, 59),
        (6, 2),
        (7, 6),
        (8, 0),
        (9, 5),
        (10, 5),
        (11, 2),
        (35, 2),
    ] {
        let mut raw = REPLY;
        raw[at] = value;
        assert!(parse_reply(&raw).is_err(), "offset {at}: {value}");
    }
    assert!(parse_reply(&[]).unwrap().is_none());
    assert!(parse_reply(&[0]).unwrap().is_none());
    let mut other = REPLY;
    other[1] = 43;
    assert!(parse_reply(&other).unwrap().is_none());
}
#[test]
fn six_source_modes_set_and_read_back_with_exact_ten_field_order() {
    let (session, wire) = session("dock-A:1");
    for mode in 0..=5 {
        let mut light = before();
        light.mode = mode;
        light.sync = mode == 5;
        light.enabled = mode != 0;
        light.color = [40 + mode, 180, 210];
        light.brightness = mode % 5;
        light.speed = 4 - mode % 5;
        light.direction = mode % 2;
        assert_eq!(session.apply(&light).unwrap().0, light);
        let w = wire.lock().unwrap();
        let raw = &w.writes[w.writes.len() - 2];
        let expected = [
            0,
            170,
            39,
            0,
            0,
            0,
            10,
            u8::from(mode != 0),
            mode,
            5,
            4 - mode % 5,
            mode % 5,
            u8::from(mode == 5),
            40 + mode,
            180,
            210,
            mode % 2,
        ];
        assert_eq!(&raw[..17], &expected);
        assert!(raw[17..].iter().all(|b| *b == 0));
        assert_eq!(w.body[9], 1);
        assert_eq!(w.body[10], 45);
        assert_eq!(w.body[12..14], [9, 2]);
    }
    assert_eq!(commands(&wire), [39, 7].repeat(6));
}
#[test]
fn twelve_rgb_pixels_stream_in_order_on_the_original_connection() {
    let (controller, wire) = controller("dock-A:1");
    let lease = lease("dock-A:1");
    let pixels = std::array::from_fn(|i| [i as u8, 100 + i as u8, 200 + i as u8]);
    controller.stream_frame(&lease, &pixels).unwrap();
    let w = wire.lock().unwrap();
    let raw = &w.writes[0];
    assert_eq!(&raw[..7], &[0, 170, 41, 0, 0, 0, 36]);
    for i in 0..12 {
        assert_eq!(
            &raw[7 + i * 3..10 + i * 3],
            &[i as u8, 100 + i as u8, 200 + i as u8]
        );
    }
    assert!(raw[43..].iter().all(|b| *b == 0));
}
#[test]
fn short_set_write_and_unconfirmed_readback_do_not_issue_restore_writes() {
    let (session, wire) = session("dock-A:1");
    wire.lock().unwrap().short_write = true;
    assert!(session.apply(&before()).is_err());
    assert_eq!(commands(&wire), [39]);
    let (session, wire) = self::session("dock-A:1");
    wire.lock().unwrap().ignore_set = true;
    let mut change = before();
    change.mode = 1;
    assert!(session.apply(&change).is_err());
    assert_eq!(commands(&wire), [39, 7]);
}
#[test]
fn stale_frames_and_restores_do_not_touch_or_invalidate_a_new_connection() {
    let (controller, wire) = controller("dock-B:2");
    let old = lease("dock-A:1");
    assert!(controller.stream_frame(&old, &[[1, 2, 3]; 12]).is_err());
    assert!(controller.stream_restore(&old).is_err());
    assert!(commands(&wire).is_empty());
    controller
        .stream_frame(&lease("dock-B:2"), &[[3, 4, 5]; 12])
        .unwrap();
    assert_eq!(commands(&wire), [41]);
    *controller.inner.session.lock().unwrap() = None;
    assert!(controller.stream_frame(&old, &[[0; 3]; 12]).is_err());
    assert!(controller.stream_restore(&old).is_err());
    assert!(controller.inner.session.lock().unwrap().is_none());
}
#[test]
fn failed_frame_drops_connection_and_cleanup_cannot_reopen_or_write_another_dock() {
    let (controller, a) = controller("dock-A:1");
    let old = lease("dock-A:1");
    a.lock().unwrap().disconnect = true;
    assert!(controller.stream_frame(&old, &[[1, 2, 3]; 12]).is_err());
    assert!(controller.inner.session.lock().unwrap().is_none());
    assert!(controller.stream_restore(&old).is_err());
    let (replacement, b) = session("dock-B:2");
    *controller.inner.session.lock().unwrap() = Some(replacement);
    assert!(controller.stream_restore(&old).is_err());
    assert!(commands(&b).is_empty());
    assert_eq!(commands(&a), [41]);
}
#[test]
fn changed_external_settings_are_not_overwritten_during_stream_restore() {
    let (controller, wire) = controller("dock-A:1");
    let lease = lease("dock-A:1");
    let mut external = lease.applied.clone();
    external.color = [4, 5, 6];
    set_body(&wire, &external);
    assert!(controller.stream_restore(&lease).is_err());
    assert_eq!(commands(&wire), [7]);
    assert_eq!(&wire.lock().unwrap().body[6..9], &[4, 5, 6]);
}
#[test]
fn stream_restore_preserves_raw_sync_base_mode_and_already_restored_is_read_only() {
    let (controller, wire) = controller("dock-A:1");
    let mut lease = lease("dock-A:1");
    lease.before.mode = 0;
    lease.before.sync = true;
    set_body(&wire, &lease.applied);
    controller.stream_restore(&lease).unwrap();
    assert_eq!(commands(&wire), [7, 39, 7]);
    assert_eq!(wire.lock().unwrap().body[1], 0);
    assert_eq!(wire.lock().unwrap().body[5], 1);
    assert_eq!(wire.lock().unwrap().body[2], 5);
    controller.stream_restore(&lease).unwrap();
    assert_eq!(commands(&wire), [7, 39, 7, 7]);
}
#[test]
fn stale_report_drain_is_bounded_before_a_get_is_sent() {
    let (session, wire) = session("dock-A:1");
    wire.lock().unwrap().flood = true;
    assert!(session.read().is_err());
    let w = wire.lock().unwrap();
    assert_eq!(w.reads, 32);
    assert!(w.writes.is_empty());
}

fn backup_directory(controller: &Controller) -> std::path::PathBuf {
    controller
        .inner
        .session
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .backup_dir
        .clone()
        .unwrap()
}
#[test]
fn production_stream_begin_backs_up_before_set_and_normal_restore_uses_its_lease() {
    let (controller, wire) = controller("dock-A:1");
    let dir = backup_directory(&controller);
    let lease = controller.stream_begin().unwrap();
    assert_eq!(lease.connection, "dock-A:1");
    assert_eq!(lease.before, before());
    let files = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    let saved: Light = serde_json::from_slice(&std::fs::read(&files[0]).unwrap()).unwrap();
    assert_eq!(saved, before());
    assert_eq!(commands(&wire), [7, 7, 39, 7]);
    controller
        .stream_frame(&lease, &[[10, 20, 30]; 12])
        .unwrap();
    controller.stream_restore(&lease).unwrap();
    assert_eq!(commands(&wire), [7, 7, 39, 7, 41, 7, 39, 7]);
    assert_eq!(wire.lock().unwrap().body, REPLY[6..]);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn failed_stream_backup_prevents_all_setting_and_color_writes() {
    let (controller, wire) = controller("dock-A:1");
    let dir = backup_directory(&controller);
    std::fs::write(&dir, b"authored backup path conflict").unwrap();
    assert!(controller.stream_begin().is_err());
    assert_eq!(commands(&wire), [7]);
    std::fs::remove_file(dir).unwrap();
}
#[test]
fn controller_unconfirmed_apply_keeps_backup_and_never_issues_a_blind_restore() {
    let (controller, wire) = controller("dock-A:1");
    let dir = backup_directory(&controller);
    wire.lock().unwrap().ignore_set = true;
    let mut change = before();
    change.color = [1, 2, 3];
    let error = controller.apply(change).err().unwrap();
    assert!(error.contains("已保留写入前备份"));
    assert_eq!(commands(&wire), [7, 39, 7]);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    assert!(controller.inner.session.lock().unwrap().is_none());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn actual_static_effect_worker_sends_frames_and_restores_through_its_native_lease() {
    let (controller, wire) = controller("dock-A:1");
    let dir = backup_directory(&controller);
    let engine = crate::effects::Engine::default();
    let config = crate::effects::Config {
        kind: "static".into(),
        style: 0,
        color: [30, 90, 70],
        palette: vec![[1, 2, 3], [4, 5, 6]],
        brightness: 1.0,
        speed: 1.0,
        gain: 2.0,
        dock: true,
        rgb: vec![],
    };
    assert!(engine.start(config, controller).unwrap().running);
    let until = Instant::now() + Duration::from_secs(1);
    while !commands(&wire).contains(&41) {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(2));
    }
    let stopped = engine.stop().unwrap();
    assert!(!stopped.running);
    assert!(stopped.error.is_none(), "{:?}", stopped.error);
    let w = wire.lock().unwrap();
    assert_eq!(w.body, REPLY[6..]);
    let frames = w.writes.iter().filter(|b| b[2] == 41).collect::<Vec<_>>();
    assert!(!frames.is_empty());
    for frame in frames {
        assert_eq!(&frame[7..43], &[30, 90, 70].repeat(12));
    }
    assert_eq!(w.writes.iter().filter(|b| b[2] == 39).count(), 2);
    drop(w);
    std::fs::remove_dir_all(dir).unwrap();
}
