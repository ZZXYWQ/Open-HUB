//! Export and check complete offline storage images for firmware emulation.
use gearwise_steelseries::{
    prime_macro::event::{Event, Page},
    prime_macro_block::binding,
    prime_macro_once,
    prime_macro_storage::{self, Edit, Image, OBSERVED_BYTES},
};
use serde_json::{Value, json};
fn events(usage: u8) -> Vec<Event> {
    vec![
        Event::Key {
            usage,
            page: Page::Keyboard,
            down: true,
        },
        Event::Key {
            usage,
            page: Page::Keyboard,
            down: false,
        },
    ]
}
fn build(index: usize) -> Edit {
    assert!(index < 3);
    let offset = if index == 1 { 504 } else { 64 };
    let mut memory = vec![0x4a; OBSERVED_BYTES];
    let mut physical = vec![0x51, 1, 0, 0, 0].repeat(6);
    physical[25..].copy_from_slice(&[0x30, 0, 0, 0, 0]);
    let mut wheel = vec![0x31, 0, 0, 0, 0, 0x32, 0, 0, 0, 0];
    for (slot, at) in [offset, 600, 1200, 2048].into_iter().enumerate() {
        let mut body = events(4);
        body.insert(1, Event::Delay(7));
        let mut raw = prime_macro_once::compile(&body).unwrap().encode().unwrap();
        if slot == 3 {
            raw[1] = 6;
            raw[8] = 0xfd;
        }
        memory[at..at + raw.len()].copy_from_slice(&raw);
        physical[(slot + 1) * 5..(slot + 2) * 5].copy_from_slice(&binding(at).unwrap());
    }
    let control = if index == 2 {
        physical[5..10].copy_from_slice(&[0x51, 1, 0, 0, 0]);
        wheel[..5].copy_from_slice(&binding(offset).unwrap());
        7
    } else {
        2
    };
    let before = Image::new(
        &format!("mock-prime-{index}/connection-1"),
        &memory,
        &physical,
        &wheel,
    )
    .unwrap();
    prime_macro_storage::replace_once_in_place(&before, control, &events(5)).unwrap()
}
fn frames(packets: impl Iterator<Item = Vec<u8>>) -> Vec<Vec<u8>> {
    packets
        .map(|p| {
            let mut f = vec![0];
            f.extend(p);
            f
        })
        .collect()
}
fn bytes(value: &Value) -> Vec<u8> {
    serde_json::from_value(value.clone()).unwrap()
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--check-observations") {
        let data = std::fs::read_to_string(args.get(2).expect("observation file")).unwrap();
        let observed: Value = serde_json::from_str(data.trim_start_matches('\u{feff}')).unwrap();
        let rows = observed.as_array().unwrap();
        assert_eq!(rows.len(), 30);
        let mut allowed = 0;
        let mut refused = 0;
        for row in rows {
            let index = row["case"].as_u64().unwrap() as usize;
            let edit = build(index);
            let memory = bytes(&row["memory"]);
            let physical = bytes(&row["physical"]);
            let wheel = bytes(&row["wheel"]);
            let image = Image::new(row["scope"].as_str().unwrap(), &memory, &physical, &wheel);
            let result = image.and_then(|image| {
                edit.restore_with_attempts(
                    &image,
                    row["attempted"].as_u64().unwrap() as usize,
                    row["restore_attempted"].as_u64().unwrap_or(0) as usize,
                )
            });
            match row["kind"].as_str().unwrap() {
                "prefix-0" => {
                    let r = result.unwrap();
                    assert!(!r.needed && r.packets.is_empty());
                    allowed += 1;
                }
                "prefix-1" | "prefix-2" => {
                    let r = result.unwrap();
                    assert!(r.needed);
                    assert_eq!(
                        frames(r.packets.iter().map(|p| p.bytes.clone())),
                        serde_json::from_value::<Vec<Vec<u8>>>(row["restore_frames"].clone())
                            .unwrap()
                    );
                    allowed += 1;
                }
                "restore-prefix-1" | "restore-prefix-2" => {
                    let r = result.unwrap();
                    assert_eq!(r.needed, index == 1 && row["kind"] == "restore-prefix-1");
                    assert_eq!(
                        frames(r.packets.iter().map(|p| p.bytes.clone())),
                        serde_json::from_value::<Vec<Vec<u8>>>(row["restore_frames"].clone())
                            .unwrap()
                    );
                    allowed += 1;
                }
                "foreign-memory" | "foreign-table" | "other-connection" | "short-read"
                | "foreign-target" => {
                    assert!(result.is_err());
                    refused += 1;
                }
                _ => panic!("unreviewed observation type"),
            }
        }
        assert_eq!((allowed, refused), (15, 15));
        println!(
            "Verified actual emulated observations: {allowed} recognized prefix states, {refused} unsafe/foreign/incomplete states refused"
        );
        return;
    }
    let cases: Vec<_> = (0..3)
        .map(|index| {
            let edit = build(index);
            let before = edit.before();
            let scope = format!("mock-prime-{index}/connection-1");
            let states: Vec<_> = (0..=2)
                .map(|prefix| {
                    let mut memory = before.memory().to_vec();
                    memory[..prefix * 512].copy_from_slice(&edit.after().memory()[..prefix * 512]);
                    let current =
                        Image::new(&scope, &memory, before.physical(), before.wheel()).unwrap();
                    let restore = edit.restore(&current, prefix).unwrap();
                    json!({
                        "prefix":prefix, "memory":memory, "needed":restore.needed,
                        "restore_frames":frames(restore.packets.iter().map(|p| p.bytes.clone()))
                    })
                })
                .collect();
            let restore_states: Vec<_> = (1..=2)
                .map(|prefix| {
                    let mut memory = edit.after().memory().to_vec();
                    memory[..prefix * 512].copy_from_slice(&before.memory()[..prefix * 512]);
                    let current =
                        Image::new(&scope, &memory, before.physical(), before.wheel()).unwrap();
                    let restore = edit.restore_with_attempts(&current, 2, prefix).unwrap();
                    json!({
                        "prefix":prefix, "memory":memory, "needed":restore.needed,
                        "restore_frames":frames(restore.packets.iter().map(|p| p.bytes.clone()))
                    })
                })
                .collect();
            let mut same = events(4);
            same.insert(1, Event::Delay(7));
            let control = if index == 2 { 7 } else { 2 };
            let noop = prime_macro_storage::replace_once_in_place(before, control, &same).unwrap();
            json!({
                "case":index, "control":control, "scope":scope,
                "before":before.memory(), "after":edit.after().memory(),
                "physical":before.physical(), "wheel":before.wheel(),
                "frames":frames(edit.packets().iter().map(|p| p.bytes.clone())),
                "states":states, "restore_states":restore_states,
                "noop_frames":frames(noop.packets().iter().map(|p| p.bytes.clone()))
            })
        })
        .collect();
    println!("{}", json!({"cases":cases}));
}
