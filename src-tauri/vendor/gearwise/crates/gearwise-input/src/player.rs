//! Macro playback: one worker thread per active macro.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use gearwise_core::model::{Modifiers, MouseButton};
use gearwise_core::profile::{Macro, MacroMode, MacroStep};
use gearwise_core::{Error, Result};

use crate::{InputSink, SharedSink, wheel_ticks};

/// How long a `KeyTap` keeps the key down. Games that poll input once per
/// frame miss a press that is released in the same instant.
const TAP_HOLD: Duration = Duration::from_millis(5);
/// Pause between iterations of a loop that has no delays of its own, so a
/// zero-delay macro cannot saturate the CPU or flood the input queue.
const MIN_LOOP_GAP: Duration = Duration::from_millis(1);

fn lock<T: ?Sized>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a worker has been asked to do, in increasing order of urgency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Signal {
    Run,
    /// Complete the current iteration, then stop.
    Finish,
    /// Stop now: cut delays short and release everything still held.
    Abort,
}

/// Shared between the player and one worker.
struct Control {
    signal: Mutex<Signal>,
    wake: Condvar,
}

impl Control {
    fn new() -> Self {
        Self {
            signal: Mutex::new(Signal::Run),
            wake: Condvar::new(),
        }
    }

    fn wait_release(&self)->bool{let guard=lock(&self.signal);let guard=self.wake.wait_while(guard,|s|*s==Signal::Run).unwrap_or_else(PoisonError::into_inner);*guard==Signal::Abort}

    fn get(&self) -> Signal {
        *lock(&self.signal)
    }

    /// Raise the signal; it never goes back to a calmer one.
    fn raise(&self, to: Signal) {
        let mut s = lock(&self.signal);
        if to > *s {
            *s = to;
            self.wake.notify_all();
        }
    }

    /// Sleep for `d` unless aborted first. Returns `true` if aborted.
    fn sleep(&self, d: Duration) -> bool {
        let guard = lock(&self.signal);
        let (guard, _) = self
            .wake
            .wait_timeout_while(guard, d, |s| *s != Signal::Abort)
            .unwrap_or_else(PoisonError::into_inner);
        *guard == Signal::Abort
    }
}

/// Why a step stopped the macro early.
enum Halt {
    Aborted,
    Failed(Error),
}

impl From<Error> for Halt {
    fn from(e: Error) -> Self {
        Halt::Failed(e)
    }
}

/// Keys and buttons a macro has pressed and not yet released.
#[derive(Default)]
struct Held {
    keys: Vec<u8>,
    buttons: Vec<MouseButton>,
}

/// Executes macro steps against the sink and tracks what is held down.
struct Playback {
    sink: SharedSink,
    ctl: Arc<Control>,
    held: Held,
}

impl Playback {
    fn sink(&self) -> MutexGuard<'_, dyn InputSink + Send + 'static> {
        lock(&self.sink)
    }

    /// Run `steps` once, or repeatedly while the control signal stays `Run`.
    /// The first iteration always runs to completion.
    fn run(&mut self, steps: &[MacroStep], looping: bool, gap: Duration, limit:Option<u8>) -> Result<(), Halt> {
        let mut completed=0u16;
        loop {
            for step in steps {
                if self.ctl.get() == Signal::Abort {
                    return Err(Halt::Aborted);
                }
                self.step(step)?;
            }
            if let Some(limit)=limit{completed+=1;if completed>=u16::from(limit){return Ok(());}}
            if !looping || self.ctl.get() != Signal::Run {
                return Ok(());
            }
            if self.ctl.sleep(gap) {
                return Err(Halt::Aborted);
            }
        }
    }

    fn step(&mut self, step: &MacroStep) -> Result<(), Halt> {
        match step {
            MacroStep::KeyDown(usage) => self.key_down(*usage),
            MacroStep::KeyUp(usage) => self.key_up(*usage),
            MacroStep::KeyTap { usage, mods } => self.tap(*usage, *mods),
            MacroStep::MouseDown(button) => self.mouse_down(*button),
            MacroStep::MouseUp(button) => self.mouse_up(*button),
            MacroStep::Media(usage) => Ok(self.sink().media(*usage)?),
            MacroStep::Scroll(direction) => Ok(self.sink().scroll(0, if matches!(direction, gearwise_core::model::ScrollDirection::Up) {1} else {-1})?),
            MacroStep::Delay(ms) => self.delay(Duration::from_millis(u64::from(*ms))),
            MacroStep::Text(text) => Ok(self.sink().text(text)?),
            MacroStep::WaitForRelease => if self.ctl.wait_release(){Err(Halt::Aborted)}else{Ok(())},
        }
    }

