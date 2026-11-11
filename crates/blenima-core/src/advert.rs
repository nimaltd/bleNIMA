//! Adverts: their AD structures, read into words, and the beacons in them.
//!
//! An advert's data is a list of AD structures, each a length byte, a type byte
//! and the data, as the Bluetooth Core Supplement describes. The types and their
//! names are from the Bluetooth SIG's assigned numbers.

use std::fmt::Write;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::names;

/// One AD structure of an advert: its type and its data, without the length byte.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdStructure {
    pub ad_type: u8,
    pub data: Vec<u8>,
}

impl AdStructure {
    pub fn new(ad_type: u8, data: impl Into<Vec<u8>>) -> Self {
        Self {
            ad_type,
            data: data.into(),
        }
    }
}

/// A PHY an extended advert is sent on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phy {
    /// LE 1M, which every Bluetooth LE radio has.
    #[default]
    #[serde(rename = "1m")]
    OneM,
    /// LE 2M, twice as fast, Bluetooth 5.
    #[serde(rename = "2m")]
    TwoM,
    /// LE Coded, for long range, Bluetooth 5.
    #[serde(rename = "coded")]
    Coded,
}

/// What an advert is sent with besides its AD structures, where the OS and the
/// adapter let an app choose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdvertOptions {
    /// The transmit power asked for, in dBm; none for the OS's own.
    pub tx_power: Option<i16>,
    /// Says the transmit power in an extended advert's header.
    pub include_tx_power: bool,
    /// A Bluetooth 5 extended advert: longer, and on the PHYs below.
    pub extended: bool,
    /// The PHY an extended advert starts on: 1M or Coded.
    pub primary_phy: Phy,
    /// The PHY an extended advert's data goes on: 1M, 2M or Coded.
    pub secondary_phy: Phy,
}

/// AD types that the decoding and the device list use.
pub mod ad {
    pub const FLAGS: u8 = 0x01;
    pub const UUID16_SOME: u8 = 0x02;
    pub const UUID16_ALL: u8 = 0x03;
    pub const UUID32_SOME: u8 = 0x04;
    pub const UUID32_ALL: u8 = 0x05;
    pub const UUID128_SOME: u8 = 0x06;
    pub const UUID128_ALL: u8 = 0x07;
    pub const SHORT_NAME: u8 = 0x08;
    pub const NAME: u8 = 0x09;
    pub const TX_POWER: u8 = 0x0A;
    pub const CONNECTION_INTERVAL: u8 = 0x12;
    pub const SOLICIT16: u8 = 0x14;
    pub const SOLICIT128: u8 = 0x15;
    pub const SERVICE_DATA16: u8 = 0x16;
    pub const APPEARANCE: u8 = 0x19;
    pub const ADV_INTERVAL: u8 = 0x1A;
    pub const DEVICE_ADDRESS: u8 = 0x1B;
    pub const LE_ROLE: u8 = 0x1C;
    pub const SOLICIT32: u8 = 0x1F;
    pub const SERVICE_DATA32: u8 = 0x20;
    pub const SERVICE_DATA128: u8 = 0x21;
    pub const URI: u8 = 0x24;
    pub const MESH_BEACON: u8 = 0x2B;
    pub const ADV_INTERVAL_LONG: u8 = 0x2F;
    pub const BROADCAST_NAME: u8 = 0x30;
    pub const MANUFACTURER: u8 = 0xFF;
}

/// Splits an advert's data into its AD structures. A zero length ends the
/// data, as padding does; a structure longer than what is left is dropped.
pub fn parse(bytes: &[u8]) -> Vec<AdStructure> {
    let mut structures = Vec::new();
    let mut rest = bytes;
    while let Some((&length, after)) = rest.split_first() {
        let length = usize::from(length);
        if length == 0 || length > after.len() {
            break;
        }
        structures.push(AdStructure::new(after[0], &after[1..length]));
        rest = &after[length..];
    }
    structures
}

