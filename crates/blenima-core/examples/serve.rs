//! Serves the Battery service and the Nordic UART Service with echo for a
//! while, and prints what a phone does with them: connect with nRF Connect,
//! read the battery, subscribe, and write to the UART's RX.
//!
//! cargo run -p blenima-core --example serve [seconds]

use std::time::{Duration, Instant};

use blenima_core::names;
use blenima_core::radio::Serving;
use blenima_core::server::{Characteristic, Place, Service, property};

fn main() -> Result<(), String> {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|text| text.parse().ok())
        .unwrap_or(60);
    let nus = |text: &str| names::parse_uuid(text).expect("a UUID");
    let services = [
        Service {
            uuid: names::from_short(0x180F),
            characteristics: vec![Characteristic {
                uuid: names::from_short(0x2A19),
                properties: property::READ | property::NOTIFY,
                value: vec![100],
                description: String::new(),
                echo: None,
            }],
        },
        Service {
            uuid: nus("6E400001-B5A3-F393-E0A9-E50E24DCCA9E"),
            characteristics: vec![
                Characteristic {
                    uuid: nus("6E400002-B5A3-F393-E0A9-E50E24DCCA9E"),
                    properties: property::WRITE | property::WRITE_WITHOUT_RESPONSE,
                    value: vec![],
                    description: "RX".into(),
                    echo: Some(1),
                },
                Characteristic {
                    uuid: nus("6E400003-B5A3-F393-E0A9-E50E24DCCA9E"),
                    properties: property::NOTIFY,
                    value: vec![],
                    description: "TX".into(),
                    echo: None,
                },
            ],
        },
    ];
    let (sink, mut events) = tokio::sync::mpsc::unbounded_channel();
    let serving = Serving::start(&services, sink)?;
    println!("Serving Battery and Nordic UART for {seconds} s");
    let battery = Place {
        service: 0,
        characteristic: 0,
    };
    let started = Instant::now();
    let mut level = 100u8;
    while started.elapsed() < Duration::from_secs(seconds) {
        std::thread::sleep(Duration::from_secs(1));
        while let Ok(event) = events.try_recv() {
            println!("{event:?}");
        }
        level = if level == 0 { 100 } else { level - 1 };
        let subscribed = serving.set(battery, &[level], true)?;
        if subscribed > 0 {
            println!("Battery {level} % to {subscribed} subscribed");
        }
    }
    Ok(())
}
