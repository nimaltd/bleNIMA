//! The scanner's list of devices: every advert heard, merged into one row a
//! device, with its RSSI over time and how often it advertises.
//!
//! It knows nothing of radios: the radio hands it [`Advert`]s, and the app asks
//! it for the rows that changed, a device's details, or the list as CSV.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::advert::{self, AdStructure, Beacon, Field};
use crate::names;

/// How many RSSI readings a device keeps, for its plot.
pub const RSSI_HISTORY: usize = 3000;
/// How many gaps between adverts the interval is worked out from.
const GAPS: usize = 32;
/// Gaps shorter than this are the same advert heard twice, not its interval:
/// no advertiser sends more often than every 20 ms.
const SHORTEST_GAP_MS: f64 = 15.0;
/// Gaps longer than this are a device that went away and came back.
const LONGEST_GAP_MS: f64 = 12_000.0;

/// How the scanner listens: actively, asking each device for its scan
/// response, or passively, only hearing what is sent; and on the LE Coded PHY,
/// for long range, as well as on 1M.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanSetup {
    /// Only listens: sends no scan requests, so hears no scan responses.
    pub passive: bool,
    /// Hears the LE Coded PHY too, where the OS and the adapter can.
    pub coded: bool,
}

/// What kind of advertising PDU an advert came in, where the radio says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvKind {
    /// ADV_IND: anyone can connect or ask for a scan response.
    ConnectableUndirected,
    /// ADV_DIRECT_IND: for one central only.
    ConnectableDirected,
    /// ADV_SCAN_IND: a scan response can be asked for, but no connection.
    ScannableUndirected,
    /// ADV_NONCONN_IND: only broadcast.
    NonConnectableUndirected,
    /// SCAN_RSP: the answer to a scan request.
    ScanResponse,
    /// An extended advert, Bluetooth 5.
    Extended,
}

/// The kind of a device's address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressKind {
    Public,
    /// Random, of a kind not told: the radio did not give the address.
    Random,
    RandomStatic,
    /// A resolvable private address, which changes; its IRK finds the device again.
    ResolvablePrivate,
    NonResolvablePrivate,
}

impl AddressKind {
    /// The kind of a random address, from its top two bits.
    pub fn of_random(address: &str) -> Self {
        let top = address
            .get(..2)
            .and_then(|byte| u8::from_str_radix(byte, 16).ok());
        match top.map(|byte| byte >> 6) {
            Some(0b11) => Self::RandomStatic,
            Some(0b01) => Self::ResolvablePrivate,
            Some(0b00) => Self::NonResolvablePrivate,
            _ => Self::Random,
        }
    }
}

/// One advert, as a radio heard it.
#[derive(Clone, Debug, PartialEq)]
pub struct Advert {
    /// What the radio calls the device: its address on Windows and Linux, a
    /// UUID that macOS makes up for it on macOS.
    pub id: String,
    /// "C3:1A:4B:00:12:9F", where the radio tells it.
    pub address: Option<String>,
    pub address_kind: Option<AddressKind>,
    pub rssi: Option<i16>,
    /// When it was heard, in ms since the scan started.
    pub at_ms: f64,
    pub kind: Option<AdvKind>,
    pub connectable: Option<bool>,
    /// The answer to a scan request, rather than the advert itself.
    pub scan_response: bool,
    pub structures: Vec<AdStructure>,
    /// Whether `structures` are the bytes as they were sent. Where the OS only
    /// gives what it read from them, they are made again from that.
    pub raw: bool,
}

/// A device in the list, as the app shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DeviceRow {
    pub id: String,
    pub address: Option<String>,
    pub address_kind: Option<AddressKind>,
    pub name: Option<String>,
    pub rssi: Option<i16>,
    pub tx_power: Option<i8>,
    pub connectable: Option<bool>,
    pub kind: Option<AdvKind>,
    /// How often it advertises, as heard, in ms; the radio misses some adverts,
    /// so this is the shortest gap it sees often.
    pub interval_ms: Option<f64>,
    /// The interval the device says it uses, in its advert.
    pub advertised_interval_ms: Option<f64>,
    pub count: u64,
    pub first_ms: f64,
    pub last_ms: f64,
    /// "0x0059 Nordic Semiconductor ASA"
    pub companies: Vec<String>,
    /// "0x180F Battery Service"
    pub services: Vec<String>,
    /// The UUIDs alone, for filters.
    pub service_uuids: Vec<String>,
    pub appearance: Option<String>,
    pub beacon: Option<String>,
    pub raw: bool,
}

