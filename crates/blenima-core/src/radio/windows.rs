//! What bleNIMA asks of WinRT itself, where btleplug does not reach.
//!
//! Hearing adverts through WinRT's advertisement watcher, which gives every
//! advert as it came: its AD structures as sent, its PDU kind, its address type
//! and whether it is a scan response; btleplug's own watcher keeps only some of
//! it. And pairing, unpairing and the list of bonds.

use std::time::Instant;

use tokio::sync::mpsc::UnboundedSender;
use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementDataSection, BluetoothLEAdvertisementPublisher,
    BluetoothLEAdvertisementPublisherStatus, BluetoothLEAdvertisementReceivedEventArgs,
    BluetoothLEAdvertisementType, BluetoothLEAdvertisementWatcher, BluetoothLEManufacturerData,
    BluetoothLEScanningMode,
};
use windows::Devices::Bluetooth::{BluetoothAddressType, BluetoothLEDevice};
use windows::Devices::Enumeration::{
    DeviceInformation, DevicePairingProtectionLevel, DevicePairingResultStatus,
    DeviceUnpairingResultStatus,
};
use windows::Foundation::TypedEventHandler;
use windows::Storage::Streams::{DataReader, DataWriter, IBuffer};
use windows::core::Ref;

use crate::advert::{self, AdStructure, ad};
use crate::radio::Bond;
use crate::scanner::{AddressKind, AdvKind, Advert};

/// The 48 bits of "C3:1A:4B:00:12:9F", as Windows takes an address.
pub fn address_value(text: &str) -> Option<u64> {
    let hex: String = text.chars().filter(|c| *c != ':').collect();
    if hex.len() == 12 {
        u64::from_str_radix(&hex, 16).ok()
    } else {
        None
    }
}

fn winrt(error: windows::core::Error) -> String {
    let message = error.message();
    if message.is_empty() {
        format!("Windows error {:?}", error.code())
    } else {
        // Without its full stop, so it can be part of a sentence.
        message
            .to_string()
            .trim_end_matches(|c: char| c == '.' || c.is_whitespace())
            .to_string()
    }
}

fn le_device(address: &str) -> Result<BluetoothLEDevice, String> {
    let value = address_value(address).ok_or_else(|| format!("{address} is not an address"))?;
    BluetoothLEDevice::FromBluetoothAddressAsync(value)
        .and_then(|operation| operation.join())
        .map_err(winrt)
}

fn protection_text(level: DevicePairingProtectionLevel) -> &'static str {
    match level {
        DevicePairingProtectionLevel::None => "no protection",
        DevicePairingProtectionLevel::Encryption => "encrypted",
        DevicePairingProtectionLevel::EncryptionAndAuthentication => {
            "encrypted and authenticated (MITM protection)"
        }
        _ => "the OS's default protection",
    }
}

/// Pairs with a device, and bonds. Windows asks the user for a passkey or a
/// confirmation itself, in its own dialog, when the device needs one.
/// Blocks until it is done: call it off the async runtime.
pub fn pair(address: &str) -> Result<String, String> {
    let pairing = le_device(address)?
        .DeviceInformation()
        .and_then(|info| info.Pairing())
        .map_err(winrt)?;
    if pairing.IsPaired().unwrap_or(false) {
        let level = pairing
            .ProtectionLevel()
            .unwrap_or(DevicePairingProtectionLevel::Default);
        return Ok(format!("Already paired, {}", protection_text(level)));
    }
    if !pairing.CanPair().unwrap_or(false) {
        return Err("Windows says this device cannot be paired now: it may need to be advertising, or it asks for no security".into());
    }
    let result = pairing
        .PairWithProtectionLevelAsync(DevicePairingProtectionLevel::Default)
        .and_then(|operation| operation.join())
        .map_err(winrt)?;
    let level = result
        .ProtectionLevelUsed()
        .unwrap_or(DevicePairingProtectionLevel::Default);
    match result.Status().map_err(winrt)? {
        DevicePairingResultStatus::Paired | DevicePairingResultStatus::AlreadyPaired => {
            Ok(format!("Paired and bonded, {}", protection_text(level)))
        }
        DevicePairingResultStatus::NotReadyToPair => Err("The device is not ready to pair".into()),
        DevicePairingResultStatus::AuthenticationFailure => {
            Err("Pairing failed: the passkey or the confirmation did not match".into())
        }
        DevicePairingResultStatus::AuthenticationTimeout => {
            Err("Pairing timed out: nobody answered in time".into())
        }
        DevicePairingResultStatus::AuthenticationNotAllowed => {
            Err("The device does not allow pairing now".into())
        }
        DevicePairingResultStatus::PairingCanceled => Err("Pairing was cancelled".into()),
        DevicePairingResultStatus::ConnectionRejected => {
            Err("The device refused the connection to pair".into())
        }
        DevicePairingResultStatus::RemoteDeviceHasAssociation => {
            Err("The device is bonded to another computer or phone already".into())
        }
        other => Err(format!("Pairing failed: Windows status {}", other.0)),
    }
}

