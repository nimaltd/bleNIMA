//! Lists the Bluetooth LE devices the OS has a bond with.
//!
//! cargo run -p blenima-core --example bonds

use blenima_core::radio::Radio;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), String> {
    let radio = Radio::open().await?;
    let bonds = radio.bonds().await?;
    println!("{} bonded Bluetooth LE devices", bonds.len());
    for bond in bonds {
        let paired = radio.is_paired(&bond.address).await;
        println!("{:<18} {:<30} paired {:?}", bond.address, bond.name, paired);
    }
    Ok(())
}