/// Everything about one device, for its details.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DeviceDetails {
    pub row: DeviceRow,
    pub advert: Vec<Field>,
    pub response: Vec<Field>,
    pub advert_hex: String,
    pub response_hex: String,
    pub beacon: Option<Beacon>,
    /// Its RSSI over time: [ms since the scan started, dBm].
    pub rssi: Vec<[f64; 2]>,
}

/// What changed since the last time it was asked.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Changes {
    pub rows: Vec<DeviceRow>,
    /// New RSSI readings of each device that has them: [ms, dBm].
    pub points: Vec<(String, Vec<[f64; 2]>)>,
    pub devices: usize,
    /// Adverts heard in all, since the scan started.
    pub adverts: u64,
    /// Why the scan could not start, if it could not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

#[derive(Default)]
struct Device {
    id: String,
    address: Option<String>,
    address_kind: Option<AddressKind>,
    advert: Vec<AdStructure>,
    response: Vec<AdStructure>,
    raw: bool,
    rssi: Option<i16>,
    history: VecDeque<[f64; 2]>,
    first_ms: f64,
    last_ms: f64,
    count: u64,
    connectable: Option<bool>,
    kind: Option<AdvKind>,
    last_advert_ms: Option<f64>,
    gaps: VecDeque<f64>,
    changed: bool,
    new_points: Vec<[f64; 2]>,
}

impl Device {
    fn heard(&mut self, advert: Advert) {
        if self.count == 0 {
            self.first_ms = advert.at_ms;
        }
        self.count += 1;
        self.last_ms = advert.at_ms;
        self.changed = true;
        if advert.address.is_some() {
            self.address = advert.address;
        }
        if advert.address_kind.is_some() {
            self.address_kind = advert.address_kind;
        }
        if let Some(rssi) = advert.rssi {
            self.rssi = Some(rssi);
            let point = [advert.at_ms, f64::from(rssi)];
            if self.history.len() == RSSI_HISTORY {
                self.history.pop_front();
            }
            self.history.push_back(point);
            self.new_points.push(point);
        }
        self.raw = advert.raw;
        if advert.scan_response {
            if !advert.structures.is_empty() {
                self.response = advert.structures;
            }
            return;
        }
        self.kind = advert.kind.or(self.kind);
        self.connectable = advert.connectable.or(self.connectable);
        if !advert.structures.is_empty() || advert.raw {
            self.advert = advert.structures;
        }
        if let Some(last) = self.last_advert_ms.replace(advert.at_ms) {
            let gap = advert.at_ms - last;
            if (SHORTEST_GAP_MS..=LONGEST_GAP_MS).contains(&gap) {
                if self.gaps.len() == GAPS {
                    self.gaps.pop_front();
                }
                self.gaps.push_back(gap);
            }
        }
    }

    /// The interval: the lower quartile of the gaps. A missed advert only
    /// makes a gap longer, so the short ones are nearest the truth.
    fn interval_ms(&self) -> Option<f64> {
        if self.gaps.len() < 3 {
            return None;
        }
        let mut gaps: Vec<f64> = self.gaps.iter().copied().collect();
        gaps.sort_by(f64::total_cmp);
        Some(gaps[gaps.len() / 4])
    }

    fn row(&self) -> DeviceRow {
        let summary = advert::summarize(self.advert.iter().chain(&self.response));
        let advertised_interval_ms = self
            .advert
            .iter()
            .chain(&self.response)
            .find(|structure| structure.ad_type == advert::ad::ADV_INTERVAL)
            .and_then(|structure| structure.data.get(..2))
            .map(|bytes| f64::from(u16::from_le_bytes([bytes[0], bytes[1]])) * 0.625);
        DeviceRow {
            id: self.id.clone(),
            address: self.address.clone(),
            address_kind: self.address_kind,
            name: summary.name,
            rssi: self.rssi,
            tx_power: summary.tx_power,
            connectable: self.connectable,
            kind: self.kind,
            interval_ms: self.interval_ms(),
            advertised_interval_ms,
            count: self.count,
            first_ms: self.first_ms,
            last_ms: self.last_ms,
            companies: summary
                .companies
                .iter()
                .map(|code| match names::company_name(*code) {
                    Some(name) => format!("0x{code:04X} {name}"),
                    None => format!("0x{code:04X}"),
                })
                .collect(),
            services: summary.services.iter().map(advert::named_uuid).collect(),
            service_uuids: summary.services.iter().map(names::uuid_text).collect(),
            appearance: summary.appearance.map(|value| {
                names::appearance_name(value)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("0x{value:04X}"))
            }),
            beacon: summary.beacon,
            raw: self.raw,
        }
    }
}

