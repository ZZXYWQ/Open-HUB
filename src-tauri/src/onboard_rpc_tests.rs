//! Executes production on-board operations at the HID++ feature-request
//! boundary. No HID handle, simulated OS input or physical mouse is opened.
use super::*;
use crate::{
    device::FeatureInfo,
    hidpp::FeatureRequest,
    onboard_bindings::{Action, Change, Layer, Settings},
};
use std::{collections::BTreeMap, ffi::CString, sync::Mutex};
#[derive(Default)]
struct State {
    sectors: BTreeMap<u16, Vec<u8>>,
    staging: Option<(u16, usize, Vec<u8>)>,
    calls: Vec<(u8, Vec<u8>)>,
    fault: Option<(u8, usize)>,
    seen: usize,
    change_directory_on_fault: bool,
    mode: u8,
    active: u8,
    reload_fail: bool,
    corrupt_readback: bool,
    committed: bool,
    small_reply: bool,
    sector_size:Option<u16>,
    unit: [u8; 4],
    identity_reads: usize,
    short_identity: bool,
    change_unit_on_fault: bool,
    change_unrelated_on_fault: bool,
    descriptor_changed: bool,
    switch_after_read: Option<usize>,
    read_count: usize,
}
struct Memory(Mutex<State>);
fn crc(mut b: Vec<u8>) -> Vec<u8> {
    let n = b.len();
    let c = crc_ccitt(&b[..n - 2]);
    b[n - 2..].copy_from_slice(&c.to_be_bytes());
    b
}
fn profile() -> Vec<u8> {
    let mut b = (0..256).map(|n| (n % 251) as u8).collect::<Vec<_>>();
    b[0] = 1;
    b[1] = 0;
    b[2] = 0;
    b[3..13].copy_from_slice(&[0x20, 3, 0x40, 6, 0x80, 12, 0, 0, 0, 0]);
    b[0x20..0x24].copy_from_slice(&[128, 1, 0, 1]);
    b[0x30..0x34].copy_from_slice(&[0, 3, 0, 5]);
    b[0xa0..0xd0].fill(0);
    for (at,unit) in "协议模拟配置".encode_utf16().enumerate(){b[0xa0+at*2..0xa2+at*2].copy_from_slice(&unit.to_le_bytes());}
    crc(b)
}
fn memory() -> Memory {
    let mut dir = vec![255; 256];
    dir[..8].copy_from_slice(&[0, 1, 1, 0, 0, 2, 1, 0]);
    Memory(Mutex::new(State {
        sectors: [(0, crc(dir)), (1, profile()), (2, profile())].into(),
        mode: 1,
        active: 1,
        ..Default::default()
    }))
}
fn device() -> Device {
    Device {
        vendor_id: 0x046d,
        profile_key: "session:memory-only".into(),
        connection_key: "connection:memory-only".into(),
        id: "logitech-test-A".into(),
        path: CString::new("memory-only").unwrap(),
        index: 0xff,
        product_id: 0xc08b,
        product: "simulation".into(),
        name: "G502 HERO protocol simulation".into(),
        firmware_name: "simulation".into(),
        model_ids: vec![0xc08b],
        kind: "Mouse",
        protocol: "HID++ 2.0".into(),
        features: [(
            ONBOARD_PROFILES,
            FeatureInfo {
                id: ONBOARD_PROFILES,
                index: 0x12,
                version: 0,
                flags: 0,
                name: "Onboard profiles",
            },
        )]
        .into(),
    }
}
fn settings(m: &Memory) -> Settings {
    let s = m.0.lock().unwrap();
    Settings {
        expected_device: device().profile_key,
        expected_connection: device().connection_key,
        slot: 1,
        sector: 1,
        changes: vec![
            Change {
                button: 4,
                layer: Layer::Normal,
                expected: s.sectors[&1][0x2c..0x30].try_into().unwrap(),
                action: Action::Key {
                    usage: 6,
                    modifiers: 5,
                },
            },
            Change {
                button: 4,
                layer: Layer::GShift,
                expected: s.sectors[&1][0x6c..0x70].try_into().unwrap(),
                action: Action::Consumer(0xcd),
            },
        ],
    }
}
impl FeatureRequest for Memory {
    fn request(
        &self,
        device: u8,
        feature: u8,
        function: u8,
        p: &[u8],
        timeout: Duration,
    ) -> Result<Vec<u8>> {
        if feature==0x11 {
            assert_eq!((device,function),(0xff,0));
            let mut s=self.0.lock().unwrap();
            s.identity_reads+=1;
            if s.short_identity { return Ok(vec![0;3]); }
            let mut r=vec![0;16];r[1..5].copy_from_slice(&s.unit);return Ok(r);
        }
        assert_eq!(
            (device, feature),
            (0xff, 0x12),
            "exact device and feature index"
        );
        if function == 8 {
            assert_eq!(timeout, COMMIT_TIMEOUT);
        }
        let mut s = self.0.lock().unwrap();
        s.calls.push((function, p.to_vec()));
        if s.small_reply && matches!(function, 0 | 5) {
            return Ok(vec![0; 3]);
        }
        let mut r = vec![0; 16];
        match function {
            0 => {r[..11].copy_from_slice(&[1, 3, 1, 2, 0, 11, 8, 1, 0, 10, 1]);if let Some(size)=s.sector_size{r[7..9].copy_from_slice(&size.to_be_bytes());}if s.descriptor_changed{r[9]^=1;}},
            2 => r[0] = s.mode,
            11 => r[0] = 0,
            4 => r[1] = s.active,
            3 => {
                if s.reload_fail {
                    return Err(Error::Disconnected);
                }
                assert_eq!(p, [0, 1]);
                s.active = p[1];
            }
            5 => {
                s.read_count+=1;
                if s.switch_after_read==Some(s.read_count) {s.unit=[9;4];}
                assert_eq!(p.len(), 4);
                let sector = u16::from_be_bytes([p[0], p[1]]);
                let at = usize::from(u16::from_be_bytes([p[2], p[3]]));
                r.copy_from_slice(&s.sectors[&sector][at..at + 16]);
                if sector == 1 && s.committed && s.corrupt_readback {
                    s.corrupt_readback = false;
                    r[3] ^= 1;
                }
            }
            6 => {
                assert_eq!(p.len(), 6);
                assert_eq!(&p[2..4], &[0, 0]);
                let sector = u16::from_be_bytes([p[0], p[1]]);
                assert!((0..0x100).contains(&sector));
                s.staging = Some((
                    sector,
                    usize::from(u16::from_be_bytes([p[4], p[5]])),
                    Vec::new(),
                ));
            }
            7 => {
                assert_eq!(p.len(), 16);
                s.staging.as_mut().expect("write start").2.extend(p);
            }
            8 => {
                let (sector, size, mut bytes) = s.staging.take().expect("write in progress");
                assert!(bytes.len() >= size);
                bytes.truncate(size);
                assert!(sector_crc_ok(&bytes)||bytes.iter().all(|b|*b==0xff));
                s.sectors.insert(sector, bytes);
                s.committed = true;
            }
            _ => panic!("unexpected function {function}"),
        }
        if let Some((target, nth)) = s.fault {
            if target == function {
                s.seen += 1;
                if s.seen == nth {
                    s.fault = None;
                    if s.change_unit_on_fault {s.unit=[9;4];}
                    if s.change_unrelated_on_fault{let mut p=s.sectors[&2].clone();p[0x12]^=0x40;s.sectors.insert(2,crc(p));}
                    if s.change_directory_on_fault {
                        let mut dir = s.sectors[&0].clone();
                        dir[1] = 2;
                        s.sectors.insert(0, crc(dir));
                    }
                    return Err(Error::Other(
                        "simulated uncertain transfer after device effect".into(),
                    ));
                }
            }
        }
        Ok(r)
    }
}
#[test]
fn complete_native_binding_transaction_reads_writes_verifies_and_preserves_dpi_rgb_and_other_profile(
) {
    let m = memory();
    let before = m.0.lock().unwrap().sectors.clone();
    let d = device();
    let report = d.write_slot_bindings(&m, &settings(&m)).unwrap();
    assert!(!report.unchanged);
    assert!(report.active_reloaded);
    assert!(report.reload_note.is_none());
    let s = m.0.lock().unwrap();
    assert_eq!(s.sectors[&0], before[&0]);
    assert_eq!(s.sectors[&2], before[&2]);
    let p = &s.sectors[&1];
    assert_eq!(&p[0x2c..0x30], &[128, 2, 5, 6]);
    assert_eq!(&p[0x6c..0x70], &[128, 3, 0, 205]);
    for n in 0..254 {
        if !((0x2c..0x30).contains(&n) || (0x6c..0x70).contains(&n)) {
            assert_eq!(p[n], before[&1][n], "unselected byte {n}");
        }
    }
    assert_eq!(s.calls.iter().filter(|(f, _)| *f == 6).count(), 1);
}
#[test]
fn all_uncertain_write_stages_restore_original_profile_when_ownership_is_unchanged() {
    let faults = [(6, 1), (8, 1)].into_iter().chain((1..=16).map(|n| (7, n)));
    for fault in faults {
        let m = memory();
        let before = m.0.lock().unwrap().sectors.clone();
        let settings = settings(&m);
        m.0.lock().unwrap().fault = Some(fault);
        let error = device()
            .write_slot_bindings(&m, &settings)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("restored and verified"),
            "{fault:?}: {error}"
        );
        assert_eq!(m.0.lock().unwrap().sectors, before, "{fault:?}");
    }
}
#[test]
fn mismatched_readback_is_not_reported_as_success_and_is_restored() {
    let m = memory();
    let before = m.0.lock().unwrap().sectors.clone();
    let settings = settings(&m);
    m.0.lock().unwrap().corrupt_readback = true;
    assert!(device()
        .write_slot_bindings(&m, &settings)
        .unwrap_err()
        .to_string()
        .contains("restored and verified"));
    assert_eq!(m.0.lock().unwrap().sectors, before);
}
#[test]
fn directory_ownership_change_blocks_blind_restore_and_stale_slot_blocks_first_write() {
    let m = memory();
    let settings = settings(&m);
    {
        let mut s = m.0.lock().unwrap();
        s.fault = Some((7, 1));
        s.change_directory_on_fault = true;
    }
    let error = device()
        .write_slot_bindings(&m, &settings)
        .unwrap_err()
        .to_string();
    assert!(error.contains("no blind retry"));
    assert_eq!(
        m.0.lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(f, _)| *f == 6)
            .count(),
        1
    );
    let m = memory();
    let mut settings = settings.clone();
    settings.sector = 2;
    assert!(device().write_slot_bindings(&m, &settings).is_err());
    assert!(!m
        .0
        .lock()
        .unwrap()
        .calls
        .iter()
        .any(|(f, _)| matches!(f, 6 | 7 | 8)));
}
#[test]
fn inactive_or_software_mode_is_not_forced_and_reload_error_keeps_verified_save() {
    for (mode, active) in [(2, 1), (1, 2)] {
        let m = memory();
        let settings = settings(&m);
        {
            let mut s = m.0.lock().unwrap();
            s.mode = mode;
            s.active = active;
        }
        let report = device().write_slot_bindings(&m, &settings).unwrap();
        assert!(!report.active_reloaded);
        assert!(!m.0.lock().unwrap().calls.iter().any(|(f, _)| *f == 3));
    }
    let m = memory();
    let settings = settings(&m);
    m.0.lock().unwrap().reload_fail = true;
    let report = device().write_slot_bindings(&m, &settings).unwrap();
    assert!(report
        .reload_note
        .unwrap()
        .contains("saved and readback verified"));
    assert_eq!(
        &m.0.lock().unwrap().sectors[&1][0x2c..0x30],
        &[128, 2, 5, 6]
    );
}
#[test]
fn short_replies_and_invalid_sizes_fail_without_padding_or_memory_writes() {
    let m = memory();
    m.0.lock().unwrap().small_reply = true;
    assert!(device().onboard_info(&m).is_err());
    assert!(device().read_sector(&m, 1, 256).is_err());
    for size in [0, 1, 15, 4097, 65535] {
        assert!(device().read_sector(&m, 1, size).is_err());
    }
    assert!(!m
        .0
        .lock()
        .unwrap()
        .calls
        .iter()
        .any(|(f, _)| matches!(f, 6 | 7 | 8)));
}

