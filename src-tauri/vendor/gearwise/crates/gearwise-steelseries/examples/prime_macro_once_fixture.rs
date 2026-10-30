//! Export production once-compiler bytes and independent expected RAM frames.
//! No transport, USB handle, firmware or application input is opened.
use gearwise_steelseries::{
    prime_macro::event::{Event, Page},
    prime_macro_block::binding,
    prime_macro_once::compile,
    prime_macro_transfer::{DECLARED_BYTES, plan},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn vector(name: String, events: Vec<Event>) -> Value {
    let block = compile(&events)
        .expect("admitted one-shot")
        .encode()
        .unwrap();
    let mut keys = BTreeSet::new();
    let mut mouse = 0u32;
    let mut consumer = Vec::new();
    let mut expected = Vec::new();
    for event in events {
        let frames = match event {
            Event::Key {
                usage,
                page: Page::Keyboard,
                down,
            } => {
                if down {
                    keys.insert(usage);
                } else {
                    keys.remove(&usage);
                }
                1
            }
            Event::Key {
                usage,
                page: Page::Consumer,
                down,
            } => {
                if down {
                    consumer.push(usage);
                } else {
                    consumer.retain(|u| *u != usage);
                }
                1
            }
            Event::Mouse { button, down } => {
                let bit = 1 << (button - 1);
                if down {
                    mouse |= bit;
                } else {
                    mouse &= !bit;
                }
                1
            }
            Event::Delay(count) => usize::from(count) + 1,
        };
        let mut consumer_frame = consumer.clone();
        consumer_frame.resize(4, 0);
        for _ in 0..frames {
            expected
                .push(json!({"keys":keys,"mouse":mouse,"consumer":consumer_frame,"active":[2]}));
        }
    }
    expected.push(json!({"keys":[],"mouse":0,"consumer":[0,0,0,0],"active":[]}));
    json!({"name":name,"block":block,"expected":expected})
}
fn key(usage: u8, page: Page, down: bool) -> Event {
    Event::Key { usage, page, down }
}
fn main() {
    let mut programs = Vec::new();
    for page in [Page::Keyboard, Page::Consumer] {
        for usage in 1..=255 {
            programs.push(vector(
                format!("{page:?}-{usage}"),
                vec![key(usage, page, true), key(usage, page, false)],
            ));
        }
    }
    for button in 1..=8 {
        programs.push(vector(
            format!("mouse-{button}"),
            vec![
                Event::Mouse { button, down: true },
                Event::Mouse {
                    button,
                    down: false,
                },
            ],
        ));
    }
    for count in [0, 1, 2, 7, 15, 255] {
        programs.push(vector(
            format!("delay-{count}"),
            vec![
                key(4, Page::Keyboard, true),
                Event::Delay(count),
                key(4, Page::Keyboard, false),
            ],
        ));
    }
    let cons = |usage, down| key(usage, Page::Consumer, down);
    programs.push(vector(
        "consumer-compaction".into(),
        vec![
            cons(1, true),
            cons(2, true),
            cons(3, true),
            cons(4, true),
            cons(2, false),
            cons(5, true),
            cons(1, false),
            cons(3, false),
            cons(4, false),
            cons(5, false),
        ],
    ));
    let before = vec![0x35; DECLARED_BYTES];
    let mut desired = before.clone();
    let sample = compile(&[key(4, Page::Keyboard, true), key(4, Page::Keyboard, false)])
        .unwrap()
        .encode()
        .unwrap();
    desired[64..64 + sample.len()].copy_from_slice(&sample);
    let coverage = plan(&before, &desired, &[64]).unwrap();
    let frames: Vec<Vec<u8>> = coverage
        .packets
        .iter()
        .map(|p| {
            let mut v = vec![0];
            v.extend_from_slice(&p.bytes);
            v
        })
        .collect();
    let mut over_capacity: Vec<_> = (1..=5)
        .map(|usage| key(usage, Page::Consumer, true))
        .collect();
    over_capacity.extend((1..=5).map(|usage| key(usage, Page::Consumer, false)));
    let refused_fifth_consumer = compile(&over_capacity).is_err();
    println!(
        "{}",
        json!({"programs":programs,"refused_fifth_consumer":refused_fifth_consumer,"binding":binding(64).unwrap(),"coverage":{"before":before,"desired":desired,"frames":frames}})
    );
}
