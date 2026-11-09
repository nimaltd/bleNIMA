//! The computer's own Bluetooth: the one radio bleNIMA uses.
//!
//! Connecting and GATT go through btleplug on every OS. Adverts go through
//! WinRT's own watcher on Windows, which gives every advert as it was sent, and
//! through btleplug elsewhere, where the OS only gives what it read from them.

#[cfg(target_os = "windows")]
mod windows;

use std::sync::Arc;
use std::time::{Duration, Instant};

use btleplug::api::{
    AddressType, Central, CentralEvent, CentralState, Manager as _, Peripheral as _,
    PeripheralProperties, ScanFilter,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;
use serde::Serialize;
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use crate::advert::{AdStructure, ad, uuid_to_le};
use crate::names;
use crate::scanner::{AddressKind, Advert};

/// bleNIMA advertising as a peripheral, until it is dropped.
pub struct Advertising {
    #[cfg(target_os = "windows")]
    publisher: windows::Publisher,
}

impl Advertising {
    /// Starts advertising these AD structures, where the OS lets an app.
    pub fn start(structures: &[crate::advert::AdStructure]) -> Result<Self, String> {
        #[cfg(target_os = "windows")]
        {
            Ok(Self {
                publisher: windows::Publisher::start(structures)?,
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = structures;
            Err("Advertising from bleNIMA works on Windows for now.".into())
        }
    }

    /// Where the advert is, in words.
    pub fn status(&self) -> String {
        #[cfg(target_os = "windows")]
        {
            self.publisher.status()
        }
        #[cfg(not(target_os = "windows"))]
        {
            "Not advertising".into()
        }
    }
}

/// A device the OS has a bond with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Bond {
    pub name: String,
    pub address: String,
}

/// What bleNIMA says where it cannot pair by itself.
#[cfg(not(target_os = "windows"))]
const PAIR_IN_OS: &str =
    "On this OS, pair the device in the OS's Bluetooth settings; bleNIMA then uses the bond.";

/// What the radio can do here, so the UI can grey out the rest and say why.
#[derive(Clone, Debug, Serialize)]
pub struct RadioInfo {
    /// "Windows Bluetooth", "macOS Bluetooth", "BlueZ".
    pub name: String,
    /// Adverts come as they were sent, with their PDU kind and scan responses apart.
    pub raw_adverts: bool,
    /// Devices are known by their real address. macOS hides it.
    pub real_addresses: bool,
    /// Connection parameters can be read and asked for.
    pub connection_parameters: bool,
    /// bleNIMA can pair and unpair by itself, and list the bonds.
    pub pairing: bool,
    /// What the OS does not let an app do here, in words.
    pub limits: Vec<String>,
}

fn problem(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// The computer's Bluetooth adapter, shared by every tab.
pub struct Radio {
    adapter: Adapter,
    /// How many tabs want btleplug to scan, to find a device or list them.
    scans: Mutex<usize>,
}

impl Radio {
    /// Opens the first Bluetooth adapter, and checks that it is on.
    pub async fn open() -> Result<Arc<Self>, String> {
        let manager = Manager::new()
            .await
            .map_err(|error| format!("Bluetooth could not be opened: {error}"))?;
        let adapter = manager
            .adapters()
            .await
            .map_err(problem)?
            .into_iter()
            .next()
            .ok_or("This computer has no Bluetooth adapter, or it is turned off.")?;
        if let Ok(CentralState::PoweredOff) = adapter.adapter_state().await {
            return Err("Bluetooth is turned off. Turn it on in the OS's settings.".into());
        }
        Ok(Arc::new(Self {
            adapter,
            scans: Mutex::new(0),
        }))
    }

    pub fn adapter(&self) -> &Adapter {
        &self.adapter
    }

    /// What this radio can do on this OS.
    pub fn info(&self) -> RadioInfo {
        if cfg!(target_os = "windows") {
            RadioInfo {
                name: "Windows Bluetooth".into(),
                raw_adverts: true,
                real_addresses: true,
                connection_parameters: true,
                pairing: true,
                limits: vec![
                    "Windows picks the MTU and the PHY itself; bleNIMA shows what it picked."
                        .into(),
                    "Connection parameters can be asked for as one of three presets.".into(),
                ],
            }
        } else if cfg!(target_os = "macos") {
            RadioInfo {
                name: "macOS Bluetooth".into(),
                raw_adverts: false,
                real_addresses: false,
                connection_parameters: false,
                pairing: false,
                limits: vec![
                    "macOS gives each device a made-up ID instead of its address.".into(),
                    "Adverts are made again from what macOS read from them.".into(),
                    "macOS picks the MTU, the PHY and the connection parameters itself.".into(),
                ],
            }
        } else {
            RadioInfo {
                name: "BlueZ".into(),
                raw_adverts: false,
                real_addresses: true,
                connection_parameters: false,
                pairing: false,
                limits: vec![
                    "Adverts are made again from what BlueZ read from them.".into(),
                    "BlueZ picks the MTU, the PHY and the connection parameters itself.".into(),
                ],
            }
        }
    }

    /// Asks btleplug to scan, for as long as one tab or more wants it.
    pub async fn scan_on(&self) -> Result<(), String> {
        let mut scans = self.scans.lock().await;
        if *scans == 0 {
            self.adapter
                .start_scan(ScanFilter::default())
                .await
                .map_err(problem)?;
        }
        *scans += 1;
        Ok(())
    }

    /// One tab less wants btleplug to scan; the last one stops it.
    pub async fn scan_off(&self) {
        let mut scans = self.scans.lock().await;
        *scans = scans.saturating_sub(1);
        if *scans == 0 {
            let _ = self.adapter.stop_scan().await;
        }
    }

    /// Starts handing every advert heard to `sink`, timed from `started`, until
    /// the returned handle is dropped.
    pub async fn listen(
        self: &Arc<Self>,
        sink: UnboundedSender<Advert>,
        started: Instant,
    ) -> Result<Listening, String> {
        #[cfg(target_os = "windows")]
        {
            let watcher = windows::Watcher::start(sink, started)?;
            Ok(Listening {
                _watcher: Some(watcher),
                task: None,
                radio: None,
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut events = self.adapter.events().await.map_err(problem)?;
            self.scan_on().await?;
            let radio = self.clone();
            let task = tokio::spawn(async move {
                while let Some(event) = events.next().await {
                    let id = match event {
                        CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id) => id,
                        _ => continue,
                    };
                    let Ok(peripheral) = radio.adapter.peripheral(&id).await else {
                        continue;
                    };
                    if let Ok(Some(properties)) = peripheral.properties().await {
                        let at_ms = started.elapsed().as_secs_f64() * 1000.0;
                        if sink
                            .send(from_properties(&id.to_string(), &properties, at_ms))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            });
            Ok(Listening {
                task: Some(task),
                radio: Some(self.clone()),
            })
        }
    }

    /// The device with this id, scanning until it is heard or `wait` is over.
    pub async fn find(&self, id: &str, wait: Duration) -> Result<Option<Peripheral>, String> {
        if let Some(found) = self.known(id).await {
            return Ok(Some(found));
        }
        let mut events = self.adapter.events().await.map_err(problem)?;
        self.scan_on().await?;
        let found = tokio::time::timeout(wait, async {
            while let Some(event) = events.next().await {
                if let CentralEvent::DeviceDiscovered(peripheral_id)
                | CentralEvent::DeviceUpdated(peripheral_id) = event
                    && peripheral_id.to_string() == id
                    && let Ok(peripheral) = self.adapter.peripheral(&peripheral_id).await
                {
                    return Some(peripheral);
                }
            }
            None
        })
        .await
        .ok()
        .flatten();
        self.scan_off().await;
        Ok(found)
    }

    /// Pairs with the device and bonds; the OS asks for a passkey itself when one is needed.
    pub async fn pair(&self, id: &str) -> Result<String, String> {
        #[cfg(target_os = "windows")]
        {
            let id = id.to_string();
            tokio::task::spawn_blocking(move || windows::pair(&id))
                .await
                .map_err(problem)?
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = id;
            Err(PAIR_IN_OS.into())
        }
    }

    /// Takes the OS's bond with the device away.
    pub async fn unpair(&self, id: &str) -> Result<String, String> {
        #[cfg(target_os = "windows")]
        {
            let id = id.to_string();
            tokio::task::spawn_blocking(move || windows::unpair(&id))
                .await
                .map_err(problem)?
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = id;
            Err(PAIR_IN_OS.into())
        }
    }

    /// Whether the OS has a bond with the device; `None` where it cannot tell.
    pub async fn is_paired(&self, id: &str) -> Option<bool> {
        #[cfg(target_os = "windows")]
        {
            let id = id.to_string();
            tokio::task::spawn_blocking(move || windows::is_paired(&id).ok())
                .await
                .ok()
                .flatten()
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = id;
            None
        }
    }

    /// The devices the OS has a bond with.
    pub async fn bonds(&self) -> Result<Vec<Bond>, String> {
        #[cfg(target_os = "windows")]
        {
            tokio::task::spawn_blocking(windows::bonds)
                .await
                .map_err(problem)?
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(PAIR_IN_OS.into())
        }
    }

    /// The device with this id, if btleplug has heard it already.
    async fn known(&self, id: &str) -> Option<Peripheral> {
        self.adapter
            .peripherals()
            .await
            .ok()?
            .into_iter()
            .find(|peripheral| peripheral.id().to_string() == id)
    }
}

/// Adverts being heard. Dropping it stops them.
pub struct Listening {
    #[cfg(target_os = "windows")]
    _watcher: Option<windows::Watcher>,
    task: Option<JoinHandle<()>>,
    radio: Option<Arc<Radio>>,
}

impl Drop for Listening {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
        if let Some(radio) = self.radio.take() {
            tokio::spawn(async move { radio.scan_off().await });
        }
    }
}

/// An advert made again from what the OS read from it, where it does not give
/// the bytes: the name, TX power, service UUIDs, service data and manufacturer data.
pub fn from_properties(id: &str, properties: &PeripheralProperties, at_ms: f64) -> Advert {
    let mut structures = Vec::new();
    let mut lists: [Vec<u8>; 3] = Default::default();
    for uuid in &properties.services {
        match names::to_short(uuid) {
            Some(short) if short <= 0xFFFF => lists[0].extend(uuid_to_le(uuid, 2)),
            Some(_) => lists[1].extend(uuid_to_le(uuid, 4)),
            None => lists[2].extend(uuid_to_le(uuid, 16)),
        }
    }
    for (list, ad_type) in lists
        .into_iter()
        .zip([ad::UUID16_ALL, ad::UUID32_ALL, ad::UUID128_ALL])
    {
        if !list.is_empty() {
            structures.push(AdStructure::new(ad_type, list));
        }
    }
    if let Some(name) = properties
        .local_name
        .as_ref()
        .or(properties.advertisement_name.as_ref())
    {
        structures.push(AdStructure::new(ad::NAME, name.as_bytes()));
    }
    if let Some(power) = properties.tx_power_level {
        structures.push(AdStructure::new(ad::TX_POWER, [power as i8 as u8]));
    }
    let mut service_data: Vec<_> = properties.service_data.iter().collect();
    service_data.sort();
    for (uuid, data) in service_data {
        let (ad_type, size) = match names::to_short(uuid) {
            Some(short) if short <= 0xFFFF => (ad::SERVICE_DATA16, 2),
            Some(_) => (ad::SERVICE_DATA32, 4),
            None => (ad::SERVICE_DATA128, 16),
        };
        let mut bytes = uuid_to_le(uuid, size);
        bytes.extend_from_slice(data);
        structures.push(AdStructure::new(ad_type, bytes));
    }
    let mut manufacturer: Vec<_> = properties.manufacturer_data.iter().collect();
    manufacturer.sort();
    for (company, data) in manufacturer {
        let mut bytes = company.to_le_bytes().to_vec();
        bytes.extend_from_slice(data);
        structures.push(AdStructure::new(ad::MANUFACTURER, bytes));
    }
    let address = properties.address.to_string();
    // macOS gives no address: it is all zeros.
    let address = (address != "00:00:00:00:00:00").then_some(address);
    let address_kind = match (properties.address_type, &address) {
        (Some(AddressType::Public), _) => Some(AddressKind::Public),
        (Some(AddressType::Random), Some(address)) => Some(AddressKind::of_random(address)),
        (Some(AddressType::Random), None) => Some(AddressKind::Random),
        _ => None,
    };
    Advert {
        id: id.to_string(),
        address,
        address_kind,
        rssi: properties.rssi,
        at_ms,
        kind: None,
        connectable: None,
        scan_response: false,
        structures,
        raw: false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use btleplug::api::BDAddr;

    use super::*;
    use crate::advert;

    #[test]
    fn an_advert_is_made_again_from_what_the_os_read() {
        let nus = names::parse_uuid("6E400001-B5A3-F393-E0A9-E50E24DCCA9E").unwrap();
        let properties = PeripheralProperties {
            address: BDAddr::from([0xC3, 0x1A, 0x4B, 0x00, 0x12, 0x9F]),
            address_type: Some(AddressType::Random),
            local_name: Some("nrf52".into()),
            tx_power_level: Some(-8),
            rssi: Some(-61),
            manufacturer_data: HashMap::from([(0x0059, vec![0x01])]),
            service_data: HashMap::from([(names::from_short(0x180F), vec![0x64])]),
            services: vec![names::from_short(0x180D), nus],
            ..PeripheralProperties::default()
        };
        let made = from_properties("x", &properties, 5.0);
        assert_eq!(made.address.as_deref(), Some("C3:1A:4B:00:12:9F"));
        assert_eq!(made.address_kind, Some(AddressKind::RandomStatic));
        assert!(!made.raw);
        let summary = advert::summarize(&made.structures);
        assert_eq!(summary.name.as_deref(), Some("nrf52"));
        assert_eq!(summary.tx_power, Some(-8));
        assert_eq!(summary.companies, [0x0059]);
        assert!(summary.services.contains(&nus));
        assert!(summary.services.contains(&names::from_short(0x180F)));
        // Back through the parser: the bytes made are well formed.
        let bytes = advert::encode(&made.structures);
        assert_eq!(advert::parse(&bytes), made.structures);
    }

    #[test]
    fn a_hidden_address_is_not_shown() {
        let made = from_properties("uuid", &PeripheralProperties::default(), 0.0);
        assert_eq!(made.address, None);
    }
}