#[test]
fn native_logitech_onboard_fixture_export() {
    let m = memory();
    let (snapshot, _) = device().onboard_snapshot(&m).unwrap();
    assert_eq!(snapshot.profiles.len(), 2);
    assert_eq!(snapshot.info.button_count, 11);
    assert_eq!(snapshot.profiles[0].buttons[4].kind, "macro");
    if let Some(path) = std::env::var_os("OPEN_HUB_LOGITECH_BINDING_FIXTURE") {
        let d = device();
        let summary = serde_json::json!({"vendor_id":d.vendor_id,"id":d.id,"name":"G502 HERO","product":"G502 HERO","product_id":"c08b","kind":"mouse","firmware_name":"protocol simulation","model_ids":["c08b"],"connection":"wired","protocol":"HID++ 2.0","features":[{"id":ONBOARD_PROFILES,"index":0x12,"version":0,"flags":0,"name":"Onboard profiles"}]});
        let mut edited = settings(&m);
        edited.changes[0].action = Action::Key {
            usage: 6,
            modifiers: 17,
        };
        let before = m.0.lock().unwrap().sectors[&1].clone();
        edited.changes.push(Change {
            button: 7,
            layer: Layer::Normal,
            expected: before[0x38..0x3c].try_into().unwrap(),
            action: Action::Special(crate::onboard_bindings::Special::GShift),
        });
        d.write_slot_bindings(&m, &edited).unwrap();
        let (after_snapshot, _) = d.onboard_snapshot(&m).unwrap();
        let mut dpi=dpi_settings(&d);dpi.dpi_levels=vec![400,2400];dpi.default_index=1;
        let after_dpi_report=d.write_slot_settings(&m,&dpi).unwrap();
        let (after_dpi_snapshot,_)=d.onboard_snapshot(&m).unwrap();
        let macro_mem=macro_memory();
        let macro_trace=crate::onboard_macros::read(&d,&macro_mem,&macro_selection(&d)).unwrap();
        let planning_mem=planning_memory();let planning_device=unit_device(&planning_mem);
        let(planning_snapshot,_)=planning_device.onboard_snapshot(&planning_mem).unwrap();
        let macro_request=macro_request(&planning_device);let prepared=crate::onboard_macro_plan::prepare(&planning_device,&planning_mem,&macro_request).unwrap();
        let macro_preview=prepared.preview();let macro_saved=crate::onboard_macro_plan::commit(&planning_device,&planning_mem,&prepared).unwrap();
        let(after_macro_snapshot,_)=planning_device.onboard_snapshot(&planning_mem).unwrap();
        let long_mem=planning_memory();let long_device=unit_device(&long_mem);let long_request=long_macro_request(&long_device);let long_prepared=crate::onboard_macro_plan::prepare(&long_device,&long_mem,&long_request).unwrap();let long_macro_preview=long_prepared.preview();let long_macro_saved=crate::onboard_macro_plan::commit(&long_device,&long_mem,&long_prepared).unwrap();let(after_long_macro_snapshot,_)=long_device.onboard_snapshot(&long_mem).unwrap();
        let restore_file=restore_file();let restore_bytes=serde_json::to_vec_pretty(&restore_file).unwrap();let restore_digest=backup_digest(&restore_bytes);let restore_preview=restore_preview(&restore_file,&d.model_ids,&snapshot.info,&d.name,restore_digest.clone()).unwrap();
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &serde_json::json!({"restore_backup":restore_file,"restore_backup_digest":restore_digest,"restore_preview":restore_preview,"long_macro_preview":long_macro_preview,"long_macro_saved":long_macro_saved,"after_long_macro_snapshot":after_long_macro_snapshot,"planning_snapshot":planning_snapshot,"macro_preview":macro_preview,"macro_saved":macro_saved,"after_macro_snapshot":after_macro_snapshot,"macro_trace":macro_trace,"device":summary,"snapshot":snapshot,"after_snapshot":after_snapshot,"after_dpi_snapshot":after_dpi_snapshot,"after_dpi_report":after_dpi_report,"hardware_tested":false}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}
#[test]
#[ignore = "requires current Logitech button editor output"]
fn browser_logitech_bindings_execute_native_memory_transaction() {
    let path = std::env::var_os("OPEN_HUB_LOGITECH_BINDING_UI_SETTINGS").unwrap();
    let settings: Settings = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let m = memory();
    let original = m.0.lock().unwrap().sectors[&1].clone();
    assert_eq!(settings.slot, 1);
    assert_eq!(settings.changes.len(), 3);
    let report = device().write_slot_bindings(&m, &settings).unwrap();
    assert!(report.bytes_changed > 0);
    let s = m.0.lock().unwrap();
    let after = &s.sectors[&1];
    assert_eq!(&after[0x2c..0x30], &[128, 2, 17, 6]);
    assert_eq!(&after[0x6c..0x70], &[128, 3, 0, 205]);
    assert_eq!(&after[0x38..0x3c], &[144, 11, 0, 0]);
    for n in 0..254 {
        if !((0x2c..0x30).contains(&n) || (0x6c..0x70).contains(&n) || (0x38..0x3c).contains(&n)) {
            assert_eq!(after[n], original[n]);
        }
    }
}

#[test]
fn receiver_or_other_brand_never_writes_through_reused_hidpp_index() {
    let m = memory();
    let settings = settings(&m);
    for (index, vendor) in [(1, 0x046d), (0xff, 0x3837)] {
        let mut d = device();
        d.index = index;
        d.vendor_id = vendor;
        assert!(d.write_slot_bindings(&m, &settings).is_err());
    }
    assert!(m.0.lock().unwrap().calls.is_empty());
}

fn unit_device(m:&Memory)->Device{
    m.0.lock().unwrap().unit=[1,2,3,4];
    let mut d=device();d.profile_key="unit:046d:01020304".into();
    d.features.insert(crate::device::feature::DEVICE_INFO,FeatureInfo{id:crate::device::feature::DEVICE_INFO,index:0x11,version:0,flags:0,name:"Device info"});d
}
fn unit_settings(m:&Memory,d:&Device)->Settings{let mut s=settings(m);s.expected_device=d.profile_key.clone();s}
fn dpi_settings(d:&Device)->SlotSettings{SlotSettings{expected_device:d.profile_key.clone(),expected_connection:d.connection_key.clone(),sector:1,slot:1,dpi_levels:vec![400,800],default_index:0,report_rate_hz:500}}
#[test]
fn stale_preview_keys_reject_button_and_dpi_edits_before_even_reading_memory(){
    let m=memory();let d=device();let mut b=settings(&m);b.expected_device="session:another-connection".into();
    assert!(d.write_slot_bindings(&m,&b).unwrap_err().to_string().contains("connection changed"));
    let mut dpi=dpi_settings(&d);dpi.expected_device=b.expected_device;
    assert!(d.write_slot_settings(&m,&dpi).is_err());assert!(m.0.lock().unwrap().calls.is_empty());
    let json=serde_json::json!({"slot":1,"sector":1,"changes":[]});assert!(serde_json::from_value::<Settings>(json).is_err());
}
#[test]
fn same_unit_native_bindings_dpi_and_snapshot_keep_a_verified_preview_identity(){
    let m=memory();let d=unit_device(&m);let (snapshot,_)=d.onboard_snapshot(&m).unwrap();assert_eq!(snapshot.device_key,d.profile_key);
    let b=unit_settings(&m,&d);d.write_slot_bindings(&m,&b).unwrap();let dpi=dpi_settings(&d);d.write_slot_settings(&m,&dpi).unwrap();
    let s=m.0.lock().unwrap();assert!(s.identity_reads>=12);assert_eq!(s.sectors[&1][0],2);assert_eq!(&s.sectors[&1][3..7],&[0x90,1,0x20,3]);
}
#[test]
fn changed_or_truncated_unit_blocks_bindings_dpi_restore_and_snapshot_without_flash_writes(){
    for short in [false,true]{
        let m=memory();let d=unit_device(&m);let b=unit_settings(&m,&d);let dpi=dpi_settings(&d);let bytes=m.0.lock().unwrap().sectors[&1].clone();
        {let mut s=m.0.lock().unwrap();s.unit=[9;4];s.short_identity=short;}
        assert!(d.write_slot_bindings(&m,&b).is_err());assert!(d.write_slot_settings(&m,&dpi).is_err());assert!(d.restore_sectors(&m,&[(1,bytes)]).is_err());assert!(d.onboard_snapshot(&m).is_err());assert!(m.0.lock().unwrap().calls.is_empty());
    }
}
#[test]
fn a_unit_change_during_preview_or_prewrite_reads_is_detected_before_first_write(){
    for snapshot in [false,true]{let m=memory();let d=unit_device(&m);let b=unit_settings(&m,&d);m.0.lock().unwrap().switch_after_read=Some(20);
      if snapshot{assert!(d.onboard_snapshot(&m).is_err());}else{assert!(d.write_slot_bindings(&m,&b).is_err());}
      assert!(!m.0.lock().unwrap().calls.iter().any(|(f,_)|matches!(f,3|6|7|8)));
    }
}
#[test]
fn a_unit_change_after_an_uncertain_write_blocks_rollback_and_slot_reload(){
    for fault in [(6,1),(7,1),(8,1)]{let m=memory();let d=unit_device(&m);let b=unit_settings(&m,&d);
      {let mut s=m.0.lock().unwrap();s.fault=Some(fault);s.change_unit_on_fault=true;}
      let e=d.write_slot_bindings(&m,&b).unwrap_err().to_string();assert!(e.contains("no blind retry"),"{fault:?} {e}");
      let s=m.0.lock().unwrap();assert_eq!(s.calls.iter().filter(|(f,_)|*f==6).count(),1);assert!(!s.calls.iter().any(|(f,_)|*f==3));
    }
}
#[test]
fn dpi_directory_reassignment_and_receiver_restore_are_rejected(){
    let m=memory();let d=device();let mut dpi=dpi_settings(&d);dpi.sector=2;assert!(d.write_slot_settings(&m,&dpi).is_err());
    let mut receiver=d.clone();receiver.index=1;let b=m.0.lock().unwrap().sectors[&1].clone();assert!(receiver.restore_sectors(&m,&[(1,b)]).is_err());
    assert!(!m.0.lock().unwrap().calls.iter().any(|(f,_)|matches!(f,3|6|7|8)));
}

#[test]
#[ignore="requires current per-mouse on-board slot UI output"]
fn browser_logitech_dpi_slot_keeps_private_mouse_scope_and_native_target_identity(){
    let path=std::env::var_os("OPEN_HUB_LOGITECH_SLOT_UI_SETTINGS").unwrap();let s:SlotSettings=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let m=memory();let d=device();assert_eq!(s.expected_device,d.profile_key);assert_eq!((s.slot,s.sector),(1,1));assert_eq!(s.dpi_levels,vec![400,2400]);assert_eq!(s.default_index,0);
    let before=m.0.lock().unwrap().sectors.clone();let result=d.write_slot_settings(&m,&s).unwrap();assert!(!result.unchanged);
    let now=m.0.lock().unwrap();assert_eq!(now.sectors[&0],before[&0]);assert_eq!(now.sectors[&2],before[&2]);assert_eq!(&now.sectors[&1][3..7],&[0x90,1,0x60,9]);assert_eq!(&now.sectors[&1][0x0d..254],&before[&1][0x0d..254]);
}

fn macro_memory()->Memory{
 let m=memory();let mut a=vec![0xff;256];a[5..18].copy_from_slice(&[0x43,0x11,6,0x40,0,77,0x44,0x11,6,0x60,0,4,0]);a[18]=7;
 let mut b=vec![0xff;256];b[7..14].copy_from_slice(&[0x45,0,0xe2,0x46,0,0xe2,0xff]);
 {let mut s=m.0.lock().unwrap();s.sectors.insert(3,a);s.sectors.insert(4,b);}m
}
fn macro_selection(d:&Device)->crate::onboard_macros::Selection{crate::onboard_macros::Selection{expected_device:d.profile_key.clone(),expected_connection:d.connection_key.clone(),slot:1,sector:1,layer:Layer::Normal,button:5,expected:[0,3,0,5]}}
#[test]fn native_macro_reader_uses_only_verified_read_requests_and_preserves_every_sector(){
 let m=macro_memory();let d=unit_device(&m);let before=m.0.lock().unwrap().sectors.clone();let trace=crate::onboard_macros::read(&d,&m,&macro_selection(&d)).unwrap();assert!(trace.complete);assert_eq!(trace.pages.len(),2);assert_eq!(trace.steps.len(),7);assert_eq!(trace.steps[1].action,crate::onboard_macros::Action::Delay{milliseconds:77});assert_eq!(trace.steps[3].bytes,vec![0x60,0,4,0,7]);
 let s=m.0.lock().unwrap();assert_eq!(s.sectors,before);assert!(s.calls.iter().all(|(f,_)|matches!(f,0|5)));assert!(s.identity_reads>=6);
}
#[test]fn native_macro_preview_rejects_wrong_layer_stale_records_and_unit_changes_without_writes(){
 for choice in 0..4{let m=macro_memory();let d=unit_device(&m);let mut selection=macro_selection(&d);match choice{0=>selection.expected_device="session:elsewhere".into(),1=>selection.expected[3]=6,2=>selection.button=12,_=>m.0.lock().unwrap().switch_after_read=Some(40)}
  assert!(crate::onboard_macros::read(&d,&m,&selection).is_err());assert!(m.0.lock().unwrap().calls.iter().all(|(f,_)|matches!(f,0|5)));
 }
}