    fn delay(&self, d: Duration) -> Result<(), Halt> {
        if self.ctl.sleep(d) {
            Err(Halt::Aborted)
        } else {
            Ok(())
        }
    }

    fn key_down(&mut self, usage: u8) -> Result<(), Halt> {
        // Track before sending: if the send half-fails, cleanup still lets go.
        if !self.held.keys.contains(&usage) {
            self.held.keys.push(usage);
        }
        Ok(self.sink().key_down(usage)?)
    }

    fn key_up(&mut self, usage: u8) -> Result<(), Halt> {
        self.held.keys.retain(|&k| k != usage);
        Ok(self.sink().key_up(usage)?)
    }

    fn mouse_down(&mut self, button: MouseButton) -> Result<(), Halt> {
        // Wheel pseudo-buttons are one-shot scrolls, never held.
        if wheel_ticks(button).is_none() && !self.held.buttons.contains(&button) {
            self.held.buttons.push(button);
        }
        Ok(self.sink().mouse_down(button)?)
    }

    fn mouse_up(&mut self, button: MouseButton) -> Result<(), Halt> {
        self.held.buttons.retain(|&b| b != button);
        Ok(self.sink().mouse_up(button)?)
    }

    /// Modifiers down, key down, brief hold, key up, modifiers up.
    fn tap(&mut self, usage: u8, mods: Modifiers) -> Result<(), Halt> {
        // Modifier bit n is HID usage 0xe0 + n.
        let mods: Vec<u8> = (0..8)
              .filter(|n| mods.0 & (1 << n) != 0 && !self.held.keys.contains(&(0xe0 + n)))
            .map(|n| 0xe0 + n)
            .collect();
        for &m in &mods {
            self.key_down(m)?;
        }
        self.key_down(usage)?;
        self.delay(TAP_HOLD)?;
        self.key_up(usage)?;
        for &m in mods.iter().rev() {
            self.key_up(m)?;
        }
        Ok(())
    }

    /// Release whatever is still down, most recently pressed first.
    fn release_all(&mut self) {
        let held = std::mem::take(&mut self.held);
        if held.keys.is_empty() && held.buttons.is_empty() {
            return;
        }
        let mut sink = self.sink();
        for b in held.buttons.into_iter().rev() {
            if let Err(e) = sink.mouse_up(b) {
                log::warn!("macro cleanup: releasing {b:?} failed: {e}");
            }
        }
        for k in held.keys.into_iter().rev() {
            if let Err(e) = sink.key_up(k) {
                log::warn!("macro cleanup: releasing key 0x{k:02x} failed: {e}");
            }
        }
    }
}

/// Worker thread body.
fn play(sink: SharedSink, ctl: Arc<Control>, mac: Macro) {
    let looping = mac.mode != MacroMode::Once;
    let gap = if mac.duration_ms() == 0 {
        MIN_LOOP_GAP
    } else {
        Duration::ZERO
    };
    let mut playback = Playback {
        sink,
        ctl,
        held: Held::default(),
    };
    let limit=match mac.mode{MacroMode::Repeat(count)=>Some(count),_=>None};
    let outcome = playback.run(&mac.steps, looping, gap, limit);
    playback.release_all();
    if let Err(Halt::Failed(e)) = outcome {
        log::warn!("macro {:?} stopped: {e}", mac.name);
    }
}

/// A running worker.
struct Active {
    trigger: String,
    mode: MacroMode,
    ctl: Arc<Control>,
    handle: JoinHandle<()>,
    /// A stop was requested; the worker is only winding down.
    ending: bool,
}

