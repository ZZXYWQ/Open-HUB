//! Linux sink: one virtual keyboard + mouse created through `/dev/uinput`.
//!
//! Events enter the kernel input layer, so they reach X11 and Wayland
//! sessions alike. Needs write access to `/dev/uinput` (a udev rule).

use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::time::Duration;
use std::{mem, ptr, slice, thread};

use gearwise_core::model::MouseButton;
use gearwise_core::{Error, Result};

use crate::{InputSink, evdev, wheel_ticks};

const DEVICE_PATH: &str = "/dev/uinput";
const DEVICE_NAME: &str = "Gearwise virtual input";
/// Time for the desktop to notice the new device; events sent earlier are
/// silently dropped.
const SETTLE: Duration = Duration::from_millis(250);

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const SYN_REPORT: u16 = 0;
const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;
const REL_HWHEEL: u16 = 0x06;
const REL_WHEEL: u16 = 0x08;
const BUS_VIRTUAL: u16 = 0x06;

const UINPUT_IOCTL_BASE: u32 = b'U' as u32;
const UI_DEV_CREATE: libc::Ioctl = libc::_IO(UINPUT_IOCTL_BASE, 1);
const UI_DEV_DESTROY: libc::Ioctl = libc::_IO(UINPUT_IOCTL_BASE, 2);
const UI_DEV_SETUP: libc::Ioctl = libc::_IOW::<libc::uinput_setup>(UINPUT_IOCTL_BASE, 3);
const UI_SET_EVBIT: libc::Ioctl = libc::_IOW::<libc::c_int>(UINPUT_IOCTL_BASE, 100);
const UI_SET_KEYBIT: libc::Ioctl = libc::_IOW::<libc::c_int>(UINPUT_IOCTL_BASE, 101);
const UI_SET_RELBIT: libc::Ioctl = libc::_IOW::<libc::c_int>(UINPUT_IOCTL_BASE, 102);

/// The virtual input device. Dropping it removes the device.
pub(crate) struct UinputSink {
    file: File,
}

impl UinputSink {
    pub(crate) fn new() -> Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(DEVICE_PATH)
            .map_err(|e| match e.kind() {
                ErrorKind::PermissionDenied => Error::PermissionDenied(DEVICE_PATH.into()),
                ErrorKind::NotFound => {
                    Error::Unsupported("/dev/uinput is missing (is the uinput module loaded?)")
                }
                _ => Error::Io(e),
            })?;
        let sink = Self { file };
        sink.configure()?;
        thread::sleep(SETTLE);
        Ok(sink)
    }

    fn configure(&self) -> Result<()> {
        self.set_bit(UI_SET_EVBIT, EV_KEY)?;
        self.set_bit(UI_SET_EVBIT, EV_REL)?;
        for code in evdev::all_codes() {
            self.set_bit(UI_SET_KEYBIT, code)?;
        }
        // REL_X/REL_Y are never sent; declaring them makes udev and libinput
        // classify the device as a pointer so the wheel events are honoured.
        for code in [REL_X, REL_Y, REL_HWHEEL, REL_WHEEL] {
            self.set_bit(UI_SET_RELBIT, code)?;
        }

        // SAFETY: `uinput_setup` is plain old data; all-zero is valid.
        let mut setup: libc::uinput_setup = unsafe { mem::zeroed() };
        setup.id.bustype = BUS_VIRTUAL;
        setup.id.vendor = 0x1209; // pid.codes: open hardware / test vendor id
        setup.id.product = 0x0001;
        for (dst, src) in setup.name.iter_mut().zip(DEVICE_NAME.bytes()) {
            *dst = src as libc::c_char;
        }
        // SAFETY: valid fd; the request takes a `uinput_setup` pointer.
        cvt(unsafe { libc::ioctl(self.file.as_raw_fd(), UI_DEV_SETUP, ptr::from_ref(&setup)) })?;
        // SAFETY: valid fd; the request takes no argument.
        cvt(unsafe { libc::ioctl(self.file.as_raw_fd(), UI_DEV_CREATE) })
    }

    fn set_bit(&self, request: libc::Ioctl, value: u16) -> Result<()> {
        // SAFETY: valid fd; these requests take the bit number by value.
        cvt(unsafe { libc::ioctl(self.file.as_raw_fd(), request, libc::c_ulong::from(value)) })
    }

    /// Write `events` followed by `SYN_REPORT` in a single `write(2)`.
    fn emit(&mut self, events: &[(u16, u16, i32)]) -> Result<()> {
        let mut batch: Vec<libc::input_event> = Vec::with_capacity(events.len() + 1);
        for &(type_, code, value) in events.iter().chain(&[(EV_SYN, SYN_REPORT, 0)]) {
            // SAFETY: `input_event` is plain old data; all-zero is valid
            // (the kernel stamps the time itself).
            let mut ev: libc::input_event = unsafe { mem::zeroed() };
            ev.type_ = type_;
            ev.code = code;
            ev.value = value;
            batch.push(ev);
        }
        // SAFETY: a `Vec<input_event>` is a contiguous run of initialised
        // plain-old-data structs of exactly this byte length.
        let bytes = unsafe {
            slice::from_raw_parts(
                batch.as_ptr().cast::<u8>(),
                batch.len() * mem::size_of::<libc::input_event>(),
            )
        };
        self.file.write_all(bytes)?;
        Ok(())
    }

    fn key(&mut self, usage: u8, down: bool) -> Result<()> {
        let code = evdev::key(usage).ok_or(Error::Unsupported("key has no Linux key code"))?;
        self.emit(&[(EV_KEY, code, i32::from(down))])
    }

    fn button(&mut self, button: MouseButton, down: bool) -> Result<()> {
        match evdev::button(button) {
            Some(code) => self.emit(&[(EV_KEY, code, i32::from(down))]),
            None => Ok(()), // wheel pseudo-buttons are handled by the caller
        }
    }
}

