//! Export an offline coverage plan for original-firmware emulation. No HID.
use gearwise_steelseries::{
    prime_macro::{Metadata, event::Event, event::Page},
    prime_macro_block::Block,
    prime_macro_transfer::{DECLARED_BYTES, plan},
};

fn main() {
    let before = vec![0x35; DECLARED_BYTES];
    let mut desired = before.clone();
    let block = Block {
        metadata: Metadata {
            playback: 1,
            event_count: 2,
            direction: 1,
            repeats: 1,
            repeat_delay: 0,
        },
        events: vec![
            Event::Key {
                usage: 4,
                page: Page::Keyboard,
                down: true,
            },
            Event::Key {
                usage: 4,
                page: Page::Keyboard,
                down: false,
            },
        ],
    }
    .encode()
    .expect("canonical fixture");
    desired[64..64 + block.len()].copy_from_slice(&block);
    let coverage = plan(&before, &desired, &[64]).expect("covered fixture");
    let frames: Vec<Vec<u8>> = coverage
        .packets
        .iter()
        .map(|packet| {
            let mut frame = vec![0];
            frame.extend_from_slice(&packet.bytes);
            frame
        })
        .collect();
    println!(
        "{}",
        serde_json::json!({"before":before,"desired":desired,"frames":frames})
    );
}