#[test]
#[ignore="requires current native macro inspection UI selection"]
fn browser_native_macro_selection_executes_read_only_production_reader(){
 let path=std::env::var_os("OPEN_HUB_LOGITECH_MACRO_UI_SELECTION").unwrap();let selection:crate::onboard_macros::Selection=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
 let m=macro_memory();let before=m.0.lock().unwrap().sectors.clone();let tr=crate::onboard_macros::read(&device(),&m,&selection).unwrap();assert!(tr.complete);assert_eq!(tr.steps[0].action,crate::onboard_macros::Action::KeyDown{modifiers:0x11,usage:6});assert_eq!(tr.steps[1].action,crate::onboard_macros::Action::Delay{milliseconds:77});assert_eq!(tr.pages.len(),2);let s=m.0.lock().unwrap();assert_eq!(s.sectors,before);assert!(s.calls.iter().all(|(f,_)|matches!(f,0|5)));
}

fn planning_memory()->Memory{
 let m=macro_memory();let mut state=m.0.lock().unwrap();for sector in [1,2]{let mut p=state.sectors[&sector].clone();p[0x20..0xa0].fill(0xff);p[0x20..0x24].copy_from_slice(&[0x80,1,0,1]);if sector==1{p[0x30..0x34].copy_from_slice(&[0,3,0,5]);}state.sectors.insert(sector,crc(p));}for s in 5..8{state.sectors.insert(s,vec![0xff;256]);}drop(state);m
}
fn macro_request(d:&Device)->crate::onboard_macro_plan::Request{let mut selection=macro_selection(d);selection.button=4;selection.expected=[0xff;4];crate::onboard_macro_plan::Request{selection,definition:gearwise_core::profile::Macro{id:"native-new".into(),name:"copy and media".into(),mode:gearwise_core::profile::MacroMode::Once,steps:vec![gearwise_core::profile::MacroStep::KeyTap{usage:6,mods:gearwise_core::model::Modifiers(0x11)},gearwise_core::profile::MacroStep::Delay(77),gearwise_core::profile::MacroStep::Media(0xe2)]}}}
#[test]fn whole_native_bank_capture_and_two_sector_macro_commit_preserve_existing_programs(){
 let m=planning_memory();let d=unit_device(&m);let before=m.0.lock().unwrap().sectors.clone();let request=macro_request(&d);let p=crate::onboard_macro_plan::prepare(&d,&m,&request).unwrap();assert_eq!(p.preview().macro_sector,5);assert_eq!(p.raw().len(),8);assert!(m.0.lock().unwrap().calls.iter().all(|(f,_)|matches!(f,0|5)));
 let report=crate::onboard_macro_plan::commit(&d,&m,&p).unwrap();assert!(report.active_reloaded);
 let mut selected=request.selection.clone();selected.expected=[0,5,0,0];let trace=crate::onboard_macros::read(&d,&m,&selected).unwrap();assert!(trace.complete);assert_eq!(trace.steps[2].action,crate::onboard_macros::Action::Delay{milliseconds:77});
 let now=m.0.lock().unwrap();for sector in [0,2,3,4,6,7]{assert_eq!(now.sectors[&sector],before[&sector]);}assert_eq!(&now.sectors[&1][0x2c..0x30],&[0,5,0,0]);
}
#[test]fn all_thirty_six_uncertain_macro_and_binding_write_stages_restore_original_bank(){
 let faults=[(6,1),(6,2),(8,1),(8,2)].into_iter().chain((1..=32).map(|n|(7,n)));
 for fault in faults{let m=planning_memory();let d=unit_device(&m);let before=m.0.lock().unwrap().sectors.clone();let p=crate::onboard_macro_plan::prepare(&d,&m,&macro_request(&d)).unwrap();m.0.lock().unwrap().fault=Some(fault);let e=crate::onboard_macro_plan::commit(&d,&m,&p).unwrap_err().to_string();assert!(e.contains("restored/verified"),"{fault:?}: {e}");assert_eq!(m.0.lock().unwrap().sectors,before,"{fault:?}");}
}
#[test]fn stale_global_bank_and_changed_unit_never_get_macro_commit_or_rollback(){
 let m=planning_memory();let d=unit_device(&m);let p=crate::onboard_macro_plan::prepare(&d,&m,&macro_request(&d)).unwrap();m.0.lock().unwrap().sectors.get_mut(&6).unwrap()[0]=0;assert!(crate::onboard_macro_plan::commit(&d,&m,&p).is_err());assert!(!m.0.lock().unwrap().calls.iter().any(|(f,_)|matches!(f,3|6|7|8)));
 let m=planning_memory();let d=unit_device(&m);let p=crate::onboard_macro_plan::prepare(&d,&m,&macro_request(&d)).unwrap();{let mut s=m.0.lock().unwrap();s.fault=Some((8,1));s.change_unit_on_fault=true;}let e=crate::onboard_macro_plan::commit(&d,&m,&p).unwrap_err().to_string();assert!(e.contains("state uncertain"));let state=m.0.lock().unwrap();assert_eq!(state.calls.iter().filter(|(f,_)|*f==6).count(),1);assert!(!state.calls.iter().any(|(f,_)|*f==3));
}

