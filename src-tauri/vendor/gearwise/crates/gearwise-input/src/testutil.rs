//! A recording [`InputSink`] for tests.

use std::sync::{Arc, Mutex};

use gearwise_core::model::MouseButton;
use gearwise_core::{Error, Result};

use crate::InputSink;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    KeyDown(u8),
    KeyUp(u8),
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    Scroll(i32, i32),
    Media(u16),
}

/// Records every call; clones share one log.
#[derive(Clone, Default)]
pub(crate) struct Recorder {
    log: Arc<Mutex<Vec<Event>>>,
    fail_key_down: Option<u8>,
}

impl Recorder {
    /// A recorder whose `key_down` of `usage` fails without recording.
    pub(crate) fn failing_on(usage: u8) -> Self {
        Self {
            fail_key_down: Some(usage),
            ..Self::default()
        }
    }

    pub(crate) fn events(&self) -> Vec<Event> {
        self.log.lock().unwrap().clone()
    }

    fn push(&self, e: Event) -> Result<()> {
        self.log.lock().unwrap().push(e);
        Ok(())
    }
}

impl InputSink for Recorder {
    fn key_down(&mut self, usage: u8) -> Result<()> {
        if self.fail_key_down == Some(usage) {
            return Err(Error::transport("injected failure"));
        }
        self.push(Event::KeyDown(usage))
    }
    fn key_up(&mut self, usage: u8) -> Result<()> {
        self.push(Event::KeyUp(usage))
    }
    fn mouse_down(&mut self, button: MouseButton) -> Result<()> {
        self.push(Event::MouseDown(button))
    }
    fn mouse_up(&mut self, button: MouseButton) -> Result<()> {
        self.push(Event::MouseUp(button))
    }
    fn scroll(&mut self, dx: i32, dy: i32) -> Result<()> {
        self.push(Event::Scroll(dx, dy))
    }
    fn media(&mut self, consumer_usage: u16) -> Result<()> {
        self.push(Event::Media(consumer_usage))
    }
}
