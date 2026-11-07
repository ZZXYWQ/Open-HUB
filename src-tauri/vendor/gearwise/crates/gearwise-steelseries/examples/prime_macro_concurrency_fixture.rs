use gearwise_steelseries::{
    prime_macro::event::{Event, Page},
    prime_macro_block::binding,
    prime_macro_concurrency, prime_macro_once,
    prime_macro_storage::Image,
};
use serde_json::json;
fn case(peaks: [u8; 3], alias: bool) -> serde_json::Value {
    let mut m = vec![0x4a; 5120];
    let mut p = [0x51, 1, 0, 0, 0].repeat(6);
    p[25..].copy_from_slice(&[0x30, 0, 0, 0, 0]);
    let w = [0x31, 0, 0, 0, 0, 0x32, 0, 0, 0, 0];
    let mut programs = vec![];
    for (i, peak) in peaks.iter().enumerate() {
        let mut events = vec![];
        let first = if alias { 1 } else { 1 + i as u8 * 10 };
        for usage in first..first + peak {
            events.push(Event::Key {
                usage,
                page: Page::Consumer,
                down: true,
            });
        }
        events.push(Event::Delay(20));
        for usage in first..first + peak {
            events.push(Event::Key {
                usage,
                page: Page::Consumer,
                down: false,
            });
        }
        let raw = prime_macro_once::compile(&events)
            .unwrap()
            .encode()
            .unwrap();
        let at = if alias { 64 } else { 64 + i * 128 };
        m[at..at + raw.len()].copy_from_slice(&raw);
        p[(i + 1) * 5..(i + 2) * 5].copy_from_slice(&binding(at).unwrap());
        programs.push(json!({"offset":at,"block":raw,"binding":binding(at).unwrap()}));
    }
    let image = Image::new("mock-open-a", &m, &p, &w).unwrap();
    let admitted: Vec<_> = (0..=4)
        .map(|reserved| prime_macro_concurrency::analyze(&image, reserved).is_ok())
        .collect();
    json!({"peaks":peaks,"alias":alias,"programs":programs,"admitted":admitted})
}
fn main() {
    let mut cases = vec![];
    for a in 0..=4 {
        for b in 0..=4 {
            for c in 0..=4 {
                cases.push(case([a, b, c], false));
            }
        }
    }
    cases.push(case([1, 1, 1], true));
    println!("{}", json!({"cases":cases}));
}