#[test]
#[ignore="requires current native macro composer UI request"]
fn browser_native_macro_composer_request_compiles_allocates_and_commits_in_production(){
 let path=std::env::var_os("OPEN_HUB_LOGITECH_MACRO_UI_REQUEST").unwrap();let request:crate::onboard_macro_plan::Request=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
 let m=planning_memory();let d=unit_device(&m);let before=m.0.lock().unwrap().sectors.clone();let p=crate::onboard_macro_plan::prepare(&d,&m,&request).unwrap();assert_eq!(p.preview().macro_sector,5);let report=crate::onboard_macro_plan::commit(&d,&m,&p).unwrap();assert!(report.active_reloaded);
 let mut selected=request.selection.clone();selected.expected=[0,5,0,0];let trace=crate::onboard_macros::read(&d,&m,&selected).unwrap();assert!(trace.complete);assert_eq!(trace.steps[0].action,crate::onboard_macros::Action::KeyDown{modifiers:0x11,usage:6});assert_eq!(trace.steps[2].action,crate::onboard_macros::Action::Delay{milliseconds:77});assert_eq!(trace.steps[3].action,crate::onboard_macros::Action::ConsumerDown{usage:0xe2});
 let now=m.0.lock().unwrap();for sector in [0,2,3,4,6,7]{assert_eq!(now.sectors[&sector],before[&sector]);}
}

