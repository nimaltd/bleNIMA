//! A device tab's connection: the device found, connected, its GATT table read,
//! and kept connected, coming back by itself when the device does.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use btleplug::api::{
    Central, CentralEvent, Characteristic, ConnectionParameterPreset, Descriptor, Peripheral as _,
    WriteType,
};
use btleplug::platform::Peripheral;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::gatt::{self, PresentationFormat};
use crate::names::{self, Kind};
use crate::radio::Radio;

/// How long one search for the device lasts before it is told again that it is looked for.
const SEARCH: Duration = Duration::from_secs(5);
const CONNECT: Duration = Duration::from_secs(30);
const DISCOVER: Duration = Duration::from_secs(30);
/// How long to wait after a failed try before the next one.
const RETRY: Duration = Duration::from_secs(2);

/// What the tab asks of the connection.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Read {
        key: String,
    },
    Write {
        key: String,
        data: Vec<u8>,
        with_response: bool,
    },
    Subscribe {
        key: String,
        on: bool,
    },
    ReadDescriptor {
        key: String,
    },
    /// Asks for connection parameters: "balanced", "throughput" or "power".
    Parameters {
        preset: String,
    },
    Close,
}

/// Where the connection is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    Searching,
    Connecting,
    Discovering,
    Connected,
    /// Lost, or a try failed: it tries again by itself.
    Waiting,
    Closed,
}

/// Where a value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Read,
    Notify,
    Write,
    WriteWithoutResponse,
    Descriptor,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DescriptorInfo {
    pub key: String,
    pub uuid: String,
    pub name: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CharacteristicInfo {
    pub key: String,
    pub uuid: String,
    pub name: Option<&'static str>,
    pub properties: u8,
    pub property_names: Vec<&'static str>,
    pub descriptors: Vec<DescriptorInfo>,
    /// From its 0x2901 descriptor.
    pub user_description: Option<String>,
    /// From its 0x2904 descriptor.
    pub format: Option<PresentationFormat>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ServiceInfo {
    pub key: String,
    pub uuid: String,
    pub name: Option<&'static str>,
    pub primary: bool,
    pub characteristics: Vec<CharacteristicInfo>,
}

/// What happens on the connection, for the tab.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeviceEvent {
    State {
        state: LinkState,
        text: String,
    },
    Services {
        services: Vec<ServiceInfo>,
    },
    Value {
        key: String,
        data: Vec<u8>,
        source: Source,
        at_ms: f64,
    },
    Subscribed {
        key: String,
        on: bool,
    },
    Failed {
        key: Option<String>,
        action: String,
        problem: String,
        at_ms: f64,
    },
    Link {
        mtu: u16,
        interval_ms: Option<f64>,
        latency: Option<u16>,
        timeout_ms: Option<f64>,
    },
    /// A line for the log, such as the time it was lost and found again.
    Mark {
        text: String,
        at_ms: f64,
    },
}

/// The key a characteristic is known by: its service's UUID and its own.
pub fn characteristic_key(service: &Uuid, characteristic: &Uuid) -> String {
    format!("{service}/{characteristic}")
}

/// The key of a descriptor: its characteristic's key and its own UUID.
pub fn descriptor_key(service: &Uuid, characteristic: &Uuid, descriptor: &Uuid) -> String {
    format!("{service}/{characteristic}/{descriptor}")
}

/// A device tab's connection, running on its own task.
pub struct DeviceSession {
    commands: UnboundedSender<Command>,
    task: JoinHandle<()>,
}

impl DeviceSession {
    /// Starts looking for the device with this id, and connects when it is heard.
    pub fn open(radio: Arc<Radio>, id: String, events: UnboundedSender<DeviceEvent>) -> Self {
        let (commands, receiver) = unbounded_channel();
        let task = tokio::spawn(run(radio, id, events, receiver));
        Self { commands, task }
    }

    /// Hands a command to the connection.
    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Disconnects and ends the task.
    pub async fn close(self) {
        let _ = self.commands.send(Command::Close);
        let _ = tokio::time::timeout(Duration::from_secs(5), self.task).await;
    }
}

struct Session {
    id: String,
    events: UnboundedSender<DeviceEvent>,
    started: Instant,
}

impl Session {
    fn at_ms(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1000.0
    }

    fn emit(&self, event: DeviceEvent) {
        let _ = self.events.send(event);
    }

    fn state(&self, state: LinkState, text: impl Into<String>) {
        self.emit(DeviceEvent::State {
            state,
            text: text.into(),
        });
    }

    fn failed(&self, key: Option<&str>, action: &str, problem: impl std::fmt::Display) {
        self.emit(DeviceEvent::Failed {
            key: key.map(str::to_string),
            action: action.into(),
            problem: gatt::explain(&problem.to_string()),
            at_ms: self.at_ms(),
        });
    }
}

/// Whether the tab asked to close, while it waits for something else.
fn close_asked(commands: &mut UnboundedReceiver<Command>) -> bool {
    loop {
        match commands.try_recv() {
            Ok(Command::Close) | Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                return true;
            }
            // Nothing can be done while not connected.
            Ok(_) => continue,
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => return false,
        }
    }
}

