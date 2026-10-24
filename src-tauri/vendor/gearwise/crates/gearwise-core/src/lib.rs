//! Vendor-neutral core of Gearwise.
//!
//! * [`transport`] — HID I/O abstraction (real backend in `gearwise-hid`).
//! * [`device`] — the [`Device`] / [`Driver`] contract drivers implement.
//! * [`model`] — DPI, lighting, button-action value types.
//! * [`profile`] — host profiles, macros and preferences.
//! * [`manager`] — discovery and device lifetime.
//! * [`demo`] — simulated devices.

pub mod demo;
pub mod device;
pub mod error;
pub mod keys;
pub mod manager;
pub mod model;
pub mod profile;
pub mod transport;

pub use device::{
    Capabilities, Device, DeviceDescriptor, DeviceEvent, DeviceState, Driver, Setting,
};
pub use error::{Error, Result};
pub use manager::{DeviceManager, ScanIssue, ScanReport};
pub use transport::{HidBackend, HidInterfaceInfo, HidTransport};