#[test]fn native_macro_save_never_forces_host_or_another_slot_and_keeps_reload_failure_distinct(){
 for (mode,active,reload_fail) in [(2,1,false),(1,2,false),(1,1,true)]{let m=planning_memory();let d=unit_device(&m);{let mut state=m.0.lock().unwrap();state.mode=mode;state.active=active;state.reload_fail=reload_fail;}let p=crate::onboard_macro_plan::prepare(&d,&m,&macro_request(&d)).unwrap();let report=crate::onboard_macro_plan::commit(&d,&m,&p).unwrap();assert!(!report.active_reloaded);assert_eq!(report.reload_note.is_some(),reload_fail);if !reload_fail{assert!(!m.0.lock().unwrap().calls.iter().any(|(f,_)|*f==3));}}
}
#[test]fn directory_change_after_macro_commit_blocks_recovery_instead_of_overwriting_other_owner(){
 let m=planning_memory();let d=unit_device(&m);let p=crate::onboard_macro_plan::prepare(&d,&m,&macro_request(&d)).unwrap();{let mut state=m.0.lock().unwrap();state.fault=Some((8,1));state.change_directory_on_fault=true;}let error=crate::onboard_macro_plan::commit(&d,&m,&p).unwrap_err().to_string();assert!(error.contains("no blind restoration"));let state=m.0.lock().unwrap();assert_eq!(state.calls.iter().filter(|(f,_)|*f==6).count(),1);assert!(!state.calls.iter().any(|(f,_)|*f==3));
}

#[test]fn native_macro_preflight_rejects_unsupported_definitions_missing_unit_and_last_left_click(){
 let m=planning_memory();let d=unit_device(&m);let mut request=macro_request(&d);request.definition.steps=vec![gearwise_core::profile::MacroStep::Text("unsupported".into())];assert!(crate::onboard_macro_plan::prepare(&d,&m,&request).is_err());assert!(m.0.lock().unwrap().calls.is_empty());
 let m=planning_memory();let d=device();let request=macro_request(&d);assert!(crate::onboard_macro_plan::prepare(&d,&m,&request).is_err());assert!(m.0.lock().unwrap().calls.is_empty());
 let m=planning_memory();let d=unit_device(&m);let mut request=macro_request(&d);request.selection.button=1;request.selection.expected=[0x80,1,0,1];assert!(crate::onboard_macro_plan::prepare(&d,&m,&request).is_err());assert!(!m.0.lock().unwrap().calls.iter().any(|(f,_)|matches!(f,3|6|7|8)));
}

#[test]fn native_macro_can_replace_primary_left_only_when_another_normal_record_still_clicks_left(){
 let m=planning_memory();let d=unit_device(&m);{let mut state=m.0.lock().unwrap();let mut page=state.sectors[&1].clone();page[0x24..0x28].copy_from_slice(&[0x80,1,0,3]);state.sectors.insert(1,crc(page));}
 let mut request=macro_request(&d);request.selection.button=1;request.selection.expected=[0x80,1,0,1];let p=crate::onboard_macro_plan::prepare(&d,&m,&request).unwrap();let report=crate::onboard_macro_plan::commit(&d,&m,&p).unwrap();assert!(report.active_reloaded);assert_eq!(&m.0.lock().unwrap().sectors[&1][0x24..0x28],&[0x80,1,0,3]);
}

