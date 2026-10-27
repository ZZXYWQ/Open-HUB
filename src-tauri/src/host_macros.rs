//! Device-scoped software macro routing. Platform input collection is separate;
//! simulation always uses a recording InputSink, never SendInput.
use gearwise_core::{
    model::MouseButton,
    profile::{Macro, MacroMode, MacroStep},
};
use gearwise_input::{InputSink, MacroPlayer, SharedSink};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub vendor: u16,
    pub product: u16,
    pub path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub button: u16,
    pub action: Macro,
}
pub fn validate(m: &Macro) -> Result<(), String> {
    if matches!(m.mode,MacroMode::Repeat(n) if !(2..=252).contains(&n)){return Err("固定重复次数为 2–252；只执行一次请选择单次播放".into());}
    if m.id.is_empty()
        || m.id.len() > 128
        || m.name.len() > 256
        || m.steps.is_empty()
        || m.steps.len() > 512
    {
        return Err("宏标识、名称或事件数无效（1–512 步）".into());
    }
    let mut keys = HashSet::new();
    let mut buttons = HashSet::new();
    let mut delay = 0u64;
    for s in &m.steps {
        match s {
            MacroStep::KeyDown(k) => {
                if !valid_key(*k) || !keys.insert(*k) {
                    return Err("宏含无效键码或重复按下".into());
                }
            }
            MacroStep::KeyUp(k) => {
                if !keys.remove(k) {
                    return Err("宏含未按下的释放事件".into());
                }
            }
            MacroStep::KeyTap { usage, .. } if !(4..=0xa4).contains(usage) || keys.contains(usage) => {
                return Err("快捷键码无效，或快捷键主键已被此宏按住".into())
            }
            MacroStep::MouseDown(b) => {
                if !buttons.insert(*b) {
                    return Err("鼠标重复按下".into());
                }
            }
            MacroStep::MouseUp(b) => {
                if !buttons.remove(b) {
                    return Err("鼠标释放没有对应按下".into());
                }
            }
            MacroStep::Media(usage) if !gearwise_core::model::media::ALL.iter().any(|(code,_)|code==usage)=>return Err("未知多媒体键码".into()),
            MacroStep::Delay(ms) => {
                delay += u64::from(*ms);
                if *ms > 10000 || delay > 60000 {
                    return Err("单次延迟最多 10 秒，总延迟最多 60 秒".into());
                }
            }
            MacroStep::WaitForRelease if m.mode!=MacroMode::WhileHeld => return Err("按住事件只能用于按住模式".into()),
            MacroStep::Text(t) if t.len() > 4096 => return Err("文本宏最多 4096 字节".into()),
            _ => {}
        }
    }
    if !keys.is_empty() || !buttons.is_empty() {
        return Err("宏有未释放按键".into());
    }
    Ok(())
}
fn valid_key(k: u8) -> bool {
    (4..=0xa4).contains(&k) || (0xe0..=0xe7).contains(&k)
}
struct Routes {
    source: Source,
    bindings: HashMap<u16, Macro>,
    generation: u64,
    pressed: HashSet<u16>,
}
pub struct Router {
    player: MacroPlayer,
    counted: Arc<Mutex<CountingSink>>,
    routes: Mutex<HashMap<String, Routes>>,
    next: std::sync::atomic::AtomicU64,
}
impl Router {
    pub fn new(sink: SharedSink) -> Self {
        let counted=Arc::new(Mutex::new(CountingSink::new(sink)));
        Self {
            player: MacroPlayer::new(counted.clone()),
            counted,
            routes: Mutex::new(HashMap::new()),
            next: std::sync::atomic::AtomicU64::new(1),
        }
    }
    pub fn configure(&self, source: Source, bindings: Vec<Binding>) -> Result<(), String> {
        if !crate::multibrand::supported(source.vendor, source.product)
            || source.path.is_empty()
            || source.path.len() > 4096
            || bindings.len() > 32
        {
            return Err("宏来源不是已适配的具体设备".into());
        }
        let mut map = HashMap::new();
        for b in bindings {
            validate(&b.action)?;
            if b.button == 0 || map.insert(b.button, b.action).is_some() {
                return Err("宏按钮重复或无效".into());
            }
        }
        self.remove(&source.path);
        let generation = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.routes.lock().map_err(|_| "宏路由锁异常")?.insert(
            source.path.clone(),
            Routes {
                source,
                bindings: map,
                generation,
                pressed: HashSet::new(),
            },
        );
        Ok(())
    }
    pub fn input(&self, source: &Source, button: u16, down: bool) -> bool {
        let event=self.routes.lock().ok().and_then(|mut routes|{let route=routes.get_mut(&source.path)?;if &route.source!=source{return None;}let action=route.bindings.get(&button)?.clone();if down{if !route.pressed.insert(button){return None;}}else if !route.pressed.remove(&button){return None;}Some((action,format!("{}:{button}",route.generation)))});
        if let Some((action, trigger)) = event {
            if down {
                self.player.press(&action, &trigger);
            } else {
                self.player.release(&trigger);
            }
            true
        } else {
            false
        }
    }
    pub fn remove(&self, path: &str) {
        if let Ok(mut routes) = self.routes.lock() {
            if let Some(route) = routes.remove(path) {
                for button in route.bindings.keys() {
                    self.player
                        .cancel(&format!("{}:{button}", route.generation));
                }
            }
        }
    }
    pub fn errors(&self)->Vec<String>{self.counted.lock().map(|s|s.errors.clone()).unwrap_or_else(|_|vec!["input ownership lock poisoned".into()])}
    pub fn stop_all(&self) {
        self.player.stop_all();
        if let Ok(mut counted)=self.counted.lock(){counted.release_owned();}
        if let Ok(mut routes) = self.routes.lock() {
            routes.clear();
        }
    }
}
impl Drop for Router {
    fn drop(&mut self) {
        self.stop_all();
    }
}
#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<String>>>);
impl Recorder {
    fn push(&self, s: String) -> gearwise_core::Result<()> {
        let mut events = self
            .0
            .lock()
            .map_err(|_| gearwise_core::Error::invalid("recording lock"))?;
        if events.len() >= 4096 {
            return Err(gearwise_core::Error::invalid("simulation event limit"));
        }
        events.push(s);
        Ok(())
    }
}
impl InputSink for Recorder {
    fn key_down(&mut self, k: u8) -> gearwise_core::Result<()> {
        self.push(format!("key_down:{k}"))
    }
    fn key_up(&mut self, k: u8) -> gearwise_core::Result<()> {
        self.push(format!("key_up:{k}"))
    }
    fn mouse_down(&mut self, b: MouseButton) -> gearwise_core::Result<()> {
        self.push(format!("mouse_down:{b:?}"))
    }
    fn mouse_up(&mut self, b: MouseButton) -> gearwise_core::Result<()> {
        self.push(format!("mouse_up:{b:?}"))
    }
    fn scroll(&mut self, x: i32, y: i32) -> gearwise_core::Result<()> {
        self.push(format!("scroll:{x},{y}"))
    }
    fn media(&mut self, u: u16) -> gearwise_core::Result<()> {
        self.push(format!("media:{u}"))
    }
    fn text(&mut self, t: &str) -> gearwise_core::Result<()> {
        self.push(format!("text:{t}"))
    }
}
pub fn simulate(source: Source, button: u16, action: Macro) -> Result<serde_json::Value, String> {
    validate(&action)?;
    let rec = Recorder::default();
    let sink: SharedSink = Arc::new(Mutex::new(rec.clone()));
    let router = Router::new(sink);
    let mut fast = action.clone();
    fast.mode = MacroMode::Once;
    for s in &mut fast.steps {
        if matches!(s,MacroStep::WaitForRelease){*s=MacroStep::Delay(0);}
        if let MacroStep::Delay(ms) = s {
            *ms = 0;
        }
    }
    router.configure(
        source.clone(),
        vec![Binding {
            button,
            action: fast,
        }],
    )?;
    if !router.input(&source, button, true) {
        return Err("模拟宏未被正确设备触发".into());
    }
    // Stop_all joins a once macro only after it has run. Give bounded completion
    // time, independent of delays in the original script (virtual clock above).
    let deadline = Instant::now() + Duration::from_secs(2);
    let trigger = "1:".to_string() + &button.to_string();
    while router.player.is_running(&trigger) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    let timed_out = router.player.is_running(&trigger);
    router.stop_all();
    if timed_out {
        return Err("宏模拟超时".into());
    }
    let events = rec.0.lock().map_err(|_| "模拟记录锁")?.clone();
    Ok(
        serde_json::json!({"simulated":true,"hardware_tested":false,"native_input_injected":false,"mode":action.mode,"virtual_delay_ms":action.duration_ms(),"events":events,"note":"虚拟时间单次播放；循环模式由独立取消/释放测试验证。"}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Source {
        Source {
            vendor: 0x1038,
            product: 0x1720,
            path: "simulation:steel-A".into(),
        }
    }
    fn action() -> Macro {
        Macro {
            id: "m1".into(),
            name: "example".into(),
            mode: MacroMode::Once,
            steps: vec![
                MacroStep::KeyDown(4),
                MacroStep::Delay(5),
                MacroStep::KeyUp(4),
            ],
        }
    }
    #[test]
    fn simulated_recording_executes_down_delay_up_without_desktop_input() {
        let r = simulate(source(), 6, action()).unwrap();
        assert_eq!(r["events"], serde_json::json!(["key_down:4", "key_up:4"]));
        assert_eq!(r["native_input_injected"], false);
        assert_eq!(r["virtual_delay_ms"], 5);
    }
    #[test]
    fn foreign_mouse_cannot_trigger_macro() {
        let rec = Recorder::default();
        let router = Router::new(Arc::new(Mutex::new(rec.clone())));
        router
            .configure(
                source(),
                vec![Binding {
                    button: 6,
                    action: action(),
                }],
            )
            .unwrap();
        for (v, p, path) in [
            (0x046d, 0x1720, "simulation:steel-A"),
            (0x3837, 0x1720, "simulation:steel-A"),
            (0x1038, 0x1720, "simulation:steel-B"),
        ] {
            assert!(!router.input(
                &Source {
                    vendor: v,
                    product: p,
                    path: path.into()
                },
                6,
                true
            ));
        }
        assert!(rec.0.lock().unwrap().is_empty());
    }
    #[test]
    fn configure_failure_never_changes_existing_bindings() {
        let router = Router::new(Arc::new(Mutex::new(Recorder::default())));
        router
            .configure(
                source(),
                vec![Binding {
                    button: 6,
                    action: action(),
                }],
            )
            .unwrap();
        let mut bad = action();
        bad.steps.pop();
        assert!(router
            .configure(
                source(),
                vec![Binding {
                    button: 6,
                    action: bad
                }]
            )
            .is_err());
        assert!(router.routes.lock().unwrap()[&source().path]
            .bindings
            .contains_key(&6));
    }
    #[test]
    fn removing_source_cancels_delay_and_releases_held_key() {
        let rec = Recorder::default();
        let router = Router::new(Arc::new(Mutex::new(rec.clone())));
        let mut m = action();
        m.steps[1] = MacroStep::Delay(10000);
        router
            .configure(
                source(),
                vec![Binding {
                    button: 6,
                    action: m,
                }],
            )
            .unwrap();
        router.input(&source(), 6, true);
        let deadline = Instant::now() + Duration::from_secs(1);
        while rec.0.lock().unwrap().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        router.remove(&source().path);
        router.stop_all();
        assert_eq!(*rec.0.lock().unwrap(), vec!["key_down:4", "key_up:4"]);
    }
    #[test]
    fn rejects_unbalanced_and_excessive_macros() {
        let mut m = action();
        m.steps.pop();
        assert!(validate(&m).is_err());
        m = action();
        m.steps[1] = MacroStep::Delay(10001);
        assert!(validate(&m).is_err());
        m = action();
        m.steps = vec![MacroStep::Delay(0); 513];
        assert!(validate(&m).is_err());
    }
}

// Shared ownership of synthesized holds across concurrent macros. A worker may
// release only its own reference; it cannot release another macro's key.
struct CountingSink{inner:SharedSink,keys:HashMap<u8,usize>,buttons:HashMap<MouseButton,usize>,errors:Vec<String>}
impl CountingSink{
 fn new(inner:SharedSink)->Self{Self{inner,keys:HashMap::new(),buttons:HashMap::new(),errors:vec![]}}
 fn record<T>(&mut self,result:gearwise_core::Result<T>)->gearwise_core::Result<T>{if let Err(e)=&result{if self.errors.len()<128{self.errors.push(e.to_string());}}result}
 fn release_owned(&mut self){let keys=self.keys.keys().copied().collect::<Vec<_>>();for k in keys{let result=self.inner.lock().map_err(|_|gearwise_core::Error::invalid("input sink lock")).and_then(|mut s|s.key_up(k));if self.record(result).is_ok(){self.keys.remove(&k);}}let buttons=self.buttons.keys().copied().collect::<Vec<_>>();for b in buttons{let result=self.inner.lock().map_err(|_|gearwise_core::Error::invalid("input sink lock")).and_then(|mut s|s.mouse_up(b));if self.record(result).is_ok(){self.buttons.remove(&b);}}}
}
impl InputSink for CountingSink{
 fn key_down(&mut self,k:u8)->gearwise_core::Result<()>{if let Some(n)=self.keys.get_mut(&k){*n+=1;return Ok(());}let result=self.inner.lock().map_err(|_|gearwise_core::Error::invalid("input sink lock")).and_then(|mut s|s.key_down(k));self.record(result)?;self.keys.insert(k,1);Ok(())}
 fn key_up(&mut self,k:u8)->gearwise_core::Result<()>{match self.keys.get_mut(&k){Some(n) if *n>1=>{*n-=1;return Ok(());},None=>return Ok(()),_=>{}}let result=self.inner.lock().map_err(|_|gearwise_core::Error::invalid("input sink lock")).and_then(|mut s|s.key_up(k));self.record(result)?;self.keys.remove(&k);Ok(())}
 fn mouse_down(&mut self,b:MouseButton)->gearwise_core::Result<()>{if let Some(n)=self.buttons.get_mut(&b){*n+=1;return Ok(());}let result=self.inner.lock().map_err(|_|gearwise_core::Error::invalid("input sink lock")).and_then(|mut s|s.mouse_down(b));self.record(result)?;self.buttons.insert(b,1);Ok(())}
 fn mouse_up(&mut self,b:MouseButton)->gearwise_core::Result<()>{match self.buttons.get_mut(&b){Some(n) if *n>1=>{*n-=1;return Ok(());},None=>return Ok(()),_=>{}}let result=self.inner.lock().map_err(|_|gearwise_core::Error::invalid("input sink lock")).and_then(|mut s|s.mouse_up(b));self.record(result)?;self.buttons.remove(&b);Ok(())}
 fn scroll(&mut self,x:i32,y:i32)->gearwise_core::Result<()>{let r=self.inner.lock().map_err(|_|gearwise_core::Error::invalid("input sink lock")).and_then(|mut s|s.scroll(x,y));self.record(r)}
 fn media(&mut self,u:u16)->gearwise_core::Result<()>{let r=self.inner.lock().map_err(|_|gearwise_core::Error::invalid("input sink lock")).and_then(|mut s|s.media(u));self.record(r)}
 fn text(&mut self,t:&str)->gearwise_core::Result<()>{let r=self.inner.lock().map_err(|_|gearwise_core::Error::invalid("input sink lock")).and_then(|mut s|s.text(t));self.record(r)}
}
#[cfg(test)]mod ownership_tests{
 use super::*;
 #[test]fn concurrent_workers_release_only_their_own_key_hold(){let rec=Recorder::default();let mut sink=CountingSink::new(Arc::new(Mutex::new(rec.clone())));sink.key_down(4).unwrap();sink.key_down(4).unwrap();sink.key_up(4).unwrap();assert_eq!(*rec.0.lock().unwrap(),vec!["key_down:4"]);sink.key_up(4).unwrap();assert_eq!(*rec.0.lock().unwrap(),vec!["key_down:4","key_up:4"]);sink.key_up(4).unwrap();assert_eq!(rec.0.lock().unwrap().len(),2);}
 #[test]fn final_cleanup_releases_all_owned_holds(){let rec=Recorder::default();let mut sink=CountingSink::new(Arc::new(Mutex::new(rec.clone())));sink.key_down(4).unwrap();sink.mouse_down(MouseButton::Back).unwrap();sink.release_owned();assert!(sink.keys.is_empty()&&sink.buttons.is_empty());assert!(rec.0.lock().unwrap().contains(&"mouse_up:Back".into()));}
}


#[cfg(test)]mod held_tap_tests{
 use super::*;use gearwise_core::model::Modifiers;
 #[test]fn tap_cannot_release_a_key_already_held_by_the_same_macro(){let m=Macro{id:"t".into(),name:"t".into(),mode:MacroMode::Once,steps:vec![MacroStep::KeyDown(4),MacroStep::KeyTap{usage:4,mods:Modifiers(0)},MacroStep::KeyUp(4)]};assert!(validate(&m).is_err());}
 #[test]fn shortcut_preserves_preexisting_modifier_until_source_cancel(){let rec=Recorder::default();let router=Router::new(Arc::new(Mutex::new(rec.clone())));let source=Source{vendor:0x1038,product:0x1720,path:"simulation:heldtap".into()};let m=Macro{id:"t".into(),name:"t".into(),mode:MacroMode::Once,steps:vec![MacroStep::KeyDown(0xe0),MacroStep::KeyTap{usage:4,mods:Modifiers(1)},MacroStep::Delay(10000),MacroStep::KeyUp(0xe0)]};router.configure(source.clone(),vec![Binding{button:6,action:m}]).unwrap();router.input(&source,6,true);let end=Instant::now()+Duration::from_secs(1);while !rec.0.lock().unwrap().iter().any(|e|e=="key_up:4")&&Instant::now()<end{std::thread::sleep(Duration::from_millis(1));}assert_eq!(*rec.0.lock().unwrap(),vec!["key_down:224","key_down:4","key_up:4"]);router.remove(&source.path);router.stop_all();assert_eq!(*rec.0.lock().unwrap(),vec!["key_down:224","key_down:4","key_up:4","key_up:224"]);assert!(router.counted.lock().unwrap().keys.is_empty());}
}
#[cfg(test)]mod media_route_tests{
 use super::*;
 #[test]fn media_steps_use_only_the_selected_mouse_route_and_reject_unknown_usages(){
  let source=Source{vendor:0x1038,product:0x1720,path:"simulation:media-A".into()};let other=Source{path:"simulation:media-B".into(),..source.clone()};let rec=Recorder::default();let router=Router::new(Arc::new(Mutex::new(rec.clone())));
  let action=|id:&str,usage|Macro{id:id.into(),name:id.into(),mode:MacroMode::Once,steps:vec![MacroStep::Media(usage)]};let a=action("mute",0xe2);assert!(validate(&action("invalid",0xffff)).is_err());router.configure(source.clone(),vec![Binding{button:6,action:a}]).unwrap();router.configure(other.clone(),vec![Binding{button:6,action:action("play",0xcd)}]).unwrap();
  router.input(&source,6,true);let until=Instant::now()+Duration::from_secs(1);while rec.0.lock().unwrap().is_empty()&&Instant::now()<until{std::thread::sleep(Duration::from_millis(1));}assert_eq!(*rec.0.lock().unwrap(),vec!["media:226"]);router.input(&other,6,true);let until=Instant::now()+Duration::from_secs(1);while rec.0.lock().unwrap().len()<2&&Instant::now()<until{std::thread::sleep(Duration::from_millis(1));}assert_eq!(*rec.0.lock().unwrap(),vec!["media:226","media:205"]);router.stop_all();
 }
}
