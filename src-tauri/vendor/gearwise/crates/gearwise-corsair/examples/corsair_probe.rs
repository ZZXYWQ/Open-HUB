//! Read-only probe: open every Corsair device and print what the
//! driver discovers. Changes nothing except switching a device that was left
//! in host mode back to its onboard profiles.
//!
//!     cargo run -p gearwise-corsair --example corsair_probe

use gearwise_core::DeviceManager;
use gearwise_corsair::CorsairDriver;
use gearwise_hid::HidapiBackend;

struct StderrLog;

impl log::Log for StderrLog {
    fn enabled(&self, _: &log::Metadata) -> bool {
        std::env::var_os("PROBE_DEBUG").is_some()
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            eprintln!("[{}] {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}

fn main() {
    log::set_logger(&StderrLog).ok();
    log::set_max_level(log::LevelFilter::Debug);
    let backend = HidapiBackend::new().expect("hidapi");
    let mut manager = DeviceManager::new(Box::new(backend), vec![Box::new(CorsairDriver)]);
    let report = manager.scan();
    for issue in &report.issues {
        println!(
            "! {:04x}:{:04x} {}: {}",
            issue.vendor_id, issue.product_id, issue.path, issue.message
        );
    }
    for d in manager.devices_mut() {
        let desc = d.descriptor().clone();
        println!(
            "\n== {} [{}] {:?} {:?}",
            desc.name, desc.model_key, desc.kind, desc.connection
        );
        println!("   id {}  firmware {:?}", desc.id, desc.firmware);
        let caps = d.capabilities().clone();
        println!("   dpi {:?}", caps.dpi);
        println!("   report rates {:?}", caps.report_rates);
        println!(
            "   onboard profiles {}  battery {}",
            caps.onboard_profiles, caps.battery
        );
        for z in &caps.lighting {
            println!("   zone {} {:?}: {:?}", z.id, z.name, z.effects);
        }
        if let Some(l) = &caps.per_key {
            println!("   per-key: {} ({} keys)", l.name, l.keys.len());
        }
        for b in &caps.buttons {
            println!(
                "   button {:#06x} {:?} default={}",
                b.id,
                b.name,
                b.default_action.label()
            );
        }
        match d.read_state() {
            Ok(s) => {
                println!("   state dpi {:?}", s.dpi);
                println!(
                    "   state rate {:?} profile {:?} battery {:?}",
                    s.report_rate, s.onboard_profile, s.battery
                );
                for (z, st) in &s.lighting {
                    println!("   state zone {z}: {:?} @ {}%", st.effect, st.brightness);
                }
                for (b, a) in &s.buttons {
                    println!("   state button {b:#06x} = {}", a.label());
                }
            }
            Err(e) => println!("   read_state failed: {e}"),
        }
    }
    if manager.devices().count() == 0 {
        println!("no Corsair devices opened");
    }
}
