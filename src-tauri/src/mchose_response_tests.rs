//! Captured receiver rejection, normal integrity, and per-transport routing.
use super::*;

fn captured_rejection() -> [u8; 64] {
    let mut report = [0; 64];
    report[..9].copy_from_slice(&[0x4d, 1, 1, 1, 0, 9, 0, 0, 0xff]);
    report
}
fn pending(command: u16) -> Shared {
    Shared {
        pending: Mutex::new(Pending {
            command: Some(command),
            reply: None,
        }),
        ready: Condvar::new(),
        alive: AtomicBool::new(true),
    }
}

#[test]
fn captured_enumeration_selects_only_the_settings_collection() {
    // Current six-interface metadata capture, with paths and serials omitted.
    let rows = [
        (0x1012, 0xff00, 1, "MCHOSE MagDock"),
        (0x1014, 0xff0b, 196, "MCHOSE A7 V3 Ultra+"),
        (0x1014, 0xff01, 1, "MCHOSE A7 V3 Ultra+"),
        (0x1014, 12, 1, "MCHOSE A7 V3 Ultra+"),
        (0x1014, 1, 6, "MCHOSE A7 V3 Ultra+"),
        (0x1014, 1, 2, "MCHOSE A7 V3 Ultra+"),
    ];
    let selected = rows
        .iter()
        .enumerate()
        .filter_map(|(i, (pid, page, usage, name))| {
            known_interface(0x3837, *pid, *page, *usage, Some(name)).then_some(i)
        })
        .collect::<Vec<_>>();
    assert_eq!(selected, [2]);
    assert!(!known_interface(
        0x046d,
        0x1014,
        0xff01,
        1,
        Some("MCHOSE A7 V3 Ultra+")
    ));
    assert!(!known_interface(
        0x3837,
        0x1014,
        0xff01,
        1,
        Some("MCHOSE A5 V3 Ultra+")
    ));
}

#[test]
fn exact_captured_identity_rejection_completes_only_its_pending_transport_as_error() {
    let raw = captured_rejection();
    assert!(decode(&raw).is_err());
    let (command, reply) = response(&raw).unwrap();
    assert_eq!(command, 0x0900);
    assert!(reply.is_err());
    let a = pending(0x0900);
    let b = pending(0x0900);
    assert!(complete_reply(&a, command, reply));
    let mut completed = a.pending.lock().unwrap();
    assert!(completed.command.is_none());
    assert!(completed
        .reply
        .take()
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("0xFF"));
    drop(completed);
    let other = b.pending.lock().unwrap();
    assert_eq!(other.command, Some(0x0900));
    assert!(other.reply.is_none());
    let wrong = pending(3);
    let (command, reply) = response(&raw).unwrap();
    assert!(!complete_reply(&wrong, command, reply));
    assert_eq!(wrong.pending.lock().unwrap().command, Some(3));
}

#[test]
fn omission_is_not_a_success_payload_or_a_relaxed_write_checksum_rule() {
    for command in [0x0101u16, 0x0102, 0x0103, 0x0109, 0x090b, 0x0906, 0x0003] {
        let mut raw = captured_rejection();
        raw[4..6].copy_from_slice(&command.to_le_bytes());
        assert!(response(&raw).is_err());
    }
    let mut raw = captured_rejection();
    raw[63] = 1;
    assert!(response(&raw).is_err());
    raw = captured_rejection();
    raw[8] = 0xfe;
    assert!(response(&raw).is_err());
    assert!(response(&captured_rejection()[..10]).is_err());
    // A checksum-valid one-byte result still cannot satisfy model identity.
    let requests = Arc::new(Mutex::new(vec![]));
    let seen = requests.clone();
    let directory =
        std::env::temp_dir().join(format!("open-hub-status-{}", crate::profiles::new_id()));
    let transport = Transport::memory(
        move |command, _| {
            seen.lock().unwrap().push(command);
            Ok(vec![0xff])
        },
        directory.clone(),
    );
    assert!(transport
        .apply_hardware_for(0, crate::mchose_config::Action::Lod { index: 2 })
        .is_err());
    assert_eq!(
        *requests.lock().unwrap(),
        [0x0900; IDENTITY_GROUPS * IDENTITY_ATTEMPTS_PER_GROUP]
    );
    assert!(!directory.exists());
}

