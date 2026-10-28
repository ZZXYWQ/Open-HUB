use super::*;
use gearwise_core::model::{DpiStage, DpiState, LightingEffect, Rgb, ZoneState};
fn color() -> Rgb {
    Rgb {
        r: 17,
        g: 34,
        b: 51,
    }
}
fn plan() -> Plan {
    Plan {
        vendor: 0x373b,
        product: 0x1017,
        macros: vec![],
        source: None,
        settings: vec![
            Setting::DpiColor {
                stage: 0,
                color: color(),
            },
            Setting::Lighting {
                zone: 0,
                state: ZoneState {
                    effect: LightingEffect::Static { color: color() },
                    brightness: 100,
                },
            },
            Setting::Dpi(DpiState {
                stages: vec![DpiStage::uniform(800), DpiStage::uniform(1600)],
                active: 0,
            }),
        ],
    }
}
#[test]
fn full_palette_survives_serde_and_compiles_after_dpi_before_alias() {
    let mut p = plan();
    for stage in 1..8 {
        p.settings.push(Setting::DpiColor {
            stage,
            color: Rgb {
                r: stage,
                g: 2,
                b: 3,
            },
        });
    }
    let loaded: Plan = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
    assert_eq!(loaded, p);
    let c = compile(&loaded).unwrap();
    assert!(matches!(c.settings[0], Setting::Dpi(_)));
    assert!(matches!(c.settings.last(), Some(Setting::Lighting { .. })));
    assert!(c.settings[1..9]
        .iter()
        .all(|s| matches!(s, Setting::DpiColor { .. })));
    assert!(!c.software_input && c.bindings.is_empty());
    assert!(simulate(&loaded).is_ok());
}
#[test]
fn duplicate_conflicting_and_unspecified_active_stage_plans_reject() {
    let mut p = plan();
    p.settings.push(p.settings[0].clone());
    assert!(validate(&p).is_err());
    p = plan();
    p.settings[0] = Setting::DpiColor {
        stage: 0,
        color: Rgb { r: 1, g: 2, b: 3 },
    };
    assert!(validate(&p).is_err());
    p = plan();
    p.settings.retain(|s| !matches!(s, Setting::Dpi(_)));
    assert!(validate(&p).is_err());
}
#[test]
fn unverified_models_and_live_capability_reject_palette() {
    for (v, p) in [
        (0x373b, 0x1115),
        (0x373b, 0x103f),
        (0x3554, 0xf507),
        (0x1532, 0x0084),
        (0x1b1c, 0x1b2e),
        (0x1038, 0x1720),
    ] {
        let plan = Plan {
            vendor: v,
            product: p,
            settings: vec![Setting::DpiColor {
                stage: 0,
                color: color(),
            }],
            macros: vec![],
            source: None,
        };
        assert!(validate(&plan).is_err());
    }
    assert!(validate_capabilities(&plan(), &Capabilities::default()).is_err());
}
#[test]
fn older_capability_and_state_json_omit_empty_palette_and_accept_absence() {
    let c = serde_json::to_value(Capabilities::default()).unwrap();
    let s = serde_json::to_value(DeviceState::default()).unwrap();
    assert!(c.get("dpi_color_stages").is_none());
    assert!(s.get("dpi_colors").is_none());
    assert_eq!(
        serde_json::from_value::<Capabilities>(c)
            .unwrap()
            .dpi_color_stages,
        0
    );
    assert!(serde_json::from_value::<DeviceState>(s)
        .unwrap()
        .dpi_colors
        .is_empty());
}
#[test]
fn session_refresh_only_selected_colors_and_respects_mouse_ownership() {
    let (mut d, _) = crate::atk::simulated(0x1017).unwrap();
    let mut out = view(d.as_mut()).unwrap();
    let service = Service::default();
    let p = plan();
    service.remember_session(&out, &p, false).unwrap();
    let current = Rgb { r: 9, g: 8, b: 7 };
    out.state.dpi_colors.insert(0, current);
    out.state.dpi_colors.insert(7, color());
    out.state.lighting.insert(
        0,
        ZoneState {
            effect: LightingEffect::Static { color: current },
            brightness: 100,
        },
    );
    service
        .attach_session(&mut out, &crate::macro_runtime::Runtime::default())
        .unwrap();
    let selected = &out.session.as_ref().unwrap().plan.settings;
    assert_eq!(
        selected
            .iter()
            .filter(|s| matches!(s, Setting::DpiColor { .. }))
            .count(),
        1
    );
    assert!(selected.contains(&Setting::DpiColor {
        stage: 0,
        color: current
    }));
    assert!(selected.contains(&Setting::Lighting {
        zone: 0,
        state: out.state.lighting[&0]
    }));
    out.descriptor.serial = Some("another-mouse".into());
    service
        .attach_session(&mut out, &crate::macro_runtime::Runtime::default())
        .unwrap();
    assert!(out.session.is_none());
}
#[test]
fn export_palette_native_ui_fixtures() {
    let mut views = Vec::new();
    for m in crate::atk::models().iter().filter(|m| m.native_macros) {
        let (mut d, _) = crate::atk::simulated(m.product).unwrap();
        d.apply(&Setting::Button {
            id: 2,
            action: ButtonAction::Mouse(gearwise_core::model::MouseButton::Back),
        })
        .unwrap();
        let mac = gearwise_core::profile::Macro {
            id: "retained".into(),
            name: "Existing macro".into(),
            mode: gearwise_core::profile::MacroMode::Once,
            steps: vec![
                gearwise_core::profile::MacroStep::KeyDown(7),
                gearwise_core::profile::MacroStep::Delay(77),
                gearwise_core::profile::MacroStep::KeyUp(7),
            ],
        };
        d.set_macro_definitions(&[mac.clone()]).unwrap();
        d.apply(&Setting::Button {
            id: 5,
            action: ButtonAction::Macro(mac.id),
        })
        .unwrap();
        let mut dto = view(d.as_mut()).unwrap();
        if m.dpi_color_stages > 0 {
            d.apply(&Setting::DpiColor {
                stage: 7,
                color: Rgb {
                    r: 17,
                    g: 34,
                    b: 51,
                },
            })
            .unwrap();
            dto = view(d.as_mut()).unwrap();
            assert_eq!(dto.state.dpi_colors.len(), 8);
        }
        views.push(dto);
    }
    assert_eq!(
        views
            .iter()
            .filter(|v| v.capabilities.dpi_color_stages > 0)
            .count(),
        crate::atk::models().iter().filter(|m|m.dpi_color_stages>0).count()
    );
    if let Some(path) = std::env::var_os("OPEN_HUB_COMPX_PALETTE_FIXTURE") {
        std::fs::write(path, serde_json::to_vec_pretty(&views).unwrap()).unwrap();
    }
}
#[test]
#[ignore = "requires palette UI-created saved profiles"]
fn browser_palette_profiles_reimport_and_execute() {
    let path = std::env::var_os("OPEN_HUB_COMPX_PALETTE_UI_PROFILES").unwrap();
    let mut profiles: Vec<crate::profiles::Profile> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(profiles.len(), 5);
    for p in &mut profiles {
        if p.id.is_empty() {
            p.id = crate::profiles::new_id();
        }
    }
    let mut store = crate::profiles::ProfileStore::default();
    crate::profile_transfer::merge(&mut store, profiles.clone()).unwrap();
    for profile in profiles {
        let p = profile.multi.as_ref().unwrap();
        let c = compile(p).unwrap();
        let (mut d, _) = crate::atk::simulated(p.product).unwrap();
        d.set_macro_definitions(&p.macros).unwrap();
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        let state = d.read_state().unwrap();
        assert_eq!(
            state.dpi_colors[&0],
            Rgb {
                r: 170,
                g: 187,
                b: 204
            }
        );
        assert_eq!(
            state.dpi_colors[&7],
            Rgb {
                r: 221,
                g: 68,
                b: 85
            }
        );
        assert_eq!(state.dpi.as_ref().unwrap().active, 7);
        assert_eq!(state.dpi.as_ref().unwrap().stages.len(), 8);
        assert_eq!(
            state.buttons[&2],
            ButtonAction::Mouse(gearwise_core::model::MouseButton::Back)
        );
        assert_eq!(
            state.lighting[&0].effect,
            LightingEffect::Static {
                color: state.dpi_colors[&7]
            }
        );
    }
}

