use super::*;
use crate::{
    profile_routing::{Endpoint, Route},
    profiles::{Profile, ProfileStore},
};
fn full() -> Profile {
    to_profile(plan()).unwrap()
}
fn endpoint(id: &str) -> Endpoint {
    Endpoint {
        id: id.into(),
        name: id.into(),
        key: format!("session:{id}"),
        connection_key:None,
        observed_key: format!("session:{id}"),
        vendor: 0x3837,
        models: vec![0x4026],
        firmware: "fixture".into(),
        route: Route::Native,
    }
}
fn sync(p: &mut Profile) {
    let plan = p.mchose.as_ref().unwrap();
    p.levels = profile_levels(plan);
    p.active_level = plan.active as usize;
    p.report_rate_hz = rate_hz(plan.wireless_rate);
}
#[test]
fn full_profile_preserves_all_payload_bytes_and_exact_summary_through_serde() {
    let p = full();
    crate::profile_transfer::validate(&p).unwrap();
    let bytes = serde_json::to_vec(&p).unwrap();
    let p2: Profile = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(p.mchose, p2.mchose);
    assert_eq!(
        p.levels[0],
        crate::profiles::Level {
            x: 400,
            y: Some(450)
        }
    );
    assert_eq!(p.active_level, 1);
    assert_eq!(p.report_rate_hz, Some(2000));
    assert_eq!(p.mchose.as_ref().unwrap().usb_rate, 3);
    assert!(p.mchose_board.is_none());
    assert!(p.ghub_id.is_none());
    assert!(p.targets(0x3837, &[0x4026]));
    assert!(!p.targets(0x046d, &[0x4026]));
    assert!(!p.targets(0x3837, &[0x4033]));
    let mut inactive = p.clone();
    inactive.mchose.as_mut().unwrap().independent = false;
    sync(&mut inactive);
    assert!(inactive.levels.iter().all(|l| l.y.is_none()));
    assert!(inactive.mchose.as_ref().unwrap().dpi_y.is_some());
    crate::profile_transfer::validate(&inactive).unwrap();
}
#[test]
fn wrong_summary_scope_or_mixed_source_cannot_be_saved_or_prepared() {
    for variant in 0..7 {
        let mut p = full();
        match variant {
            0 => p.levels[0].x = 2000,
            1 => p.active_level = 2,
            2 => p.report_rate_hz = Some(1000),
            3 => p.target_vendor = Some(0x046d),
            4 => p.target_model = Some(0x4033),
            5 => p.mchose_board = Some(0),
            _ => p.ghub_id = Some("foreign-source".into()),
        };
        assert!(crate::profile_transfer::validate(&p).is_err());
        let mut store = ProfileStore::default();
        assert!(crate::profile_transfer::merge(&mut store, vec![p.clone()]).is_err());
        assert_eq!(store.profiles.len(), 1);
        store.upsert(p.clone());
        assert!(crate::profile_routing::prepare(
            &store,
            &[endpoint("A")],
            &p.id,
            Some("A"),
            false,
            true
        )
        .is_err());
    }
}
#[test]
fn legacy_profiles_keep_their_serialized_shape_and_references() {
    let p = Profile::desktop();
    let value = serde_json::to_value(&p).unwrap();
    assert!(value.get("mchose").is_none());
    let p: Profile = serde_json::from_value(value).unwrap();
    assert!(p.mchose.is_none());
    let index: Value =
        serde_json::from_str(include_str!("../tests/fixtures/mhub-boards.json")).unwrap();
    let mut store = ProfileStore::default();
    crate::mhub_import::import(&mut store, &index).unwrap();
    assert_eq!(store.profiles.len(), 4);
    assert!(store.profiles.iter().all(|p| p.mchose.is_none()));
}
#[test]
fn package_v2_roundtrip_retains_full_plan_and_v1_cannot_silently_drop_it() {
    let p = full();
    let file = std::env::temp_dir().join(format!(
        "open-hub-mhub-pack-{}.json",
        crate::profiles::new_id()
    ));
    assert_eq!(
        crate::profile_transfer::pack_format(&[p.clone()]),
        "open-hub-profiles/2"
    );
    assert_eq!(
        crate::profile_transfer::pack_format(&[Profile::desktop()]),
        "open-hub-profiles/1"
    );
    for format in ["open-hub-profiles/2", "open-hub-profiles/1"] {
        std::fs::write(
            &file,
            serde_json::to_vec(&json!({"format":format,"profiles":[p.clone()]})).unwrap(),
        )
        .unwrap();
        let result =
            crate::profile_transfer::preview("openhub", Some(file.to_string_lossy().into_owned()));
        if format.ends_with('2') {
            let preview = result.unwrap();
            assert_eq!(preview.profiles.len(), 1);
            assert_eq!(preview.profiles[0].mchose, p.mchose);
        } else {
            match result {
                Ok(_) => panic!("v1 wrapper accepted new payload"),
                Err(e) => assert!(e.contains("格式2")),
            }
        }
    }
    std::fs::remove_file(file).unwrap();
}
#[test]
fn independent_copies_survive_store_reload_and_reimport_updates_only_template() {
    let mut store = ProfileStore::default();
    let source = full();
    crate::profile_transfer::merge(&mut store, vec![source.clone()]).unwrap();
    let a = endpoint("A");
    let b = endpoint("B");
    let ca = store.private_copy(&a, &source).unwrap();
    let cb = store.private_copy(&b, &source).unwrap();
    assert_ne!(ca.id, cb.id);
    let mut edited = ca.clone();
    edited.name = "A 独立副本".into();
    edited.mchose.as_mut().unwrap().dpi_x[0] = 1350;
    sync(&mut edited);
    crate::profile_transfer::validate(&edited).unwrap();
    store.save_for(&a, &edited).unwrap();
    store.select_for(&b, &cb.id).unwrap();
    let mut update = source.clone();
    update.mchose.as_mut().unwrap().dpi_x[0] = 1400;
    sync(&mut update);
    crate::profile_transfer::merge(&mut store, vec![update]).unwrap();
    assert_eq!(
        store
            .get(&source.id)
            .unwrap()
            .mchose
            .as_ref()
            .unwrap()
            .dpi_x[0],
        1400
    );
    assert_eq!(
        store.get(&ca.id).unwrap().mchose.as_ref().unwrap().dpi_x[0],
        1350
    );
    assert_eq!(
        store.get(&cb.id).unwrap().mchose.as_ref().unwrap().dpi_x[0],
        400
    );
    let file = std::env::temp_dir().join(format!(
        "open-hub-mhub-store-{}.json",
        crate::profiles::new_id()
    ));
    store.save(&file).unwrap();
    let restored = ProfileStore::load(&file);
    std::fs::remove_file(file).unwrap();
    assert_eq!(
        restored.get(&ca.id).unwrap().mchose,
        store.get(&ca.id).unwrap().mchose
    );
    assert_eq!(
        restored.get(&cb.id).unwrap().mchose,
        store.get(&cb.id).unwrap().mchose
    );
    assert_eq!(restored.profile_owners[&ca.id], a.key);
    assert_eq!(restored.profile_owners[&cb.id], b.key);
}
#[test]
fn ambiguous_same_model_requires_target_and_cannot_route_to_other_brands_or_services() {
    let p = full();
    let a = endpoint("A");
    let b = endpoint("B");
    let mut other = endpoint("Logitech");
    other.vendor = 0x046d;
    other.models = vec![0xc08b];
    let peers = [a.clone(), b.clone(), other.clone()];
    let mut store = ProfileStore::default();
    store.upsert(p.clone());
    assert!(crate::profile_routing::choose(&store, &peers, &p, None).is_err());
    assert_eq!(
        crate::profile_routing::choose(&store, &peers, &p, Some("B"))
            .unwrap()
            .id,
        "B"
    );
    assert!(!other.matches(&p));
    for route in [Route::Sino, Route::Multi] {
        let mut wrong = a.clone();
        wrong.route = route;
        assert!(!wrong.matches(&p));
    }
    assert!(
        crate::profile_routing::prepare(&store, &peers, &p.id, Some("A"), false, false).is_err()
    ); // no RF unit proof for automatic apply
    let prepared =
        crate::profile_routing::prepare(&store, &peers, &p.id, Some("A"), false, true).unwrap();
    assert_eq!(prepared.profile.mchose, p.mchose);
    assert_eq!(store.profiles.len(), 2);
    assert!(store.profile_owners.is_empty());
}
