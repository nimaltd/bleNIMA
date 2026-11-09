//! Everything in bleNIMA that does not draw on the screen.
//!
//! The radio, sessions, config and decoders live here, apart from the window,
//! so they can be tested on their own.

pub mod advert;
pub mod device;
pub mod gatt;
pub mod names;
pub mod radio;
pub mod scanner;
pub mod settings;
mod tomlfile;
pub mod workspace;

/// The version of bleNIMA, from the workspace's `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
