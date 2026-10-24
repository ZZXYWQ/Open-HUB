//! Prints the focused application once a second for ten seconds.
//!
//! Run with `cargo run -p gearwise-input --example focus`, then switch windows.

use std::thread::sleep;
use std::time::Duration;

fn main() {
    for _ in 0..10 {
        match gearwise_input::focused_app() {
            Some(app) => println!("focused: {app}"),
            None => println!("focused: (unknown)"),
        }
        sleep(Duration::from_secs(1));
    }
}