/// Waits `time`, or less if the tab asks to close; says whether it did.
async fn pause(commands: &mut UnboundedReceiver<Command>, time: Duration) -> bool {
    let deadline = tokio::time::sleep(time);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => return false,
            command = commands.recv() => match command {
                None | Some(Command::Close) => return true,
                Some(_) => continue,
            },
        }
    }
}

async fn run(
    radio: Arc<Radio>,
    id: String,
    events: UnboundedSender<DeviceEvent>,
    mut commands: UnboundedReceiver<Command>,
) {
    let session = Session {
        id,
        events,
        started: Instant::now(),
    };
    let mut subscribed: BTreeSet<String> = BTreeSet::new();
    // When the connection was lost, to say how long it was gone once it is back.
    let mut lost: Option<(Instant, String)> = None;
    'connection: loop {
        session.state(
            LinkState::Searching,
            format!(
                "Looking for {}: it connects as soon as it is heard",
                session.id
            ),
        );
        let peripheral = loop {
            match radio.find(&session.id, SEARCH).await {
                Ok(Some(peripheral)) => break peripheral,
                Ok(None) => {}
                Err(problem) => {
                    session.failed(None, "look for the device", problem);
                    if pause(&mut commands, RETRY).await {
                        break 'connection;
                    }
                }
            }
            if close_asked(&mut commands) {
                break 'connection;
            }
        };

        session.state(
            LinkState::Connecting,
            format!("Connecting to {}", session.id),
        );
        let connected = tokio::time::timeout(CONNECT, peripheral.connect()).await;
        if !matches!(connected, Ok(Ok(()))) {
            match connected {
                Ok(Err(error)) => session.failed(None, "connect", error),
                _ => session.failed(None, "connect", "timed out"),
            }
            session.state(LinkState::Waiting, "Could not connect: trying again");
            if pause(&mut commands, RETRY).await {
                break;
            }
            continue;
        }

        session.state(LinkState::Discovering, "Reading the GATT table");
        let discovered = tokio::time::timeout(DISCOVER, peripheral.discover_services()).await;
        if !matches!(discovered, Ok(Ok(()))) {
            match discovered {
                Ok(Err(error)) => session.failed(None, "read the GATT table", error),
                _ => session.failed(None, "read the GATT table", "timed out"),
            }
            let _ = peripheral.disconnect().await;
            session.state(
                LinkState::Waiting,
                "Could not read the GATT table: trying again",
            );
            if pause(&mut commands, RETRY).await {
                break;
            }
            continue;
        }

        let services = services(&peripheral).await;
        session.emit(DeviceEvent::Services { services });
        send_link(&session, &peripheral).await;
        if let Some((when, clock)) = lost.take() {
            session.emit(DeviceEvent::Mark {
                text: format!(
                    "---- disconnected {clock}, reconnected {} ({:.1} s) ----",
                    clock_now(),
                    when.elapsed().as_secs_f64()
                ),
                at_ms: session.at_ms(),
            });
        }
        session.state(LinkState::Connected, format!("Connected to {}", session.id));

        let mut notifications = match peripheral.notifications().await {
            Ok(stream) => Some(stream),
            Err(error) => {
                session.failed(None, "listen for notifications", error);
                None
            }
        };
        // Subscribed again to what was subscribed before it was lost.
        for key in subscribed.clone() {
            if let Some(characteristic) = find_characteristic(&peripheral, &key) {
                match peripheral.subscribe(&characteristic).await {
                    Ok(()) => session.emit(DeviceEvent::Subscribed { key, on: true }),
                    Err(error) => {
                        subscribed.remove(&key);
                        session.failed(Some(&key), "subscribe again", error);
                    }
                }
            }
        }
        let mut central = radio.adapter().events().await.ok();
        let mut check = tokio::time::interval(Duration::from_secs(2));
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    None | Some(Command::Close) => {
                        let _ = peripheral.disconnect().await;
                        break 'connection;
                    }
                    Some(command) => handle(&session, &peripheral, &mut subscribed, command).await,
                },
                Some(notification) = async {
                    match notifications.as_mut() {
                        Some(stream) => stream.next().await,
                        None => std::future::pending().await,
                    }
                } => {
                    session.emit(DeviceEvent::Value {
                        key: characteristic_key(&notification.service_uuid, &notification.uuid),
                        data: notification.value,
                        source: Source::Notify,
                        at_ms: session.at_ms(),
                    });
                }
                Some(event) = async {
                    match central.as_mut() {
                        Some(stream) => stream.next().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let CentralEvent::DeviceDisconnected(peripheral_id) = event
                        && peripheral_id.to_string() == session.id
                    {
                        break;
                    }
                }
                _ = check.tick() => {
                    if !peripheral.is_connected().await.unwrap_or(false) {
                        break;
                    }
                }
            }
        }
        lost = Some((Instant::now(), clock_now()));
        session.state(
            LinkState::Waiting,
            format!("Lost at {}: connecting again when it is heard", clock_now()),
        );
    }
    session.state(LinkState::Closed, "Not connected");
}

