//! Real worker cancellation/join routing without opening RGB, audio or HID.
use super::*;
use std::sync::mpsc::{self, Receiver};

fn running_worker(owns_dock: bool, published_dock: bool) -> (Engine, Receiver<()>) {
    let engine = Engine::default();
    let stop = Arc::new(AtomicBool::new(false));
    let signal = stop.clone();
    let state = engine.state.clone();
    let (stopped_tx, stopped_rx) = mpsc::channel();
    // Intentionally allow stale published config, as occurs while Engine.start
    // is initializing a new worker before it publishes its running status.
    *engine.state.lock().unwrap() = Status {
        running: true,
        kind: "static".into(),
        config: Some(Config {
            kind: "static".into(),
            style: 0,
            color: [17, 34, 51],
            palette: vec![[255, 0, 0], [0, 0, 255]],
            brightness: 0.7,
            speed: 1.0,
            gain: 2.0,
            dock: published_dock,
            rgb: vec![crate::rgb::Target {
                id: 2,
                key: "simulation:independent-pc".into(),
            }],
        }),
        ..Default::default()
    };
    let handle = std::thread::spawn(move || {
        while !signal.load(Ordering::SeqCst) {
            std::thread::park_timeout(Duration::from_millis(1));
        }
        state.lock().unwrap().running = false;
        stopped_tx.send(()).unwrap();
    });
    *engine.worker.lock().unwrap() = Some(Worker {
        stop,
        handle,
        owns_dock,
    });
    (engine, stopped_rx)
}

#[test]
fn dock_edits_preserve_independent_pc_worker_even_with_stale_dock_status() {
    for published_dock in [false, true] {
        let (engine, stopped) = running_worker(false, published_dock);
        for _ in 0..2 {
            // Same ownership route used by both onboard apply and reset.
            let status = engine.stop_for_dock().unwrap();
            assert!(status.running);
            assert_eq!(
                status.config.unwrap().rgb[0].key,
                "simulation:independent-pc"
            );
            assert!(matches!(stopped.try_recv(), Err(mpsc::TryRecvError::Empty)));
            let worker = engine.worker.lock().unwrap();
            assert!(!worker.as_ref().unwrap().stop.load(Ordering::SeqCst));
        }
        // The PC worker remains under the ordinary explicit stop lifecycle.
        assert!(!engine.stop().unwrap().running);
        stopped.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(engine.worker.lock().unwrap().is_none());
    }
}

#[test]
fn dock_edits_join_shared_dock_pc_worker_before_releasing_ownership() {
    for published_dock in [false, true] {
        let (engine, stopped) = running_worker(true, published_dock);
        assert!(!engine.stop_for_dock().unwrap().running);
        stopped.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(engine.worker.lock().unwrap().is_none());
        // Repeating the route after restoration completes is harmless.
        assert!(!engine.stop_for_dock().unwrap().running);
    }
}

#[test]
fn competing_effect_handoff_cannot_enter_between_dock_stop_and_mutation() {
    for owns_dock in [false, true] {
        let (engine, stopped) = running_worker(owns_dock, !owns_dock);
        let engine = Arc::new(engine);
        let mutated = Arc::new(AtomicBool::new(false));
        let (holding_tx, holding_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (competing_tx, competing_rx) = mpsc::channel();
        let (events_tx, events_rx) = mpsc::channel();
        let first_engine = engine.clone();
        let first_mutated = mutated.clone();
        let first_events = events_tx.clone();
        let first = std::thread::spawn(move || {
            first_engine.route(|| {
                first_engine.stop_for_dock()?;
                // The independent PC worker must remain live for this whole dock
                // mutation; a shared dock worker must already be joined/restored.
                assert_eq!(first_engine.status()?.running, !owns_dock);
                holding_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                first_mutated.store(true, Ordering::Release);
                first_events.send("dock mutation completed").unwrap();
                Ok(())
            })
        });
        holding_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let next_engine = engine.clone();
        let next = std::thread::spawn(move || {
            // Probe while the first command is deliberately paused after its
            // stop. This is deterministic, without relying on sleeps or thread
            // scheduling to infer that a competing command has been blocked.
            let blocked = matches!(
                next_engine.routing.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            );
            competing_tx.send(blocked).unwrap();
            next_engine.route(|| {
                assert!(mutated.load(Ordering::Acquire));
                events_tx.send("competing effect handoff entered").unwrap();
                next_engine.stop()?;
                Ok(())
            })
        });
        assert!(competing_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        assert!(matches!(
            events_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        release_tx.send(()).unwrap();
        first.join().unwrap().unwrap();
        next.join().unwrap().unwrap();
        assert_eq!(events_rx.recv().unwrap(), "dock mutation completed");
        assert_eq!(
            events_rx.recv().unwrap(),
            "competing effect handoff entered"
        );
        stopped.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(!engine.status().unwrap().running);
    }
}
