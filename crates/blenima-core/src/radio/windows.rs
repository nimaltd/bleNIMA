//! What bleNIMA asks of WinRT itself, where btleplug does not reach.
//!
//! Hearing adverts through WinRT's advertisement watcher, which gives every
//! advert as it came: its AD structures as sent, its PDU kind, its address type
//! and whether it is a scan response; btleplug's own watcher keeps only some of
//! it. Pairing, unpairing and the list of bonds. A connection's PHY. And
//! bleNIMA as a peripheral: advertising, and a GATT server through WinRT's
//! GattServiceProvider.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use tokio::sync::mpsc::UnboundedSender;
use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementDataSection, BluetoothLEAdvertisementPhyType,
    BluetoothLEAdvertisementPublisher, BluetoothLEAdvertisementPublisherStatus,
    BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementType,
    BluetoothLEAdvertisementWatcher, BluetoothLEManufacturerData, BluetoothLEScanningMode,
};
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristicProperties, GattCommunicationStatus, GattLocalCharacteristic,
    GattLocalCharacteristicParameters, GattProtectionLevel, GattReadRequestedEventArgs,
    GattServiceProvider, GattServiceProviderAdvertisementStatus,
    GattServiceProviderAdvertisementStatusChangedEventArgs,
    GattServiceProviderAdvertisingParameters, GattSession, GattWriteOption,
    GattWriteRequestedEventArgs,
};
use windows::Devices::Bluetooth::{
    BluetoothAdapter, BluetoothAddressType, BluetoothError, BluetoothLEConnectionPhyInfo,
    BluetoothLEDevice,
};
use windows::Devices::Enumeration::{
    DeviceInformation, DevicePairingProtectionLevel, DevicePairingResultStatus,
    DeviceUnpairingResultStatus,
};
use windows::Foundation::{IReference, PropertyValue, TypedEventHandler};
use windows::Storage::Streams::{DataReader, DataWriter, IBuffer};
use windows::core::{GUID, HSTRING, IInspectable, Interface, Ref};

