//! Names for the numbers of Bluetooth: services, characteristics, descriptors,
//! company IDs and appearances.
//!
//! The tables are Nordic Semiconductor's Bluetooth numbers database, under the
//! BSD 3-Clause licence in `data/LICENSE-nordic-bluetooth-numbers`, built into
//! the app and read the first time a name is asked for.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;
use uuid::Uuid;

/// The base every 16 and 32-bit Bluetooth UUID is short for:
/// `0000xxxx-0000-1000-8000-00805F9B34FB`.
pub const BASE_UUID: u128 = 0x0000_0000_0000_1000_8000_0080_5F9B_34FB;

/// What a UUID is in the tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Service,
    Characteristic,
    Descriptor,
}

#[derive(Deserialize)]
struct Attribute {
    name: String,
    uuid: String,
}

#[derive(Deserialize)]
struct Company {
    code: u16,
    name: String,
}

#[derive(Deserialize)]
struct Subcategory {
    value: u16,
    name: String,
}

#[derive(Deserialize)]
struct Appearance {
    category: u16,
    name: String,
    #[serde(default)]
    subcategory: Vec<Subcategory>,
}

struct Tables {
    services: HashMap<Uuid, String>,
    characteristics: HashMap<Uuid, String>,
    descriptors: HashMap<Uuid, String>,
    companies: HashMap<u16, String>,
    /// The full 16-bit appearance value: category in the top 10 bits, subcategory in the low 6.
    appearances: HashMap<u16, String>,
}

fn attributes(json: &str) -> HashMap<Uuid, String> {
    let list: Vec<Attribute> = serde_json::from_str(json).unwrap_or_default();
    list.into_iter()
        .filter_map(|attribute| Some((parse_uuid(&attribute.uuid)?, attribute.name)))
        .collect()
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let companies: Vec<Company> =
            serde_json::from_str(include_str!("../data/company_ids.json")).unwrap_or_default();
        let appearances: Vec<Appearance> =
            serde_json::from_str(include_str!("../data/gap_appearance.json")).unwrap_or_default();
        let mut appearance_names = HashMap::new();
        for category in appearances {
            appearance_names.insert(category.category << 6, category.name.clone());
            for sub in category.subcategory {
                appearance_names.insert(
                    (category.category << 6) | sub.value,
                    format!("{}: {}", category.name, sub.name),
                );
            }
        }
        Tables {
            services: attributes(include_str!("../data/service_uuids.json")),
            characteristics: attributes(include_str!("../data/characteristic_uuids.json")),
            descriptors: attributes(include_str!("../data/descriptor_uuids.json")),
            companies: companies
                .into_iter()
                .map(|company| (company.code, company.name))
                .collect(),
            appearances: appearance_names,
        }
    })
}

/// Reads a UUID as the tables and people write it: "180F", "0000180F", or the full
/// 128 bits with or without dashes.
pub fn parse_uuid(text: &str) -> Option<Uuid> {
    let hex: String = text
        .trim()
        .trim_start_matches("0x")
        .chars()
        .filter(|c| *c != '-')
        .collect();
    match hex.len() {
        4 | 8 => u32::from_str_radix(&hex, 16).ok().map(from_short),
        32 => u128::from_str_radix(&hex, 16).ok().map(Uuid::from_u128),
        _ => None,
    }
}

/// The full UUID that a 16 or 32-bit UUID is short for.
pub fn from_short(short: u32) -> Uuid {
    Uuid::from_u128(BASE_UUID | (u128::from(short) << 96))
}

/// The 16 or 32-bit number a UUID is short for, when it is on the Bluetooth base.
pub fn to_short(uuid: &Uuid) -> Option<u32> {
    let value = uuid.as_u128();
    (value & ((1u128 << 96) - 1) == BASE_UUID).then_some((value >> 96) as u32)
}

/// A UUID as it is shown: "0x180F" for one on the Bluetooth base, else in full, in capitals.
pub fn uuid_text(uuid: &Uuid) -> String {
    match to_short(uuid) {
        Some(short) if short <= 0xFFFF => format!("0x{short:04X}"),
        Some(short) => format!("0x{short:08X}"),
        None => uuid.hyphenated().to_string().to_uppercase(),
    }
}

/// The name of a service, characteristic or descriptor, if the tables know it.
pub fn uuid_name(uuid: &Uuid, kind: Kind) -> Option<&'static str> {
    let tables = tables();
    let table = match kind {
        Kind::Service => &tables.services,
        Kind::Characteristic => &tables.characteristics,
        Kind::Descriptor => &tables.descriptors,
    };
    table.get(uuid).map(String::as_str)
}

