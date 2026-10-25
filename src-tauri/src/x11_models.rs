use super::*;
use gearwise_core::{transport::MockBackend, Device};
struct Unit {
    inner: Box<dyn Device>,
    desc: DeviceDescriptor,
}
impl Device for Unit {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }
    fn capabilities(&self) -> &Capabilities {
        self.inner.capabilities()
    }
    fn read_state(&mut self) -> gearwise_core::Result<DeviceState> {
        self.inner.read_state()
    }
    fn apply(&mut self, s: &Setting) -> gearwise_core::Result<()> {
        self.inner.apply(s)
    }
}
#[test]
fn x11_service_applies_to_one_mouse_and_keeps_atomic_session_without_global_routing() {
    let mut manager = DeviceManager::new(
        Box::new(MockBackend::new(vec![], |_| panic!("no physical access"))),
        vec![],
    );
    let mut ids = vec![];
    let mut logs = vec![];
    for unit in ["A", "B"] {
        let (inner, log) = crate::x11::simulated(crate::x11::WIRED).unwrap();
        let mut desc = inner.descriptor().clone();
        desc.id = format!("x11:unit:{unit}");
        desc.serial = Some(unit.into());
        ids.push(desc.id.clone());
        logs.push(log);
        manager.add_virtual(vec![Box::new(Unit { inner, desc })]);
    }
    let service = Service::default();
    *service.0.lock().unwrap() = Some(manager);
    let runtime = crate::macro_runtime::Runtime::default();
    let root = std::env::temp_dir().join(format!("x11-service-{}", crate::profiles::new_id()));
    let mut x = crate::x11::Profile::example();
    x.dpi.stages[0] = 400;
    x.preferences.light = crate::x11::Light::Neon;
    let plan = Plan {
        vendor: crate::x11::VID,
        product: crate::x11::WIRED,
        settings: vec![Setting::VendorProfile {
            schema: crate::x11::SCHEMA.into(),
            data: serde_json::to_value(&x).unwrap(),
        }],
        macros: vec![],
        source: None,
    };
    let result = service
        .apply_at(&ids[0], &plan, &runtime, &root, None)
        .unwrap();
    assert_eq!(result.session.unwrap().plan, plan);
    assert_eq!(
        result.state.vendor_profile,
        Some(serde_json::to_value(&x).unwrap())
    );
    assert_eq!(runtime.status().source_count, 0);
    assert_eq!(logs[0].lock().unwrap().len(), 4);
    assert!(logs[1].lock().unwrap().is_empty());
    let other = service.read(&ids[1], &runtime).unwrap();
    assert!(other.session.is_none());
    assert_eq!(
        other.state.vendor_profile,
        Some(serde_json::to_value(crate::x11::Profile::example()).unwrap())
    );
    let reread = service.read(&ids[0], &runtime).unwrap();
    assert_eq!(reread.session.unwrap().plan, plan);
    let mut wrong = plan.clone();
    wrong.product = crate::x11::WIRELESS;
    assert!(service
        .apply_at(&ids[1], &wrong, &runtime, &root, None)
        .is_err());
    assert!(logs[1].lock().unwrap().is_empty());
    assert!(choose_profile_device(&ids, None).is_err());
    assert_eq!(choose_profile_device(&ids, Some(&ids[1])).unwrap(), ids[1]);
    runtime.shutdown();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]fn all_native_templates_still_generate_executable_self_test_plans(){for view in templates().unwrap(){let plan=template_plan(&view).unwrap();let result=simulate(&plan).unwrap();assert!(result["packet_count"].as_u64().unwrap()>0);}}
