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
    assert_eq!(*requests.lock().unwrap(), [0x0900]);
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
