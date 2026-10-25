//! Explicit one-shot persistence of an already applied exact native plan.
//! No automatic flash write, no saved host F13 mappings and no retry on uncertainty.
use super::*;
use std::{io::Write, path::Path};
#[derive(Debug, Serialize)]
pub struct Saved {
    pub command_sent: bool,
    pub readback_confirmed: bool,
    pub note: String,
}
fn complete(plan: &Plan) -> Result<(), String> {
    if !crate::steelseries::supported(plan.vendor, plan.product) {
        return Err("此型号尚未开放明确板载保存".into());
    }
    validate(plan)?;
    let compiled = compile(plan)?;
    if !compiled.bindings.is_empty() || compiled.software_input {
        return Err("软件宏由 Open HUB 执行，不能把临时触发映射保存到鼠标".into());
    }
    let buttons = plan
        .settings
        .iter()
        .filter(|s| matches!(s, Setting::Button { .. }))
        .cloned()
        .collect::<Vec<_>>();
    let dpi = plan
        .settings
        .iter()
        .filter(|s| matches!(s, Setting::Dpi(_)))
        .collect::<Vec<_>>();
    let rates = plan
        .settings
        .iter()
        .filter(|s| matches!(s, Setting::ReportRate(_)))
        .count();
    let zones = plan
        .settings
        .iter()
        .filter_map(|s| {
            if let Setting::Lighting { zone, .. } = s {
                Some(*zone)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if !crate::steelseries::complete_table(plan.product, &buttons)
        || dpi.len() != 1
        || !matches!(dpi[0],Setting::Dpi(d)if crate::steelseries::fixed_dpi_stages(plan.product).is_none_or(|n|d.stages.len()==n))
        || rates != 1
        || zones.len() != usize::from(crate::steelseries::zone_count(plan.product).unwrap())
        || !(0..crate::steelseries::zone_count(plan.product).unwrap()).all(|zone|zones.contains(&zone))
        || plan.settings.len() != buttons.len() + 2 + zones.len()
    {
        return Err("板载保存需要先应用此型号的完整 DPI、回报率、全部已知按键和所有灯区".into());
    }
    Ok(())
}
impl Service {
    pub fn save_native_hardware(
        &self,
        id: &str,
        plan: &Plan,
        expected_connection: &str,
    ) -> Result<Saved, String> {
        self.save_native_hardware_at(
            id,
            plan,
            expected_connection,
            &crate::macro_recovery::directory()?
                .parent()
                .ok_or("配置目录无效")?
                .to_path_buf(),
        )
    }
    fn save_native_hardware_at(
        &self,
        id: &str,
        plan: &Plan,
        expected_connection: &str,
        root: &Path,
    ) -> Result<Saved, String> {
        complete(plan)?;
        if expected_connection.is_empty() || expected_connection.len() > 512 {
            return Err("请重新读取当前鼠标连接后再保存".into());
        }
        self.with(|manager|{
   let device=manager.device_mut(raw_id(id)).ok_or("鼠标已断开")?;let d=device.descriptor();
   if (d.vendor_id,d.product_id)!=(plan.vendor,plan.product)||device.connection_key()!=Some(expected_connection){return Err("鼠标型号或控制连接已变化，未发送板载保存命令".into());}
   if self.1.lock().map_err(|_|"宏恢复记录锁异常")?.contains_key(raw_id(id)){return Err("此鼠标仍有软件动作或待恢复映射，请先停止并完成恢复".into());}
   let current=view(device.as_mut())?;
   // This audit records known template/session data, never claims a flash dump.
   let dir=root.join("backups");std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
   let path=dir.join(format!("steelseries-save-intent-{}.json",crate::profiles::new_id()));
   let bytes=serde_json::to_vec_pretty(&serde_json::json!({"format":"open-hub-native-save-intent/1","device":current.descriptor,"connection":expected_connection,"plan":plan,"state_kind":"write-only-session-cache; not a hardware flash backup"})).map_err(|e|e.to_string())?;
   let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e|e.to_string())?;file.write_all(&bytes).and_then(|_|file.sync_all()).map_err(|e|e.to_string())?;drop(file);
   self.take_hardware_save(&current,plan)?;
   device.save_onboard().map_err(|e|format!("板载保存未获完整发送结果，是否持久化未知；没有重试：{e}"))?;
   Ok(Saved{command_sent:true,readback_confirmed:false,note:"已发送此鼠标的板载保存命令。此型号无法回读闪存确认，软件宏没有写入鼠标。".into()})
  })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gearwise_core::{
        model::{DpiStage, DpiState, LightingEffect, MouseButton, Rgb, ZoneState},
        profile::{Macro, MacroMode, MacroStep},
        transport::{HidInterfaceInfo, MockBackend, MockRecord, MockReply, MockTransport},
        Device, Driver, HidTransport,
    };
    use std::sync::atomic::AtomicU8;
    struct Silent;
    impl gearwise_input::InputSink for Silent {
        fn key_down(&mut self, _: u8) -> gearwise_core::Result<()> {
            panic!("no OS or recorded playback expected")
        }
        fn key_up(&mut self, _: u8) -> gearwise_core::Result<()> {
            panic!("no OS or recorded playback expected")
        }
        fn mouse_down(&mut self, _: MouseButton) -> gearwise_core::Result<()> {
            panic!("unexpected mouse output")
        }
        fn mouse_up(&mut self, _: MouseButton) -> gearwise_core::Result<()> {
            panic!("unexpected mouse output")
        }
        fn scroll(&mut self, _: i32, _: i32) -> gearwise_core::Result<()> {
            panic!("unexpected scroll")
        }
        fn media(&mut self, _: u16) -> gearwise_core::Result<()> {
            panic!("unexpected media")
        }
    }
    struct SaveFault {
        inner: MockTransport,
        mode: Arc<AtomicU8>,
    }
    impl HidTransport for SaveFault {
        fn write(&mut self, data: &[u8]) -> gearwise_core::Result<usize> {
            let size = self.inner.write(data)?;
            if matches!(data, [0, 9, 0] | [0, 0x59, 0]) {
                match self.mode.load(Ordering::Acquire) {
                    1 => return Ok(size - 1),
                    2 => {
                        return Err(gearwise_core::Error::Unsupported(
                            "simulated uncertain save",
                        ))
                    }
                    _ => {}
                }
            }
            Ok(size)
        }
        fn read_timeout(&mut self, b: &mut [u8], t: i32) -> gearwise_core::Result<usize> {
            self.inner.read_timeout(b, t)
        }
        fn send_feature_report(&mut self, b: &[u8]) -> gearwise_core::Result<()> {
            self.inner.send_feature_report(b)
        }
        fn get_feature_report(&mut self, b: &mut [u8]) -> gearwise_core::Result<usize> {
            self.inner.get_feature_report(b)
        }
    }
    fn opened(pid: u16) -> (Box<dyn Device>, MockLog, Arc<AtomicU8>) {
        opened_at(pid, "")
    }
    fn opened_at(pid: u16, suffix: &str) -> (Box<dyn Device>, MockLog, Arc<AtomicU8>) {
        let iface = HidInterfaceInfo {
            path: format!("simulation:steel-persist:{pid:04x}{suffix}"),
            vendor_id: 0x1038,
            product_id: pid,
            interface_number: 0,
            usage_page: 0xffc0,
            usage: 1,
            serial: None,
            ..Default::default()
        };
        let inner = MockTransport::new(|_| MockReply::none());
        let log = inner.log();
        let mode = Arc::new(AtomicU8::new(0));
        let slot = Mutex::new(Some(SaveFault {
            inner,
            mode: mode.clone(),
        }));
        let backend = MockBackend::new(vec![iface.clone()], move |_| {
            Ok(Box::new(slot.lock().unwrap().take().unwrap()))
        });
        (
            gearwise_steelseries::SteelSeriesDriver::with_gap(std::time::Duration::ZERO)
                .open(&iface, &backend)
                .unwrap()
                .remove(0),
            log,
            mode,
        )
    }
    fn manager(device: Box<dyn Device>) -> DeviceManager {
        let mut m = DeviceManager::new(
            Box::new(MockBackend::new(vec![], |_| {
                panic!("no real HID permitted")
            })),
            vec![],
        );
        m.add_virtual(vec![device]);
        m
    }
    struct Case {
        service: Service,
        id: String,
        connection: String,
        log: MockLog,
        mode: Arc<AtomicU8>,
        runtime: crate::macro_runtime::Runtime,
        root: std::path::PathBuf,
        plan: Plan,
    }
    impl Case {
        fn new(pid: u16) -> Self {
            let (d, log, mode) = opened(pid);
            let id = d.descriptor().id.clone();
            let connection = d.connection_key().unwrap().to_string();
            let settings = vec![
                Setting::Dpi(DpiState {
                    stages: vec![DpiStage::uniform(800), DpiStage::uniform(1600)],
                    active: 0,
                }),
                Setting::ReportRate(500),
            ];
            let mut plan = Plan {
                vendor: 0x1038,
                product: pid,
                settings,
                macros: vec![],
                source: None,
            };
            plan.settings.extend(
                d.capabilities()
                    .buttons
                    .iter()
                    .filter(|b| b.id != 1)
                    .map(|b| Setting::Button {
                        id: b.id,
                        action: b.default_action.clone(),
                    }),
            );
            for zone in 0..crate::steelseries::zone_count(pid).unwrap() {
                plan.settings.push(Setting::Lighting {
                    zone,
                    state: ZoneState {
                        effect: LightingEffect::Static {
                            color: Rgb {
                                r: 10 + zone,
                                g: 20,
                                b: 30,
                            },
                        },
                        brightness: 80,
                    },
                });
            }
            if pid==0x1832 {
                plan.settings.retain(|s|!matches!(s,Setting::Dpi(_)));
                plan.settings.push(Setting::Dpi(DpiState{stages:[50,800,1200,2400,18000].into_iter().map(DpiStage::uniform).collect(),active:4}));
            }
            let service = Service::default();
            *service.0.lock().unwrap() = Some(manager(d));
            let runtime = crate::macro_runtime::Runtime::simulation(Arc::new(Mutex::new(Silent)));
            Self {
                service,
                id,
                connection,
                log,
                mode,
                runtime,
                root: std::env::temp_dir()
                    .join(format!("open-hub-steel-save-{}", crate::profiles::new_id())),
                plan,
            }
        }
        fn apply(&self) -> View {
            self.service
                .apply_at(&self.id, &self.plan, &self.runtime, &self.root, None)
                .unwrap()
        }
        fn save(&self) -> Result<Saved, String> {
            self.service
                .save_native_hardware_at(&self.id, &self.plan, &self.connection, &self.root)
        }
        fn saves(&self) -> usize {
            self.log.lock().unwrap().iter().filter(|r|matches!(r,MockRecord::Write(bytes)if matches!(bytes.as_slice(),[0,9,0]|[0,0x59,0]))).count()
        }
    }
    impl Drop for Case {
        fn drop(&mut self) {
            self.runtime.shutdown();
            for name in ["backups", "recovery"] {
                let dir = self.root.join(name);
                if let Ok(files) = std::fs::read_dir(&dir) {
                    for file in files {
                        let p = file.unwrap().path();
                        assert!(p.is_file());
                        std::fs::remove_file(p).unwrap();
                    }
                    std::fs::remove_dir(dir).unwrap();
                }
            }
            if self.root.exists() {
                std::fs::remove_dir(&self.root).unwrap();
            }
        }
    }
    #[test]
    fn exact_five_models_save_only_after_complete_apply_and_one_explicit_native_command() {
        let mut fixtures = Vec::new();
        for pid in crate::steelseries::PRODUCTS {
            let c = Case::new(pid);
            assert!(c.save().unwrap_err().contains("先应用"));
            assert_eq!(c.saves(), 0);
            let applied = c.apply();
            fixtures.push(serde_json::to_value(&applied).unwrap());
            assert_eq!(c.saves(), 0);
            let result = c.save().unwrap();
            assert!(result.command_sent && !result.readback_confirmed);
            assert_eq!(c.saves(), 1);
            assert_eq!(
                c.log.lock().unwrap().last().unwrap(),
                &MockRecord::Write(if matches!(pid,0x1710|0x1724) {
                    vec![0, 9, 0]
                } else {
                    vec![0, 0x59, 0]
                })
            );
            assert!(c.save().unwrap_err().contains("已经发出"));
            assert_eq!(c.saves(), 1);
        }
        if let Some(path) = std::env::var_os("OPEN_HUB_STEELSERIES_SAVE_FIXTURE") {
            std::fs::write(path, serde_json::to_vec_pretty(&fixtures).unwrap()).unwrap();
        }
    }
    #[test]
    fn sensei_ten_complete_five_stage_plan_persists_current_stage_once_and_rejects_invalid_sixth() {
        let mut c=Case::new(0x1832);
        c.plan.settings.retain(|s|!matches!(s,Setting::Dpi(_)));
        c.plan.settings.push(Setting::Dpi(DpiState{stages:[50,800,1200,2400,18000].into_iter().map(DpiStage::uniform).collect(),active:4}));
        assert!(c.save().is_err());assert_eq!(c.saves(),0);c.apply();
        let result=c.save().unwrap();assert!(result.command_sent&&!result.readback_confirmed);assert_eq!(c.saves(),1);assert!(c.save().is_err());assert_eq!(c.saves(),1);
        if let Some(Setting::Dpi(d))=c.plan.settings.iter_mut().find(|s|matches!(s,Setting::Dpi(_))){d.stages.push(DpiStage::uniform(800));}
        assert!(c.save().is_err());assert_eq!(c.saves(),1);
    }
    #[test]
    fn missing_duplicate_invalid_or_host_macro_plan_cannot_emit_any_save() {
        let c = Case::new(0x1722);
        c.apply();
        c.log.lock().unwrap().clear();
        for case in 0..8 {
            let mut p = c.plan.clone();
            match case {
                0 => {
                    p.settings.pop();
                }
                1 => {
                    p.settings.push(p.settings[2].clone());
                }
                2 => {
                    if let Setting::Dpi(d) = &mut p.settings[0] {
                        d.stages.pop();
                    }
                }
                3 => {
                    p.settings[2] = Setting::Button {
                        id: 2,
                        action: ButtonAction::Macro("host".into()),
                    };
                    p.macros.push(Macro {
                        id: "host".into(),
                        name: "host".into(),
                        mode: MacroMode::Once,
                        steps: vec![MacroStep::KeyDown(6), MacroStep::KeyUp(6)],
                    });
                }
                4 => p.product = 0x1724,
                5 => p.vendor = 0x1532,
                6 => {
                    if let Setting::ReportRate(rate) = &mut p.settings[1] {
                        *rate = 123;
                    }
                }
                _ => {
                    if let Some(Setting::Lighting { zone, .. }) = p.settings.last_mut() {
                        *zone = 0;
                    }
                }
            }
            assert!(
                c.service
                    .save_native_hardware_at(&c.id, &p, &c.connection, &c.root)
                    .is_err(),
                "case {case}"
            );
            assert!(c.log.lock().unwrap().is_empty());
        }
    }
    #[test]
    fn stale_connection_reopened_same_path_draft_or_cached_state_changes_reject_before_save() {
        let c = Case::new(0x1722);
        c.apply();
        c.log.lock().unwrap().clear();
        for connection in ["", "stale"] {
            assert!(c
                .service
                .save_native_hardware_at(&c.id, &c.plan, connection, &c.root)
                .is_err());
        }
        let mut draft = c.plan.clone();
        draft.settings[1] = Setting::ReportRate(250);
        assert!(c
            .service
            .save_native_hardware_at(&c.id, &draft, &c.connection, &c.root)
            .is_err());
        assert!(c.log.lock().unwrap().is_empty());
        c.service
            .with(|m| {
                m.device_mut(&c.id)
                    .unwrap()
                    .apply(&Setting::ReportRate(250))
                    .map_err(|e| e.to_string())
            })
            .unwrap();
        c.log.lock().unwrap().clear();
        assert!(c.save().unwrap_err().contains("状态已变化"));
        assert!(c.log.lock().unwrap().is_empty());
        let (d, log, _) = opened(0x1722);
        let new_connection = d.connection_key().unwrap().to_string();
        assert_ne!(new_connection, c.connection);
        *c.service.0.lock().unwrap() = Some(manager(d));
        assert!(c.save().is_err());
        assert!(c
            .service
            .save_native_hardware_at(&c.id, &c.plan, &new_connection, &c.root)
            .is_err());
        assert!(log.lock().unwrap().is_empty());
    }
    #[test]
    fn active_or_stopped_unrecovered_host_triggers_are_never_written_to_flash() {
        let c = Case::new(0x1722);
        c.apply();
        let mut p = c.plan.clone();
        p.source = Some(crate::host_macros::Source {
            vendor: p.vendor,
            product: p.product,
            path: "simulation:flash-host".into(),
        });
        p.settings[2] = Setting::Button {
            id: 2,
            action: ButtonAction::Macro("host".into()),
        };
        p.macros.push(Macro {
            id: "host".into(),
            name: "host".into(),
            mode: MacroMode::Once,
            steps: vec![MacroStep::KeyDown(6), MacroStep::KeyUp(6)],
        });
        c.service
            .apply_at(&c.id, &p, &c.runtime, &c.root, None)
            .unwrap();
        c.log.lock().unwrap().clear();
        assert!(c.save().unwrap_err().contains("软件动作"));
        assert!(c.log.lock().unwrap().is_empty());
        assert_eq!(c.runtime.status().source_count, 1);
        c.service.stop_device_software(&c.id, &c.runtime).unwrap();
        c.log.lock().unwrap().clear();
        assert!(c.save().unwrap_err().contains("软件宏"));
        assert!(c.log.lock().unwrap().is_empty());
        c.apply();
        c.save().unwrap();
        assert_eq!(c.saves(), 1);
    }
    #[test]
    fn uncertain_short_or_error_save_is_sent_once_and_requires_new_apply_before_retry() {
        for mode in [1, 2] {
            let c = Case::new(0x1720);
            c.apply();
            c.log.lock().unwrap().clear();
            c.mode.store(mode, Ordering::Release);
            let error = c.save().unwrap_err();
            assert!(error.contains("未知") && error.contains("没有重试"));
            assert_eq!(c.saves(), 1);
            assert!(c.save().unwrap_err().contains("已经发出"));
            assert_eq!(c.saves(), 1);
            c.mode.store(0, Ordering::Release);
            c.apply();
            c.save().unwrap();
            assert_eq!(c.saves(), 2);
        }
    }
    #[test]
    fn saving_another_same_model_mouse_never_stops_or_persists_the_first_mouses_host_route() {
        let c = Case::new(0x1722);
        let mut host = c.plan.clone();
        host.source = Some(crate::host_macros::Source {
            vendor: host.vendor,
            product: host.product,
            path: "simulation:host-A".into(),
        });
        host.settings[2] = Setting::Button {
            id: 2,
            action: ButtonAction::Macro("host".into()),
        };
        host.macros.push(Macro {
            id: "host".into(),
            name: "host".into(),
            mode: MacroMode::Once,
            steps: vec![MacroStep::KeyDown(6), MacroStep::KeyUp(6)],
        });
        c.service
            .apply_at(&c.id, &host, &c.runtime, &c.root, None)
            .unwrap();
        let (b, log, _) = opened_at(0x1722, ":B");
        let id = b.descriptor().id.clone();
        let connection = b.connection_key().unwrap().to_owned();
        c.service
            .with(|m| {
                m.add_virtual(vec![b]);
                Ok(())
            })
            .unwrap();
        c.service
            .apply_at(&id, &c.plan, &c.runtime, &c.root, None)
            .unwrap();
        c.log.lock().unwrap().clear();
        log.lock().unwrap().clear();
        c.service
            .save_native_hardware_at(&id, &c.plan, &connection, &c.root)
            .unwrap();
        assert_eq!(
            log.lock().unwrap().as_slice(),
            &[MockRecord::Write(vec![0, 0x59, 0])]
        );
        assert!(c.log.lock().unwrap().is_empty());
        assert_eq!(c.runtime.status().source_count, 1);
        assert!(
            c.service
                .read(&c.id, &c.runtime)
                .unwrap()
                .session
                .unwrap()
                .software_state
                == "running"
        );
    }
    #[test]
    fn audit_file_failure_prevents_save_and_does_not_consume_the_applied_sessions_one_shot() {
        let c = Case::new(0x1710);
        c.apply();
        let not_directory = c.root.join("file-instead-of-directory");
        std::fs::write(&not_directory, b"not a directory").unwrap();
        assert!(c
            .service
            .save_native_hardware_at(&c.id, &c.plan, &c.connection, &not_directory)
            .is_err());
        assert_eq!(c.saves(), 0);
        std::fs::remove_file(not_directory).unwrap();
        c.save().unwrap();
        assert_eq!(c.saves(), 1);
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct BrowserRequest {
        id: String,
        plan: Plan,
        expected_connection: String,
    }
    #[test]
    #[ignore = "requires current exact hardware save UI requests and native DTOs"]
    fn browser_hardware_save_requests_carry_the_reviewed_connection_reject_stale_reopen_and_execute_after_fresh_apply(
    ) {
        let bytes =
            std::fs::read(std::env::var_os("OPEN_HUB_STEELSERIES_UI_SAVE_REQUESTS").unwrap())
                .unwrap();
        let requests: Vec<BrowserRequest> = serde_json::from_slice(&bytes).unwrap();
        let fixtures: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var_os("OPEN_HUB_STEELSERIES_SAVE_FIXTURE").unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(requests.len(), 5);
        for request in requests {
            let pid = request.plan.product;
            let original = fixtures
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["descriptor"]["product_id"] == pid)
                .unwrap();
            assert_eq!(
                request.expected_connection,
                original["write_connection"].as_str().unwrap()
            );
            let c = Case::new(pid);
            assert_eq!(raw_id(&request.id), c.id);
            assert_ne!(request.expected_connection, c.connection);
            c.service
                .apply_at(&c.id, &request.plan, &c.runtime, &c.root, None)
                .unwrap();
            c.log.lock().unwrap().clear();
            assert!(c
                .service
                .save_native_hardware_at(
                    &request.id,
                    &request.plan,
                    &request.expected_connection,
                    &c.root
                )
                .is_err());
            assert!(c.log.lock().unwrap().is_empty());
            let fresh = c.service.read(&c.id, &c.runtime).unwrap();
            let saved = c
                .service
                .save_native_hardware_at(
                    &request.id,
                    &request.plan,
                    fresh.connection_key.as_deref().unwrap(),
                    &c.root,
                )
                .unwrap();
            assert!(saved.command_sent && !saved.readback_confirmed);
            assert_eq!(
                c.log.lock().unwrap().as_slice(),
                &[MockRecord::Write(if matches!(pid,0x1710|0x1724) {
                    vec![0, 9, 0]
                } else {
                    vec![0, 0x59, 0]
                })]
            );
            assert!(
                c.service
                    .read(&c.id, &c.runtime)
                    .unwrap()
                    .session
                    .unwrap()
                    .hardware_save_attempted
            );
        }
    }
}
