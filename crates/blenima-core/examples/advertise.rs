//! Advertises an iBeacon for a few seconds and says what the OS made of it.
//!
//! cargo run -p blenima-core --example advertise

use std::time::Duration;

use blenima_core::advert::{AdStructure, ad};
use blenima_core::radio::Advertising;

fn main() -> Result<(), String> {
    let mut ibeacon = vec![0x4C, 0x00, 0x02, 0x15];
    ibeacon.extend_from_slice(&[0xE2; 16]);
    ibeacon.extend_from_slice(&[0x00, 0x01, 0x00, 0x2A, 0xC5]);
    let advertising = Advertising::start(&[AdStructure::new(ad::MANUFACTURER, ibeacon)])?;
    for _ in 0..4 {
        std::thread::sleep(Duration::from_secs(1));
        println!("{}", advertising.status());
    }
    // Flags are the OS's own: this one is expected to be refused.
    match Advertising::start(&[AdStructure::new(ad::FLAGS, [0x06])]) {
        Ok(_) => println!("Flags were taken"),
        Err(problem) => println!("{problem}"),
    }
    Ok(())
}