fn long_macro_request(d:&Device)->crate::onboard_macro_plan::Request{let mut request=macro_request(d);request.definition.name="cross-page copy and media".into();request.definition.steps.extend((0..40).map(|_|gearwise_core::profile::MacroStep::KeyTap{usage:4,mods:gearwise_core::model::Modifiers(0)}));request}
#[test]fn production_long_macro_chain_commits_and_reads_back_for_255_and_256_byte_geometry(){
 for size in [255,256]{let m=planning_memory();let d=unit_device(&m);if size==255{let mut state=m.0.lock().unwrap();state.sector_size=Some(size);for bytes in state.sectors.values_mut(){bytes.truncate(255);}for sector in [0,1,2]{let bytes=state.sectors[&sector].clone();state.sectors.insert(sector,crc(bytes));}}
  let before=m.0.lock().unwrap().sectors.clone();let request=long_macro_request(&d);let prepared=crate::onboard_macro_plan::prepare(&d,&m,&request).unwrap();assert_eq!(prepared.preview().page_count,2);crate::onboard_macro_plan::commit(&d,&m,&prepared).unwrap();let mut selection=request.selection;selection.expected=[0,5,0,0];let tr=crate::onboard_macros::read(&d,&m,&selection).unwrap();assert!(tr.complete);assert_eq!(tr.pages.len(),2);assert_eq!(tr.steps.len(),87);let now=m.0.lock().unwrap();for sector in [0,2,3,4,7]{assert_eq!(now.sectors[&sector],before[&sector]);}
 }
}
#[test]fn all_fifty_four_uncertain_long_macro_write_stages_restore_every_original_page(){
 let faults=(1..=3).map(|n|(6,n)).chain((1..=3).map(|n|(8,n))).chain((1..=48).map(|n|(7,n)));
 for fault in faults{let m=planning_memory();let d=unit_device(&m);let before=m.0.lock().unwrap().sectors.clone();let p=crate::onboard_macro_plan::prepare(&d,&m,&long_macro_request(&d)).unwrap();m.0.lock().unwrap().fault=Some(fault);let error=crate::onboard_macro_plan::commit(&d,&m,&p).unwrap_err().to_string();assert!(error.contains("restored/verified"),"{fault:?}: {error}");assert_eq!(m.0.lock().unwrap().sectors,before,"{fault:?}");}
}
#[test]fn a_unit_change_on_the_second_macro_page_never_rebinds_or_restores_to_the_other_unit(){
 let m=planning_memory();let d=unit_device(&m);let original=m.0.lock().unwrap().sectors[&1].clone();let p=crate::onboard_macro_plan::prepare(&d,&m,&long_macro_request(&d)).unwrap();{let mut state=m.0.lock().unwrap();state.fault=Some((8,2));state.change_unit_on_fault=true;}let error=crate::onboard_macro_plan::commit(&d,&m,&p).unwrap_err().to_string();assert!(error.contains("state uncertain"));let state=m.0.lock().unwrap();assert_eq!(state.sectors[&1],original);assert_eq!(state.calls.iter().filter(|(f,_)|*f==6).count(),2);assert!(!state.calls.iter().any(|(f,_)|*f==3));
}

#[test]
#[ignore="requires current long native macro composer UI request"]
fn browser_long_native_macro_request_uses_production_cross_page_allocation_and_readback(){
 let path=std::env::var_os("OPEN_HUB_LOGITECH_LONG_MACRO_UI_REQUEST").unwrap();let request:crate::onboard_macro_plan::Request=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(request.definition.steps.len(),43);let m=planning_memory();let d=unit_device(&m);let original=m.0.lock().unwrap().sectors.clone();let prepared=crate::onboard_macro_plan::prepare(&d,&m,&request).unwrap();assert_eq!(prepared.preview().page_count,2);crate::onboard_macro_plan::commit(&d,&m,&prepared).unwrap();let mut selection=request.selection;selection.expected=[0,5,0,0];let tr=crate::onboard_macros::read(&d,&m,&selection).unwrap();assert!(tr.complete);assert_eq!(tr.steps.len(),87);assert!(tr.steps.iter().any(|s|s.address.offset==249&&s.bytes==[0x60,0,6,0,0]));let actual=m.0.lock().unwrap();for sector in [0,2,3,4,7]{assert_eq!(actual.sectors[&sector],original[&sector]);}
}

#[test]fn same_model_same_unit_new_connection_rejects_every_old_preview_before_any_hid_io(){
 let m=planning_memory();let d=unit_device(&m);let binding=unit_settings(&m,&d);let dpi=dpi_settings(&d);let selection=macro_selection(&d);let request=macro_request(&d);let prepared=crate::onboard_macro_plan::prepare(&d,&m,&request).unwrap();let mut reconnected=d.clone();reconnected.connection_key="connection:reconnected".into();{let mut state=m.0.lock().unwrap();state.calls.clear();state.identity_reads=0;}
 assert!(reconnected.write_slot_bindings(&m,&binding).is_err());assert!(reconnected.write_slot_settings(&m,&dpi).is_err());assert!(crate::onboard_macros::read(&reconnected,&m,&selection).is_err());assert!(crate::onboard_macro_plan::prepare(&reconnected,&m,&request).is_err());assert!(crate::onboard_macro_plan::commit(&reconnected,&m,&prepared).is_err());let state=m.0.lock().unwrap();assert!(state.calls.is_empty());assert_eq!(state.identity_reads,0);
}
#[test]fn a_fresh_preview_on_reconnection_keeps_the_same_durable_profile_owner(){
 let m=planning_memory();let mut d=unit_device(&m);let owner=d.profile_key.clone();d.connection_key="connection:reconnected".into();let(snapshot,_)=d.onboard_snapshot(&m).unwrap();assert_eq!(snapshot.device_key,owner);assert_eq!(snapshot.connection_key,d.connection_key);let request=macro_request(&d);let prepared=crate::onboard_macro_plan::prepare(&d,&m,&request).unwrap();crate::onboard_macro_plan::commit(&d,&m,&prepared).unwrap();assert_eq!(d.profile_key,owner);
}

#[test]fn flash_cli_never_selects_the_first_of_two_mice_or_a_receiver_keyboard_or_other_brand(){
 let a=device();let mut b=a.clone();b.id="second-mouse".into();assert!(crate::onboard_bindings::sole_cli_target(&[a.clone(),b]).is_err());let mut keyboard=a.clone();keyboard.kind="Keyboard";let mut receiver=a.clone();receiver.index=1;let mut other=a.clone();other.vendor_id=0x3837;assert!(crate::onboard_bindings::sole_cli_target(&[keyboard.clone(),receiver.clone(),other.clone()]).is_err());assert_eq!(crate::onboard_bindings::sole_cli_target(&[keyboard,a.clone(),receiver,other]).unwrap(),a.id);
}
#[test]
#[ignore="requires current reconnection-preserved macro UI request"]
fn browser_reconnected_macro_draft_preserves_definition_but_uses_only_the_fresh_connection(){
 let path=std::env::var_os("OPEN_HUB_LOGITECH_RECONNECTED_MACRO_UI_REQUEST").unwrap();let request:crate::onboard_macro_plan::Request=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();let m=planning_memory();let mut d=unit_device(&m);d.connection_key="connection:reconnected".into();assert_eq!(request.selection.expected_connection,d.connection_key);assert_eq!(request.definition.name,"重连保留的宏草稿");let prepared=crate::onboard_macro_plan::prepare(&d,&m,&request).unwrap();crate::onboard_macro_plan::commit(&d,&m,&prepared).unwrap();let mut selection=request.selection;selection.expected=[0,5,0,0];let trace=crate::onboard_macros::read(&d,&m,&selection).unwrap();assert!(trace.complete);assert_eq!(trace.steps[0].action,crate::onboard_macros::Action::KeyDown{usage:6,modifiers:17});
}

