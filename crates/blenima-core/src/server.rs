//! bleNIMA as a GATT server: the services a Peripheral simulator tab offers,
//! the values they hold, and what the clients do with them.
//!
//! The OS does the radio work; this file has what does not need a radio, so
//! it is tested on its own: checking services before the OS gets them,
//! reading and writing values at an offset, and the clients' addresses.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::names::{self, Kind};

/// The most bytes an attribute value can have: Core spec, Vol 3, Part F, 3.2.9.
pub const VALUE_MAX: usize = 512;

/// Characteristic properties, the bits of the declaration.
pub mod property {
    pub const READ: u8 = 0x02;
    pub const WRITE_WITHOUT_RESPONSE: u8 = 0x04;
    pub const WRITE: u8 = 0x08;
    pub const NOTIFY: u8 = 0x10;
    pub const INDICATE: u8 = 0x20;
    /// The properties a served characteristic can have.
    pub const ALL: u8 = READ | WRITE_WITHOUT_RESPONSE | WRITE | NOTIFY | INDICATE;
}

/// ATT error codes the server answers with: Core spec, Vol 3, Part F, 3.4.1.1.
pub mod att {
    pub const INVALID_OFFSET: u8 = 0x07;
    pub const INVALID_LENGTH: u8 = 0x0D;
}

/// One characteristic of a served service.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Characteristic {
    pub uuid: Uuid,
    /// The property bits: read, write, write without response, notify, indicate.
    pub properties: u8,
    /// The value it starts with.
    #[serde(default)]
    pub value: Vec<u8>,
    /// Its user description, the 0x2901 descriptor; none when empty.
    #[serde(default)]
    pub description: String,
    /// What a client writes to it is notified back on this characteristic of
    /// the same service, counted from 0: the echo of a UART service.
    #[serde(default)]
    pub echo: Option<usize>,
}

/// One served service: a primary service with its characteristics.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Service {
    pub uuid: Uuid,
    pub characteristics: Vec<Characteristic>,
}

/// Where a characteristic is: its service and its place in it, both counted from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Place {
    pub service: usize,
    pub characteristic: usize,
}

/// What happened on the server, for its tab.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    /// Where a service's advert is, in words.
    Advertising {
        service: usize,
        on: bool,
        text: String,
    },
    /// A client read a value.
    Read {
        place: Place,
        client: String,
        offset: u32,
        data: Vec<u8>,
    },
    /// A client wrote a value: `data` is what it wrote, `value` the value now.
    Written {
        place: Place,
        client: String,
        with_response: bool,
        offset: u32,
        data: Vec<u8>,
        value: Vec<u8>,
    },
    /// The clients subscribed to a characteristic now.
    Subscribers { place: Place, clients: Vec<String> },
    /// A value was notified or indicated: by hand, on a timer, or as an echo.
    Notified {
        place: Place,
        data: Vec<u8>,
        /// The clients it reached.
        sent: Vec<String>,
        /// The clients it did not reach, each with why.
        failed: Vec<String>,
        echo: bool,
    },
    /// Something went wrong.
    Failed {
        place: Option<Place>,
        action: String,
        problem: String,
    },
}

/// Services the OS keeps for itself, as every GATT server has them.
const OWN_SERVICES: [u32; 2] = [0x1800, 0x1801];

/// A service's or characteristic's name for messages: its standard name, or its UUID.
pub fn name(uuid: &Uuid, kind: Kind) -> String {
    names::uuid_name(uuid, kind)
        .map(str::to_string)
        .unwrap_or_else(|| names::uuid_text(uuid))
}