/// Plays macros on background threads, one per trigger (the button or key
/// that fired it).
///
/// * [`MacroMode::Once`] — plays through on press; releasing does not
///   interrupt it, and pressing again while it runs is ignored.
/// * [`MacroMode::WhileHeld`] — loops from press until [`release`](Self::release),
///   then completes the current iteration (at least one always runs).
/// * [`MacroMode::Toggle`] — the first press starts looping, the next press of
///   the same trigger stops it immediately.
///
/// Stopping immediately (toggle off, [`stop_all`](Self::stop_all), drop)
/// interrupts delays and releases every key and button the macro still
/// holds, so nothing stays stuck down. A sink error ends the macro the same
/// way.
pub struct MacroPlayer {
    sink: SharedSink,
    active: Mutex<Vec<Active>>,
}

impl MacroPlayer {
    pub fn new(sink: SharedSink) -> Self {
        Self {
            sink,
            active: Mutex::new(Vec::new()),
        }
    }

    /// The trigger was pressed: start (or, for a running toggle, stop) `mac`.
    pub fn press(&self, mac: &Macro, trigger: &str) {
        let mut active = lock(&self.active);
        reap(&mut active);

        if let Some(run) = active
            .iter_mut()
            .find(|a| a.trigger == trigger && !a.ending)
        {
            if run.mode == MacroMode::Toggle {
                run.ending = true;
                run.ctl.raise(Signal::Abort);
            }
            return;
        }
        if mac.steps.is_empty() || matches!(mac.mode,MacroMode::Repeat(0)) {
            return;
        }

        let ctl = Arc::new(Control::new());
        let spawned = thread::Builder::new().name("gearwise-macro".into()).spawn({
            let (sink, ctl, mac) = (self.sink.clone(), ctl.clone(), mac.clone());
            move || play(sink, ctl, mac)
        });
        match spawned {
            Ok(handle) => active.push(Active {
                trigger: trigger.to_owned(),
                mode: mac.mode,
                ctl,
                handle,
                ending: false,
            }),
            Err(e) => log::error!("cannot start macro {:?}: {e}", mac.name),
        }
    }

    /// The trigger was released: let a held-loop macro finish its iteration.
    pub fn release(&self, trigger: &str) {
        let mut active = lock(&self.active);
        let held = active
            .iter_mut()
            .find(|a| a.trigger == trigger && !a.ending && a.mode == MacroMode::WhileHeld);
        if let Some(run) = held {
            run.ending = true;
            run.ctl.raise(Signal::Finish);
        }
    }

    /// Whether a macro started by `trigger` is still playing (including one
    /// that is finishing up).
    pub fn is_running(&self, trigger: &str) -> bool {
        let mut active = lock(&self.active);
        reap(&mut active);
        active.iter().any(|a| a.trigger == trigger)
    }

    /// Stop every macro now and wait until their keys and buttons are released.
    /// Open HUB: cancel only this device/button trigger; other sources keep running.
    pub fn cancel(&self, trigger: &str) { let active=lock(&self.active); for a in active.iter().filter(|a| a.trigger == trigger) { a.ctl.raise(Signal::Abort); } }

    pub fn stop_all(&self) {
        let runs = std::mem::take(&mut *lock(&self.active));
        for run in &runs {
            run.ctl.raise(Signal::Abort);
        }
        for run in runs {
            if run.handle.join().is_err() {
                log::error!("macro worker for {:?} panicked", run.trigger);
            }
        }
    }
}