/// Takes the bond with a device away, so it pairs anew next time.
pub fn unpair(address: &str) -> Result<String, String> {
    let pairing = le_device(address)?
        .DeviceInformation()
        .and_then(|info| info.Pairing())
        .map_err(winrt)?;
    if !pairing.IsPaired().unwrap_or(false) {
        return Ok("It was not paired".into());
    }
    let status = pairing
        .UnpairAsync()
        .and_then(|operation| operation.join())
        .and_then(|result| result.Status())
        .map_err(winrt)?;
    match status {
        DeviceUnpairingResultStatus::Unpaired | DeviceUnpairingResultStatus::AlreadyUnpaired => {
            Ok("The bond is gone: it pairs anew next time".into())
        }
        DeviceUnpairingResultStatus::AccessDenied => {
            Err("Windows refused to take the bond away".into())
        }
        other => Err(format!("Could not unpair: Windows status {}", other.0)),
    }
}

/// Whether Windows has a bond with the device.
pub fn is_paired(address: &str) -> Result<bool, String> {
    le_device(address)?
        .DeviceInformation()
        .and_then(|info| info.Pairing())
        .and_then(|pairing| pairing.IsPaired())
        .map_err(winrt)
}

/// The Bluetooth LE devices Windows has a bond with.
pub fn bonds() -> Result<Vec<Bond>, String> {
    let selector = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true).map_err(winrt)?;
    let found = DeviceInformation::FindAllAsyncAqsFilter(&selector)
        .and_then(|operation| operation.join())
        .map_err(winrt)?;
    let mut list = Vec::new();
    for info in &found {
        let name = info.Name().map(|name| name.to_string()).unwrap_or_default();
        let address = info
            .Id()
            .and_then(|id| BluetoothLEDevice::FromIdAsync(&id))
            .and_then(|operation| operation.join())
            .and_then(|device| device.BluetoothAddress())
            .map(address_text);
        if let Ok(address) = address {
            list.push(Bond { name, address });
        }
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

/// A running watcher. Dropping it stops it.
pub struct Watcher {
    watcher: BluetoothLEAdvertisementWatcher,
    token: i64,
}

impl Watcher {
    /// Starts an active scan, handing every advert to `sink`, timed from `started`.
    pub fn start(sink: UnboundedSender<Advert>, started: Instant) -> Result<Self, String> {
        let problem =
            |error: windows::core::Error| format!("Windows could not scan: {}", error.message());
        let watcher = BluetoothLEAdvertisementWatcher::new().map_err(problem)?;
        watcher
            .SetScanningMode(BluetoothLEScanningMode::Active)
            .map_err(problem)?;
        // Bluetooth 5 adverts too, where the adapter and Windows can.
        let _ = watcher.SetAllowExtendedAdvertisements(true);
        let handler = TypedEventHandler::new(
            move |_watcher, args: Ref<BluetoothLEAdvertisementReceivedEventArgs>| {
                if let Ok(args) = args.ok()
                    && let Some(advert) = advert(args, started)
                {
                    let _ = sink.send(advert);
                }
                Ok(())
            },
        );
        let token = watcher.Received(&handler).map_err(problem)?;
        watcher.Start().map_err(problem)?;
        Ok(Self { watcher, token })
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.watcher.Stop();
        let _ = self.watcher.RemoveReceived(self.token);
    }
}

/// "C3:1A:4B:00:12:9F" from the 48 bits Windows gives.
pub fn address_text(address: u64) -> String {
    (0..6)
        .rev()
        .map(|byte| format!("{:02X}", (address >> (byte * 8)) & 0xFF))
        .collect::<Vec<_>>()
        .join(":")
}

fn bytes(buffer: &IBuffer) -> Vec<u8> {
    let Ok(reader) = DataReader::FromBuffer(buffer) else {
        return Vec::new();
    };
    let length = reader.UnconsumedBufferLength().unwrap_or(0) as usize;
    let mut data = vec![0u8; length];
    if reader.ReadBytes(&mut data).is_err() {
        data.clear();
    }
    data
}

fn advert(args: &BluetoothLEAdvertisementReceivedEventArgs, started: Instant) -> Option<Advert> {
    let at_ms = started.elapsed().as_secs_f64() * 1000.0;
    let address = address_text(args.BluetoothAddress().ok()?);
    let address_kind = match args.BluetoothAddressType() {
        Ok(BluetoothAddressType::Public) => Some(AddressKind::Public),
        Ok(BluetoothAddressType::Random) => Some(AddressKind::of_random(&address)),
        _ => None,
    };
    let kind = match args.AdvertisementType().ok()? {
        BluetoothLEAdvertisementType::ConnectableUndirected => AdvKind::ConnectableUndirected,
        BluetoothLEAdvertisementType::ConnectableDirected => AdvKind::ConnectableDirected,
        BluetoothLEAdvertisementType::ScannableUndirected => AdvKind::ScannableUndirected,
        BluetoothLEAdvertisementType::NonConnectableUndirected => AdvKind::NonConnectableUndirected,
        BluetoothLEAdvertisementType::ScanResponse => AdvKind::ScanResponse,
        _ => AdvKind::Extended,
    };
    let scan_response = args
        .IsScanResponse()
        .unwrap_or(kind == AdvKind::ScanResponse);
    let connectable = args.IsConnectable().ok().or(match kind {
        AdvKind::ConnectableUndirected | AdvKind::ConnectableDirected => Some(true),
        AdvKind::ScanResponse | AdvKind::Extended => None,
        _ => Some(false),
    });
    let mut structures = Vec::new();
    if let Ok(sections) = args
        .Advertisement()
        .and_then(|advert| advert.DataSections())
    {
        for section in &sections {
            let (Ok(ad_type), Ok(data)) = (section.DataType(), section.Data()) else {
                continue;
            };
            structures.push(AdStructure::new(ad_type, bytes(&data)));
        }
    }
    Some(Advert {
        id: address.clone(),
        address: Some(address),
        address_kind,
        rssi: args.RawSignalStrengthInDBm().ok(),
        at_ms,
        kind: Some(kind),
        connectable: if scan_response { None } else { connectable },
        scan_response,
        structures,
        raw: true,
    })
}

fn buffer(data: &[u8]) -> Result<IBuffer, String> {
    let writer = DataWriter::new().map_err(winrt)?;
    writer.WriteBytes(data).map_err(winrt)?;
    writer.DetachBuffer().map_err(winrt)
}

/// bleNIMA advertising, as a peripheral does. Dropping it stops.
pub struct Publisher {
    publisher: BluetoothLEAdvertisementPublisher,
}

impl Publisher {
    /// Starts advertising these AD structures. Windows puts some in the advert
    /// itself and refuses others, such as Flags and the name: each refused one
    /// is said, by its type.
    pub fn start(structures: &[AdStructure]) -> Result<Self, String> {
        let publisher = BluetoothLEAdvertisementPublisher::new().map_err(winrt)?;
        let advert = publisher.Advertisement().map_err(winrt)?;
        for structure in structures {
            let refused = |error: windows::core::Error| {
                format!(
                    "Windows does not let an app advertise {} (0x{:02X}): {}",
                    advert::type_name(structure.ad_type),
                    structure.ad_type,
                    winrt(error)
                )
            };
            match structure.ad_type {
                ad::MANUFACTURER if structure.data.len() >= 2 => {
                    let company = u16::from_le_bytes([structure.data[0], structure.data[1]]);
                    let data = BluetoothLEManufacturerData::Create(
                        company,
                        &buffer(&structure.data[2..])?,
                    )
                    .map_err(refused)?;
                    advert
                        .ManufacturerData()
                        .and_then(|list| list.Append(&data))
                        .map_err(refused)?;
                }
                ad::UUID16_SOME
                | ad::UUID16_ALL
                | ad::UUID32_SOME
                | ad::UUID32_ALL
                | ad::UUID128_SOME
                | ad::UUID128_ALL => {
                    let size = match structure.ad_type {
                        ad::UUID16_SOME | ad::UUID16_ALL => 2,
                        ad::UUID32_SOME | ad::UUID32_ALL => 4,
                        _ => 16,
                    };
                    let list = advert.ServiceUuids().map_err(refused)?;
                    for uuid in advert::uuid_list(&structure.data, size) {
                        list.Append(windows::core::GUID::from_u128(uuid.as_u128()))
                            .map_err(refused)?;
                    }
                }
                ad::NAME | ad::SHORT_NAME => {
                    let name = String::from_utf8_lossy(&structure.data).into_owned();
                    advert
                        .SetLocalName(&windows::core::HSTRING::from(name))
                        .map_err(refused)?;
                }
                _ => {
                    let section = BluetoothLEAdvertisementDataSection::Create(
                        structure.ad_type,
                        &buffer(&structure.data)?,
                    )
                    .map_err(refused)?;
                    advert
                        .DataSections()
                        .and_then(|list| list.Append(&section))
                        .map_err(refused)?;
                }
            }
        }
        if let Err(error) = publisher.Start() {
            // Windows keeps some types for itself, and then refuses to start at all.
            let own: Vec<String> = structures
                .iter()
                .filter(|s| matches!(s.ad_type, ad::FLAGS | ad::TX_POWER))
                .map(|s| format!("{} (0x{:02X})", advert::type_name(s.ad_type), s.ad_type))
                .collect();
            let hint = if own.is_empty() {
                String::new()
            } else {
                format!(
                    ". Windows keeps {} for itself: take it out",
                    own.join(" and ")
                )
            };
            return Err(format!(
                "Windows could not start advertising: {}{hint}",
                winrt(error)
            ));
        }
        Ok(Self { publisher })
    }

    /// Where the advert is, in words.
    pub fn status(&self) -> String {
        match self.publisher.Status() {
            Ok(BluetoothLEAdvertisementPublisherStatus::Started) => "Advertising".into(),
            Ok(BluetoothLEAdvertisementPublisherStatus::Waiting) => {
                "Waiting for the radio: another app may be advertising".into()
            }
            Ok(BluetoothLEAdvertisementPublisherStatus::Aborted) => {
                "Windows stopped the advert: the radio is off, busy, or does not allow it".into()
            }
            Ok(BluetoothLEAdvertisementPublisherStatus::Stopped) => "Stopped".into(),
            Ok(other) => format!("Windows status {}", other.0),
            Err(error) => winrt(error),
        }
    }
}

impl Drop for Publisher {
    fn drop(&mut self) {
        let _ = self.publisher.Stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_is_written_as_people_do() {
        assert_eq!(address_text(0xC31A_4B00_129F), "C3:1A:4B:00:12:9F");
        assert_eq!(address_text(0x1), "00:00:00:00:00:01");
        assert_eq!(address_value("C3:1A:4B:00:12:9F"), Some(0xC31A_4B00_129F));
        assert_eq!(address_value("not one"), None);
    }
}