/// The name of a UUID of any kind, a service's first: for UUIDs in adverts,
/// which do not say what they are.
pub fn any_uuid_name(uuid: &Uuid) -> Option<&'static str> {
    uuid_name(uuid, Kind::Service)
        .or_else(|| uuid_name(uuid, Kind::Characteristic))
        .or_else(|| uuid_name(uuid, Kind::Descriptor))
}

/// Every service, characteristic or descriptor the tables name, by name, so
/// one can be picked by its name: the UUID in full, and the name.
pub fn all_names(kind: Kind) -> Vec<(String, &'static str)> {
    let tables = tables();
    let table = match kind {
        Kind::Service => &tables.services,
        Kind::Characteristic => &tables.characteristics,
        Kind::Descriptor => &tables.descriptors,
    };
    let mut list: Vec<_> = table
        .iter()
        .map(|(uuid, name)| (uuid.hyphenated().to_string(), name.as_str()))
        .collect();
    list.sort_by(|a, b| a.1.cmp(b.1).then_with(|| a.0.cmp(&b.0)));
    list
}

/// The company with this Bluetooth SIG company ID, as in manufacturer data.
pub fn company_name(code: u16) -> Option<&'static str> {
    tables().companies.get(&code).map(String::as_str)
}

/// What an appearance value says the device is: "Watch: Sports Watch", or its category.
pub fn appearance_name(value: u16) -> Option<&'static str> {
    let table = &tables().appearances;
    table
        .get(&value)
        .or_else(|| table.get(&(value & !0x3F)))
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_uuids_are_on_the_bluetooth_base() {
        let battery = from_short(0x180F);
        assert_eq!(
            battery.hyphenated().to_string(),
            "0000180f-0000-1000-8000-00805f9b34fb"
        );
        assert_eq!(to_short(&battery), Some(0x180F));
        assert_eq!(uuid_text(&battery), "0x180F");
        let nus = parse_uuid("6E400001-B5A3-F393-E0A9-E50E24DCCA9E").unwrap();
        assert_eq!(to_short(&nus), None);
        assert_eq!(uuid_text(&nus), "6E400001-B5A3-F393-E0A9-E50E24DCCA9E");
    }

    #[test]
    fn uuids_are_read_as_people_write_them() {
        assert_eq!(parse_uuid("180F"), Some(from_short(0x180F)));
        assert_eq!(parse_uuid("0x2A19"), Some(from_short(0x2A19)));
        assert_eq!(
            parse_uuid("6e400001b5a3f393e0a9e50e24dcca9e"),
            parse_uuid("6E400001-B5A3-F393-E0A9-E50E24DCCA9E")
        );
        assert_eq!(parse_uuid("12345"), None);
        assert_eq!(parse_uuid("zz0F"), None);
    }

    #[test]
    fn the_tables_know_the_common_names() {
        assert_eq!(
            uuid_name(&from_short(0x180F), Kind::Service),
            Some("Battery Service")
        );
        assert_eq!(
            uuid_name(&from_short(0x2A19), Kind::Characteristic),
            Some("Battery Level")
        );
        assert_eq!(
            uuid_name(&from_short(0x2902), Kind::Descriptor),
            Some("Client Characteristic Configuration")
        );
        assert_eq!(company_name(0x0059), Some("Nordic Semiconductor ASA"));
        assert_eq!(company_name(0x004C), Some("Apple, Inc."));
        let nus = parse_uuid("6E400001-B5A3-F393-E0A9-E50E24DCCA9E").unwrap();
        assert!(uuid_name(&nus, Kind::Service).is_some());
    }

    #[test]
    fn every_name_can_be_listed_by_name() {
        let services = all_names(Kind::Service);
        assert!(services.len() > 100);
        assert!(services.windows(2).all(|pair| pair[0].1 <= pair[1].1));
        assert!(services.contains(&(
            "0000180f-0000-1000-8000-00805f9b34fb".to_string(),
            "Battery Service"
        )));
        assert!(all_names(Kind::Characteristic).len() > services.len());
    }

    #[test]
    fn an_appearance_is_named_by_its_subcategory_or_category() {
        // 0x00C1: category 3 (Watch), subcategory 1 (Sports Watch).
        assert_eq!(appearance_name(0x00C1), Some("Watch: Sports Watch"));
        // An unknown subcategory falls back to its category.
        assert_eq!(appearance_name(0x00FF), Some("Watch"));
        assert_eq!(appearance_name(0x0040), Some("Phone"));
    }
}
