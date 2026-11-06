//! Isolated journal/read/write interoperation; no HID backend or app caller.
use gearwise_core::{Error, Result};
use gearwise_steelseries::{
    prime_image_session::{Scope, Session, Status},
    prime_macro::event::{Event, Page},
    prime_macro_journal::Journal,
    prime_macro_storage::Image,
};
use serde_json::{Value, json};
use std::{path::Path, thread, time::Duration};
#[path = "support/remote_transport.rs"]
mod remote_transport;
use remote_transport::{Remote, receive, rpc};
fn status() -> Result<Status> {
    let reply = rpc(json!({"op":"guard"}))?;
    let scope: Scope = serde_json::from_value(reply["scope"].clone())
        .map_err(|e| Error::protocol(e.to_string()))?;
    Ok(Status {
        scope,
        cancelled: reply["cancelled"]
            .as_bool()
            .ok_or(Error::protocol("missing cancellation"))?,
    })
}
fn guard(scope: &Scope) -> Result<()> {
    let now = status()?;
    if now.cancelled || &now.scope != scope {
        return Err(Error::protocol("journal example lease expired"));
    }
    Ok(())
}
fn observe(scope: &Scope, sequence: &mut u8) -> Result<Image> {
    *sequence = sequence
        .checked_add(1)
        .ok_or(Error::protocol("fixture nonce budget"))?;
    let mut seed = [0x63; 16];
    seed[15] = *sequence; // deterministic test data only
    let observed = Session::new(Remote, seed, scope.clone(), status)?.collect()?;
    Ok(observed.image().clone())
}
fn send(scope: &Scope, packet: &[u8]) -> Result<()> {
    guard(scope)?;
    if packet.len() != 642 || packet[0] != 0x5e {
        return Err(Error::protocol("unexpected journal packet"));
    }
    let mut frame = vec![0];
    frame.extend_from_slice(packet);
    let result = rpc(json!({"op":"feature","data":frame}));
    guard(scope)?;
    thread::sleep(Duration::from_millis(30));
    result.map(|_| ())
}
fn run(input: &Value) -> Result<Value> {
    let scope: Scope = serde_json::from_value(input["scope"].clone())
        .map_err(|e| Error::protocol(e.to_string()))?;
    let directory = Path::new(
        input["journal"]
            .as_str()
            .ok_or(Error::protocol("missing private journal directory"))?,
    );
    let scenario = input["scenario"]
        .as_str()
        .ok_or(Error::protocol("missing scenario"))?;
    let mut nonce = 0;
    let mut current = observe(&scope, &mut nonce)?;
    let original = current.clone();
    let events = [
        Event::Key {
            usage: 6,
            page: Page::Keyboard,
            down: true,
        },
        Event::Key {
            usage: 6,
            page: Page::Keyboard,
            down: false,
        },
    ];
    let mut journal = Journal::create(directory, scope.clone(), &current, 2, &events)?;
    let desired = journal.after().clone();
    let mut fault_seen = false;
    for index in 0..2 {
        guard(&scope)?;
        let packet = journal.reserve_forward(&current)?;
        match send(&scope, &packet.bytes) {
            Err(_) if scenario == "failed-forward" && index == 1 => fault_seen = true,
            result => result?,
        }
        current = observe(&scope, &mut nonce)?;
        if fault_seen {
            break;
        }
    }
    if scenario == "commit" {
        guard(&scope)?;
        journal.verify_committed(&current)?;
        return Ok(
            json!({"phase":"committed","memory":current.memory(),"expected":desired.memory(),"snapshots":nonce}),
        );
    }
    if scenario == "interrupted-restore" {
        journal.begin_restore(&current)?;
        let packet = journal.reserve_restore(&current)?;
        if send(&scope, &packet.bytes).is_ok() {
            return Err(Error::protocol(
                "expected modeled restore transport failure",
            ));
        }
        fault_seen = true;
        current = observe(&scope, &mut nonce)?;
        drop(journal);
        journal = Journal::open(directory, &scope)?;
    }
    if !fault_seen {
        return Err(Error::protocol("expected scenario failure not injected"));
    }
    guard(&scope)?;
    journal.begin_restore(&current)?;
    for _ in 0..2 {
        let packet = journal.reserve_restore(&current)?;
        send(&scope, &packet.bytes)?;
        current = observe(&scope, &mut nonce)?;
    }
    guard(&scope)?;
    journal.verify_restored(&current)?;
    Ok(
        json!({"phase":"restored","memory":current.memory(),"expected":original.memory(),"snapshots":nonce}),
    )
}
fn main() {
    let input = receive().unwrap();
    match run(&input) {
        Ok(result) => println!("{}", json!({"op":"done","result":result})),
        Err(error) => println!("{}", json!({"op":"done","error":error.to_string()})),
    }
}