/// Checks services before the OS gets them, and says in words what is wrong.
pub fn check(services: &[Service]) -> Result<(), String> {
    if services.is_empty() {
        return Err("Add a service first: a preset or one by hand.".into());
    }
    for (at, service) in services.iter().enumerate() {
        let service_name = name(&service.uuid, Kind::Service);
        if let Some(short) = names::to_short(&service.uuid)
            && OWN_SERVICES.contains(&short)
        {
            return Err(format!(
                "{service_name} is the OS's own service: every GATT server has it already. Take it out."
            ));
        }
        if services[..at]
            .iter()
            .any(|other| other.uuid == service.uuid)
        {
            return Err(format!(
                "{service_name} is there twice: each service can be served once."
            ));
        }
        if service.characteristics.is_empty() {
            return Err(format!("{service_name} has no characteristics: add one."));
        }
        for characteristic in &service.characteristics {
            let what = format!(
                "{} in {service_name}",
                name(&characteristic.uuid, Kind::Characteristic)
            );
            if characteristic.properties & property::ALL == 0 {
                return Err(format!(
                    "{what} needs read, write, write without response, notify or indicate."
                ));
            }
            if characteristic.value.len() > VALUE_MAX {
                return Err(format!(
                    "{what}: a value is {VALUE_MAX} bytes at most; it has {}.",
                    characteristic.value.len()
                ));
            }
            if let Some(echo) = characteristic.echo {
                let target = service.characteristics.get(echo);
                if !target.is_some_and(|target| {
                    target.properties & (property::NOTIFY | property::INDICATE) != 0
                }) {
                    return Err(format!(
                        "{what} echoes to a characteristic that cannot notify."
                    ));
                }
            }
        }
    }
    Ok(())
}

/// What a client reads from a value at an offset, as in a long read; or the
/// ATT error to answer with when the offset is past the end.
pub fn read_at(value: &[u8], offset: u32) -> Result<Vec<u8>, u8> {
    value
        .get(offset as usize..)
        .map(<[u8]>::to_vec)
        .ok_or(att::INVALID_OFFSET)
}

/// The value after a client writes `data` at `offset`, as in a long write:
/// the old value up to the offset, then what was written. Or the ATT error to
/// answer with.
pub fn write_at(old: &[u8], offset: u32, data: &[u8]) -> Result<Vec<u8>, u8> {
    let offset = offset as usize;
    if offset > old.len() {
        return Err(att::INVALID_OFFSET);
    }
    if offset + data.len() > VALUE_MAX {
        return Err(att::INVALID_LENGTH);
    }
    let mut value = old[..offset].to_vec();
    value.extend_from_slice(data);
    Ok(value)
}

/// A client's address from the OS's id for it, such as Windows'
/// "BluetoothLE#BluetoothLEb0:a4:60:12:34:56-c3:1a:4b:00:12:9f": the part
/// after the last dash, in capitals. The id as it is when it has no address.
pub fn client_address(id: &str) -> String {
    let last = id.rsplit('-').next().unwrap_or(id);
    let is_address = last.len() == 17
        && last.split(':').count() == 6
        && last
            .split(':')
            .all(|byte| byte.len() == 2 && byte.chars().all(|c| c.is_ascii_hexdigit()));
    if is_address {
        last.to_uppercase()
    } else {
        id.to_string()
    }
}

/// The values of every characteristic, by service and place: what reads get,
/// writes change and notifications send.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Values(Vec<Vec<Vec<u8>>>);

impl Values {
    /// The values the services start with.
    pub fn new(services: &[Service]) -> Self {
        Self(
            services
                .iter()
                .map(|service| {
                    service
                        .characteristics
                        .iter()
                        .map(|characteristic| characteristic.value.clone())
                        .collect()
                })
                .collect(),
        )
    }

    pub fn get(&self, place: Place) -> Option<&[u8]> {
        self.0
            .get(place.service)?
            .get(place.characteristic)
            .map(Vec::as_slice)
    }