/// The time of day: "14:02:11.204".
pub fn clock_now() -> String {
    chrono::Local::now().format("%H:%M:%S%.3f").to_string()
}

async fn send_link(session: &Session, peripheral: &Peripheral) {
    let parameters = peripheral.connection_parameters().await.ok().flatten();
    session.emit(DeviceEvent::Link {
        mtu: peripheral.mtu(),
        interval_ms: parameters.map(|p| f64::from(p.interval_us) / 1000.0),
        latency: parameters.map(|p| p.latency),
        timeout_ms: parameters.map(|p| f64::from(p.supervision_timeout_us) / 1000.0),
    });
}

fn find_characteristic(peripheral: &Peripheral, key: &str) -> Option<Characteristic> {
    let mut parts = key.split('/');
    let service = parts.next()?.parse::<Uuid>().ok()?;
    let uuid = parts.next()?.parse::<Uuid>().ok()?;
    peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.service_uuid == service && c.uuid == uuid)
}

fn find_descriptor(peripheral: &Peripheral, key: &str) -> Option<Descriptor> {
    let (characteristic, descriptor) = key.rsplit_once('/')?;
    let uuid = descriptor.parse::<Uuid>().ok()?;
    find_characteristic(peripheral, characteristic)?
        .descriptors
        .into_iter()
        .find(|d| d.uuid == uuid)
}

async fn handle(
    session: &Session,
    peripheral: &Peripheral,
    subscribed: &mut BTreeSet<String>,
    command: Command,
) {
    match command {
        Command::Read { key } => {
            let Some(characteristic) = find_characteristic(peripheral, &key) else {
                return session.failed(Some(&key), "read", "no such characteristic");
            };
            match peripheral.read(&characteristic).await {
                Ok(data) => session.emit(DeviceEvent::Value {
                    key,
                    data,
                    source: Source::Read,
                    at_ms: session.at_ms(),
                }),
                Err(error) => session.failed(Some(&key), "read", error),
            }
        }
        Command::Write {
            key,
            data,
            with_response,
        } => {
            let Some(characteristic) = find_characteristic(peripheral, &key) else {
                return session.failed(Some(&key), "write", "no such characteristic");
            };
            let kind = if with_response {
                WriteType::WithResponse
            } else {
                WriteType::WithoutResponse
            };
            match peripheral.write(&characteristic, &data, kind).await {
                Ok(()) => session.emit(DeviceEvent::Value {
                    key,
                    data,
                    source: if with_response {
                        Source::Write
                    } else {
                        Source::WriteWithoutResponse
                    },
                    at_ms: session.at_ms(),
                }),
                Err(error) => session.failed(Some(&key), "write", error),
            }
        }
        Command::Subscribe { key, on } => {
            let Some(characteristic) = find_characteristic(peripheral, &key) else {
                return session.failed(Some(&key), "subscribe", "no such characteristic");
            };
            let result = if on {
                peripheral.subscribe(&characteristic).await
            } else {
                peripheral.unsubscribe(&characteristic).await
            };
            match result {
                Ok(()) => {
                    if on {
                        subscribed.insert(key.clone());
                    } else {
                        subscribed.remove(&key);
                    }
                    session.emit(DeviceEvent::Subscribed { key, on });
                }
                Err(error) => session.failed(
                    Some(&key),
                    if on { "subscribe" } else { "unsubscribe" },
                    error,
                ),
            }
        }
        Command::ReadDescriptor { key } => {
            let Some(descriptor) = find_descriptor(peripheral, &key) else {
                return session.failed(Some(&key), "read", "no such descriptor");
            };
            match peripheral.read_descriptor(&descriptor).await {
                Ok(data) => session.emit(DeviceEvent::Value {
                    key,
                    data,
                    source: Source::Descriptor,
                    at_ms: session.at_ms(),
                }),
                Err(error) => session.failed(Some(&key), "read", error),
            }
        }
        Command::Parameters { preset } => {
            let preset = match preset.as_str() {
                "throughput" => ConnectionParameterPreset::ThroughputOptimized,
                "power" => ConnectionParameterPreset::PowerOptimized,
                _ => ConnectionParameterPreset::Balanced,
            };
            match peripheral.request_connection_parameters(preset).await {
                Ok(()) => {
                    // The device takes a moment to agree to them.
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    send_link(session, peripheral).await;
                }
                Err(error) => session.failed(None, "ask for connection parameters", error),
            }
        }
        Command::Close => {}
    }
}

