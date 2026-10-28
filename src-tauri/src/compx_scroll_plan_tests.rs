use super::*;
use gearwise_core::model::ScrollDirection;
use gearwise_core::profile::{Macro, MacroMode, MacroStep};
fn views() -> Vec<View> {
    crate::atk::models()
        .iter()
        .filter(|s| crate::atk::horizontal_scroll_supported(s.vendor, s.product))
        .map(|s| {
            let (mut d, _) = crate::atk::simulated(s.product).unwrap();
            let m = Macro {
                id: "scroll".into(),
                name: "原生滚轮宏".into(),
                mode: MacroMode::Once,
                steps: vec![
                    MacroStep::Scroll(ScrollDirection::Up),
                    MacroStep::Delay(37),
                    MacroStep::Scroll(ScrollDirection::Down),
                ],
            };
            d.set_macro_definitions(&[m]).unwrap();
            d.apply(&Setting::Button {
                id: 4,
                action: ButtonAction::Macro("scroll".into()),
            })
            .unwrap();
            view(d.as_mut()).unwrap()
        })
        .collect()
}
#[test]
#[ignore = "exports production scroll macro DTOs for built UI"]
fn native_compx_scroll_fixture_export() {
    let values = views();
    assert_eq!(values.len(), crate::atk::native_test_count());
    std::fs::write(
        std::env::var_os("OPEN_HUB_COMPX_SCROLL_FIXTURE").unwrap(),
        serde_json::to_vec_pretty(&values).unwrap(),
    )
    .unwrap();
}
#[test]
#[ignore = "requires actual UI saved scroll profiles"]
fn browser_scroll_profiles_store_import_compile_and_native_replay() {
    let mut profiles: Vec<crate::profiles::Profile> = serde_json::from_slice(
        &std::fs::read(std::env::var_os("OPEN_HUB_COMPX_SCROLL_UI_PROFILES").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(profiles.len(), 9);
    for p in &mut profiles {if p.id.is_empty(){p.id=crate::profiles::new_id();}}
    let mut store = crate::profiles::ProfileStore::default();
    crate::profile_transfer::merge(&mut store, profiles.clone()).unwrap();
    for p in profiles {
        let plan = p.multi.unwrap();
        let expected = vec![
            MacroStep::Scroll(ScrollDirection::Down),
            MacroStep::Delay(37),
            MacroStep::Scroll(ScrollDirection::Down),
            MacroStep::Scroll(ScrollDirection::Up),
        ];
        assert_eq!(plan.macros[0].steps, expected);
        assert_eq!(plan.macros[0].mode, MacroMode::Repeat(5));
        let c = compile(&plan).unwrap();
        assert!(!c.software_input);
        let (mut d, _) = crate::atk::simulated(plan.product).unwrap();
        d.set_macro_definitions(&plan.macros).unwrap();
        for s in &c.settings {
            d.apply(s).unwrap();
        }
        let state = d.read_state().unwrap();
        let ButtonAction::Macro(id) = &state.buttons[&4] else {
            panic!("binding lost")
        };
        let m = d
            .macro_definitions()
            .into_iter()
            .find(|m| &m.id == id)
            .unwrap();
        assert_eq!(m.steps, expected);
        assert_eq!(m.mode, MacroMode::Repeat(5));
    }
}

#[test]
#[ignore="exports production X1 Lite view for full UI plan test"]
fn export_x1lite_native_fixture(){let(mut d,_)=crate::atk::simulated(0x113e).unwrap();let m=Macro{id:"wheel".into(),name:"X1 Lite 原生宏".into(),mode:MacroMode::Once,steps:vec![MacroStep::Scroll(ScrollDirection::Up),MacroStep::Delay(37),MacroStep::Scroll(ScrollDirection::Down)]};d.set_macro_definitions(&[m]).unwrap();d.apply(&Setting::Button{id:4,action:ButtonAction::Macro("wheel".into())}).unwrap();std::fs::write(std::env::var_os("OPEN_HUB_X1LITE_FIXTURE").unwrap(),serde_json::to_vec_pretty(&view(d.as_mut()).unwrap()).unwrap()).unwrap();}
#[test]
#[ignore="requires actual X1 Lite UI saved profile"]
fn browser_x1lite_saved_full_known_plan_imports_and_executes(){let mut profile:crate::profiles::Profile=serde_json::from_slice(&std::fs::read(std::env::var_os("OPEN_HUB_X1LITE_UI_PROFILE").unwrap()).unwrap()).unwrap();profile.id=crate::profiles::new_id();let mut store=crate::profiles::ProfileStore::default();crate::profile_transfer::merge(&mut store,vec![profile.clone()]).unwrap();let p=profile.multi.unwrap();assert_eq!((p.vendor,p.product),(0x373b,0x113e));let compiled=compile(&p).unwrap();assert!(!compiled.software_input);let(mut d,_)=crate::atk::simulated(p.product).unwrap();d.set_macro_definitions(&p.macros).unwrap();for s in &compiled.settings{d.apply(s).unwrap();}let state=d.read_state().unwrap();assert_eq!(state.dpi.unwrap().stages[0],gearwise_core::model::DpiStage{x:400,y:800});assert_eq!(state.report_rate,Some(500));let tuning=state.tuning.as_ref().unwrap();assert_eq!(tuning.debounce_ms,Some(20));assert_eq!(tuning.motion_sync,Some(false));assert_eq!(tuning.angle_snapping,Some(true));assert_eq!(tuning.ripple_control,Some(false));assert_eq!(tuning.led_idle_seconds,Some(1800));assert_eq!(tuning.sensor_sleep_enabled,Some(false));assert_eq!(tuning.sensor_sleep_seconds,Some(300));assert_eq!(tuning.lift_off_microns,Some(2000));assert_eq!(state.dpi_colors[&7],gearwise_core::model::Rgb{r:170,g:187,b:204});let ButtonAction::Macro(shortcut_id)=&state.buttons[&2]else{panic!("native shortcut macro lost")};let shortcut=d.macro_definitions().into_iter().find(|m|&m.id==shortcut_id).unwrap();assert_eq!(shortcut.mode,MacroMode::Once);assert_eq!(shortcut.steps.iter().filter(|s|!matches!(s,MacroStep::Delay(_))).cloned().collect::<Vec<_>>(),vec![MacroStep::KeyDown(224),MacroStep::KeyDown(6),MacroStep::KeyUp(6),MacroStep::KeyUp(224)]);let ButtonAction::Macro(id)=&state.buttons[&4]else{panic!("macro lost")};let m=d.macro_definitions().into_iter().find(|m|&m.id==id).unwrap();assert_eq!(m.mode,MacroMode::Repeat(5));assert_eq!(m.steps,vec![MacroStep::Scroll(ScrollDirection::Down),MacroStep::Delay(37),MacroStep::Scroll(ScrollDirection::Down),MacroStep::Scroll(ScrollDirection::Up)]);}