use crate::advert::{self, AdStructure, AdvertOptions, Phy, ad};
use crate::gatt;
use crate::names::Kind;
use crate::radio::Bond;
use crate::scanner::{AddressKind, AdvKind, Advert, ScanSetup};
use crate::server::{self, Place, ServerEvent, Service, VALUE_MAX, Values, property};

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
    /// Starts a scan, active or passive as `setup` says, handing every advert
    /// to `sink`, timed from `started`.
    pub fn start(
        sink: UnboundedSender<Advert>,
        started: Instant,
        setup: ScanSetup,
    ) -> Result<Self, String> {
        let problem =
            |error: windows::core::Error| format!("Windows could not scan: {}", error.message());
        let watcher = BluetoothLEAdvertisementWatcher::new().map_err(problem)?;
        let mode = if setup.passive {
            BluetoothLEScanningMode::Passive
        } else {
            BluetoothLEScanningMode::Active
        };
        watcher.SetScanningMode(mode).map_err(problem)?;
        // Bluetooth 5 adverts too, where the adapter and Windows can.
        let _ = watcher.SetAllowExtendedAdvertisements(true);
        if setup.coded {
            watcher.SetUseCodedPhy(true).map_err(|error| {
                format!("Windows could not scan the Coded PHY: {}", winrt(error))
            })?;
        }
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
    /// Starts advertising these AD structures, with `options`. Windows puts
    /// some in the advert itself and refuses others, such as Flags and the
    /// name: each refused one is said, by its type.
    pub fn start(structures: &[AdStructure], options: &AdvertOptions) -> Result<Self, String> {
        let publisher = BluetoothLEAdvertisementPublisher::new().map_err(winrt)?;
        set_options(&publisher, options)?;
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

/// Why this computer's Bluetooth adapter cannot be a GATT server, in words;
/// nothing when it can. Blocks while Windows answers.
/// Sets what an advert is sent with besides its AD structures.
fn set_options(
    publisher: &BluetoothLEAdvertisementPublisher,
    options: &AdvertOptions,
) -> Result<(), String> {
    let refused = |what: &'static str| {
        move |error: windows::core::Error| format!("Windows could not set {what}: {}", winrt(error))
    };
    if let Some(dbm) = options.tx_power {
        let value = PropertyValue::CreateInt16(dbm)
            .and_then(|value| value.cast::<IReference<i16>>())
            .map_err(refused("the transmit power"))?;
        publisher
            .SetPreferredTransmitPowerLevelInDBm(&value)
            .map_err(refused("the transmit power"))?;
    }
    if options.extended {
        publisher
            .SetUseExtendedAdvertisement(true)
            .map_err(refused("an extended advert"))?;
        if options.include_tx_power {
            publisher
                .SetIncludeTransmitPowerLevel(true)
                .map_err(refused("the transmit power in the advert"))?;
        }
        // 1M is what Windows uses unless told, so asking for it needs no newer Windows.
        if options.primary_phy != Phy::OneM {
            publisher
                .SetPrimaryPhy(phy_type(options.primary_phy))
                .map_err(refused("the primary PHY"))?;
        }
        if options.secondary_phy != Phy::OneM {
            publisher
                .SetSecondaryPhy(phy_type(options.secondary_phy))
                .map_err(refused("the secondary PHY"))?;
        }
    }
    Ok(())
}

fn phy_type(phy: Phy) -> BluetoothLEAdvertisementPhyType {
    match phy {
        Phy::OneM => BluetoothLEAdvertisementPhyType::Uncoded1MPhy,
        Phy::TwoM => BluetoothLEAdvertisementPhyType::Uncoded2MPhy,
        Phy::Coded => BluetoothLEAdvertisementPhyType::CodedPhy,
    }
}

/// What this Windows and its Bluetooth adapter can do besides the basics.
/// Each is asked once; an older Windows has no way to say some of them.
#[derive(Clone, Copy, Debug, Default)]
pub struct Features {
    /// The adapter has the LE 2M PHY; none where Windows cannot say.
    pub phy_2m: Option<bool>,
    /// The adapter has the LE Coded PHY; none where Windows cannot say.
    pub phy_coded: Option<bool>,
    /// The adapter can send extended adverts; none where Windows cannot say.
    pub extended_adverts: Option<bool>,
    /// Windows can choose the PHYs it scans on.
    pub scan_phys: bool,
    /// Windows can set an advert's transmit power.
    pub advert_tx_power: bool,
    /// Windows can send an extended advert.
    pub advert_extended: bool,
    /// Windows can choose an extended advert's PHYs.
    pub advert_phys: bool,
}

/// Asks Windows what it and the adapter can do. Blocks: call it off the async runtime.
pub fn features() -> Features {
    let adapter = BluetoothAdapter::GetDefaultAsync()
        .and_then(|operation| operation.join())
        .ok();
    let watcher = BluetoothLEAdvertisementWatcher::new().ok();
    let publisher = BluetoothLEAdvertisementPublisher::new().ok();
    // A property that this Windows cannot read is one it does not have.
    Features {
        phy_2m: adapter
            .as_ref()
            .and_then(|adapter| adapter.IsLowEnergyUncoded2MPhySupported().ok()),
        phy_coded: adapter
            .as_ref()
            .and_then(|adapter| adapter.IsLowEnergyCodedPhySupported().ok()),
        extended_adverts: adapter
            .as_ref()
            .and_then(|adapter| adapter.IsExtendedAdvertisingSupported().ok()),
        scan_phys: watcher
            .as_ref()
            .is_some_and(|watcher| watcher.UseCodedPhy().is_ok()),
        advert_tx_power: publisher
            .as_ref()
            .is_some_and(|publisher| publisher.IncludeTransmitPowerLevel().is_ok()),
        advert_extended: publisher
            .as_ref()
            .is_some_and(|publisher| publisher.UseExtendedAdvertisement().is_ok()),
        advert_phys: publisher
            .as_ref()
            .is_some_and(|publisher| publisher.PrimaryPhy().is_ok()),
    }
}

/// "1M", "2M" or "Coded".
fn phy_name(info: &BluetoothLEConnectionPhyInfo) -> &'static str {
    if info.IsCodedPhy().unwrap_or(false) {
        "Coded"
    } else if info.IsUncoded2MPhy().unwrap_or(false) {
        "2M"
    } else if info.IsUncoded1MPhy().unwrap_or(false) {
        "1M"
    } else {
        "?"
    }
}