struct UnreadRow {
    inner: Box<dyn gearwise_core::Device>,
    definitions: Arc<std::sync::atomic::AtomicUsize>,
}
impl gearwise_core::Device for UnreadRow {
    fn descriptor(&self) -> &DeviceDescriptor {
        self.inner.descriptor()
    }
    fn capabilities(&self) -> &Capabilities {
        self.inner.capabilities()
    }
    fn read_state(&mut self) -> gearwise_core::Result<DeviceState> {
        let mut s = self.inner.read_state()?;
        s.dpi_colors.remove(&7);
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
fn unreadable_planned_row_blocks_whole_service_before_settings_and_macro_allocation() {
    let (d, log) = crate::atk::simulated(0x1017).unwrap();
    let id = d.descriptor().id.clone();
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let device = Box::new(UnreadRow {
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
    p.settings.push(Setting::DpiColor {
        stage: 7,
        color: color(),
    });
    p.settings.insert(0, Setting::ReportRate(500));
    log.lock().unwrap().clear();
    let root = std::env::temp_dir().join(format!("compx-palette-{}", crate::profiles::new_id()));
    let error = service
        .apply_at(
            &id,
            &p,
            &crate::macro_runtime::Runtime::default(),
            &root,
            None,
        )
        .err()
        .expect("unreadable palette must reject");
    assert!(error.contains("原始指示颜色"), "{error}");
    assert!(log
        .lock()
        .unwrap()
        .iter()
        .any(|r| matches!(r,gearwise_core::transport::MockRecord::Write(p)if p[1]==8)));
    assert_eq!(count.load(Ordering::Relaxed), 0);
    assert!(!log
        .lock()
        .unwrap()
        .iter()
        .any(|r| matches!(r,gearwise_core::transport::MockRecord::Write(p)if p[1]==7)));
    assert!(!root.exists());
}