fn restore_file()->serde_json::Value{
 let m=planning_memory();let d=unit_device(&m);{let mut state=m.0.lock().unwrap();let mut p=state.sectors[&1].clone();p[0]=2;p[0xa0..0xd0].fill(0);for(at,u)in "备份桌面".encode_utf16().enumerate(){p[0xa0+at*2..0xa2+at*2].copy_from_slice(&u.to_le_bytes());}state.sectors.insert(1,crc(p));let mut p=state.sectors[&2].clone();p[0x0d]=42;p[0xa0..0xd0].fill(0);for(at,u)in "备份游戏".encode_utf16().enumerate(){p[0xa0+at*2..0xa2+at*2].copy_from_slice(&u.to_le_bytes());}state.sectors.insert(2,crc(p));let mut dir=state.sectors[&0].clone();dir[6]=0;state.sectors.insert(0,crc(dir));}
 let(snapshot,mut raw)=d.onboard_snapshot(&m).unwrap();raw.push(RawSector{sector:3,crc_valid:Some(true),hex:hex(&crc(vec![0x55;256]))});
 serde_json::json!({"format":"open-ghub-onboard-backup/1","model_ids":["c08b"],"snapshot":snapshot,"sectors":raw})
}
#[test]fn scoped_restore_uses_model_and_layout_and_only_directory_referenced_profile_pages(){
 let file=restore_file();let m=planning_memory();let d=unit_device(&m);let before=m.0.lock().unwrap().sectors.clone();let info=d.onboard_info(&m).unwrap();let sectors=scoped_restore(&file,&d.model_ids,&info).unwrap();assert_eq!(sectors.iter().map(|(s,_)|*s).collect::<Vec<_>>(),vec![1,2,0]);assert_eq!(d.restore_sectors(&m,&sectors).unwrap(),3);
 let state=m.0.lock().unwrap();assert_eq!(state.sectors[&1][0],2);assert_eq!(state.sectors[&2][0x0d],42);assert_eq!(state.sectors[&0][6],0);for sector in [3,4,5,6,7]{assert_eq!(state.sectors[&sector],before[&sector]);}
 assert_eq!(state.calls.iter().filter(|(f,_)|*f==6).map(|(_,p)|u16::from_be_bytes([p[0],p[1]])).collect::<Vec<_>>(),vec![1,2,0]);
}
#[test]fn restore_rejects_wrong_missing_or_malformed_model_tags_and_memory_layout(){
 let m=memory();let d=device();let info=d.onboard_info(&m).unwrap();
 for tag in [serde_json::json!([]),serde_json::json!(["C09D"]),serde_json::json!(["GGGG"]),serde_json::json!([1]),serde_json::json!(["FFFF"])]{let mut file=restore_file();file["model_ids"]=tag;assert!(scoped_restore(&file,&d.model_ids,&info).is_err());}
 for field in ["memory_model","profile_format","macro_format","profile_count","button_count","sector_count","sector_size","mechanical_layout"]{let mut file=restore_file();file["snapshot"]["info"][field]=serde_json::json!(255);assert!(scoped_restore(&file,&d.model_ids,&info).is_err(),"{field}");}
 let mut file=restore_file();file.as_object_mut().unwrap().remove("model_ids");assert!(scoped_restore(&file,&d.model_ids,&info).is_err());let mut file=restore_file();file["format"]=serde_json::json!("diagnostic-dump");assert!(scoped_restore(&file,&d.model_ids,&info).is_err());
}
#[test]fn restore_rejects_ambiguous_incomplete_and_corrupt_sector_containers(){
 let m=memory();let d=device();let info=d.onboard_info(&m).unwrap();
 let mut file=restore_file();let duplicate=file["sectors"][1].clone();file["sectors"].as_array_mut().unwrap().push(duplicate);assert!(scoped_restore(&file,&d.model_ids,&info).is_err());
 let mut file=restore_file();file["sectors"].as_array_mut().unwrap().retain(|s|s["sector"]!=2);assert!(scoped_restore(&file,&d.model_ids,&info).is_err());
 let mut file=restore_file();file["sectors"][0]["crc_valid"]=serde_json::json!(false);assert!(scoped_restore(&file,&d.model_ids,&info).is_err());
 let mut file=restore_file();file["sectors"][1]["hex"]=serde_json::json!("00".repeat(256));assert!(scoped_restore(&file,&d.model_ids,&info).is_err());
}
#[test]fn restore_core_rejects_late_invalid_inputs_and_reads_all_preimages_before_first_write(){
 for case in 0..3{let m=memory();let d=device();let good=crc(vec![0xff;256]);let mut bad=good.clone();bad[9]^=1;let sectors=match case{0=>vec![(8,good)],1=>vec![(1,good.clone()),(1,good)],_=>vec![(1,good),(2,bad)]};assert!(d.restore_sectors(&m,&sectors).is_err());assert!(!m.0.lock().unwrap().calls.iter().any(|(f,_)|matches!(f,6|7|8)));}
 let m=memory();let d=device();let good=crc(vec![0xff;256]);m.0.lock().unwrap().fault=Some((5,17));assert!(d.restore_sectors(&m,&[(1,good.clone()),(2,good)]).is_err());assert!(!m.0.lock().unwrap().calls.iter().any(|(f,_)|matches!(f,6|7|8)));
}

#[test]fn native_profile_names_decode_utf16_without_guessing_or_replacing_opaque_bytes(){
 for name in ["协议模拟配置","桌面 🎮","Open HUB"]{let mut bytes=vec![0u8;48];for(at,u)in name.encode_utf16().enumerate(){bytes[at*2..at*2+2].copy_from_slice(&u.to_le_bytes());}assert_eq!(decode_name(&bytes).as_deref(),Some(name));}
 assert_eq!(decode_name(&[255;48]),None);assert_eq!(decode_name(&[0;48]),None);assert_eq!(decode_name(&[0,0xd8,0,0]),None);
}
#[test]fn restore_preview_decodes_verified_raw_profiles_and_pins_original_file_bytes(){
 assert_eq!(backup_digest(b"abc"),"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
 let m=planning_memory();let d=unit_device(&m);let info=d.onboard_info(&m).unwrap();let mut file=restore_file();file["snapshot"]["profiles"][0]["name"]=serde_json::json!("untrusted metadata name");let bytes=serde_json::to_vec_pretty(&file).unwrap();let digest=backup_digest(&bytes);let preview=restore_preview(&file,&d.model_ids,&info,&d.name,digest.clone()).unwrap();assert_eq!(preview.digest,digest);assert_eq!(preview.profiles[0].name.as_deref(),Some("备份桌面"));assert_eq!(preview.profiles[1].name.as_deref(),Some("备份游戏"));assert_eq!(preview.profiles[0].report_rate_hz,500);assert!(!preview.profiles[1].enabled);assert!(preview.macro_references>0);
 let mut changed=bytes.clone();changed.push(b' ');assert_ne!(backup_digest(&changed),digest);assert_eq!(serde_json::from_slice::<serde_json::Value>(&changed).unwrap(),file);
}
#[test]
#[ignore="requires current backup restore preview browser request"]
fn browser_reviewed_restore_request_uses_selected_unit_connection_and_exact_backup_bytes(){
 let path=std::env::var_os("OPEN_HUB_LOGITECH_RESTORE_UI_REQUEST").unwrap();let request:serde_json::Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();let m=memory();let d=device();d.verify_native_preview(&m,request["expectedDevice"].as_str().unwrap(),request["expectedConnection"].as_str().unwrap()).unwrap();assert_eq!(request["id"],d.id);let file=restore_file();let digest=backup_digest(&serde_json::to_vec_pretty(&file).unwrap());assert_eq!(request["expectedBackupDigest"],digest);let plan=scoped_restore(&file,&d.model_ids,&d.onboard_info(&m).unwrap()).unwrap();assert_eq!(d.restore_sectors(&m,&plan).unwrap(),3);assert_eq!(d.onboard_snapshot(&m).unwrap().0.profiles[0].name.as_deref(),Some("备份桌面"));
}