/// The PHY a connection sends on, and the one it receives on.
fn connection_phy(
    device: &BluetoothLEDevice,
) -> windows::core::Result<(&'static str, &'static str)> {
    let phy = device.GetConnectionPhy()?;
    Ok((
        phy_name(&phy.TransmitInfo()?),
        phy_name(&phy.ReceiveInfo()?),
    ))
}

/// A connection's PHY, told now and each time it changes, until dropped.
pub struct PhyWatch {
    device: BluetoothLEDevice,
    token: i64,
}

impl PhyWatch {
    /// Tells `told` the PHYs of the connection to `address`, sending and
    /// receiving, now and each time they change. Blocks: call it off the
    /// async runtime.
    pub fn start(
        address: &str,
        told: impl Fn(&'static str, &'static str) + Send + 'static,
    ) -> Result<Self, String> {
        let device = le_device(address)?;
        let (tx, rx) = connection_phy(&device).map_err(winrt)?;
        told(tx, rx);
        let handler = TypedEventHandler::new(move |device: Ref<BluetoothLEDevice>, _| {
            if let Ok(device) = device.ok()
                && let Ok((tx, rx)) = connection_phy(device)
            {
                told(tx, rx);
            }
            Ok(())
        });
        let token = device.ConnectionPhyChanged(&handler).map_err(winrt)?;
        Ok(Self { device, token })
    }
}

impl Drop for PhyWatch {
    fn drop(&mut self) {
        let _ = self.device.RemoveConnectionPhyChanged(self.token);
    }
}

pub fn peripheral_problem() -> Option<String> {
    let adapter = match BluetoothAdapter::GetDefaultAsync().and_then(|operation| operation.join()) {
        Ok(adapter) => adapter,
        Err(error) => {
            return Some(format!(
                "Windows found no Bluetooth adapter to be a peripheral with: {}",
                winrt(error)
            ));
        }
    };
    match adapter.IsPeripheralRoleSupported() {
        Ok(true) => None,
        Ok(false) => Some(
            "This Bluetooth adapter cannot be a peripheral: Windows says its chip or driver does not support the peripheral role, so it cannot be a GATT server"
                .into(),
        ),
        Err(error) => Some(format!(
            "Windows could not tell whether this adapter can be a peripheral: {}",
            winrt(error)
        )),
    }
}

/// What a Bluetooth error from Windows means, in words.
fn bluetooth_error(error: BluetoothError) -> String {
    match error {
        BluetoothError::RadioNotAvailable => "the Bluetooth radio is off or not there".into(),
        BluetoothError::ResourceInUse => {
            "the radio is busy: another app may be serving the same service".into()
        }
        BluetoothError::DisabledByPolicy => {
            "a policy on this computer does not allow it".into()
        }
        BluetoothError::NotSupported => {
            "this adapter or Windows does not support it; Windows also keeps some services for itself"
                .into()
        }
        BluetoothError::DisabledByUser => "Bluetooth is turned off".into(),
        BluetoothError::ConsentRequired => "Windows wants the user's consent first".into(),
        BluetoothError::TransportNotSupported => {
            "the adapter does not support Bluetooth LE".into()
        }
        other => format!("Windows error {}", other.0),
    }
}