/// Every device heard since the scan started, or since it was cleared.
#[derive(Default)]
pub struct DeviceTable {
    devices: HashMap<String, Device>,
    /// The ids in the order the devices were first heard.
    order: Vec<String>,
    adverts: u64,
}

impl DeviceTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes in an advert.
    pub fn heard(&mut self, advert: Advert) {
        self.adverts += 1;
        let device = self.devices.entry(advert.id.clone()).or_insert_with(|| {
            self.order.push(advert.id.clone());
            Device {
                id: advert.id.clone(),
                ..Device::default()
            }
        });
        device.heard(advert);
    }

    pub fn len(&self) -> usize {
        self.devices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// Forgets every device.
    pub fn clear(&mut self) {
        self.devices.clear();
        self.order.clear();
        self.adverts = 0;
    }

    /// The rows that changed, and the new RSSI readings, since the last call.
    pub fn changes(&mut self) -> Changes {
        let mut changes = Changes {
            devices: self.devices.len(),
            adverts: self.adverts,
            ..Changes::default()
        };
        for id in &self.order {
            let Some(device) = self.devices.get_mut(id) else {
                continue;
            };
            if !device.changed {
                continue;
            }
            device.changed = false;
            changes.rows.push(device.row());
            if !device.new_points.is_empty() {
                changes
                    .points
                    .push((id.clone(), std::mem::take(&mut device.new_points)));
            }
        }
        changes
    }

    /// Every row, in the order the devices were first heard.
    pub fn rows(&self) -> Vec<DeviceRow> {
        self.order
            .iter()
            .filter_map(|id| self.devices.get(id))
            .map(Device::row)
            .collect()
    }

    /// Everything about one device.
    pub fn details(&self, id: &str) -> Option<DeviceDetails> {
        let device = self.devices.get(id)?;
        let all: Vec<AdStructure> = device
            .advert
            .iter()
            .chain(&device.response)
            .cloned()
            .collect();
        Some(DeviceDetails {
            row: device.row(),
            advert: device.advert.iter().map(advert::describe).collect(),
            response: device.response.iter().map(advert::describe).collect(),
            advert_hex: advert::hex(&advert::encode(&device.advert)),
            response_hex: advert::hex(&advert::encode(&device.response)),
            beacon: advert::beacon(&all),
            rssi: device.history.iter().copied().collect(),
        })
    }

    /// The list as CSV, one device a line, with a header line.
    pub fn csv(&self) -> String {
        let mut text = String::from(
            "address,address kind,name,rssi dBm,tx power dBm,connectable,interval ms,\
             advertised interval ms,adverts,first seen s,last seen s,companies,services,\
             appearance,beacon,advert,scan response,raw\n",
        );
        for id in &self.order {
            let Some(device) = self.devices.get(id) else {
                continue;
            };
            let row = device.row();
            let number = |value: Option<f64>| value.map(|v| format!("{v:.1}")).unwrap_or_default();
            let fields = [
                row.address.clone().unwrap_or_else(|| row.id.clone()),
                row.address_kind
                    .map(|kind| format!("{kind:?}"))
                    .unwrap_or_default(),
                row.name.clone().unwrap_or_default(),
                row.rssi.map(|v| v.to_string()).unwrap_or_default(),
                row.tx_power.map(|v| v.to_string()).unwrap_or_default(),
                row.connectable.map(|v| v.to_string()).unwrap_or_default(),
                number(row.interval_ms),
                number(row.advertised_interval_ms),
                row.count.to_string(),
                format!("{:.3}", row.first_ms / 1000.0),
                format!("{:.3}", row.last_ms / 1000.0),
                row.companies.join("; "),
                row.services.join("; "),
                row.appearance.clone().unwrap_or_default(),
                row.beacon.clone().unwrap_or_default(),
                advert::hex(&advert::encode(&device.advert)),
                advert::hex(&advert::encode(&device.response)),
                row.raw.to_string(),
            ];
            let line: Vec<String> = fields.iter().map(|field| csv_field(field)).collect();
            text.push_str(&line.join(","));
            text.push('\n');
        }
        text
    }
}

