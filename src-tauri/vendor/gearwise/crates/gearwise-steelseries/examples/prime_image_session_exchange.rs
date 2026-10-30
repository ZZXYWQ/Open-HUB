//! Run the actual scoped image collector over the isolated JSON-lines bus.
use gearwise_core::{
    Error,
    transport::{BusKind, HidInterfaceInfo},
};
use gearwise_steelseries::prime_image_session::{Scope, Session, Status};
use serde_json::json;
#[path = "support/remote_transport.rs"]
mod remote_transport;
use remote_transport::{Remote, receive, rpc};
fn main() {
    let input = receive().unwrap();
    let seed: [u8; 16] = serde_json::from_value(input["seed"].clone()).unwrap();
    let info = HidInterfaceInfo {
        path: "mock-prime-a".into(),
        vendor_id: 0x1038,
        product_id: 0x182e,
        interface_number: 0,
        usage_page: 0xffc0,
        usage: 1,
        serial: Some("mock-serial-a".into()),
        ..Default::default()
    };
    let expected = Scope::new(&info, "mock-open-a", "mock-read-a").unwrap();
    let check = move || {
        let value = rpc(json!({"op":"guard"}))?;
        let mut current = info.clone();
        current.path = value["path"]
            .as_str()
            .ok_or(Error::protocol("missing scope path"))?
            .into();
        current.serial = value["serial"].as_str().map(str::to_owned);
        current.vendor_id = value["vendor"]
            .as_u64()
            .ok_or(Error::protocol("missing scope vendor"))? as u16;
        current.product_id = value["product"]
            .as_u64()
            .ok_or(Error::protocol("missing scope product"))? as u16;
        current.bus = if value["usb"].as_bool() == Some(true) {
            BusKind::Usb
        } else {
            BusKind::Unknown
        };
        let scope = Scope::new(
            &current,
            value["connection"]
                .as_str()
                .ok_or(Error::protocol("missing connection"))?,
            value["operation"]
                .as_str()
                .ok_or(Error::protocol("missing operation"))?,
        )?;
        Ok(Status {
            scope,
            cancelled: value["cancelled"].as_bool().unwrap_or(true),
        })
    };
    let result =
        Session::new(Remote, seed, expected, check).and_then(|mut session| session.collect());
    match result {
        Ok(observed) => println!(
            "{}",
            json!({"op":"done","memory":observed.image().memory(),"physical":observed.image().physical(),"wheel":observed.image().wheel(),"references":observed.image().references().len(),"passes":observed.equal_passes(),"context_checks":observed.inactive_context_observations(),"connection":observed.scope().connection(),"operation":observed.scope().operation(),"identity_verified":observed.physical_identity_verified(),"atomic_verified":observed.atomic_image_verified()})
        ),
        Err(error) => println!("{}", json!({"op":"done","error":error.to_string()})),
    }
}