fn guid(uuid: &uuid::Uuid) -> GUID {
    GUID::from_u128(uuid.as_u128())
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A client's address, from its GATT session with us.
fn client(session: windows::core::Result<GattSession>) -> String {
    session
        .and_then(|session| session.DeviceId())
        .and_then(|id| id.Id())
        .map(|id| server::client_address(&id.to_string()))
        .unwrap_or_else(|_| "A client".into())
}

/// The clients subscribed to a characteristic's notifications or indications.
fn subscribers(characteristic: &GattLocalCharacteristic) -> Vec<String> {
    let mut list = Vec::new();
    if let Ok(clients) = characteristic.SubscribedClients() {
        for subscribed in &clients {
            list.push(client(subscribed.Session()));
        }
    }
    list
}

fn failed(place: Place, action: &str, problem: String) -> ServerEvent {
    ServerEvent::Failed {
        place: Some(place),
        action: action.into(),
        problem,
    }
}

/// The values every handler reads and changes.
type Shared = Arc<Mutex<Values>>;

/// Answers a client's read with the value as it is now, from the offset it asked.
fn answer_read(
    args: &GattReadRequestedEventArgs,
    place: Place,
    values: &Shared,
    sink: &UnboundedSender<ServerEvent>,
) {
    let deferral = args.GetDeferral().ok();
    let client = client(args.Session());
    let event = match args
        .GetRequestAsync()
        .and_then(|operation| operation.join())
    {
        Ok(request) => {
            let offset = request.Offset().unwrap_or(0);
            let value = lock(values).get(place).unwrap_or_default().to_vec();
            match server::read_at(&value, offset) {
                Ok(data) => match buffer(&data)
                    .and_then(|data| request.RespondWithValue(&data).map_err(winrt))
                {
                    Ok(()) => ServerEvent::Read {
                        place,
                        client,
                        offset,
                        data,
                    },
                    Err(problem) => failed(place, "read", problem),
                },
                Err(code) => {
                    let _ = request.RespondWithProtocolError(code);
                    failed(
                        place,
                        "read",
                        format!(
                            "{client} read at offset {offset}, past the end of the {}-byte value: answered with ATT error 0x{code:02X}, {}",
                            value.len(),
                            gatt::att_error(code)
                        ),
                    )
                }
            }
        }
        Err(error) => failed(
            place,
            "read",
            format!(
                "Windows did not hand over {client}'s read: {}",
                winrt(error)
            ),
        ),
    };
    let _ = sink.send(event);
    if let Some(deferral) = deferral {
        let _ = deferral.Complete();
    }
}

/// A characteristic a write is echoed to, and where it is.
type Echo = Option<(GattLocalCharacteristic, Place)>;

/// Takes a client's write into the value, answers it when it wants an
/// answer, and echoes what was written where the service says.
fn answer_write(
    args: &GattWriteRequestedEventArgs,
    place: Place,
    values: &Shared,
    sink: &UnboundedSender<ServerEvent>,
    echo: &Echo,
) {
    let deferral = args.GetDeferral().ok();
    let client = client(args.Session());
    let mut echoed = None;
    match args
        .GetRequestAsync()
        .and_then(|operation| operation.join())
    {
        Ok(request) => {
            let with_response = request
                .Option()
                .map(|option| option == GattWriteOption::WriteWithResponse)
                .unwrap_or(true);
            let offset = request.Offset().unwrap_or(0);
            let data = request
                .Value()
                .map(|value| bytes(&value))
                .unwrap_or_default();
            let written = {
                let mut values = lock(values);
                let old = values.get(place).unwrap_or_default().to_vec();
                server::write_at(&old, offset, &data).inspect(|value| {
                    values.set(place, value.clone());
                })
            };
            match written {
                Ok(value) => {
                    if with_response {
                        let _ = request.Respond();
                    }
                    echoed = Some(data.clone());
                    let _ = sink.send(ServerEvent::Written {
                        place,
                        client,
                        with_response,
                        offset,
                        data,
                        value,
                    });
                }
                Err(code) => {
                    if with_response {
                        let _ = request.RespondWithProtocolError(code);
                    }
                    let _ = sink.send(failed(
                        place,
                        "write",
                        format!(
                            "{client} wrote {} bytes at offset {offset}: refused with ATT error 0x{code:02X}, {}",
                            data.len(),
                            gatt::att_error(code)
                        ),
                    ));
                }
            }
        }
        Err(error) => {
            let _ = sink.send(failed(
                place,
                "write",
                format!(
                    "Windows did not hand over {client}'s write: {}",
                    winrt(error)
                ),
            ));
        }
    }
    if let Some(deferral) = deferral {
        let _ = deferral.Complete();
    }
    // After the write is answered, so the client is not kept waiting for it.
    if let (Some(data), Some((target, target_place))) = (echoed, echo)
        && let Err(problem) = notify(target, *target_place, &data, values, sink, true)
    {
        let _ = sink.send(failed(*target_place, "echo", problem));
    }
}

/// Sets a value and notifies or indicates it to every client subscribed, as
/// each one asked. Returns how many were subscribed; what reached them goes
/// to `sink`.
fn notify(
    characteristic: &GattLocalCharacteristic,
    place: Place,
    data: &[u8],
    values: &Shared,
    sink: &UnboundedSender<ServerEvent>,
    echo: bool,
) -> Result<usize, String> {
    lock(values).set(place, data.to_vec());
    let subscribed = characteristic
        .SubscribedClients()
        .and_then(|clients| clients.Size())
        .unwrap_or(0) as usize;
    if subscribed == 0 {
        return Ok(0);
    }
    let results = characteristic
        .NotifyValueAsync(&buffer(data)?)
        .and_then(|operation| operation.join())
        .map_err(|error| format!("Windows could not notify: {}", winrt(error)))?;
    let (mut sent, mut missed) = (Vec::new(), Vec::new());
    for result in &results {
        let who = client(result.SubscribedClient().and_then(|c| c.Session()));
        match result.Status() {
            Ok(GattCommunicationStatus::Success) => sent.push(who),
            Ok(GattCommunicationStatus::Unreachable) => {
                missed.push(format!("{who}: it did not answer"));
            }
            Ok(GattCommunicationStatus::ProtocolError) => {
                let code = result
                    .ProtocolError()
                    .and_then(|code| code.Value())
                    .unwrap_or(0);
                missed.push(format!(
                    "{who}: ATT error 0x{code:02X}, {}",
                    gatt::att_error(code)
                ));
            }
            Ok(GattCommunicationStatus::AccessDenied) => {
                missed.push(format!("{who}: access denied"));
            }
            Ok(other) => missed.push(format!("{who}: Windows status {}", other.0)),
            Err(error) => missed.push(format!("{who}: {}", winrt(error))),
        }
    }
    let _ = sink.send(ServerEvent::Notified {
        place,
        data: data.to_vec(),
        sent,
        failed: missed,
        echo,
    });
    Ok(subscribed)
}

/// Where a service's advert is, in words, and whether it is on the air.
fn advertising_text(
    args: &GattServiceProviderAdvertisementStatusChangedEventArgs,
) -> (bool, String) {
    match args.Status() {
        Ok(GattServiceProviderAdvertisementStatus::Started) => (true, "Advertising".into()),
        Ok(GattServiceProviderAdvertisementStatus::StartedWithoutAllAdvertisementData) => (
            true,
            "Advertising, without all of its data: the advert is full".into(),
        ),
        Ok(GattServiceProviderAdvertisementStatus::Created) => {
            (false, "Ready, not advertising yet".into())
        }
        Ok(GattServiceProviderAdvertisementStatus::Stopped) => (false, "Not advertising".into()),
        // With no error, Windows is making the advert again, as when another
        // service joins it, and starts it again by itself.
        Ok(GattServiceProviderAdvertisementStatus::Aborted) => match args.Error() {
            Ok(BluetoothError::Success) => {
                (false, "Paused while Windows makes the advert again".into())
            }
            Ok(error) => (
                false,
                format!("Windows stopped advertising it: {}", bluetooth_error(error)),
            ),
            Err(error) => (
                false,
                format!("Windows stopped advertising it: {}", winrt(error)),
            ),
        },
        Ok(other) => (false, format!("Windows status {}", other.0)),
        Err(error) => (false, winrt(error)),
    }
}

/// The event handlers on a characteristic, to take them away again.
enum Handler {
    Read(i64),
    Write(i64),
    Subscribed(i64),
}

/// bleNIMA as a GATT server: one GattServiceProvider for each service, each
/// advertised as connectable. Dropping it stops them all.
pub struct GattServer {
    providers: Vec<(GattServiceProvider, i64)>,
    characteristics: Vec<Vec<(GattLocalCharacteristic, Vec<Handler>)>>,
    values: Shared,
    sink: UnboundedSender<ServerEvent>,
}

impl GattServer {
    /// Makes the services, answers their reads and writes, and advertises
    /// them. What clients do goes to `sink`. Blocks while Windows makes them.
    pub fn start(services: &[Service], sink: UnboundedSender<ServerEvent>) -> Result<Self, String> {
        server::check(services)?;
        if let Some(problem) = peripheral_problem() {
            return Err(problem);
        }
        // On an error the server is dropped, and what was made so far with it.
        let mut made = Self {
            providers: Vec::new(),
            characteristics: Vec::new(),
            values: Arc::new(Mutex::new(Values::new(services))),
            sink,
        };
        for (at, service) in services.iter().enumerate() {
            made.add(at, service)?;
        }
        Ok(made)
    }

    fn add(&mut self, at: usize, service: &Service) -> Result<(), String> {
        let service_name = server::name(&service.uuid, Kind::Service);
        let refused = |why: String| format!("Windows could not serve {service_name}: {why}");
        let result = GattServiceProvider::CreateAsync(guid(&service.uuid))
            .and_then(|operation| operation.join())
            .map_err(|error| refused(winrt(error)))?;
        match result.Error() {
            Ok(BluetoothError::Success) => {}
            Ok(error) => return Err(refused(bluetooth_error(error))),
            Err(error) => return Err(refused(winrt(error))),
        }
        let provider = result
            .ServiceProvider()
            .map_err(|error| refused(winrt(error)))?;
        let local = provider.Service().map_err(|error| refused(winrt(error)))?;

        let mut characteristics = Vec::new();
        for characteristic in &service.characteristics {
            let what = format!(
                "{} in {service_name}",
                server::name(&characteristic.uuid, Kind::Characteristic)
            );
            let refused = |error: windows::core::Error| {
                format!("Windows could not make {what}: {}", winrt(error))
            };
            let parameters = GattLocalCharacteristicParameters::new().map_err(refused)?;
            parameters
                .SetCharacteristicProperties(GattCharacteristicProperties(u32::from(
                    characteristic.properties & property::ALL,
                )))
                .map_err(refused)?;
            parameters
                .SetReadProtectionLevel(GattProtectionLevel::Plain)
                .map_err(refused)?;
            parameters
                .SetWriteProtectionLevel(GattProtectionLevel::Plain)
                .map_err(refused)?;
            if !characteristic.description.is_empty() {
                parameters
                    .SetUserDescription(&HSTRING::from(characteristic.description.as_str()))
                    .map_err(refused)?;
            }
            let result = local
                .CreateCharacteristicAsync(guid(&characteristic.uuid), &parameters)
                .and_then(|operation| operation.join())
                .map_err(refused)?;
            match result.Error() {
                Ok(BluetoothError::Success) => {}
                Ok(error) => {
                    return Err(format!(
                        "Windows could not make {what}: {}",
                        bluetooth_error(error)
                    ));
                }
                Err(error) => return Err(refused(error)),
            }
            characteristics.push(result.Characteristic().map_err(refused)?);
        }

        // The handlers, once every characteristic is made, so a write can echo
        // to one made after it.
        let mut handled = Vec::new();
        for (index, (setup, characteristic)) in service
            .characteristics
            .iter()
            .zip(&characteristics)
            .enumerate()
        {
            let place = Place {
                service: at,
                characteristic: index,
            };
            let mut handlers = Vec::new();
            let problem = |error: windows::core::Error| {
                format!("Windows refused a handler: {}", winrt(error))
            };
            if setup.properties & property::READ != 0 {
                let (values, sink) = (self.values.clone(), self.sink.clone());
                let token = characteristic
                    .ReadRequested(&TypedEventHandler::new(
                        move |_, args: Ref<GattReadRequestedEventArgs>| {
                            if let Ok(args) = args.ok() {
                                answer_read(args, place, &values, &sink);
                            }
                            Ok(())
                        },
                    ))
                    .map_err(problem)?;
                handlers.push(Handler::Read(token));
            }
            if setup.properties & (property::WRITE | property::WRITE_WITHOUT_RESPONSE) != 0 {
                let (values, sink) = (self.values.clone(), self.sink.clone());
                let echo: Echo = setup.echo.and_then(|target| {
                    let local = characteristics.get(target)?.clone();
                    Some((
                        local,
                        Place {
                            service: at,
                            characteristic: target,
                        },
                    ))
                });
                let token = characteristic
                    .WriteRequested(&TypedEventHandler::new(
                        move |_, args: Ref<GattWriteRequestedEventArgs>| {
                            if let Ok(args) = args.ok() {
                                answer_write(args, place, &values, &sink, &echo);
                            }
                            Ok(())
                        },
                    ))
                    .map_err(problem)?;
                handlers.push(Handler::Write(token));
            }
            if setup.properties & (property::NOTIFY | property::INDICATE) != 0 {
                let sink = self.sink.clone();
                let token = characteristic
                    .SubscribedClientsChanged(&TypedEventHandler::new(
                        move |sender: Ref<GattLocalCharacteristic>, _: Ref<IInspectable>| {
                            if let Ok(sender) = sender.ok() {
                                let _ = sink.send(ServerEvent::Subscribers {
                                    place,
                                    clients: subscribers(sender),
                                });
                            }
                            Ok(())
                        },
                    ))
                    .map_err(problem)?;
                handlers.push(Handler::Subscribed(token));
            }
            handled.push((characteristic.clone(), handlers));
        }
        self.characteristics.push(handled);

        let sink = self.sink.clone();
        let token = provider
            .AdvertisementStatusChanged(&TypedEventHandler::new(
                move |_, args: Ref<GattServiceProviderAdvertisementStatusChangedEventArgs>| {
                    if let Ok(args) = args.ok() {
                        let (on, text) = advertising_text(args);
                        let _ = sink.send(ServerEvent::Advertising {
                            service: at,
                            on,
                            text,
                        });
                    }
                    Ok(())
                },
            ))
            .map_err(|error| refused(winrt(error)))?;
        self.providers.push((provider.clone(), token));
        let parameters = GattServiceProviderAdvertisingParameters::new()
            .map_err(|error| refused(winrt(error)))?;
        parameters
            .SetIsConnectable(true)
            .and_then(|()| parameters.SetIsDiscoverable(true))
            .map_err(|error| refused(winrt(error)))?;
        provider
            .StartAdvertisingWithParameters(&parameters)
            .map_err(|error| {
                format!(
                    "Windows could not advertise {service_name}: {}",
                    winrt(error)
                )
            })
    }

    /// Sets a characteristic's value, for reads; with `notify`, sends it too,
    /// to every client subscribed. Returns how many were subscribed. Blocks
    /// while it notifies.
    pub fn set(&self, place: Place, data: &[u8], notify_it: bool) -> Result<usize, String> {
        if data.len() > VALUE_MAX {
            return Err(format!(
                "A value is {VALUE_MAX} bytes at most; this one has {}",
                data.len()
            ));
        }
        let (characteristic, _) = self
            .characteristics
            .get(place.service)
            .and_then(|service| service.get(place.characteristic))
            .ok_or("There is no such characteristic on the server")?;
        if notify_it {
            notify(characteristic, place, data, &self.values, &self.sink, false)
        } else {
            lock(&self.values).set(place, data.to_vec());
            Ok(0)
        }
    }
}

impl Drop for GattServer {
    fn drop(&mut self) {
        for (provider, token) in &self.providers {
            let _ = provider.RemoveAdvertisementStatusChanged(*token);
            let _ = provider.StopAdvertising();
        }
        for (characteristic, handlers) in self.characteristics.iter().flatten() {
            for handler in handlers {
                let _ = match handler {
                    Handler::Read(token) => characteristic.RemoveReadRequested(*token),
                    Handler::Write(token) => characteristic.RemoveWriteRequested(*token),
                    Handler::Subscribed(token) => {
                        characteristic.RemoveSubscribedClientsChanged(*token)
                    }
                };
            }
        }
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