/// The bytes of an advert made of these AD structures, as it is sent on the air.
pub fn encode(structures: &[AdStructure]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for structure in structures {
        bytes.push((structure.data.len() + 1).min(255) as u8);
        bytes.push(structure.ad_type);
        bytes.extend_from_slice(&structure.data);
    }
    bytes
}

/// The name of an AD type, from the assigned numbers.
pub fn type_name(ad_type: u8) -> &'static str {
    match ad_type {
        0x01 => "Flags",
        0x02 => "Incomplete list of 16-bit service UUIDs",
        0x03 => "Complete list of 16-bit service UUIDs",
        0x04 => "Incomplete list of 32-bit service UUIDs",
        0x05 => "Complete list of 32-bit service UUIDs",
        0x06 => "Incomplete list of 128-bit service UUIDs",
        0x07 => "Complete list of 128-bit service UUIDs",
        0x08 => "Shortened local name",
        0x09 => "Complete local name",
        0x0A => "TX power level",
        0x0D => "Class of device",
        0x0E => "Simple pairing hash C-192",
        0x0F => "Simple pairing randomizer R-192",
        0x10 => "Device ID / Security Manager TK value",
        0x11 => "Security Manager OOB flags",
        0x12 => "Peripheral connection interval range",
        0x14 => "16-bit service solicitation UUIDs",
        0x15 => "128-bit service solicitation UUIDs",
        0x16 => "Service data, 16-bit UUID",
        0x17 => "Public target address",
        0x18 => "Random target address",
        0x19 => "Appearance",
        0x1A => "Advertising interval",
        0x1B => "LE Bluetooth device address",
        0x1C => "LE role",
        0x1D => "Simple pairing hash C-256",
        0x1E => "Simple pairing randomizer R-256",
        0x1F => "32-bit service solicitation UUIDs",
        0x20 => "Service data, 32-bit UUID",
        0x21 => "Service data, 128-bit UUID",
        0x22 => "LE Secure Connections confirmation value",
        0x23 => "LE Secure Connections random value",
        0x24 => "URI",
        0x25 => "Indoor positioning",
        0x26 => "Transport discovery data",
        0x27 => "LE supported features",
        0x28 => "Channel map update indication",
        0x29 => "PB-ADV",
        0x2A => "Mesh message",
        0x2B => "Mesh beacon",
        0x2C => "BIGInfo",
        0x2D => "Broadcast code",
        0x2E => "Resolvable set identifier",
        0x2F => "Advertising interval, long",
        0x30 => "Broadcast name",
        0x31 => "Encrypted advertising data",
        0x32 => "Periodic advertising response timing",
        0x34 => "Electronic shelf label",
        0x3D => "3D information data",
        0xFF => "Manufacturer specific data",
        _ => "Unknown type",
    }
}

/// An AD structure in words, for the details of a device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Field {
    pub ad_type: u8,
    /// What it is: "Flags", "Complete local name".
    pub name: String,
    /// What it says: "LE General Discoverable, BR/EDR not supported".
    pub value: String,
    /// Its data in hex, as it was sent.
    pub hex: String,
}

/// Bytes as hex, a space between each: "02 01 06".
pub fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 3);
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            text.push(' ');
        }
        let _ = write!(text, "{byte:02X}");
    }
    text
}

fn u16_le(bytes: &[u8]) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(..2)?.try_into().ok()?))
}

/// A UUID with its name, if known: "0x180F Battery Service".
pub fn named_uuid(uuid: &Uuid) -> String {
    match names::any_uuid_name(uuid) {
        Some(name) => format!("{} {name}", names::uuid_text(uuid)),
        None => names::uuid_text(uuid),
    }
}

/// The UUIDs in a list of 16, 32 or 128-bit UUIDs, each sent little-endian.
pub fn uuid_list(data: &[u8], size: usize) -> Vec<Uuid> {
    data.chunks_exact(size)
        .map(|chunk| uuid_from_le(chunk).unwrap_or_default())
        .collect()
}

