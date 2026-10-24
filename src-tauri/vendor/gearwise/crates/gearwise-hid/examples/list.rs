//! Print the HID interfaces `gearwise-hid` enumerates.
//!
//!     cargo run -p gearwise-hid --example list           # supported vendors only
//!     cargo run -p gearwise-hid --example list -- --all  # every HID interface
//!
//! Enumeration never opens a device, so this needs no special permissions.

use gearwise_core::HidBackend;
use gearwise_hid::{HidapiBackend, VendorFilter};

fn main() {
    let all = std::env::args().skip(1).any(|a| a == "--all");
    let filter = if all {
        VendorFilter::any()
    } else {
        VendorFilter::default()
    };

    let backend = match HidapiBackend::with_filter(filter) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot start HID backend: {e}");
            std::process::exit(1);
        }
    };
    let mut interfaces = match backend.enumerate() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("enumeration failed: {e}");
            std::process::exit(1);
        }
    };
    interfaces.sort_by(|a, b| a.path.cmp(&b.path));

    if interfaces.is_empty() {
        println!("no matching HID interfaces found");
        return;
    }
    for i in &interfaces {
        println!(
            "{:04x}:{:04x} if={:<2} usage={:04x}/{:04x} {:?} {:<20} {}",
            i.vendor_id,
            i.product_id,
            i.interface_number,
            i.usage_page,
            i.usage,
            i.bus,
            i.product.as_deref().unwrap_or("?"),
            i.path,
        );
    }
    println!("{} interface(s)", interfaces.len());
}
