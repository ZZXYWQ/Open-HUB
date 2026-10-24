//! Host-side input for Gearwise.
//!
//! * [`InputSink`] — synthesises keyboard, mouse and media events;
//!   [`create_sink`] picks the platform implementation (`/dev/uinput` on
//!   Linux, `SendInput` on Windows).
//! * [`MacroPlayer`] — plays [`gearwise_core::profile::Macro`]s on worker
//!   threads, honouring [`MacroMode`](gearwise_core::profile::MacroMode).
//! * [`focused_app`] — executable name of the focused window, for per-app
//!   profile switching.
//!
//! ```no_run
//! use std::sync::{Arc, Mutex};
//! use gearwise_input::{MacroPlayer, SharedSink, create_sink};
//!
//! let sink: SharedSink = Arc::new(Mutex::new(create_sink()?));
//! let player = MacroPlayer::new(sink.clone());
//! # Ok::<(), gearwise_core::Error>(())
//! ```

mod focus;
mod player;
#[cfg(test)]
mod testutil;
mod text;

#[cfg(any(target_os = "linux", test))]
mod evdev;
#[cfg(target_os = "linux")]
mod uinput;

#[cfg(windows)]
mod sendinput;
#[cfg(any(windows, test))]
mod winkeys;

use std::sync::{Arc, Mutex};

use gearwise_core::Result;
use gearwise_core::model::MouseButton;

pub use focus::focused_app;
pub use player::MacroPlayer;

/// Something that can inject input events into the desktop.
///
/// Key arguments are HID keyboard usages (page 0x07, see
/// [`gearwise_core::keys`]); media arguments are HID consumer usages (page
/// 0x0C, see [`gearwise_core::model::media`]).
pub trait InputSink {
    fn key_down(&mut self, usage: u8) -> Result<()>;
    fn key_up(&mut self, usage: u8) -> Result<()>;

    /// Press a mouse button. [`MouseButton::WheelLeft`] and
    /// [`MouseButton::WheelRight`] are not held buttons: pressing them emits
    /// one horizontal scroll notch and releasing them does nothing.
    fn mouse_down(&mut self, button: MouseButton) -> Result<()>;
    fn mouse_up(&mut self, button: MouseButton) -> Result<()>;

    /// Scroll by whole wheel notches: positive `dy` scrolls up (away from
    /// the user), positive `dx` scrolls right.
    fn scroll(&mut self, dx: i32, dy: i32) -> Result<()>;

    /// Tap a media key (press and release). Best effort: usages without a
    /// platform equivalent return [`gearwise_core::Error::Unsupported`].
    fn media(&mut self, consumer_usage: u16) -> Result<()>;

    /// Type a string. Best effort: the default implementation covers
    /// printable ASCII, Tab and newline on a **US keyboard layout** by
    /// pressing key usages (plus Shift). On other layouts the produced
    /// characters differ, and characters outside ASCII are skipped with a
    /// warning.
    fn text(&mut self, text: &str) -> Result<()> {
        text::type_text(self, text)
    }
}

impl<T: InputSink + ?Sized> InputSink for Box<T> {
    fn key_down(&mut self, usage: u8) -> Result<()> {
        (**self).key_down(usage)
    }
    fn key_up(&mut self, usage: u8) -> Result<()> {
        (**self).key_up(usage)
    }
    fn mouse_down(&mut self, button: MouseButton) -> Result<()> {
        (**self).mouse_down(button)
    }
    fn mouse_up(&mut self, button: MouseButton) -> Result<()> {
        (**self).mouse_up(button)
    }
    fn scroll(&mut self, dx: i32, dy: i32) -> Result<()> {
        (**self).scroll(dx, dy)
    }
    fn media(&mut self, consumer_usage: u16) -> Result<()> {
        (**self).media(consumer_usage)
    }
    fn text(&mut self, text: &str) -> Result<()> {
        (**self).text(text)
    }
}

/// A sink shared between the macro player and direct callers (for example
/// button remaps that emit a plain keystroke). Build one from
/// [`create_sink`] with `Arc::new(Mutex::new(create_sink()?))`.
pub type SharedSink = Arc<Mutex<dyn InputSink + Send>>;

/// Create the input sink for this platform.
///
/// On Linux this opens `/dev/uinput` and creates the virtual device
/// "Gearwise virtual input" (blocking ~250 ms so the desktop can start
/// listening to it); permission problems surface as
/// [`gearwise_core::Error::PermissionDenied`]. Other platforms return
/// [`gearwise_core::Error::Unsupported`].
pub fn create_sink() -> Result<Box<dyn InputSink + Send>> {
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(uinput::UinputSink::new()?))
    }
    #[cfg(windows)]
    {
        Ok(Box::new(sendinput::SendInputSink))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        Err(gearwise_core::Error::Unsupported(
            "input injection on this platform",
        ))
    }
}

/// The scroll a wheel "button" stands for, or `None` for real buttons.
fn wheel_ticks(button: MouseButton) -> Option<(i32, i32)> {
    match button {
        MouseButton::WheelLeft => Some((-1, 0)),
        MouseButton::WheelRight => Some((1, 0)),
        _ => None,
    }
}