/// A 2, 4 or 16-byte UUID as it is sent, little-endian.
pub fn uuid_from_le(bytes: &[u8]) -> Option<Uuid> {
    match bytes.len() {
        2 => Some(names::from_short(u32::from(u16::from_le_bytes(
            bytes.try_into().ok()?,
        )))),
        4 => Some(names::from_short(u32::from_le_bytes(
            bytes.try_into().ok()?,
        ))),
        16 => {
            let mut big: [u8; 16] = bytes.try_into().ok()?;
            big.reverse();
            Some(Uuid::from_bytes(big))
        }
        _ => None,
    }
}

/// A 2, 4 or 16-byte UUID's bytes as they are sent, little-endian.
pub fn uuid_to_le(uuid: &Uuid, size: usize) -> Vec<u8> {
    match (size, names::to_short(uuid)) {
        (2, Some(short)) => (short as u16).to_le_bytes().to_vec(),
        (4, Some(short)) => short.to_le_bytes().to_vec(),
        _ => {
            let mut bytes = *uuid.as_bytes();
            bytes.reverse();
            bytes.to_vec()
        }
    }
}

/// The meaning of the Flags bits that are set.
pub fn flags_text(flags: u8) -> String {
    const BITS: [&str; 5] = [
        "LE Limited Discoverable",
        "LE General Discoverable",
        "BR/EDR not supported",
        "LE and BR/EDR controller",
        "LE and BR/EDR host",
    ];
    let set: Vec<&str> = BITS
        .iter()
        .enumerate()
        .filter(|(bit, _)| flags & (1 << bit) != 0)
        .map(|(_, name)| *name)
        .collect();
    if set.is_empty() {
        "none".into()
    } else {
        set.join(", ")
    }
}

/// A time in units of 1.25 ms, as connection intervals are given.
fn interval_1_25(units: u16) -> String {
    if units == 0xFFFF {
        "no preference".into()
    } else {
        milliseconds(f64::from(units) * 1.25)
    }
}

/// "7.5 ms", "100 ms", "1.28 s"
pub fn milliseconds(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{} s", trim_number(ms / 1000.0))
    } else {
        format!("{} ms", trim_number(ms))
    }
}