    /// Sets a value; false when there is no characteristic at `place`.
    pub fn set(&mut self, place: Place, value: Vec<u8>) -> bool {
        match self
            .0
            .get_mut(place.service)
            .and_then(|service| service.get_mut(place.characteristic))
        {
            Some(slot) => {
                *slot = value;
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn characteristic(short: u32, properties: u8) -> Characteristic {
        Characteristic {
            uuid: names::from_short(short),
            properties,
            value: vec![],
            description: String::new(),
            echo: None,
        }
    }

    fn service(short: u32, characteristics: Vec<Characteristic>) -> Service {
        Service {
            uuid: names::from_short(short),
            characteristics,
        }
    }

    fn battery() -> Service {
        service(
            0x180F,
            vec![characteristic(0x2A19, property::READ | property::NOTIFY)],
        )
    }

    #[test]
    fn good_services_pass() {
        assert_eq!(check(&[battery()]), Ok(()));
    }

    #[test]
    fn what_is_wrong_is_said_in_words() {
        assert!(check(&[]).unwrap_err().starts_with("Add a service"));
        let own = check(&[service(0x1800, vec![characteristic(0x2A00, 2)])]).unwrap_err();
        assert!(own.contains("the OS's own service"), "{own}");
        let twice = check(&[battery(), battery()]).unwrap_err();
        assert!(twice.contains("there twice"), "{twice}");
        let empty = check(&[service(0x180F, vec![])]).unwrap_err();
        assert!(empty.contains("no characteristics"), "{empty}");
        let none = check(&[service(0x180F, vec![characteristic(0x2A19, 0)])]).unwrap_err();
        assert!(
            none.starts_with("Battery Level in Battery Service"),
            "{none}"
        );
        let mut long = battery();
        long.characteristics[0].value = vec![0; VALUE_MAX + 1];
        assert!(check(&[long]).unwrap_err().contains("512 bytes at most"));
    }

    #[test]
    fn an_echo_goes_to_a_characteristic_that_notifies() {
        let mut rx = characteristic(0x2A3D, property::WRITE);
        rx.echo = Some(1);
        let tx = characteristic(0x2A3E, property::NOTIFY);
        assert_eq!(check(&[service(0xFFE0, vec![rx.clone(), tx])]), Ok(()));
        let read_only = characteristic(0x2A3E, property::READ);
        assert!(
            check(&[service(0xFFE0, vec![rx.clone(), read_only])])
                .unwrap_err()
                .contains("cannot notify")
        );
        rx.echo = Some(5);
        assert!(check(&[service(0xFFE0, vec![rx])]).is_err());
    }

    #[test]
    fn a_long_read_starts_at_its_offset() {
        assert_eq!(read_at(b"hello", 0), Ok(b"hello".to_vec()));
        assert_eq!(read_at(b"hello", 3), Ok(b"lo".to_vec()));
        assert_eq!(read_at(b"hello", 5), Ok(vec![]));
        assert_eq!(read_at(b"hello", 6), Err(att::INVALID_OFFSET));
    }

    #[test]
    fn a_write_at_an_offset_keeps_what_was_before_it() {
        assert_eq!(write_at(b"hello", 0, b"hi"), Ok(b"hi".to_vec()));
        assert_eq!(write_at(b"hello", 3, b"p!"), Ok(b"help!".to_vec()));
        assert_eq!(write_at(b"hi", 3, b"x"), Err(att::INVALID_OFFSET));
        assert_eq!(write_at(&[], 0, &[0; 513]), Err(att::INVALID_LENGTH));
    }

    #[test]
    fn a_client_is_known_by_its_address() {
        assert_eq!(
            client_address("BluetoothLE#BluetoothLEb0:a4:60:12:34:56-c3:1a:4b:00:12:9f"),
            "C3:1A:4B:00:12:9F"
        );
        assert_eq!(client_address("something else"), "something else");
    }

    #[test]
    fn values_start_as_given_and_change() {
        let mut first = battery();
        first.characteristics[0].value = vec![87];
        let mut values = Values::new(&[first]);
        let place = Place {
            service: 0,
            characteristic: 0,
        };
        assert_eq!(values.get(place), Some(&[87][..]));
        assert!(values.set(place, vec![86]));
        assert_eq!(values.get(place), Some(&[86][..]));
        let nowhere = Place {
            service: 3,
            characteristic: 0,
        };
        assert!(!values.set(nowhere, vec![1]));
        assert_eq!(values.get(nowhere), None);
    }

    #[test]
    fn events_are_tagged_for_the_ui() {
        let event = ServerEvent::Subscribers {
            place: Place {
                service: 1,
                characteristic: 2,
            },
            clients: vec!["C3:1A:4B:00:12:9F".into()],
        };
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(
            json,
            r#"{"type":"subscribers","place":{"service":1,"characteristic":2},"clients":["C3:1A:4B:00:12:9F"]}"#
        );
    }
}