#[test]
fn valid_identity_and_corruption_keep_original_integrity_and_pending_rules() {
    let raw = [
        0x4d, 1, 1, 0x10, 0, 9, 0, 0, 0x37, 0x38, 0x26, 0x40, 4, 0, 0, 0, 0, 0x10, 2, 1, 100, 0, 8,
        0xe4, 0xee,
    ];
    let (command, reply) = response(&raw).unwrap();
    let payload = reply.unwrap();
    assert_eq!(payload[12], 100);
    let state = pending(0x0900);
    assert!(complete_reply(&state, command, Ok(payload.clone())));
    assert_eq!(
        state.pending.lock().unwrap().reply.take().unwrap().unwrap(),
        payload
    );
    let mut corrupt = raw;
    corrupt[20] ^= 1;
    assert!(response(&corrupt).is_err());
    let waiting = pending(0x0900);
    if let Ok((command, reply)) = response(&corrupt) {
        complete_reply(&waiting, command, reply);
    }
    assert_eq!(waiting.pending.lock().unwrap().command, Some(0x0900));
    assert!(waiting.pending.lock().unwrap().reply.is_none());
}

fn accepted_identity() -> Vec<u8> {
    vec![
        0x37, 0x38, 0x26, 0x40, 4, 0, 0, 0, 0, 0x10, 2, 1, 100, 0, 8, 0xe4,
    ]
}
#[test]
fn unavailable_identity_retry_matches_source_bounds_and_delay_without_any_set() {
    let mut calls = 0;
    let mut waits = vec![];
    let result = identity_with_retry(
        || {
            calls += 1;
            Err(Error::MchoseIdentityUnavailable)
        },
        |duration| waits.push(duration),
    );
    assert!(matches!(result, Err(Error::MchoseIdentityUnavailable)));
    assert_eq!(calls, 20);
    assert_eq!(waits, vec![Duration::from_millis(30); 16]);
    let directory = std::env::temp_dir().join(format!(
        "open-hub-identity-retry-{}",
        crate::profiles::new_id()
    ));
    let seen = Arc::new(Mutex::new(vec![]));
    let capture = seen.clone();
    let transport = Transport::memory(
        move |cmd, args| {
            assert!(args.is_empty());
            capture.lock().unwrap().push(cmd);
            Ok(vec![0xff])
        },
        directory.clone(),
    );
    assert!(transport
        .apply_hardware_for(0, crate::mchose_config::Action::Lod { index: 2 })
        .is_err());
    assert_eq!(*seen.lock().unwrap(), vec![0x900; 20]);
    assert!(!directory.exists());
}
#[test]
fn retry_stops_on_first_valid_identity_even_at_last_permitted_attempt() {
    for success_at in [1, 2, 5, 6, 19, 20] {
        let directory = std::env::temp_dir().join(format!(
            "open-hub-identity-ready-{}",
            crate::profiles::new_id()
        ));
        let seen = Arc::new(Mutex::new(0));
        let capture = seen.clone();
        let transport = Transport::memory(
            move |cmd, _| {
                assert_eq!(cmd, 0x900);
                let mut count = capture.lock().unwrap();
                *count += 1;
                if *count < success_at {
                    Ok(vec![0xff])
                } else {
                    Ok(accepted_identity())
                }
            },
            directory.clone(),
        );
        assert_eq!(transport.identity().unwrap(), accepted_identity());
        assert_eq!(*seen.lock().unwrap(), success_at);
        assert!(!directory.exists());
    }
}
#[test]
fn corrupt_timeout_disconnect_and_other_failures_do_not_repeat_identity() {
    for case in 0..4 {
        let mut calls = 0;
        let mut waits = 0;
        let result = identity_with_retry(
            || {
                calls += 1;
                Err(match case {
                    0 => Error::Timeout,
                    1 => Error::Disconnected,
                    2 => invalid("checksum mismatch"),
                    _ => invalid("unverified model"),
                })
            },
            |_| waits += 1,
        );
        assert!(result.is_err());
        assert_eq!(calls, 1);
        assert_eq!(waits, 0);
    }
    let raw = frame(0x900, &[0xff]).unwrap();
    assert!(matches!(
        response(&raw).unwrap().1,
        Err(Error::MchoseIdentityUnavailable)
    ));
    let mut corrupt = raw;
    corrupt[9] ^= 1;
    assert!(response(&corrupt).is_err());
}
#[test]
fn unavailable_retry_never_loosens_model_layout_status_or_zero_battery_validation() {
    for case in 0..5 {
        let mut identity = accepted_identity();
        match case {
            0 => identity[2] = 0x33,
            1 => identity[0] = 0x6d,
            2 => {
                identity.truncate(4);
            }
            3 => identity[10] = 0,
            _ => identity[12] = 0,
        };
        let calls = Arc::new(Mutex::new(0));
        let seen = calls.clone();
        let t = Transport::memory(
            move |_, _| {
                *seen.lock().unwrap() += 1;
                Ok(identity.clone())
            },
            std::env::temp_dir().join(format!(
                "open-hub-identity-invalid-{}",
                crate::profiles::new_id()
            )),
        );
        assert_eq!(t.identity().is_ok(), case == 4);
        assert_eq!(*calls.lock().unwrap(), 1);
    }
}
#[test]
fn partial_output_closes_only_its_transport_and_cannot_receive_late_reply_or_retry() {
    let events: EventSink = Arc::new(|_| {});
    for n in [0, 1, 63, 65] {
        let shared = pending(0x102);
        assert!(finish_output(&shared, Ok(n), 64, &events).is_err());
        assert!(!shared.alive.load(Ordering::SeqCst));
        assert!(shared.pending.lock().unwrap().command.is_none());
        assert!(shared.pending.lock().unwrap().reply.is_none());
        assert!(!complete_reply(&shared, 0x102, Ok(vec![])));
    }
    let shared = pending(0x102);
    finish_output(&shared, Ok(64), 64, &events).unwrap();
    assert!(shared.alive.load(Ordering::SeqCst));
    assert_eq!(shared.pending.lock().unwrap().command, Some(0x102));
    let count = Arc::new(Mutex::new(0));
    let seen = count.clone();
    let t = Transport::memory(
        move |_, _| {
            *seen.lock().unwrap() += 1;
            Ok(vec![])
        },
        std::env::temp_dir().join(format!(
            "open-hub-short-write-{}",
            crate::profiles::new_id()
        )),
    );
    finish_output(&t.shared, Ok(63), 64, &events).unwrap_err();
    assert!(matches!(t.request(0x102, &[]), Err(Error::Disconnected)));
    assert_eq!(*count.lock().unwrap(), 0);
    let other = Transport::memory(
        |_, _| Ok(accepted_identity()),
        std::env::temp_dir().join(format!(
            "open-hub-other-identity-{}",
            crate::profiles::new_id()
        )),
    );
    assert_eq!(other.identity().unwrap(), accepted_identity());
}

#[test]
fn failed_output_signals_one_rescan_event_and_discards_any_pending_reply() {
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = count.clone();
    let events: EventSink = Arc::new(move |event| {
        assert!(matches!(event, Event::Gone));
        seen.fetch_add(1, Ordering::SeqCst);
    });
    let shared = pending(0x900);
    shared.pending.lock().unwrap().reply = Some(Ok(accepted_identity()));
    assert!(finish_output(&shared, Ok(63), 64, &events).is_err());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(shared.pending.lock().unwrap().reply.is_none());
    assert!(finish_output(&shared, Ok(0), 64, &events).is_err());
    assert_eq!(count.load(Ordering::SeqCst), 1);
}
