//! Scans with the computer's Bluetooth for a few seconds and prints what it heard,
//! and what the radio can do here.
//!
//! cargo run -p blenima-core --example scan -- [seconds] [passive] [coded]

use std::time::{Duration, Instant};

use blenima_core::radio::Radio;
use blenima_core::scanner::{DeviceTable, ScanSetup};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), String> {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|text| text.parse().ok())
        .unwrap_or(8);
    let words: Vec<String> = std::env::args().collect();
    let setup = ScanSetup {
        passive: words.iter().any(|word| word == "passive"),
        coded: words.iter().any(|word| word == "coded"),
    };
    let radio = Radio::open().await?;
    let info = radio.info();
    println!("{}: raw adverts {}", info.name, info.raw_adverts);
    println!("2M PHY {:?}, Coded PHY {:?}", info.phy_2m, info.phy_coded);
    println!("passive: {:?}", info.passive_problem);
    println!("coded scan: {:?}", info.coded_scan_problem);
    println!("tx power: {:?}", info.tx_power_problem);
    println!("extended: {:?}", info.extended_problem);
    println!("advert PHYs: {:?}", info.advert_phy_problem);
    println!("connection PHY: {:?}", info.phy_problem);
    println!("scanning with {setup:?}");
    let (sink, mut adverts) = tokio::sync::mpsc::unbounded_channel();
    let listening = radio.listen(sink, Instant::now(), setup).await?;
    let mut table = DeviceTable::new();
    let _ = tokio::time::timeout(Duration::from_secs(seconds), async {
        while let Some(advert) = adverts.recv().await {
            table.heard(advert);
        }
    })
    .await;
    drop(listening);
    let changes = table.changes();
    println!("{} devices, {} adverts", changes.devices, changes.adverts);
    for row in changes.rows {
        println!(
            "{:<18} {:>4} dBm {:>5} adv  {:<22} {:>8} {}  {}",
            row.address.as_deref().unwrap_or(&row.id),
            row.rssi.unwrap_or(0),
            row.count,
            row.name.as_deref().unwrap_or("-"),
            row.interval_ms
                .map(|ms| format!("{ms:.0} ms"))
                .unwrap_or_else(|| "-".into()),
            row.companies.join(", "),
            row.beacon.as_deref().unwrap_or(""),
        );
        println!(
            "    connectable {:?} kind {:?} address {:?} services {:?}",
            row.connectable, row.kind, row.address_kind, row.services,
        );
    }
    Ok(())
}
