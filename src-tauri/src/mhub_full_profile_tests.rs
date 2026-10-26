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
#[test]
fn editor_preparation_reencodes_canonical_edits_and_preserves_opaque_macros() {
    let original=full();
    let mut edit=original.clone();
    let plan=edit.mchose.as_mut().unwrap();
    plan.dpi_x[1]=1250;plan.dpi_y.as_mut().unwrap()[1]=1300;
    plan.wireless_rate=5;plan.usb_rate=2;plan.macros[0].name="编辑后的宏".into();plan.macros[0].mode=0;
    let ready=prepare_profile(edit).unwrap();
    assert_eq!(ready.levels[1],crate::profiles::Level{x:1250,y:Some(1300)});
    assert_eq!(ready.report_rate_hz,Some(4000));
    let edited=&ready.mchose.as_ref().unwrap().macros[0];
    assert_eq!(edited.raw[4],0);assert_ne!(edited.raw,original.mchose.as_ref().unwrap().macros[0].raw);
    crate::profile_transfer::validate(&ready).unwrap();
    let mut opaque=original.clone();opaque.mchose.as_mut().unwrap().macros[0].raw[6]=0xa7;
    let kept=prepare_profile(opaque.clone()).unwrap();assert_eq!(kept.mchose,opaque.mchose);
    opaque.mchose.as_mut().unwrap().macros[0].name="不可清空未解释字段".into();
    assert!(prepare_profile(opaque).unwrap_err().contains("未解释"));
}
#[test]
fn editor_preparation_does_not_fix_invalid_sources_missing_refs_or_unbalanced_events() {
    let mut p=full();p.target_vendor=Some(0x046d);assert!(prepare_profile(p).is_err());
    let mut p=full();p.mchose.as_mut().unwrap().buttons[4]=vec![0x21,17,0];assert!(prepare_profile(p).is_err());
    let mut p=full();let m=&mut p.mchose.as_mut().unwrap().macros[0];m.raw.clear();m.events=vec![MacroEvent{kind:"key".into(),code:4,down:true,delay_ms:0}];assert!(prepare_profile(p).is_err());
    let mut p=full();p.mchose.as_mut().unwrap().active=5;p.mchose.as_mut().unwrap().count=2;assert!(prepare_profile(p).is_err());
    let mut p=full();p.mchose.as_mut().unwrap().buttons[0]=vec![0xfe,0,0];assert!(prepare_profile(p).is_err());
}
#[test]
fn explicit_source_files_prepare_complete_profile_without_mutation_or_device_access() {
    let dir=std::env::temp_dir().join(format!("open-hub-explicit-mhub-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir(&dir).unwrap();let source=dir.join("template.json");let library=dir.join("library.json");
    let source_bytes=include_bytes!("../tests/fixtures/mhub-full-template.json");let library_bytes=include_bytes!("../tests/fixtures/mhub-macro-library.json");
    std::fs::write(&source,source_bytes).unwrap();std::fs::write(&library,library_bytes).unwrap();
    assert!(prepare_file(&source,None).unwrap_err().contains("缺少正文"));
    let imported=prepare_file(&source,Some(&library)).unwrap();assert_eq!(imported.mchose,full().mchose);
    assert_eq!(std::fs::read(&source).unwrap(),source_bytes);assert_eq!(std::fs::read(&library).unwrap(),library_bytes);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(),2);assert!(prepare_file(&dir,None).is_err());
    let oversized=dir.join("large.json");std::fs::File::create(&oversized).unwrap().set_len(8*1024*1024+1).unwrap();assert!(prepare_file(&oversized,None).unwrap_err().contains("8 MiB"));
    for file in [&source,&library,&oversized] {std::fs::remove_file(file).unwrap();}std::fs::remove_dir(dir).unwrap();
}
#[test]
#[ignore = "test-only pure-function bridge, explicit request and response paths required"]
fn mhub_editor_ui_bridge() {
    #[derive(serde::Deserialize)]
    #[serde(rename_all="camelCase",deny_unknown_fields)]
    struct ImportArgs {path:String,macro_library_path:Option<String>}
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct EditArgs {profile:Profile}
    let bytes=std::fs::read(std::env::var("OPEN_HUB_MHUB_EDITOR_UI_REQUEST").unwrap()).unwrap();assert!(bytes.len()<2*1024*1024);
    let request:Value=serde_json::from_slice(&bytes).unwrap();
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct LocalArgs {path:String,origin:Option<String>}
    let result:Result<Value>=match request["cmd"].as_str().unwrap() {
        "prepare_mhub_import"=>{let args:ImportArgs=serde_json::from_value(request["args"].clone()).unwrap();prepare_file(std::path::Path::new(&args.path),args.macro_library_path.as_deref().map(std::path::Path::new)).map(|p|serde_json::to_value(p).unwrap())},
        "prepare_mchose_profile"=>{let args:EditArgs=serde_json::from_value(request["args"].clone()).unwrap();prepare_profile(args.profile).map(|p|serde_json::to_value(p).unwrap())},
        "prepare_mhub_local_import"=>{let args:LocalArgs=serde_json::from_value(request["args"].clone()).unwrap();crate::mhub_local::prepare_in(std::path::Path::new(&std::env::var("OPEN_HUB_MHUB_LOCAL_UI_DATABASE").unwrap()),std::path::Path::new(&args.path),args.origin.as_deref()).map(|p|serde_json::to_value(p).unwrap())},
        _=>panic!("bridge only permits pure preparation functions"),
    };
    let response=match result {Ok(profile)=>json!({"ok":profile}),Err(error)=>json!({"error":error})};
    std::fs::write(std::env::var("OPEN_HUB_MHUB_EDITOR_UI_RESPONSE").unwrap(),serde_json::to_vec_pretty(&response).unwrap()).unwrap();
}