/// A number with no more than 3 decimals, and no zeros at the end.
fn trim_number(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Puts an AD structure into words.
pub fn describe(structure: &AdStructure) -> Field {
    let data = &structure.data;
    let value = describe_value(structure.ad_type, data).unwrap_or_else(|| {
        if data.is_empty() {
            "(empty)".into()
        } else {
            hex(data)
        }
    });
    Field {
        ad_type: structure.ad_type,
        name: type_name(structure.ad_type).into(),
        value,
        hex: hex(data),
    }
}

fn describe_value(ad_type: u8, data: &[u8]) -> Option<String> {
    let uuids = |size: usize| {
        let list: Vec<String> = uuid_list(data, size).iter().map(named_uuid).collect();
        if list.is_empty() {
            "none".into()
        } else {
            list.join(", ")
        }
    };
    Some(match ad_type {
        ad::FLAGS => flags_text(*data.first()?),
        ad::UUID16_SOME | ad::UUID16_ALL | ad::SOLICIT16 => uuids(2),
        ad::UUID32_SOME | ad::UUID32_ALL | ad::SOLICIT32 => uuids(4),
        ad::UUID128_SOME | ad::UUID128_ALL | ad::SOLICIT128 => uuids(16),
        ad::SHORT_NAME | ad::NAME | ad::BROADCAST_NAME => {
            format!("\"{}\"", String::from_utf8_lossy(data))
        }
        ad::TX_POWER => format!("{} dBm", *data.first()? as i8),
        ad::CONNECTION_INTERVAL => {
            let min = u16_le(data)?;
            let max = u16_le(data.get(2..)?)?;
            format!("{} to {}", interval_1_25(min), interval_1_25(max))
        }
        ad::SERVICE_DATA16 | ad::SERVICE_DATA32 | ad::SERVICE_DATA128 => {
            let size = match ad_type {
                ad::SERVICE_DATA16 => 2,
                ad::SERVICE_DATA32 => 4,
                _ => 16,
            };
            let uuid = uuid_from_le(data.get(..size)?)?;
            format!("{}: {}", named_uuid(&uuid), hex(&data[size..]))
        }
        0x17 | 0x18 => data
            .as_chunks::<6>()
            .0
            .iter()
            .map(|address| address_text(address))
            .collect::<Vec<_>>()
            .join(", "),
        ad::APPEARANCE => {
            let value = u16_le(data)?;
            match names::appearance_name(value) {
                Some(name) => format!("{name} (0x{value:04X})"),
                None => format!("0x{value:04X}"),
            }
        }
        ad::ADV_INTERVAL => milliseconds(f64::from(u16_le(data)?) * 0.625),
        ad::ADV_INTERVAL_LONG => {
            let mut bytes = [0u8; 4];
            bytes[..data.len().min(4)].copy_from_slice(&data[..data.len().min(4)]);
            milliseconds(f64::from(u32::from_le_bytes(bytes)) * 0.625)
        }
        ad::DEVICE_ADDRESS => {
            let address = address_text(data.get(..6)?);
            let kind = if data.get(6)? & 1 == 0 {
                "public"
            } else {
                "random"
            };
            format!("{address} ({kind})")
        }
        ad::LE_ROLE => match data.first()? {
            0 => "Only peripheral".into(),
            1 => "Only central".into(),
            2 => "Peripheral and central, peripheral preferred".into(),
            3 => "Peripheral and central, central preferred".into(),
            other => format!("0x{other:02X}"),
        },
        ad::URI => uri_text(data),
        ad::MESH_BEACON => match data.first()? {
            0 => format!("Unprovisioned device: {}", hex(&data[1..])),
            1 => format!("Secure network: {}", hex(&data[1..])),
            2 => format!("Private: {}", hex(&data[1..])),
            other => format!("Type 0x{other:02X}: {}", hex(&data[1..])),
        },
        ad::MANUFACTURER => {
            let company = u16_le(data)?;
            let name = names::company_name(company).unwrap_or("unknown company");
            format!("0x{company:04X} {name}: {}", hex(&data[2..]))
        }
        _ => return None,
    })
}

/// A device address sent little-endian, as people write it: "C3:1A:4B:00:12:9F".
pub fn address_text(bytes: &[u8]) -> String {
    bytes
        .iter()
        .rev()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// A URI from an advert: its first character is a code for the scheme.
fn uri_text(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    let mut chars = text.chars();
    let scheme = match chars.next() {
        Some('\u{01}') => "",
        Some('\u{16}') => "http:",
        Some('\u{17}') => "https:",
        _ => return text.into_owned(),
    };
    format!("{scheme}{}", chars.as_str())
}

/// A beacon found in an advert.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Beacon {
    /// "iBeacon", "Eddystone URL", "AltBeacon".
    pub kind: String,
    /// Its fields in words, in order: ("Major", "1").
    pub fields: Vec<(String, String)>,
}

impl Beacon {
    fn new(kind: &str, fields: Vec<(&str, String)>) -> Self {
        Self {
            kind: kind.into(),
            fields: fields
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
        }
    }
}

/// The beacon in an advert, if it is one: iBeacon, Eddystone or AltBeacon.
pub fn beacon(structures: &[AdStructure]) -> Option<Beacon> {
    structures
        .iter()
        .find_map(|structure| match structure.ad_type {
            ad::MANUFACTURER => manufacturer_beacon(&structure.data),
            ad::SERVICE_DATA16 if structure.data.starts_with(&[0xAA, 0xFE]) => {
                eddystone(&structure.data[2..])
            }
            _ => None,
        })
}

fn manufacturer_beacon(data: &[u8]) -> Option<Beacon> {
    let company = u16_le(data)?;
    let rest = &data[2..];
    if company == 0x004C && rest.len() == 23 && rest[0] == 0x02 && rest[1] == 0x15 {
        let uuid = Uuid::from_slice(&rest[2..18]).ok()?;
        let major = u16::from_be_bytes([rest[18], rest[19]]);
        let minor = u16::from_be_bytes([rest[20], rest[21]]);
        return Some(Beacon::new(
            "iBeacon",
            vec![
                ("UUID", uuid.hyphenated().to_string().to_uppercase()),
                ("Major", major.to_string()),
                ("Minor", minor.to_string()),
                ("Power at 1 m", format!("{} dBm", rest[22] as i8)),
            ],
        ));
    }
    if rest.len() == 24 && rest[0] == 0xBE && rest[1] == 0xAC {
        return Some(Beacon::new(
            "AltBeacon",
            vec![
                ("Beacon ID", hex(&rest[2..22])),
                ("Power at 1 m", format!("{} dBm", rest[22] as i8)),
                ("Reserved", format!("0x{:02X}", rest[23])),
            ],
        ));
    }
    None
}

fn eddystone(frame: &[u8]) -> Option<Beacon> {
    let (&kind, rest) = frame.split_first()?;
    match kind {
        0x00 if rest.len() >= 17 => Some(Beacon::new(
            "Eddystone UID",
            vec![
                ("Power at 0 m", format!("{} dBm", rest[0] as i8)),
                ("Namespace", hex(&rest[1..11]).replace(' ', "")),
                ("Instance", hex(&rest[11..17]).replace(' ', "")),
            ],
        )),
        0x10 if rest.len() >= 2 => Some(Beacon::new(
            "Eddystone URL",
            vec![
                ("Power at 0 m", format!("{} dBm", rest[0] as i8)),
                ("URL", eddystone_url(&rest[1..])?),
            ],
        )),
        0x20 if rest.len() >= 13 && rest[0] == 0x00 => {
            let battery = u16::from_be_bytes([rest[1], rest[2]]);
            let temperature = i16::from_be_bytes([rest[3], rest[4]]);
            let adverts = u32::from_be_bytes(rest[5..9].try_into().ok()?);
            let tenths = u32::from_be_bytes(rest[9..13].try_into().ok()?);
            Some(Beacon::new(
                "Eddystone TLM",
                vec![
                    ("Battery", format!("{battery} mV")),
                    (
                        "Temperature",
                        if temperature == i16::MIN {
                            "not known".into()
                        } else {
                            format!("{} °C", trim_number(f64::from(temperature) / 256.0))
                        },
                    ),
                    ("Adverts sent", adverts.to_string()),
                    (
                        "Time since power on",
                        format!("{:.1} s", f64::from(tenths) / 10.0),
                    ),
                ],
            ))
        }
        0x30 if rest.len() >= 9 => Some(Beacon::new(
            "Eddystone EID",
            vec![
                ("Power at 0 m", format!("{} dBm", rest[0] as i8)),
                ("Ephemeral ID", hex(&rest[1..9]).replace(' ', "")),
            ],
        )),
        _ => None,
    }
}

fn eddystone_url(encoded: &[u8]) -> Option<String> {
    const SCHEMES: [&str; 4] = ["http://www.", "https://www.", "http://", "https://"];
    const ENDINGS: [&str; 14] = [
        ".com/", ".org/", ".edu/", ".net/", ".info/", ".biz/", ".gov/", ".com", ".org", ".edu",
        ".net", ".info", ".biz", ".gov",
    ];
    let (&scheme, rest) = encoded.split_first()?;
    let mut url = SCHEMES.get(usize::from(scheme))?.to_string();
    for &byte in rest {
        match ENDINGS.get(usize::from(byte)) {
            Some(ending) => url.push_str(ending),
            None => url.push(char::from(byte)),
        }
    }
    Some(url)
}

/// What the device list shows of a device's adverts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub name: Option<String>,
    pub tx_power: Option<i8>,
    /// Every service UUID in the lists and the service data, once each.
    pub services: Vec<Uuid>,
    /// The company IDs of the manufacturer data, once each.
    pub companies: Vec<u16>,
    pub appearance: Option<u16>,
    pub flags: Option<u8>,
    /// The kind of beacon, such as "iBeacon".
    pub beacon: Option<String>,
}

