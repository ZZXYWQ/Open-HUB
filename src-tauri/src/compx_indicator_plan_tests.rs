use super::*;
use gearwise_core::{
    model::{DpiIndicatorMode, DpiIndicatorState, DpiStage, DpiState, IndicatorLevel},
    Device,
};
fn desired() -> DpiIndicatorState {
    DpiIndicatorState {
        enabled: false,
        mode: DpiIndicatorMode::Breathing,
        brightness: IndicatorLevel::High,
        speed: IndicatorLevel::Low,
    }
}
fn plan() -> Plan {
    Plan {
        vendor: 0x373b,
        product: 0x1017,
        settings: vec![
            Setting::DpiIndicator(desired()),
            Setting::DpiColor {
                stage: 7,
                color: gearwise_core::model::Rgb {
                    r: 17,
                    g: 34,
                    b: 51,
                },
            },
            Setting::Dpi(DpiState {
                stages: vec![DpiStage::uniform(800); 8],
                active: 7,
            }),
        ],
        macros: vec![],
        source: None,
    }
}
#[test]
fn canonical_indicator_profile_roundtrips_and_follows_dpi_colors() {
    let p = plan();
    let loaded: Plan = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
    assert_eq!(loaded, p);
    let c = compile(&loaded).unwrap();
    assert!(matches!(c.settings[0], Setting::Dpi(_)));
    assert!(matches!(c.settings[1], Setting::DpiColor { .. }));
    assert!(matches!(c.settings[2], Setting::DpiIndicator(_)));
    assert!(!c.software_input && c.bindings.is_empty());
    assert!(simulate(&loaded).is_ok());
}
#[test]
fn duplicate_mixed_legacy_unknown_levels_and_missing_fields_reject() {
    let mut p = plan();
    p.settings.push(p.settings[0].clone());
    assert!(validate(&p).is_err());
    p = plan();
    p.settings.push(Setting::Lighting {
        zone: 0,
        state: gearwise_core::model::ZoneState {
            effect: gearwise_core::model::LightingEffect::Off,
            brightness: 100,
        },
    });
    assert!(validate(&p).is_err());
    for value in [
        r#"{"enabled":true,"mode":"Breathing","brightness":"VeryHigh","speed":"Low"}"#,
        r#"{"enabled":true,"mode":"Wave","brightness":"High","speed":"Low"}"#,
        r#"{"enabled":true,"mode":"Breathing","brightness":"High"}"#,
        r#"{"enabled":true,"mode":"Breathing","brightness":"High","speed":"Low","period_ms":4000}"#,
    ] {
        assert!(serde_json::from_str::<DpiIndicatorState>(value).is_err());
    }
}
#[test]
fn exact_model_and_live_capability_guards_reject_unverified_indicator() {
    for (vendor, product) in [
        (0x373b, 0x1115),
        (0x373b, 0x103f),
        (0x3554, 0xf507),
        (0x1532, 0x0084),
        (0x1038, 0x1720),
        (0x1b1c, 0x1b2e),
    ] {
        let p = Plan {
            vendor,
            product,
            settings: vec![Setting::DpiIndicator(desired())],
            macros: vec![],
            source: None,
        };
        assert!(validate(&p).is_err());
    }
    assert!(validate_capabilities(&plan(), &Capabilities::default()).is_err());
}
#[test]
fn indicator_session_refreshes_only_selected_parameters_and_keeps_unknown_pending() {
    let (mut d, _) = crate::atk::simulated(0x1017).unwrap();
    let mut out = view(d.as_mut()).unwrap();
    let service = Service::default();
    service.remember_session(&out, &plan(), false).unwrap();
    let live = out.state.dpi_indicator.unwrap();
    service
        .attach_session(&mut out, &crate::macro_runtime::Runtime::default())
        .unwrap();
    assert!(out
        .session
        .as_ref()
        .unwrap()
        .plan
        .settings
        .contains(&Setting::DpiIndicator(live)));
    out.state.dpi_indicator = None;
    service
        .attach_session(&mut out, &crate::macro_runtime::Runtime::default())
        .unwrap();
    assert!(out
        .session
        .as_ref()
        .unwrap()
        .plan
        .settings
        .contains(&Setting::DpiIndicator(desired())));
    let mut p = plan();
    p.settings
        .retain(|s| !matches!(s, Setting::DpiIndicator(_)));
    service.remember_session(&out, &p, false).unwrap();
    service
        .attach_session(&mut out, &crate::macro_runtime::Runtime::default())
        .unwrap();
    assert!(!out
        .session
        .as_ref()
        .unwrap()
        .plan
        .settings
        .iter()
        .any(|s| matches!(s, Setting::DpiIndicator(_))));
}
#[test]
fn legacy_dto_does_not_gain_indicator_fields_for_other_brands() {
    let c = serde_json::to_value(Capabilities::default()).unwrap();
    let s = serde_json::to_value(DeviceState::default()).unwrap();
    assert!(c.get("dpi_indicator").is_none());
    assert!(s.get("dpi_indicator").is_none());
    assert!(
        !serde_json::from_value::<Capabilities>(c)
            .unwrap()
            .dpi_indicator
    );
    assert!(serde_json::from_value::<DeviceState>(s)
        .unwrap()
        .dpi_indicator
        .is_none());
}
struct Unread {
    inner: Box<dyn Device>,
    definitions: Arc<std::sync::atomic::AtomicUsize>,
}
impl Device for Unread {
    fn descriptor(&self) -> &DeviceDescriptor {
        self.inner.descriptor()
    }
    fn capabilities(&self) -> &Capabilities {
        self.inner.capabilities()
    }
    fn read_state(&mut self) -> gearwise_core::Result<DeviceState> {
        let mut s = self.inner.read_state()?;
        s.dpi_indicator = None;
        Ok(s)
    }
    fn apply(&mut self, s: &Setting) -> gearwise_core::Result<()> {
        self.inner.apply(s)
    }
    fn set_macro_definitions(
        &mut self,
        m: &[gearwise_core::profile::Macro],
    ) -> gearwise_core::Result<()> {
        self.definitions.fetch_add(1, Ordering::Relaxed);
        self.inner.set_macro_definitions(m)
    }
}
#[test]
fn unknown_indicator_blocks_entire_service_before_any_setting_or_macro_allocation() {
    let (d, log) = crate::atk::simulated(0x1017).unwrap();
    let id = d.descriptor().id.clone();
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let device = Box::new(Unread {
        inner: d,
        definitions: count.clone(),
    });
    let mut manager = DeviceManager::new(
        Box::new(gearwise_core::transport::MockBackend::new(vec![], |_| {
            Err(gearwise_core::Error::Unsupported("no hardware"))
        })),
        vec![],
    );
    manager.add_virtual(vec![device]);
    let service = Service::default();
    *service.0.lock().unwrap() = Some(manager);
    let mut p = plan();
    p.settings.insert(0, Setting::ReportRate(500));
    log.lock().unwrap().clear();
    let root = std::env::temp_dir().join(format!("compx-indicator-{}", crate::profiles::new_id()));
    let error = service
        .apply_at(
            &id,
            &p,
            &crate::macro_runtime::Runtime::default(),
            &root,
            None,
        )
        .err()
        .expect("must reject unreadable indicator");
    assert!(error.contains("原始指示灯"), "{error}");
    assert_eq!(count.load(Ordering::Relaxed), 0);
    assert!(log
        .lock()
        .unwrap()
        .iter()
        .any(|r| matches!(r,gearwise_core::transport::MockRecord::Write(p)if p[1]==8)));
    assert!(!log
        .lock()
        .unwrap()
        .iter()
        .any(|r| matches!(r,gearwise_core::transport::MockRecord::Write(p)if p[1]==7)));
    assert!(!root.exists());
}
#[test]
fn export_indicator_ui_fixtures() {
    if let Some(path) = std::env::var_os("OPEN_HUB_COMPX_INDICATOR_FIXTURE") {
        let mut views = Vec::new();
        for m in crate::atk::models().iter().filter(|m| m.native_macros) {
            let (mut d, _) = crate::atk::simulated(m.product).unwrap();
            d.apply(&Setting::Button {
                id: 2,
                action: ButtonAction::Mouse(gearwise_core::model::MouseButton::Back),
            })
            .unwrap();
            views.push(view(d.as_mut()).unwrap());
        }
        assert_eq!(
            views
                .iter()
                .filter(|v| v.capabilities.dpi_indicator)
                .count(),
            5
        );
        std::fs::write(path, serde_json::to_vec_pretty(&views).unwrap()).unwrap();
    }
}
#[test]
#[ignore = "requires built indicator UI profiles"]
fn browser_indicator_profiles_import_and_execute_native_driver() {
    let path = std::env::var_os("OPEN_HUB_COMPX_INDICATOR_UI_PROFILES").unwrap();
    let mut profiles: Vec<crate::profiles::Profile> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(profiles.len(), 7);
    for p in &mut profiles {
        if p.id.is_empty() {
            p.id = crate::profiles::new_id();
        }
    }
    let mut store = crate::profiles::ProfileStore::default();
    crate::profile_transfer::merge(&mut store, profiles.clone()).unwrap();
    for profile in profiles {
        let p = profile.multi.as_ref().unwrap();
        validate(p).unwrap();
        let c = compile(p).unwrap();
        let (mut d, _) = crate::atk::simulated(p.product).unwrap();
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        let state = d.read_state().unwrap();
        let setting = p
            .settings
            .iter()
            .find_map(|s| {
                if let Setting::DpiIndicator(v) = s {
                    Some(*v)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(state.dpi_indicator, Some(setting));
        let active = state.dpi.as_ref().unwrap().active;
        assert_eq!(
            state.dpi_colors[&active],
            gearwise_core::model::Rgb {
                r: 171,
                g: 205,
                b: 239
            }
        );
        assert_eq!(
            state.buttons[&2],
            ButtonAction::Mouse(gearwise_core::model::MouseButton::Back)
        );
        assert!(!p
            .settings
            .iter()
            .any(|s| matches!(s, Setting::Lighting { zone: 0, .. })));
    }
}

#[test]
fn unspecified_mode_in_saved_plan_is_only_valid_while_disabled() {
    let mut p = plan();
    let mut v = desired();
    v.mode = DpiIndicatorMode::Unspecified;
    p.settings[0] = Setting::DpiIndicator(v);
    assert!(validate(&p).is_ok());
    v.enabled = true;
    p.settings[0] = Setting::DpiIndicator(v);
    assert!(validate(&p).is_err());
}