#[test]fn all_fifty_four_uncertain_restore_write_stages_put_back_every_original_selected_sector(){
 for fault in [(6,1),(6,2),(6,3),(8,1),(8,2),(8,3)].into_iter().chain((1..=48).map(|n|(7,n))){
  let m=planning_memory();let d=unit_device(&m);let before=m.0.lock().unwrap().sectors.clone();let info=d.onboard_info(&m).unwrap();let plan=scoped_restore(&restore_file(),&d.model_ids,&info).unwrap();let prepared=crate::onboard_restore::prepare(&d,&m,&info,&plan).unwrap();m.0.lock().unwrap().fault=Some(fault);let err=crate::onboard_restore::commit(&d,&m,&prepared).unwrap_err().to_string();assert!(err.contains("original restore state restored and verified"),"{fault:?}: {err}");assert_eq!(m.0.lock().unwrap().sectors,before,"{fault:?}");
 }
}
#[test]fn restore_transaction_rechecks_reviewed_pages_descriptor_and_connection_before_first_write(){
 for case in 0..3{let m=planning_memory();let mut d=unit_device(&m);let info=d.onboard_info(&m).unwrap();let plan=scoped_restore(&restore_file(),&d.model_ids,&info).unwrap();let p=crate::onboard_restore::prepare(&d,&m,&info,&plan).unwrap();match case{0=>{let mut state=m.0.lock().unwrap();let mut b=state.sectors[&2].clone();b[17]^=1;state.sectors.insert(2,crc(b));},1=>m.0.lock().unwrap().descriptor_changed=true,_=>d.connection_key="connection:new".into()};assert!(crate::onboard_restore::commit(&d,&m,&p).is_err());assert!(!m.0.lock().unwrap().calls.iter().any(|(f,_)|matches!(f,6|7|8)));}
}
#[test]fn restore_transaction_never_overwrites_third_party_changes_or_a_replaced_unit(){
 for case in 0..3{let m=planning_memory();let d=unit_device(&m);let info=d.onboard_info(&m).unwrap();let plan=scoped_restore(&restore_file(),&d.model_ids,&info).unwrap();let p=crate::onboard_restore::prepare(&d,&m,&info,&plan).unwrap();{let mut s=m.0.lock().unwrap();s.fault=Some((8,2));match case{0=>s.change_unrelated_on_fault=true,1=>s.change_directory_on_fault=true,_=>s.change_unit_on_fault=true};}let err=crate::onboard_restore::commit(&d,&m,&p).unwrap_err().to_string();assert!(err.contains("state uncertain"),"{err}");let s=m.0.lock().unwrap();assert_eq!(s.calls.iter().filter(|(f,_)|*f==6).count(),2);}
}
#[test]fn restore_preimage_includes_newly_referenced_destination_and_preserves_untouched_pages(){
 let m=planning_memory();let d=unit_device(&m);let before=m.0.lock().unwrap().sectors.clone();let info=d.onboard_info(&m).unwrap();let mut dir=before[&0].clone();dir[1]=5;let plan=vec![(5,profile()),(2,before[&2].clone()),(0,crc(dir))];let p=crate::onboard_restore::prepare(&d,&m,&info,&plan).unwrap();let raw=p.raw();assert_eq!(raw.iter().map(|r|r.sector).collect::<Vec<_>>(),vec![0,2,5]);assert_eq!(parse_hex(&raw[2].hex).unwrap(),before[&5]);assert_eq!(crate::onboard_restore::commit(&d,&m,&p).unwrap(),2);let s=m.0.lock().unwrap();for sector in [1,2,3,4,6,7]{assert_eq!(s.sectors[&sector],before[&sector]);}
}

#[test]fn native_dpi_eighteen_uncertain_flash_stages_use_reviewed_state_and_restore_original(){
 for fault in [(6,1),(8,1)].into_iter().chain((1..=16).map(|n|(7,n))){let m=memory();let d=unit_device(&m);let before=m.0.lock().unwrap().sectors.clone();let p=d.prepare_slot_settings(&m,&dpi_settings(&d)).unwrap();m.0.lock().unwrap().fault=Some(fault);let e=d.commit_slot_settings(&m,p).unwrap_err().to_string();assert!(e.contains("original restore state restored and verified"),"{fault:?}: {e}");assert_eq!(m.0.lock().unwrap().sectors,before);}
}
#[test]fn native_dpi_save_rechecks_exact_preimage_and_never_restores_external_directory_changes(){
 let m=memory();let d=unit_device(&m);let p=d.prepare_slot_settings(&m,&dpi_settings(&d)).unwrap();assert_eq!(p.raw().iter().map(|r|r.sector).collect::<Vec<_>>(),vec![0,1]);{let mut s=m.0.lock().unwrap();s.fault=Some((8,1));s.change_directory_on_fault=true;}let e=d.commit_slot_settings(&m,p).unwrap_err().to_string();assert!(e.contains("state uncertain"));assert_eq!(m.0.lock().unwrap().calls.iter().filter(|(f,_)|*f==6).count(),1);
 let m=memory();let d=unit_device(&m);let p=d.prepare_slot_settings(&m,&dpi_settings(&d)).unwrap();m.0.lock().unwrap().descriptor_changed=true;assert!(d.commit_slot_settings(&m,p).is_err());assert!(!m.0.lock().unwrap().calls.iter().any(|(f,_)|matches!(f,6|7|8)));
}
#[test]fn dpi_and_restore_reload_only_current_enabled_board_slot_and_report_reload_failure_after_save(){
 for(mode,active,reload_fail)in [(2,1,false),(1,2,false),(1,1,false),(1,1,true)]{let m=memory();let d=unit_device(&m);{let mut s=m.0.lock().unwrap();s.mode=mode;s.active=active;s.reload_fail=reload_fail;}let r=d.write_slot_settings(&m,&dpi_settings(&d)).unwrap();assert_eq!(r.active_reloaded,mode==1&&active==1&&!reload_fail);assert_eq!(r.reload_note.is_some(),reload_fail);assert_eq!(m.0.lock().unwrap().sectors[&1][0],2);}
 for(mode,active,reload_fail)in [(2,1,false),(1,2,false),(1,1,false),(1,1,true)]{let m=planning_memory();let d=unit_device(&m);let info=d.onboard_info(&m).unwrap();let plan=scoped_restore(&restore_file(),&d.model_ids,&info).unwrap();let p=crate::onboard_restore::prepare(&d,&m,&info,&plan).unwrap();{let mut s=m.0.lock().unwrap();s.mode=mode;s.active=active;s.reload_fail=reload_fail;}assert_eq!(crate::onboard_restore::commit(&d,&m,&p).unwrap(),3);let result=crate::onboard_restore::reload_current(&d,&m,&p);if active==2||reload_fail{assert!(result.is_err());}else{assert_eq!(result.unwrap(),mode==1);}assert_eq!(m.0.lock().unwrap().calls.iter().filter(|(f,_)|*f==3).count(),usize::from(mode==1&&active==1));}
}
