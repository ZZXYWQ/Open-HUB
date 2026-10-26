use super::*;
fn mouse(id: &str, key: &str, vendor: u16, model: u16) -> Endpoint {
    Endpoint {
        id: id.into(),
        name: id.into(),
        key: key.into(),
        connection_key:None,
        observed_key: key.into(),
        vendor,
        models: vec![model],
        firmware: "fixture".into(),
        route: Route::Native,
    }
}
fn template() -> Profile {
    Profile::desktop()
}
fn assign(store: &mut ProfileStore, e: &Endpoint, p: &Profile) -> Profile {
    let copy = store.private_copy(e, p).unwrap();
    store.select_for(e, &copy.id).unwrap();
    copy
}
#[test]
fn two_native_logitech_mice_require_one_target_before_any_write() {
    let store = ProfileStore::default();
    let peers = vec![
        mouse("usb:A", "unit:046d:01020304", 0x046d, 0xc08b),
        mouse("usb:B", "unit:046d:01020305", 0x046d, 0xc08b),
    ];
    let mut writes = Vec::new();
    if let Ok(e) = choose(&store, &peers, &template(), None) {
        writes.push(e.id.clone());
    }
    assert!(writes.is_empty());
    let e = choose(&store, &peers, &template(), Some("usb:B")).unwrap();
    writes.push(e.id.clone());
    assert_eq!(writes, vec!["usb:B"]);
    assert!(choose(&store, &peers, &template(), Some("unplugged")).is_err());
    assert!(store.profile_owners.is_empty());
}
#[test]
fn two_mchose_boards_do_not_broadcast_or_cross_logitech() {
    let store = ProfileStore::default();
    let peers = vec![
        mouse("m:A", "session:A", 0x3837, 0x4026),
        mouse("m:B", "session:B", 0x3837, 0x4026),
        mouse("logi", "unit:046d:a", 0x046d, 0xc08b),
    ];
    let mut p = template();
    p.mchose_board = Some(2);
    assert!(choose(&store, &peers, &p, None).is_err());
    assert_eq!(choose(&store, &peers, &p, Some("m:A")).unwrap().id, "m:A");
    assert!(choose(&store, &peers, &p, Some("logi")).is_err());
}
#[test]
fn private_copies_and_dpi_edits_are_independent_and_import_cannot_replace_them() {
    let mut store = ProfileStore::default();
    let a = mouse("a", "unit:046d:a", 0x046d, 0xc08b);
    let b = mouse("b", "unit:046d:b", 0x046d, 0xc08b);
    let mut source = template();
    source.ghub_id = Some("ghub:desktop".into());
    let ca = assign(&mut store, &a, &source);
    let cb = assign(&mut store, &b, &source);
    assert_ne!(ca.id, cb.id);
    assert_eq!(store.get("desktop").unwrap().active().unwrap().x, 1600);
    assert!(ca.ghub_id.is_none());
    assert!(cb.ghub_id.is_none());
    let mut edited = ca.clone();
    edited.levels[2].x = 2400;
    store.save_for(&a, &edited).unwrap();
    assert_eq!(store.active_for(&a).unwrap().active().unwrap().x, 2400);
    assert_eq!(store.active_for(&b).unwrap().active().unwrap().x, 1600);
    source.levels[2].x = 5000;
    store.upsert(source.clone());
    assert_eq!(
        store.private_copy(&a, &source).unwrap().active().unwrap().x,
        2400
    );
    assert_eq!(store.active_for(&b).unwrap().active().unwrap().x, 1600);
    assert_eq!(store.profile_owners.len(), 2);
}
#[test]
fn owned_profile_cannot_be_applied_or_saved_for_another_unit() {
    let mut s = ProfileStore::default();
    let a = mouse("a", "unit:046d:a", 0x046d, 0xc08b);
    let b = mouse("b", "unit:046d:b", 0x046d, 0xc08b);
    let p = assign(&mut s, &a, &template());
    assert!(choose(&s, &[b.clone()], &p, Some("b")).is_err());
    assert!(s.private_copy(&b, &p).is_err());
    assert!(s.save_for(&b, &p).is_err());
    assert!(s.active_for(&b).is_none());
}
#[test]
fn per_mouse_auto_switch_uses_only_its_own_profiles_and_default() {
    let mut s = ProfileStore::default();
    let a = mouse("a", "unit:046d:a", 0x046d, 0xc08b);
    let b = mouse("b", "unit:046d:b", 0x046d, 0xc08b);
    let ca = assign(&mut s, &a, &template());
    let cb = assign(&mut s, &b, &template());
    let mut game = template();
    game.id = "game".into();
    game.name = "game".into();
    game.app_path = Some(r"C:\Games\One".into());
    let ga = assign(&mut s, &a, &game);
    assert_eq!(s.match_for(&a, r"c:/games/one/run.exe").unwrap().id, ga.id);
    assert_eq!(s.match_for(&b, r"C:\Games\One\run.exe").unwrap().id, cb.id);
    assert_eq!(s.match_for(&a, r"C:\Other\run.exe").unwrap().id, ca.id);
    assert_eq!(
        s.match_for(&a, r"C:\Games\OneMore\run.exe").unwrap().id,
        ca.id
    );
    let mut specific = game.clone();
    specific.id = "specific".into();
    specific.app_path = Some(r"C:\Games\One\run.exe".into());
    let p = assign(&mut s, &a, &specific);
    assert_eq!(s.match_for(&a, r"C:\Games\One\run.exe").unwrap().id, p.id);
}
#[test]
fn no_default_never_falls_back_to_global_desktop() {
    let mut s = ProfileStore::default();
    let e = mouse("a", "unit:046d:a", 0x046d, 0xc08b);
    let mut p = template();
    p.id = "game".into();
    p.app_path = Some("C:/game.exe".into());
    assign(&mut s, &e, &p);
    assert!(s.match_for(&e, "C:/other.exe").is_none());
}
#[test]
fn serial_less_connections_are_separate_and_never_automatic() {
    let mut sessions = Sessions::default();
    let mut peers = vec![
        mouse("same-port-a", "", 0x3837, 0x4026),
        mouse("same-port-b", "", 0x3837, 0x4026),
    ];
    sessions.resolve(&mut peers);
    let original = peers[0].key.clone();
    assert_ne!(peers[0].key, peers[1].key);
    assert!(!peers[0].can_auto_apply());
    let mut again = vec![mouse("same-port-a", "", 0x3837, 0x4026)];
    sessions.resolve(&mut again);
    assert_eq!(again[0].key, original);
    sessions.resolve(&mut []);
    let mut replacement = vec![mouse("same-port-a", "", 0x3837, 0x4026)];
    sessions.resolve(&mut replacement);
    assert_ne!(replacement[0].key, original);
}
#[test]
fn duplicate_unit_ids_are_quarantined_even_after_one_disconnects() {
    let mut sessions = Sessions::default();
    let mut peers = vec![
        mouse("a", "unit:046d:same", 0x046d, 0xc08b),
        mouse("b", "unit:046d:same", 0x046d, 0xc08b),
    ];
    sessions.resolve(&mut peers);
    assert_ne!(peers[0].key, peers[1].key);
    assert!(peers.iter().all(|p| !p.can_auto_apply()));
    let b = peers[1].key.clone();
    let mut remaining = vec![mouse("b", "unit:046d:same", 0x046d, 0xc08b)];
    sessions.resolve(&mut remaining);
    assert_eq!(remaining[0].key, b);
    assert!(!remaining[0].can_auto_apply());
}
#[test]
fn unit_id_requires_observed_long_payload_and_not_factory_blank() {
    assert!(logitech_unit(&[0, 1, 2]).is_none());
    assert!(logitech_unit(&[0; 16]).is_none());
    assert!(logitech_unit(&[255; 16]).is_none());
    let mut b = [0; 16];
    b[1..5].copy_from_slice(&[1, 2, 3, 4]);
    assert_eq!(logitech_unit(&b).as_deref(), Some("unit:046d:01020304"));
}
#[test]
fn path_changes_with_same_unit_keep_owned_copy_but_repaired_unit_does_not() {
    let mut s = ProfileStore::default();
    let old = mouse("receiver:slot1", "unit:046d:01020304", 0x046d, 0xc08b);
    let p = assign(&mut s, &old, &template());
    let new_path = mouse("usb:other-port", "unit:046d:01020304", 0x046d, 0xc08b);
    assert_eq!(s.active_for(&new_path).unwrap().id, p.id);
    let new_mouse = mouse("receiver:slot1", "unit:046d:01020305", 0x046d, 0xc08b);
    assert!(s.active_for(&new_mouse).is_none());
    assert!(choose(&s, &[new_mouse], &p, None).is_err());
}
#[test]
fn old_store_load_does_not_create_assignments_or_reconnect_jobs() {
    let dir = std::env::temp_dir().join(format!("open-hub-routing-{}", new_id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("profiles.json");
    let mut v = serde_json::to_value(ProfileStore::default()).unwrap();
    v.as_object_mut().unwrap().remove("device_profiles");
    v.as_object_mut().unwrap().remove("profile_owners");
    v["auto_switch"] = true.into();
    std::fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
    let s = ProfileStore::load(&path);
    assert!(s.device_profiles.is_empty());
    assert!(s.profile_owners.is_empty());
    let e = mouse("a", "unit:046d:a", 0x046d, 0xc08b);
    assert!(s.active_for(&e).is_none());
    assert!(s.match_for(&e, "C:/game.exe").is_none());
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
#[test]
fn store_roundtrip_and_deletion_preserve_other_units() {
    let mut s = ProfileStore::default();
    let a = mouse("a", "unit:046d:a", 0x046d, 0xc08b);
    let b = mouse("b", "unit:046d:b", 0x046d, 0xc08b);
    let ca = assign(&mut s, &a, &template());
    let cb = assign(&mut s, &b, &template());
    let mut s: ProfileStore = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
    s.repair_device_profiles();
    s.delete(&ca.id);
    assert!(s.active_for(&a).is_none());
    assert_eq!(s.active_for(&b).unwrap().id, cb.id);
    assert!(s.get("desktop").is_some());
}
#[test]
fn serial_keys_reject_missing_factory_and_control_characters() {
    for serial in [
        None,
        Some(""),
        Some("000000"),
        Some("FFFFFFFF"),
        Some("Unknown"),
        Some("a\nb"),
    ] {
        assert!(serial_key(0x1532, 0xaa, serial).is_empty());
    }
    assert_ne!(
        serial_key(0x1532, 0xaa, Some("unitA")),
        serial_key(0x1532, 0xab, Some("unitA"))
    );
}
#[test]
fn shared_desktop_crosses_routes_only_with_one_explicit_endpoint() {
    let mut s = ProfileStore::default();
    let a = mouse("native", "session:native", 0x3837, 0x4026);
    let mut b = mouse("sino:one", "session:sino", 0x258a, 0x0036);
    b.route = Route::Sino;
    let peers = [a, b.clone()];
    assert!(choose(&s, &peers, &template(), None).is_err());
    let p = assign(&mut s, &b, &template());
    assert!(b.matches(&p));
    assert_eq!(choose(&s, &peers, &p, None).unwrap().id, "sino:one");
    let mut multi = mouse("multi:one", "serial:razer", 0x1532, 0xaa);
    multi.route = Route::Multi;
    assert!(!multi.matches(&template()));
}
#[test]
fn production_prepare_rejects_ambiguity_auto_without_identity_and_cross_unit_reuse() {
    let mut s = ProfileStore::default();
    let a = mouse("a", "unit:046d:a", 0x046d, 0xc08b);
    let b = mouse("b", "unit:046d:b", 0x046d, 0xc08b);
    assert!(prepare(&s, &[a.clone(), b.clone()], "desktop", None, false, true).is_err());
    assert!(s.profile_owners.is_empty());
    let Prepared {
        target,
        profile,
        mut updated,
    } = prepare(
        &s,
        &[a.clone(), b.clone()],
        "desktop",
        Some("a"),
        false,
        true,
    )
    .unwrap();
    assert_eq!(target.id, "a");
    updated.select_for(&target, &profile.id).unwrap();
    s = updated;
    assert!(prepare(&s, &[b.clone()], &profile.id, Some("b"), false, true).is_err());
    assert!(prepare(&s, &[b.clone()], &profile.id, None, true, true).is_err());
    assert!(prepare(&s, &[b.clone()], &profile.id, Some("b"), true, false).is_err());
    let Prepared {
        target,
        profile: copy,
        mut updated,
    } = prepare(&s, &[b.clone()], &profile.id, Some("b"), true, true).unwrap();
    updated.select_for(&target, &copy.id).unwrap();
    assert_ne!(copy.id, profile.id);
    assert_eq!(updated.active_for(&a).unwrap().id, profile.id);
    assert_eq!(updated.active_for(&b).unwrap().id, copy.id);
    let no_id = mouse("mchose", "session:only", 0x3837, 0x4026);
    assert!(prepare(
        &s,
        &[no_id.clone()],
        "desktop",
        Some("mchose"),
        false,
        false
    )
    .is_err());
    assert!(prepare(&s, &[no_id], "desktop", Some("mchose"), false, true).is_ok());
}
fn fixture() -> (ProfileStore, Vec<crate::DeviceSummary>) {
    let mut s = ProfileStore::default();
    let mut p = template();
    p.id = "logi-template".into();
    p.name = "罗技游戏模板".into();
    p.target_vendor = Some(0x046d);
    p.target_model = Some(0xc08b);
    s.upsert(p.clone());
    let mut devices = Vec::new();
    for (unit, key, level) in [
        ("A", "unit:046d:01020304", 800),
        ("B", "unit:046d:01020305", 3200),
    ] {
        let e = mouse(&format!("logi-{unit}"), key, 0x046d, 0xc08b);
        let mut copy = assign(&mut s, &e, &p);
        copy.id = format!("private-{unit}");
        copy.name = format!("鼠标 {unit} 的独立配置");
        copy.levels = vec![
            crate::profiles::Level { x: level, y: None },
            crate::profiles::Level { x: 1600, y: None },
        ];
        copy.active_level = 0;
        // Fixed fixture IDs let UI output be replayed deterministically.
        s.upsert(copy.clone());
        s.profile_owners.insert(copy.id.clone(), e.key.clone());
        s.device_profiles
            .get_mut(&e.key)
            .unwrap()
            .copies
            .insert(p.id.clone(), copy.id.clone());
        s.select_for(&e, &copy.id).unwrap();
        devices.push(crate::DeviceSummary {
            connection_key: None,
            vendor_id: 0x046d,
            id: e.id,
            profile_key: e.key,
            name: "G502 HERO".into(),
            product: "G502 HERO".into(),
            product_id: "C08B".into(),
            kind: "Mouse",
            firmware_name: "simulation".into(),
            model_ids: vec!["C08B".into()],
            connection: format!("USB · {unit}"),
            protocol: "simulation".into(),
            features: vec![crate::device::FeatureInfo {
                id: 0x2201,
                index: 2,
                version: 0,
                flags: 0,
                name: "Adjustable DPI",
            }],
        });
    }
    // Remove temporary random-ID profiles created by assign, not the fixed copies.
    s.profiles
        .retain(|p| !s.profile_owners.contains_key(&p.id) || p.id.starts_with("private-"));
    s.repair_device_profiles();
    let mut mchose = template();
    mchose.id = "mchose-template".into();
    mchose.name = "迈从板载模板".into();
    mchose.target_vendor = Some(0x3837);
    mchose.target_model = Some(0x4026);
    mchose.mchose_board = Some(1);
    mchose.levels.clear();
    mchose.report_rate_hz = None;
    s.upsert(mchose);
    for unit in ["A", "B"] {
        devices.push(crate::DeviceSummary {
            connection_key: Some(format!("connection:mchose-{unit}:1")),
            vendor_id: 0x3837,
            id: format!("mchose-{unit}"),
            profile_key: format!("session:mchose-{unit}"),
            name: "MCHOSE A7 V3 Ultra+".into(),
            product: "MCHOSE A7 V3 Ultra+".into(),
            product_id: "1014".into(),
            kind: "Mouse",
            firmware_name: "simulation".into(),
            model_ids: vec!["4026".into()],
            connection: format!("2.4 GHz · {unit}"),
            protocol: "simulation".into(),
            features: vec![],
        });
    }
    s.auto_switch = true;
    (s, devices)
}
#[test]
fn export_native_profile_routing_fixture() {
    let (s, devices) = fixture();
    assert_eq!(devices.len(), 4);
    assert_eq!(s.profile_owners.len(), 2);
    assert_eq!(s.active_id, "private-B");
    if let Some(path) = std::env::var_os("OPEN_HUB_PROFILE_ROUTING_FIXTURE") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &serde_json::json!({"view":crate::view(&s),"devices":devices,"store":s}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}
#[test]
#[ignore = "requires UI-generated independent configuration IPC calls"]
fn replay_profile_routing_ui_through_production_preflight_and_private_storage() {
    let path = std::env::var_os("OPEN_HUB_PROFILE_ROUTING_UI_CALLS").expect("set UI call fixture");
    let calls: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let (mut store, devices) = fixture();
    let peers = crate::endpoints(&devices);
    let mut touched = Vec::new();
    for call in calls.as_array().unwrap() {
        let a = &call["args"];
        match call["cmd"].as_str().unwrap() {
            "activate_profile" => {
                let id = a["id"].as_str().unwrap();
                let selected = a["deviceId"].as_str();
                let as_copy = a["asCopy"].as_bool().unwrap_or(false);
                let Prepared {
                    target,
                    profile,
                    mut updated,
                } = prepare(&store, &peers, id, selected, as_copy, true).unwrap();
                if target.vendor == crate::mchose::VID {
                    let device = crate::mchose_target::simulated_device(&target.id);
                    crate::mchose_target::check_profile_connection(&device, a["expectedConnection"].as_str()).unwrap();
                } else {
                    assert!(a.get("expectedConnection").is_none());
                }
                // The real dispatcher receives just one endpoint; record it rather
                // than opening a HID handle. Commit the production store transition.
                touched.push(target.id.clone());
                updated.select_for(&target, &profile.id).unwrap();
                store = updated;
            }
            "save_profile" => {
                let edited: Profile = serde_json::from_value(a["profile"].clone()).unwrap();
                let e = choose(&store, &peers, &edited, a["deviceId"].as_str())
                    .unwrap()
                    .clone();
                store.save_for(&e, &edited).unwrap();
            }
            _ => {}
        }
    }
    assert_eq!(touched, vec!["mchose-B", "logi-A", "logi-A", "logi-B"]);
    let a = &peers[0];
    let b = &peers[1];
    assert_eq!(store.active_for(a).unwrap().active().unwrap().x, 1600);
    assert_eq!(store.active_for(b).unwrap().active().unwrap().x, 1600);
    assert_ne!(
        store.active_for(a).unwrap().id,
        store.active_for(b).unwrap().id
    );
    assert_eq!(store.get("private-B").unwrap().active().unwrap().x, 3200);
    assert!(store.active_for(&peers[2]).is_none());
    assert_eq!(store.active_for(&peers[3]).unwrap().mchose_board, Some(1));
}

#[test]
fn duplicate_identity_quarantine_survives_store_roundtrip_and_app_restart() {
    let mut s = ProfileStore::default();
    let mut sessions = Sessions::default();
    let mut peers = vec![
        mouse("a", "unit:046d:duplicate", 0x046d, 0xc08b),
        mouse("b", "unit:046d:duplicate", 0x046d, 0xc08b),
    ];
    sessions.resolve(&mut peers);
    s.ambiguous_keys
        .extend(sessions.collisions().iter().cloned());
    let restored: ProfileStore = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
    let mut restarted = Sessions::default();
    restarted.quarantine(restored.ambiguous_keys);
    let mut single = vec![mouse("new-port", "unit:046d:duplicate", 0x046d, 0xc08b)];
    restarted.resolve(&mut single);
    assert!(!single[0].can_auto_apply());
    assert!(single[0].key.starts_with("session:"));
}

#[test]
fn saving_private_draft_does_not_make_it_an_automatic_or_active_profile() {
    let mut s = ProfileStore::default();
    let e = mouse("unit-A", "unit:046d:01020304", 0x046d, 0xc08b);
    let mut p = template();
    p.levels[2].x = 2400;
    let saved = s.save_private(&e, &p).unwrap();
    assert_eq!(s.profile_owners.get(&saved.id), Some(&e.key));
    assert!(s.active_for(&e).is_none());
    assert!(s.match_for(&e, "C:/game.exe").is_none());
    let Prepared {
        target,
        profile,
        mut updated,
    } = prepare(&s, &[e.clone()], &saved.id, Some("unit-A"), false, true).unwrap();
    updated.select_for(&target, &profile.id).unwrap();
    assert_eq!(updated.active_for(&e).unwrap().active().unwrap().x, 2400);
}

#[test]
fn mouse_profile_endpoints_exclude_keyboards_but_keep_dpi_capable_unknown_names() {
    let (_, mut devices) = fixture();
    devices[0].kind = "Keyboard";
    devices[0].features.clear();
    assert_eq!(crate::endpoints(&devices).len(), 3);
    devices[0].kind = "Device";
    devices[0].features.push(crate::device::FeatureInfo {
        id: 0x2201,
        index: 2,
        version: 0,
        flags: 0,
        name: "DPI",
    });
    assert_eq!(crate::endpoints(&devices).len(), 4);
}
