//! Production macro compiler/storage packets for isolated original-firmware tests.
use gearwise_steelseries::{
    prime_macro::event::{Event, Page},
    prime_macro_block::binding,
    prime_macro_once,
    prime_macro_program::{self, Playback, Trigger},
    prime_macro_storage::{Image, replace_program_in_place},
};
use serde_json::json;
fn events(delay: u16) -> Vec<Event> {
    vec![
        Event::Key {
            usage: 4,
            page: Page::Keyboard,
            down: true,
        },
        Event::Delay(delay),
        Event::Key {
            usage: 4,
            page: Page::Keyboard,
            down: false,
        },
    ]
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--check-journals") {
        let root = std::path::Path::new(args.get(2).expect("retained journal root"));
        for name in [
            "commit",
            "failed-forward",
            "interrupted-restore",
            "foreign-change",
            "cancel-before-feature",
        ] {
            let directory = root.join(name);
            let data: serde_json::Value =
                serde_json::from_slice(&std::fs::read(directory.join("intent.json")).unwrap())
                    .unwrap();
            let scope = serde_json::from_value(data["payload"]["scope"].clone()).unwrap();
            let journal =
                gearwise_steelseries::prime_macro_journal::Journal::open(&directory, &scope)
                    .unwrap();
            println!(
                "{}",
                json!({"case":name,"phase":format!("{:?}",journal.phase()),"forward_attempts":journal.forward_attempts(),"restore_attempts":journal.restore_attempts()})
            );
        }
        return;
    }

    let mut memory = vec![0x4a; 5120];
    let mut physical = [0x51, 1, 0, 0, 0].repeat(6);
    physical[25..].copy_from_slice(&[0x30, 0, 0, 0, 0]);
    let mut old = events(9);
    old.push(Event::Delay(1));
    let raw = prime_macro_once::compile(&old).unwrap().encode().unwrap();
    memory[504..504 + raw.len()].copy_from_slice(&raw);
    physical[5..10].copy_from_slice(&binding(504).unwrap());
    let mut foreign = raw.clone();
    foreign[1] = 6;
    foreign[8] = 0xfd;
    memory[1200..1200 + foreign.len()].copy_from_slice(&foreign);
    physical[10..15].copy_from_slice(&binding(1200).unwrap());
    let wheel = [0x31, 0, 0, 0, 0, 0x32, 0, 0, 0, 0];
    let before = Image::new("fixture-open-a", &memory, &physical, &wheel).unwrap();
    let mut cases = vec![];
    for delay in [0, 1, 3] {
        for trigger in [Trigger::Press, Trigger::Release] {
            let mut modes = vec![Playback::Once, Playback::Toggle];
            if trigger == Trigger::Press {
                modes.push(Playback::WhileHeld);
            }
            for count in [1, 2, 3, 255] {
                for interval_ticks in [0, 1, 7] {
                    modes.push(Playback::Counted {
                        count,
                        interval_ticks,
                    });
                }
            }
            for mode in modes {
                let block = prime_macro_program::compile(&events(delay), mode, trigger)
                    .unwrap()
                    .encode()
                    .unwrap();
                let edit =
                    replace_program_in_place(&before, 2, &events(delay), mode, trigger).unwrap();
                let frames: Vec<_> = edit
                    .packets()
                    .iter()
                    .map(|p| {
                        let mut out = vec![0];
                        out.extend_from_slice(&p.bytes);
                        out
                    })
                    .collect();
                cases.push(json!({"mode":mode,"trigger":trigger,"body_delay":delay,"block":block,"frames":frames,"after":edit.after().memory()}));
            }
        }
    }
    println!(
        "{}",
        json!({"before":before.memory(),"physical":physical,"wheel":wheel,"cases":cases})
    );
}