/// The GATT table, with the names the tables know, and each characteristic's
/// user description (0x2901) and presentation format (0x2904) read.
async fn services(peripheral: &Peripheral) -> Vec<ServiceInfo> {
    let user_description = names::from_short(0x2901);
    let presentation = names::from_short(0x2904);
    let mut list = Vec::new();
    for service in peripheral.services() {
        let mut characteristics = Vec::new();
        for characteristic in &service.characteristics {
            let mut info = CharacteristicInfo {
                key: characteristic_key(&service.uuid, &characteristic.uuid),
                uuid: names::uuid_text(&characteristic.uuid),
                name: names::uuid_name(&characteristic.uuid, Kind::Characteristic),
                properties: characteristic.properties.bits(),
                property_names: gatt::property_names(characteristic.properties.bits()),
                descriptors: Vec::new(),
                user_description: None,
                format: None,
            };
            for descriptor in &characteristic.descriptors {
                info.descriptors.push(DescriptorInfo {
                    key: descriptor_key(&service.uuid, &characteristic.uuid, &descriptor.uuid),
                    uuid: names::uuid_text(&descriptor.uuid),
                    name: names::uuid_name(&descriptor.uuid, Kind::Descriptor),
                });
                if descriptor.uuid == user_description
                    && let Ok(Ok(bytes)) = tokio::time::timeout(
                        Duration::from_secs(3),
                        peripheral.read_descriptor(descriptor),
                    )
                    .await
                {
                    info.user_description = Some(String::from_utf8_lossy(&bytes).into_owned());
                } else if descriptor.uuid == presentation
                    && let Ok(Ok(bytes)) = tokio::time::timeout(
                        Duration::from_secs(3),
                        peripheral.read_descriptor(descriptor),
                    )
                    .await
                {
                    info.format = gatt::presentation_format(&bytes);
                }
            }
            characteristics.push(info);
        }
        list.push(ServiceInfo {
            key: service.uuid.to_string(),
            uuid: names::uuid_text(&service.uuid),
            name: names::uuid_name(&service.uuid, Kind::Service),
            primary: service.primary,
            characteristics,
        });
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_hold_the_uuids_they_are_made_of() {
        let service = names::from_short(0x180F);
        let level = names::from_short(0x2A19);
        let cccd = names::from_short(0x2902);
        let key = characteristic_key(&service, &level);
        assert_eq!(
            key,
            "0000180f-0000-1000-8000-00805f9b34fb/00002a19-0000-1000-8000-00805f9b34fb"
        );
        let descriptor = descriptor_key(&service, &level, &cccd);
        assert_eq!(descriptor.rsplit_once('/').unwrap().0, key);
    }

    #[test]
    fn commands_come_from_the_ui_as_json() {
        let command: Command = serde_json::from_str(
            r#"{"type":"write","key":"a/b","data":[1,2],"with_response":false}"#,
        )
        .unwrap();
        assert!(matches!(
            command,
            Command::Write {
                with_response: false,
                ..
            }
        ));
        let command: Command =
            serde_json::from_str(r#"{"type":"parameters","preset":"throughput"}"#).unwrap();
        assert!(matches!(command, Command::Parameters { .. }));
    }

    #[test]
    fn events_go_to_the_ui_tagged_with_their_type() {
        let event = DeviceEvent::Subscribed {
            key: "a/b".into(),
            on: true,
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"type":"subscribed","key":"a/b","on":true}"#
        );
    }
}
