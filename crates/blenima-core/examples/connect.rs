//! Connects to a device by its id, prints its GATT table and what happens, for a few seconds.
//!
//! cargo run -p blenima-core --example connect -- AA:BB:CC:DD:EE:FF [seconds]

use std::time::Duration;

use blenima_core::device::{DeviceEvent, DeviceSession};
use blenima_core::radio::Radio;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let id = args.next().ok_or("give the device's id")?;
    let seconds: u64 = args.next().and_then(|text| text.parse().ok()).unwrap_or(20);
    let radio = Radio::open().await?;
    let (sink, mut events) = tokio::sync::mpsc::unbounded_channel();
    let session = DeviceSession::open(radio, id, sink);
    let _ = tokio::time::timeout(Duration::from_secs(seconds), async {
        while let Some(event) = events.recv().await {
            match event {
                DeviceEvent::Services { services } => {
                    for service in services {
                        println!("{} {}", service.uuid, service.name.unwrap_or(""));
                        for c in service.characteristics {
                            println!(
                                "    {} {} [{}]",
                                c.uuid,
                                c.name.unwrap_or(""),
                                c.property_names.join(", ")
                            );
                        }
                    }
                }
                other => println!("{other:?}"),
            }
        }
    })
    .await;
    session.close().await;
    Ok(())
}