impl Drop for MacroPlayer {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// Forget workers that have finished.
fn reap(active: &mut Vec<Active>) {
    let mut i = 0;
    while i < active.len() {
        if active[i].handle.is_finished() {
            let done = active.swap_remove(i);
            if done.handle.join().is_err() {
                log::error!("macro worker for {:?} panicked", done.trigger);
            }
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::testutil::{Event, Recorder};

    fn setup() -> (MacroPlayer, Recorder) {
        let rec = Recorder::default();
        (player_for(rec.clone()), rec)
    }

    fn player_for(rec: Recorder) -> MacroPlayer {
        MacroPlayer::new(Arc::new(Mutex::new(rec)))
    }

    fn mac(mode: MacroMode, steps: Vec<MacroStep>) -> Macro {
        let mut m = Macro::new("test");
        m.mode = mode;
        m.steps = steps;
        m
    }

    fn wait_until(what: &str, cond: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !cond() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn wait_idle(p: &MacroPlayer, trigger: &str) {
        wait_until("macro to finish", || !p.is_running(trigger));
    }

    /// A press/release pair with a delay in between: "a" held for `ms`.
    fn hold_a(ms: u32) -> Vec<MacroStep> {
        vec![
            MacroStep::KeyDown(0x04),
            MacroStep::Delay(ms),
            MacroStep::KeyUp(0x04),
        ]
    }

    #[test]
    fn once_plays_every_step_in_order() {
        let (p, rec) = setup();
        let steps = vec![
            MacroStep::KeyDown(0x04),
            MacroStep::Delay(5),
            MacroStep::KeyUp(0x04),
            MacroStep::MouseDown(MouseButton::Left),
            MacroStep::MouseUp(MouseButton::Left),
            MacroStep::KeyTap {
                usage: 0x05,
                mods: Modifiers(Modifiers::LCTRL | Modifiers::RSHIFT),
            },
            MacroStep::Text("A".into()),
        ];
        p.press(&mac(MacroMode::Once, steps), "t");
        wait_idle(&p, "t");
        assert_eq!(
            rec.events(),
            vec![
                Event::KeyDown(0x04),
                Event::KeyUp(0x04),
                Event::MouseDown(MouseButton::Left),
                Event::MouseUp(MouseButton::Left),
                // Ctrl+Shift+B: modifiers down in usage order, up in reverse.
                Event::KeyDown(0xe0),
                Event::KeyDown(0xe5),
                Event::KeyDown(0x05),
                Event::KeyUp(0x05),
                Event::KeyUp(0xe5),
                Event::KeyUp(0xe0),
                // Text "A": Shift + a.
                Event::KeyDown(0xe1),
                Event::KeyDown(0x04),
                Event::KeyUp(0x04),
                Event::KeyUp(0xe1),
            ]
        );
    }

    #[test]
    fn media_step_is_one_atomic_sink_tap_between_other_steps(){
        let (p,rec)=setup();
        p.press(&mac(MacroMode::Once,vec![MacroStep::KeyDown(4),MacroStep::KeyUp(4),MacroStep::Media(0xe2),MacroStep::Media(0xcd)]),"media");
        wait_idle(&p,"media");
        assert_eq!(rec.events(),vec![Event::KeyDown(4),Event::KeyUp(4),Event::Media(0xe2),Event::Media(0xcd)]);
    }

    #[test]
    fn vertical_macro_scroll_is_one_tick_without_held_input_state(){
        let(p,rec)=setup();p.press(&mac(MacroMode::Once,vec![MacroStep::Scroll(gearwise_core::model::ScrollDirection::Up),MacroStep::Scroll(gearwise_core::model::ScrollDirection::Down)]),"scroll");wait_idle(&p,"scroll");assert_eq!(rec.events(),vec![Event::Scroll(0,1),Event::Scroll(0,-1)]);
    }

    #[test]
    fn once_ignores_release_and_repress_while_running() {
        let (p, rec) = setup();
        let m = mac(MacroMode::Once, hold_a(40));
        p.press(&m, "t");
        wait_until("key down", || !rec.events().is_empty());
        p.release("t");
        p.press(&m, "t");
        wait_idle(&p, "t");
        assert_eq!(rec.events(), vec![Event::KeyDown(0x04), Event::KeyUp(0x04)]);
    }

    #[test]
    fn different_triggers_play_independently() {
        let (p, rec) = setup();
        let m = mac(MacroMode::Once, hold_a(20));
        p.press(&m, "one");
        p.press(&m, "two");
        wait_idle(&p, "one");
        wait_idle(&p, "two");
        assert_eq!(rec.events().len(), 4);
    }

    #[test]
    fn while_held_loops_until_release_then_finishes_the_iteration() {
        let (p, rec) = setup();
        p.press(&mac(MacroMode::WhileHeld, hold_a(5)), "t");
        wait_until("several iterations", || rec.events().len() >= 6);
        p.release("t");
        wait_idle(&p, "t");
        let events = rec.events();
        assert!(events.len() >= 6 && events.len() % 2 == 0, "{events:?}");
        for pair in events.chunks(2) {
            assert_eq!(pair, [Event::KeyDown(0x04), Event::KeyUp(0x04)]);
        }
        // Nothing more arrives once the worker has stopped.
        thread::sleep(Duration::from_millis(20));
        assert_eq!(rec.events(), events);
    }

    #[test]
    fn while_held_release_does_not_cut_the_current_iteration_short() {
        let (p, rec) = setup();
        p.press(&mac(MacroMode::WhileHeld, hold_a(60)), "t");
        wait_until("key down", || !rec.events().is_empty());
        p.release("t");
        wait_idle(&p, "t");
        // Released mid-delay: the iteration still ends with its own key-up.
        assert_eq!(rec.events(), vec![Event::KeyDown(0x04), Event::KeyUp(0x04)]);
    }

    #[test]
    fn toggle_starts_on_first_press_and_stops_on_the_second() {
        let (p, rec) = setup();
        let m = mac(MacroMode::Toggle, hold_a(5));
        p.press(&m, "t");
        wait_until("iterations", || rec.events().len() >= 4);
        p.release("t"); // releasing a toggle changes nothing
        thread::sleep(Duration::from_millis(15));
        assert!(p.is_running("t"));

        p.press(&m, "t");
        wait_idle(&p, "t");
        let events = rec.events();
        assert_eq!(events.last(), Some(&Event::KeyUp(0x04)), "{events:?}");
        thread::sleep(Duration::from_millis(20));
        assert_eq!(rec.events(), events);

        // A third press starts it again.
        p.press(&m, "t");
        wait_until("restart", || rec.events().len() > events.len());
        p.stop_all();
    }

    #[test]
    fn toggle_off_mid_delay_is_immediate_and_releases_held_keys() {
        let (p, rec) = setup();
        let m = mac(
            MacroMode::Toggle,
            vec![MacroStep::KeyDown(0x04), MacroStep::Delay(60_000)],
        );
        p.press(&m, "t");
        wait_until("key down", || !rec.events().is_empty());
        let started = Instant::now();
        p.press(&m, "t");
        wait_idle(&p, "t");
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(rec.events(), vec![Event::KeyDown(0x04), Event::KeyUp(0x04)]);
    }

    #[test]
    fn stop_all_interrupts_delays_and_releases_everything_held() {
        let (p, rec) = setup();
        let steps = vec![
            MacroStep::KeyDown(0xe1),
            MacroStep::KeyDown(0x04),
            MacroStep::MouseDown(MouseButton::Left),
            MacroStep::MouseDown(MouseButton::WheelLeft),
            MacroStep::Delay(60_000),
            MacroStep::KeyUp(0x04),
        ];
        p.press(&mac(MacroMode::Once, steps), "t");
        wait_until("steps to run", || rec.events().len() >= 4);
        let started = Instant::now();
        p.stop_all();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "stop_all waited out the delay"
        );
        assert!(!p.is_running("t"));
        assert_eq!(
            rec.events(),
            vec![
                Event::KeyDown(0xe1),
                Event::KeyDown(0x04),
                Event::MouseDown(MouseButton::Left),
                Event::MouseDown(MouseButton::WheelLeft),
                // Released newest first; the wheel "button" is never held.
                Event::MouseUp(MouseButton::Left),
                Event::KeyUp(0x04),
                Event::KeyUp(0xe1),
            ]
        );
    }

    #[test]
    fn keys_released_by_the_macro_are_not_released_again() {
        let (p, rec) = setup();
        p.press(&mac(MacroMode::Once, hold_a(1)), "t");
        wait_idle(&p, "t");
        p.stop_all();
        assert_eq!(rec.events(), vec![Event::KeyDown(0x04), Event::KeyUp(0x04)]);
    }

    #[test]
    fn dropping_the_player_stops_playback() {
        let (p, rec) = setup();
        p.press(
            &mac(
                MacroMode::Toggle,
                vec![MacroStep::KeyDown(0x04), MacroStep::Delay(60_000)],
            ),
            "t",
        );
        wait_until("key down", || !rec.events().is_empty());
        drop(p);
        assert_eq!(rec.events(), vec![Event::KeyDown(0x04), Event::KeyUp(0x04)]);
    }

    #[test]
    fn sink_error_ends_the_macro_and_releases_held_keys() {
        let rec = Recorder::failing_on(0x05);
        let p = player_for(rec.clone());
        let steps = vec![
            MacroStep::KeyDown(0x04),
            MacroStep::KeyDown(0x05),
            MacroStep::KeyUp(0x04),
        ];
        p.press(&mac(MacroMode::Once, steps), "t");
        wait_idle(&p, "t");
        // The failed key is released too, in case the send half-succeeded.
        assert_eq!(
            rec.events(),
            vec![Event::KeyDown(0x04), Event::KeyUp(0x05), Event::KeyUp(0x04)]
        );
    }

    #[test]
    fn empty_macro_does_nothing() {
        let (p, rec) = setup();
        p.press(&mac(MacroMode::WhileHeld, Vec::new()), "t");
        assert!(!p.is_running("t"));
        assert!(rec.events().is_empty());
    }

    #[test]
    fn zero_delay_loops_are_throttled() {
        let (p, rec) = setup();
        let steps = vec![MacroStep::KeyDown(0x04), MacroStep::KeyUp(0x04)];
        p.press(&mac(MacroMode::Toggle, steps), "t");
        thread::sleep(Duration::from_millis(60));
        p.stop_all();
        let n = rec.events().len();
        assert!((2..500).contains(&n), "{n} events in 60 ms");
    }
}



#[cfg(test)]
mod finite_repeat_tests{
 use super::*;
 use crate::testutil::{Event,Recorder};
 // A 1ms condvar timeout is not an upper bound on Windows scheduling latency.
 fn wait_done(p:&MacroPlayer){let end=std::time::Instant::now()+Duration::from_secs(10);while p.is_running("finite"){assert!(std::time::Instant::now()<end);thread::sleep(Duration::from_millis(1));}}
 #[test]fn finite_count_stops_at_exact_total_and_does_not_hold_inputs(){for count in [1,2,3,252]{let rec=Recorder::default();let p=MacroPlayer::new(Arc::new(Mutex::new(rec.clone())));let mut m=Macro::new("finite");m.mode=MacroMode::Repeat(count);m.steps=vec![MacroStep::KeyDown(4),MacroStep::KeyUp(4)];let started=std::time::Instant::now();p.press(&m,"finite");wait_done(&p);println!("finite count {count}: {:?}",started.elapsed());let expected=(0..count).flat_map(|_|[Event::KeyDown(4),Event::KeyUp(4)]).collect::<Vec<_>>();assert_eq!(rec.events(),expected);p.stop_all();assert_eq!(rec.events(),expected);}}
 #[test]fn zero_count_never_starts_an_unlimited_worker(){let rec=Recorder::default();let p=MacroPlayer::new(Arc::new(Mutex::new(rec.clone())));let mut m=Macro::new("finite");m.mode=MacroMode::Repeat(0);m.steps=vec![MacroStep::KeyDown(4)];p.press(&m,"finite");assert!(!p.is_running("finite"));assert!(rec.events().is_empty());}
 #[test]fn aborting_finite_repeat_releases_held_keys_without_running_remaining_plays(){let rec=Recorder::default();let p=MacroPlayer::new(Arc::new(Mutex::new(rec.clone())));let mut m=Macro::new("finite");m.mode=MacroMode::Repeat(252);m.steps=vec![MacroStep::KeyDown(4),MacroStep::Delay(60000),MacroStep::KeyUp(4)];p.press(&m,"finite");let end=std::time::Instant::now()+Duration::from_secs(3);while rec.events().is_empty(){assert!(std::time::Instant::now()<end);thread::sleep(Duration::from_millis(1));}p.stop_all();assert_eq!(rec.events(),vec![Event::KeyDown(4),Event::KeyUp(4)]);assert!(!p.is_running("finite"));}
}

#[cfg(test)]mod finite_release_tests{
 use super::*;use crate::testutil::{Recorder,Event};
 #[test]fn finite_repeat_ignores_release_and_duplicate_press_until_count_finishes(){let rec=Recorder::default();let p=MacroPlayer::new(Arc::new(Mutex::new(rec.clone())));let mut m=Macro::new("finite");m.mode=MacroMode::Repeat(3);m.steps=vec![MacroStep::KeyDown(4),MacroStep::Delay(40),MacroStep::KeyUp(4)];p.press(&m,"finite");let end=std::time::Instant::now()+Duration::from_secs(3);while rec.events().is_empty(){assert!(std::time::Instant::now()<end);thread::sleep(Duration::from_millis(1));}p.release("finite");p.press(&m,"finite");while p.is_running("finite"){assert!(std::time::Instant::now()<end);thread::sleep(Duration::from_millis(1));}assert_eq!(rec.events(),(0..3).flat_map(|_|[Event::KeyDown(4),Event::KeyUp(4)]).collect::<Vec<_>>());}
}