/// Collects what the list shows from AD structures, from an advert and its scan
/// response together. A complete name wins over a shortened one.
pub fn summarize<'a>(structures: impl IntoIterator<Item = &'a AdStructure>) -> Summary {
    let mut summary = Summary::default();
    let mut short_name = None;
    let all: Vec<&AdStructure> = structures.into_iter().collect();
    for structure in &all {
        let data = &structure.data;
        match structure.ad_type {
            ad::NAME => summary.name = Some(String::from_utf8_lossy(data).into_owned()),
            ad::SHORT_NAME => short_name = Some(String::from_utf8_lossy(data).into_owned()),
            ad::TX_POWER => summary.tx_power = data.first().map(|power| *power as i8),
            ad::FLAGS => summary.flags = data.first().copied(),
            ad::APPEARANCE => summary.appearance = u16_le(data),
            ad::UUID16_SOME | ad::UUID16_ALL => summary.services.extend(uuid_list(data, 2)),
            ad::UUID32_SOME | ad::UUID32_ALL => summary.services.extend(uuid_list(data, 4)),
            ad::UUID128_SOME | ad::UUID128_ALL => summary.services.extend(uuid_list(data, 16)),
            ad::SERVICE_DATA16 => summary
                .services
                .extend(uuid_list(data.get(..2).unwrap_or(&[]), 2)),
            ad::SERVICE_DATA32 => summary
                .services
                .extend(uuid_list(data.get(..4).unwrap_or(&[]), 4)),
            ad::SERVICE_DATA128 => summary
                .services
                .extend(uuid_list(data.get(..16).unwrap_or(&[]), 16)),
            ad::MANUFACTURER => summary.companies.extend(u16_le(data)),
            _ => {}
        }
    }
    if summary.name.is_none() {
        summary.name = short_name;
    }
    let mut seen = Vec::new();
    summary.services.retain(|uuid| {
        let new = !seen.contains(uuid);
        seen.push(*uuid);
        new
    });
    summary.companies.dedup();
    let owned: Vec<AdStructure> = all.into_iter().cloned().collect();
    summary.beacon = beacon(&owned).map(|beacon| beacon.kind);
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Zephyr peripheral's advert: flags, the NUS service, and a name.
    const ADVERT: [u8; 30] = [
        0x02, 0x01, 0x06, // flags
        0x11, 0x07, 0x9E, 0xCA, 0xDC, 0x24, 0x0E, 0xE5, 0xA9, 0xE0, 0x93, 0xF3, 0xA3, 0xB5, 0x01,
        0x00, 0x40, 0x6E, // NUS
        0x07, 0x09, b'n', b'r', b'f', b'5', b'2', b'!', // name, and one byte past the end
        0x05,
    ];

    #[test]
    fn an_advert_splits_into_its_structures() {
        let structures = parse(&ADVERT);
        assert_eq!(structures.len(), 3);
        assert_eq!(structures[0], AdStructure::new(ad::FLAGS, [0x06]));
        assert_eq!(structures[1].ad_type, ad::UUID128_ALL);
        assert_eq!(structures[2], AdStructure::new(ad::NAME, *b"nrf52!"));
        // Back to bytes, without the stray byte at the end.
        assert_eq!(encode(&structures), ADVERT[..29]);
    }

    #[test]
    fn padding_and_cut_off_structures_end_the_data() {
        assert_eq!(parse(&[0x02, 0x01, 0x06, 0x00, 0x00]).len(), 1);
        assert_eq!(parse(&[0x02, 0x01, 0x06, 0x05, 0x09, b'a']).len(), 1);
        assert!(parse(&[]).is_empty());
    }

    #[test]
    fn structures_are_put_into_words() {
        let structures = parse(&ADVERT);
        assert_eq!(
            describe(&structures[0]).value,
            "LE General Discoverable, BR/EDR not supported"
        );
        let nus = describe(&structures[1]).value;
        assert!(
            nus.starts_with("6E400001-B5A3-F393-E0A9-E50E24DCCA9E"),
            "{nus}"
        );
        assert_eq!(describe(&structures[2]).value, "\"nrf52!\"");
        assert_eq!(
            describe(&AdStructure::new(ad::TX_POWER, [0xF8])).value,
            "-8 dBm"
        );
        assert_eq!(
            describe(&AdStructure::new(ad::UUID16_ALL, [0x0F, 0x18, 0x0D, 0x18])).value,
            "0x180F Battery Service, 0x180D Heart Rate"
        );
        assert_eq!(
            describe(&AdStructure::new(
                ad::CONNECTION_INTERVAL,
                [0x06, 0x00, 0xFF, 0xFF]
            ))
            .value,
            "7.5 ms to no preference"
        );
        assert_eq!(
            describe(&AdStructure::new(ad::ADV_INTERVAL, [0xA0, 0x00])).value,
            "100 ms"
        );
        assert_eq!(
            describe(&AdStructure::new(ad::APPEARANCE, [0xC1, 0x00])).value,
            "Watch: Sports Watch (0x00C1)"
        );
        assert_eq!(
            describe(&AdStructure::new(
                ad::MANUFACTURER,
                [0x59, 0x00, 0x01, 0x02]
            ))
            .value,
            "0x0059 Nordic Semiconductor ASA: 01 02"
        );
        assert_eq!(
            describe(&AdStructure::new(ad::SERVICE_DATA16, [0x0F, 0x18, 0x64])).value,
            "0x180F Battery Service: 64"
        );
        assert_eq!(
            describe(&AdStructure::new(
                ad::DEVICE_ADDRESS,
                [0x9F, 0x12, 0x00, 0x4B, 0x1A, 0xC3, 0x01]
            ))
            .value,
            "C3:1A:4B:00:12:9F (random)"
        );
        assert_eq!(
            describe(&AdStructure::new(ad::URI, *b"\x17//nimaltd.github.io")).value,
            "https://nimaltd.github.io"
        );
        // A type with nothing to say about it shows its bytes.
        let unknown = describe(&AdStructure::new(0x77, [0xAB]));
        assert_eq!(
            (unknown.name.as_str(), unknown.value.as_str()),
            ("Unknown type", "AB")
        );
    }

    #[test]
    fn uuids_are_sent_little_endian() {
        let nus = names::parse_uuid("6E400001-B5A3-F393-E0A9-E50E24DCCA9E").unwrap();
        let bytes = uuid_to_le(&nus, 16);
        assert_eq!(bytes[0], 0x9E);
        assert_eq!(uuid_from_le(&bytes), Some(nus));
        let battery = names::from_short(0x180F);
        assert_eq!(uuid_to_le(&battery, 2), [0x0F, 0x18]);
        assert_eq!(uuid_from_le(&[0x0F, 0x18]), Some(battery));
    }

    #[test]
    fn an_ibeacon_is_read() {
        let mut data = vec![0x4C, 0x00, 0x02, 0x15];
        data.extend_from_slice(
            names::parse_uuid("E2C56DB5-DFFB-48D2-B060-D0F5A71096E0")
                .unwrap()
                .as_bytes(),
        );
        data.extend_from_slice(&[0x00, 0x01, 0x00, 0x2A, 0xC5]);
        let found = beacon(&[AdStructure::new(ad::MANUFACTURER, data)]).unwrap();
        assert_eq!(found.kind, "iBeacon");
        assert_eq!(found.fields[0].1, "E2C56DB5-DFFB-48D2-B060-D0F5A71096E0");
        assert_eq!(found.fields[1].1, "1");
        assert_eq!(found.fields[2].1, "42");
        assert_eq!(found.fields[3].1, "-59 dBm");
    }

    #[test]
    fn eddystone_frames_are_read() {
        let url = AdStructure::new(
            ad::SERVICE_DATA16,
            [
                0xAA, 0xFE, 0x10, 0xEB, 0x03, b'n', b'i', b'm', b'a', b'l', b't', b'd', 0x07,
            ],
        );
        let found = beacon(&[url]).unwrap();
        assert_eq!(found.kind, "Eddystone URL");
        assert_eq!(found.fields[1].1, "https://nimaltd.com");

        let tlm = AdStructure::new(
            ad::SERVICE_DATA16,
            [
                0xAA, 0xFE, 0x20, 0x00, 0x0B, 0xB8, 0x16, 0x80, 0x00, 0x00, 0x00, 0x64, 0x00, 0x00,
                0x01, 0xF4,
            ],
        );
        let found = beacon(&[tlm]).unwrap();
        assert_eq!(found.kind, "Eddystone TLM");
        assert_eq!(found.fields[0].1, "3000 mV");
        assert_eq!(found.fields[1].1, "22.5 °C");
        assert_eq!(found.fields[2].1, "100");
        assert_eq!(found.fields[3].1, "50.0 s");
    }

    #[test]
    fn an_altbeacon_is_read() {
        let mut data = vec![0x18, 0x01, 0xBE, 0xAC];
        data.extend_from_slice(&[0x11; 20]);
        data.extend_from_slice(&[0xC5, 0x00]);
        let found = beacon(&[AdStructure::new(ad::MANUFACTURER, data)]).unwrap();
        assert_eq!(found.kind, "AltBeacon");
        assert_eq!(found.fields[1].1, "-59 dBm");
    }

    #[test]
    fn a_summary_takes_from_the_advert_and_its_scan_response() {
        let advert = parse(&ADVERT);
        let response = vec![
            AdStructure::new(ad::TX_POWER, [0x04]),
            AdStructure::new(ad::MANUFACTURER, [0x59, 0x00, 0xAA]),
            AdStructure::new(ad::SHORT_NAME, *b"nrf"),
        ];
        let summary = summarize(advert.iter().chain(&response));
        assert_eq!(summary.name.as_deref(), Some("nrf52!"));
        assert_eq!(summary.tx_power, Some(4));
        assert_eq!(summary.flags, Some(0x06));
        assert_eq!(summary.companies, [0x0059]);
        assert_eq!(summary.services.len(), 1);
        // Only a shortened name: it is the name.
        let summary = summarize(&response);
        assert_eq!(summary.name.as_deref(), Some("nrf"));
    }

    #[test]
    fn advert_options_come_from_the_ui_as_json() {
        let options: AdvertOptions = serde_json::from_str(
            r#"{"tx_power":-4,"include_tx_power":true,"extended":true,"primary_phy":"coded","secondary_phy":"2m"}"#,
        )
        .unwrap();
        assert_eq!(options.tx_power, Some(-4));
        assert!(options.include_tx_power && options.extended);
        assert_eq!(options.primary_phy, Phy::Coded);
        assert_eq!(options.secondary_phy, Phy::TwoM);
        // Nothing chosen is a legacy advert at the OS's own power, on 1M.
        let plain: AdvertOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(plain, AdvertOptions::default());
        assert_eq!(plain.primary_phy, Phy::OneM);
    }
}
