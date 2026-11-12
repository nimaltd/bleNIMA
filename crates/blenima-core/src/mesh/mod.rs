//! Bluetooth mesh through a GATT proxy: bleNIMA as a provisioner and a
//! configuration client, with its own mesh stack. It provisions a device over
//! PB-GATT, then talks to the network through a proxy node, as the Mesh
//! Protocol spec 1.1 describes. Its crypto is checked against the spec's own
//! sample data (section 8).

pub mod crypto;
pub mod network;
pub mod pdu;
pub mod provision;
pub mod session;
pub mod stack;