/// A CSV field, quoted when it has a comma, a quote or a line break in it.
pub fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::advert::ad;

    fn advert(id: &str, at_ms: f64, rssi: i16) -> Advert {
        Advert {
            id: id.into(),
            address: Some(id.into()),
            address_kind: Some(AddressKind::Public),
            rssi: Some(rssi),
            at_ms,
            kind: Some(AdvKind::ConnectableUndirected),
            connectable: Some(true),
            scan_response: false,
            structures: vec![
                AdStructure::new(ad::FLAGS, [0x06]),
                AdStructure::new(ad::NAME, *b"nrf52"),
            ],
            raw: true,
        }
    }

    #[test]
    fn adverts_of_one_device_make_one_row() {
        let mut table = DeviceTable::new();
        table.heard(advert("AA:BB:CC:DD:EE:01", 0.0, -60));
        table.heard(advert("AA:BB:CC:DD:EE:01", 100.0, -62));
        table.heard(advert("AA:BB:CC:DD:EE:02", 50.0, -80));
        assert_eq!(table.len(), 2);
        let changes = table.changes();
        assert_eq!(changes.rows.len(), 2);
        assert_eq!(changes.adverts, 3);
        assert_eq!(changes.rows[0].count, 2);
        assert_eq!(changes.rows[0].rssi, Some(-62));
        assert_eq!(changes.rows[0].name.as_deref(), Some("nrf52"));
        assert_eq!(changes.points[0].1, vec![[0.0, -60.0], [100.0, -62.0]]);
        // Nothing new: nothing changed.
        assert!(table.changes().rows.is_empty());
        table.heard(advert("AA:BB:CC:DD:EE:02", 150.0, -79));
        let changes = table.changes();
        assert_eq!(changes.rows.len(), 1);
        assert_eq!(changes.rows[0].id, "AA:BB:CC:DD:EE:02");
    }

    #[test]
    fn the_interval_is_the_short_gaps_not_the_missed_adverts() {
        let mut table = DeviceTable::new();
        let mut at = 0.0;
        // Every 100 ms, with every third advert missed, and one heard twice.
        for step in 0..30 {
            if step % 3 != 2 {
                table.heard(advert("A", at, -50));
            }
            if step == 10 {
                table.heard(advert("A", at + 2.0, -50));
            }
            at += 100.0;
        }
        let row = &table.rows()[0];
        let interval = row.interval_ms.unwrap();
        assert!((interval - 100.0).abs() <= 2.0, "{interval}");
    }

    #[test]
    fn a_scan_response_is_kept_apart_and_named_in_the_summary() {
        let mut table = DeviceTable::new();
        table.heard(advert("A", 0.0, -50));
        table.heard(Advert {
            scan_response: true,
            kind: Some(AdvKind::ScanResponse),
            structures: vec![
                AdStructure::new(ad::TX_POWER, [0x00]),
                AdStructure::new(ad::ADV_INTERVAL, [0xA0, 0x00]),
            ],
            ..advert("A", 1.0, -51)
        });
        let details = table.details("A").unwrap();
        assert_eq!(details.advert.len(), 2);
        assert_eq!(details.response.len(), 2);
        assert_eq!(details.response_hex, "02 0A 00 03 1A A0 00");
        assert_eq!(details.row.tx_power, Some(0));
        assert_eq!(details.row.advertised_interval_ms, Some(100.0));
        // The scan response does not change the advert's kind, nor count as a gap.
        assert_eq!(details.row.kind, Some(AdvKind::ConnectableUndirected));
        assert_eq!(details.rssi.len(), 2);
    }

    #[test]
    fn random_addresses_are_told_apart_by_their_top_bits() {
        assert_eq!(
            AddressKind::of_random("C3:1A:4B:00:12:9F"),
            AddressKind::RandomStatic
        );
        assert_eq!(
            AddressKind::of_random("5E:00:00:00:00:01"),
            AddressKind::ResolvablePrivate
        );
        assert_eq!(
            AddressKind::of_random("1E:00:00:00:00:01"),
            AddressKind::NonResolvablePrivate
        );
        assert_eq!(AddressKind::of_random(""), AddressKind::Random);
    }

    #[test]
    fn the_list_comes_out_as_csv() {
        let mut table = DeviceTable::new();
        table.heard(Advert {
            structures: vec![AdStructure::new(ad::NAME, *b"a, \"b\"")],
            ..advert("A", 0.0, -50)
        });
        let csv = table.csv();
        let mut lines = csv.lines();
        assert!(
            lines
                .next()
                .unwrap()
                .starts_with("address,address kind,name")
        );
        let line = lines.next().unwrap();
        assert!(line.starts_with("A,Public,\"a, \"\"b\"\"\",-50,"), "{line}");
        assert!(line.ends_with(",true"));
    }

    #[test]
    fn clearing_forgets_every_device() {
        let mut table = DeviceTable::new();
        table.heard(advert("A", 0.0, -50));
        table.clear();
        assert!(table.is_empty());
        assert_eq!(table.changes().adverts, 0);
    }
}
