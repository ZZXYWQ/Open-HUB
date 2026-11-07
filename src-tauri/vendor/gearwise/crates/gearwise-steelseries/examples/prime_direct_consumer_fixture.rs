use gearwise_steelseries::{
    prime_macro::event::{Event, Page},
    prime_macro_block::binding,
    prime_macro_concurrency, prime_macro_once,
    prime_macro_storage::Image,
};
use serde_json::json;
fn main() {
    let mut cases = vec![];
    for peak in 0..=4 {
        for count in 0..=5 {
            for duplicate in [false, true] {
                if duplicate && count != 5 {
                    continue;
                }
                let mut m = vec![0x4a; 5120];
                let mut p = [1, 0, 0, 0, 0].repeat(6);
                p[25..].copy_from_slice(&[0x30, 0, 0, 0, 0]);
                let mut w = vec![0x31, 0, 0, 0, 0, 0x32, 0, 0, 0, 0];
                let mut events = vec![];
                for usage in 1..=peak {
                    events.push(Event::Key {
                        usage,
                        page: Page::Consumer,
                        down: true,
                    });
                }
                events.push(Event::Delay(20));
                for usage in 1..=peak {
                    events.push(Event::Key {
                        usage,
                        page: Page::Consumer,
                        down: false,
                    });
                }
                let block = prime_macro_once::compile(&events)
                    .unwrap()
                    .encode()
                    .unwrap();
                m[64..64 + block.len()].copy_from_slice(&block);
                p[5..10].copy_from_slice(&binding(64).unwrap());
                let usages = [0xe9, 0xea, 0xe2, 0xcd, 0xb5];
                let mut ordinary = vec![];
                for i in 0..count {
                    let record = [
                        0x61,
                        usages[if duplicate { 0 } else { i }],
                        0xa5,
                        0x5a,
                        0xff,
                    ];
                    ordinary.push(record);
                    if i < 3 {
                        p[(i + 2) * 5..(i + 3) * 5].copy_from_slice(&record);
                    } else {
                        w[(i - 3) * 5..(i - 2) * 5].copy_from_slice(&record);
                    }
                }
                let image = Image::new("mock-a", &m, &p, &w).unwrap();
                let admitted = prime_macro_concurrency::analyze_complete_tables(&image, 0).is_ok();
                cases.push(json!({"peak":peak,"ordinary":ordinary,"duplicate":duplicate,"block":block,"binding":binding(64).unwrap(),"admitted":admitted}));
            }
        }
    }
    println!("{}", json!({"cases":cases}));
}