impl Drop for UinputSink {
    fn drop(&mut self) {
        // SAFETY: valid fd; the request takes no argument. Closing the fd
        // would destroy the device too, this just makes it explicit.
        if unsafe { libc::ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY) } < 0 {
            log::debug!("UI_DEV_DESTROY failed: {}", io::Error::last_os_error());
        }
    }
}

impl InputSink for UinputSink {
    fn key_down(&mut self, usage: u8) -> Result<()> {
        self.key(usage, true)
    }

    fn key_up(&mut self, usage: u8) -> Result<()> {
        self.key(usage, false)
    }

    fn mouse_down(&mut self, button: MouseButton) -> Result<()> {
        match wheel_ticks(button) {
            Some((dx, dy)) => self.scroll(dx, dy),
            None => self.button(button, true),
        }
    }

    fn mouse_up(&mut self, button: MouseButton) -> Result<()> {
        self.button(button, false)
    }

    fn scroll(&mut self, dx: i32, dy: i32) -> Result<()> {
        let mut events = Vec::with_capacity(2);
        if dx != 0 {
            events.push((EV_REL, REL_HWHEEL, dx));
        }
        if dy != 0 {
            events.push((EV_REL, REL_WHEEL, dy));
        }
        if events.is_empty() {
            return Ok(());
        }
        self.emit(&events)
    }

    fn media(&mut self, consumer_usage: u16) -> Result<()> {
        let code = evdev::media(consumer_usage)
            .ok_or(Error::Unsupported("media key has no Linux key code"))?;
        self.emit(&[(EV_KEY, code, 1)])?;
        self.emit(&[(EV_KEY, code, 0)])
    }
}

fn cvt(ret: libc::c_int) -> Result<()> {
    if ret < 0 {
        Err(io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_numbers_match_uinput_h() {
        // Values from <linux/uinput.h> on x86-64 / arm64 (generic _IOC layout).
        #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
        {
            assert_eq!(UI_DEV_CREATE, 0x5501);
            assert_eq!(UI_DEV_DESTROY, 0x5502);
            assert_eq!(UI_DEV_SETUP, 0x405c_5503);
            assert_eq!(UI_SET_EVBIT, 0x4004_5564);
            assert_eq!(UI_SET_KEYBIT, 0x4004_5565);
            assert_eq!(UI_SET_RELBIT, 0x4004_5566);
        }
    }

    #[test]
    fn input_event_has_the_kernel_layout() {
        // 64-bit kernels: 16-byte timeval + u16 + u16 + i32.
        #[cfg(target_pointer_width = "64")]
        assert_eq!(mem::size_of::<libc::input_event>(), 24);
    }
}
