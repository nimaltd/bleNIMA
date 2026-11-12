//! The Mesh tab's session. It hears unprovisioned devices and the network's
//! proxies, provisions a device over PB-GATT, and talks to the network through
//! a proxy over GATT, with the stack in between. The network is kept in
//! `mesh.toml`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use btleplug::api::bleuuid::uuid_from_u16;
use btleplug::api::{Characteristic, Peripheral as _, ValueNotification, WriteType};
use btleplug::platform::Peripheral;
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::crypto::to_hex;
use super::network::{AppKey, Network, Node};
use super::pdu::{ProxyJoiner, proxy_pieces, proxy_type};
use super::provision::{self, Algorithm, AuthMethod, Capabilities, Provisioner};
use super::stack::{KeyRef, Output, ProxyOf, Stack, StackEvent};
use crate::advert::{AdStructure, ad};
use crate::radio::Radio;
use crate::scanner::ScanSetup;

const PROVISIONING_SERVICE: u16 = 0x1827;
const PROVISIONING_IN: u16 = 0x2ADB;
const PROVISIONING_OUT: u16 = 0x2ADC;
const PROXY_SERVICE: u16 = 0x1828;
const PROXY_IN: u16 = 0x2ADD;
const PROXY_OUT: u16 = 0x2ADE;

const FIND: Duration = Duration::from_secs(10);
const CONNECT: Duration = Duration::from_secs(30);
const DISCOVER: Duration = Duration::from_secs(30);
/// How long a device has to answer a provisioning PDU: the protocol's own timeout.
const PROVISIONING_TIMEOUT: Duration = Duration::from_secs(60);
/// How long a proxy may take to send its beacon before bleNIMA sets its filter anyway.
const BEACON_WAIT: Duration = Duration::from_millis(1500);
/// How long after a lost or failed proxy connection the next one is tried.
const RETRY: Duration = Duration::from_secs(3);
/// How long a device stays in the lists after it was last heard.
const FORGET: Duration = Duration::from_secs(10);
/// How often the lists of devices heard are sent.
const LISTS: Duration = Duration::from_millis(500);

/// What the tab asks of the session.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MeshCommand {
    /// Provisions the unprovisioned device with this id, asking it to show
    /// itself for `attention` seconds.
    Provision {
        id: String,
        attention: u8,
    },
    /// The OOB method chosen for the device being provisioned.
    Method {
        method: AuthMethod,
    },
    /// The OOB value the user typed: the static one, or what the device put out.
    Value {
        value: String,
    },
    /// Stops provisioning.
    Cancel,
    /// Connects to this proxy, or to any of the network's when none is named.
    Connect {
        id: Option<String>,
    },
    /// Disconnects from the proxy, and stays so until asked to connect.
    Disconnect,
    /// Sends an access message: its opcode and parameters.
    Send {
        dst: u16,
        key: KeyRef,
        ttl: Option<u8>,
        payload: Vec<u8>,
    },
    Rename {
        address: u16,
        name: String,
    },
    /// Forgets a node without resetting it: for one that is gone.
    Forget {
        address: u16,
    },
    Close,
}

/// The network, for the tab.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NetworkInfo {
    /// The file it is kept in.
    pub file: String,
    pub net_key: String,
    pub net_key_index: u16,
    pub app_keys: Vec<AppKey>,
    pub iv_index: u32,
    pub iv_update: bool,
    pub address: u16,
    pub ttl: u8,
    pub nodes: Vec<Node>,
}

/// A device that waits to be provisioned, as its advert says.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UnprovisionedRow {
    pub id: String,
    pub name: Option<String>,
    pub rssi: Option<i16>,
    /// Its Device UUID, 32 hex digits.
    pub uuid: String,
    /// Where its OOB information is: bit 5 a number, bit 6 a string, bit 11
    /// on the box, bit 15 on the device, and so on.
    pub oob: u16,
}

/// A proxy of the network, as its advert says.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProxyRow {
    pub id: String,
    pub name: Option<String>,
    pub rssi: Option<i16>,
    pub of: ProxyOf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyState {
    /// Not connected, and not trying: the tab said to disconnect.
    Off,
    /// Waiting to hear a proxy of the network.
    Looking,
    Connecting,
    Connected,
}

/// What happens, for the tab.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MeshEvent {
    Network {
        network: NetworkInfo,
    },
    Heard {
        unprovisioned: Vec<UnprovisionedRow>,
        proxies: Vec<ProxyRow>,
    },
    Proxy {
        state: ProxyState,
        id: Option<String>,
        text: String,
    },
    /// The device's capabilities: the tab asks which OOB method to use.
    Capabilities {
        capabilities: Capabilities,
        algorithm: Algorithm,
        address: u16,
    },
    /// The device's OOB value is wanted: its static one, or what it put out.
    AskValue {
        method: AuthMethod,
    },
    /// The value for the user to put into the device.
    ShowValue {
        method: AuthMethod,
        value: String,
    },
    Provisioned {
        address: u16,
        elements: u8,
    },
    ProvisionFailed {
        problem: String,
    },
    Sent {
        id: u32,
        dst: u16,
        key: KeyRef,
        payload: Vec<u8>,
        at_ms: f64,
    },
    Received {
        src: u16,
        dst: u16,
        key: KeyRef,
        ttl: u8,
        payload: Vec<u8>,
        at_ms: f64,
    },
    /// Every segment of a message was acknowledged.
    Acked {
        id: u32,
        at_ms: f64,
    },
    NotAcked {
        id: u32,
        problem: String,
        at_ms: f64,
    },
    /// A line for the log.
    Log {
        text: String,
        at_ms: f64,
    },
    Failed {
        action: String,
        problem: String,
        at_ms: f64,
    },
}

/// Sends events to the tab, timed from when the session started.
#[derive(Clone)]
struct Teller {
    events: UnboundedSender<MeshEvent>,
    started: Instant,
}

impl Teller {
    fn at_ms(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1000.0
    }

    fn emit(&self, event: MeshEvent) {
        let _ = self.events.send(event);
    }

    fn log(&self, text: impl Into<String>) {
        self.emit(MeshEvent::Log {
            text: text.into(),
            at_ms: self.at_ms(),
        });
    }

    fn failed(&self, action: &str, problem: impl std::fmt::Display) {
        self.emit(MeshEvent::Failed {
            action: action.into(),
            problem: problem.to_string(),
            at_ms: self.at_ms(),
        });
    }

    fn network(&self, stack: &Stack, file: &std::path::Path) {
        let network = stack.network();
        self.emit(MeshEvent::Network {
            network: NetworkInfo {
                file: file.display().to_string(),
                net_key: network.net_key.clone(),
                net_key_index: network.net_key_index,
                app_keys: network.app_keys.clone(),
                iv_index: network.iv_index,
                iv_update: stack.iv_update(),
                address: network.address,
                ttl: network.ttl,
                nodes: network.nodes.clone(),
            },
        });
    }

    fn proxy(&self, state: ProxyState, id: Option<&str>, text: impl Into<String>) {
        self.emit(MeshEvent::Proxy {
            state,
            id: id.map(str::to_string),
            text: text.into(),
        });
    }
}

/// The Mesh tab's session, running on its own task.
pub struct MeshSession {
    commands: UnboundedSender<MeshCommand>,
    task: JoinHandle<()>,
}

impl MeshSession {
    /// Opens the network kept in `file`, a new one when there is none, and
    /// starts listening for its devices.
    pub fn open(radio: Arc<Radio>, file: PathBuf, events: UnboundedSender<MeshEvent>) -> Self {
        let (commands, receiver) = unbounded_channel();
        let task = tokio::spawn(run(radio, file, events, receiver));
        Self { commands, task }
    }

    pub fn send(&self, command: MeshCommand) {
        let _ = self.commands.send(command);
    }

    /// Disconnects and ends the task.
    pub async fn close(self) {
        let _ = self.commands.send(MeshCommand::Close);
        let _ = tokio::time::timeout(Duration::from_secs(5), self.task).await;
    }
}

/// The service data a structure gives for a 16-bit service UUID.
fn service_data(structures: &[AdStructure], uuid: u16) -> Option<&[u8]> {
    structures.iter().find_map(|structure| {
        let data = &structure.data;
        (structure.ad_type == ad::SERVICE_DATA16
            && data.len() >= 2
            && u16::from_le_bytes([data[0], data[1]]) == uuid)
            .then(|| &data[2..])
    })
}

fn local_name(structures: &[AdStructure]) -> Option<String> {
    structures
        .iter()
        .find(|structure| structure.ad_type == ad::NAME || structure.ad_type == ad::SHORT_NAME)
        .map(|structure| String::from_utf8_lossy(&structure.data).into_owned())
}

enum HeardKind {
    Unprovisioned { uuid: String, oob: u16 },
    Proxy(ProxyOf),
}

struct HeardDevice {
    rssi: Option<i16>,
    last: Instant,
    kind: HeardKind,
}

/// The mesh devices heard: those waiting to be provisioned, and the network's proxies.
#[derive(Default)]
struct Heard {
    devices: HashMap<String, HeardDevice>,
    /// Names come in scan responses, apart from the adverts.
    names: HashMap<String, String>,
    changed: bool,
}

impl Heard {
    fn take(&mut self, advert: &crate::scanner::Advert, stack: &Stack) {
        if let Some(name) = local_name(&advert.structures) {
            if self.names.get(&advert.id) != Some(&name) {
                self.changed = true;
            }
            self.names.insert(advert.id.clone(), name);
        }
        let kind = if let Some(data) = service_data(&advert.structures, PROVISIONING_SERVICE)
            && data.len() >= 18
        {
            HeardKind::Unprovisioned {
                uuid: to_hex(&data[..16]),
                // Little-endian here, as the spec's sample data shows (8.5.1).
                oob: u16::from_le_bytes([data[16], data[17]]),
            }
        } else if let Some(of) =
            service_data(&advert.structures, PROXY_SERVICE).and_then(|data| stack.proxy_of(data))
        {
            HeardKind::Proxy(of)
        } else {
            return;
        };
        let device = HeardDevice {
            rssi: advert.rssi,
            last: Instant::now(),
            kind,
        };
        self.changed |= !self.devices.contains_key(&advert.id);
        self.devices.insert(advert.id.clone(), device);
    }

    fn forget_old(&mut self) {
        let before = self.devices.len();
        self.devices
            .retain(|_, device| device.last.elapsed() < FORGET);
        self.changed |= self.devices.len() != before;
    }

    fn rows(&self) -> (Vec<UnprovisionedRow>, Vec<ProxyRow>) {
        let mut unprovisioned = Vec::new();
        let mut proxies = Vec::new();
        for (id, device) in &self.devices {
            let name = self.names.get(id).cloned();
            match &device.kind {
                HeardKind::Unprovisioned { uuid, oob } => unprovisioned.push(UnprovisionedRow {
                    id: id.clone(),
                    name,
                    rssi: device.rssi,
                    uuid: uuid.clone(),
                    oob: *oob,
                }),
                HeardKind::Proxy(of) => proxies.push(ProxyRow {
                    id: id.clone(),
                    name,
                    rssi: device.rssi,
                    of: *of,
                }),
            }
        }
        unprovisioned.sort_by(|a, b| a.id.cmp(&b.id));
        proxies.sort_by(|a, b| a.id.cmp(&b.id));
        (unprovisioned, proxies)
    }

    /// The network's proxy heard best, for connecting to one by itself.
    fn best_proxy(&self) -> Option<String> {
        self.devices
            .iter()
            .filter(|(_, device)| matches!(device.kind, HeardKind::Proxy(_)))
            .max_by_key(|(_, device)| device.rssi.unwrap_or(i16::MIN))
            .map(|(id, _)| id.clone())
    }
}

/// What a proxy connection tells the session.
enum ProxyNews {
    Connected,
    Message(u8, Vec<u8>),
    Lost(String),
}

/// A proxy connection's task, and how to write to it.
struct ProxyLink {
    /// Which link, so news from one closed before is not taken for this one's.
    number: u32,
    id: String,
    writes: UnboundedSender<(u8, Vec<u8>)>,
    task: JoinHandle<()>,
    connected: bool,
    /// When the filter is set, if the proxy's beacon has not come by then.
    setup_at: Option<Instant>,
}

/// A device being provisioned: its task, and the tab's answers to it.
struct Provisioning {
    id: String,
    answers: UnboundedSender<MeshCommand>,
    task: JoinHandle<Result<Node, String>>,
}

async fn run(
    radio: Arc<Radio>,
    file: PathBuf,
    events: UnboundedSender<MeshEvent>,
    mut commands: UnboundedReceiver<MeshCommand>,
) {
    let tell = Teller {
        events,
        started: Instant::now(),
    };
    let network = match Network::load(&file) {
        Ok(network) => network,
        Err(error) => {
            tell.failed(&format!("open {}", file.display()), error);
            Network::random()
        }
    };
    let mut stack = Stack::new(network, Some(file.clone()));
    tell.network(&stack, &file);

    let (sink, mut adverts) = unbounded_channel();
    let _listening = match radio.listen(sink, tell.started, ScanSetup::default()).await {
        Ok(listening) => Some(listening),
        Err(problem) => {
            tell.failed("listen for mesh devices", problem);
            None
        }
    };
    let mut heard = Heard::default();
    let mut lists = tokio::time::interval(LISTS);

    let (news_sender, mut news) = unbounded_channel::<(u32, ProxyNews)>();
    let mut link: Option<ProxyLink> = None;
    let mut links = 0u32;
    // Connect by itself to a proxy of the network, or to this one.
    let mut auto = true;
    let mut target: Option<String> = None;
    let mut retry_at = Instant::now();
    tell.proxy(
        ProxyState::Looking,
        None,
        "Looking for a proxy of the network",
    );

    let mut provisioning: Option<Provisioning> = None;

    loop {
        let setup_at = link.as_ref().and_then(|link| link.setup_at);
        let wake = [stack.deadline(), setup_at].into_iter().flatten().min();
        tokio::select! {
            command = commands.recv() => match command {
                None | Some(MeshCommand::Close) => break,
                Some(MeshCommand::Provision { id, attention }) => {
                    if provisioning.is_some() {
                        tell.failed("provision", "A device is being provisioned already");
                        continue;
                    }
                    let (answers, receiver) = unbounded_channel();
                    let uuid = match heard.devices.get(&id).map(|device| &device.kind) {
                        Some(HeardKind::Unprovisioned { uuid, .. }) => uuid.clone(),
                        _ => String::new(),
                    };
                    let job = Job {
                        radio: radio.clone(),
                        id: id.clone(),
                        attention,
                        name: heard.names.get(&id).cloned().unwrap_or_default(),
                        uuid,
                        network: stack.network().clone(),
                        iv_update: stack.iv_update(),
                    };
                    let task = tokio::spawn(provision_device(job, receiver, tell.clone()));
                    provisioning = Some(Provisioning { id, answers, task });
                }
                Some(command @ (MeshCommand::Method { .. } | MeshCommand::Value { .. } | MeshCommand::Cancel)) => {
                    if let Some(provisioning) = &provisioning {
                        let _ = provisioning.answers.send(command);
                    }
                }
                Some(MeshCommand::Connect { id }) => {
                    auto = true;
                    retry_at = Instant::now();
                    if id.is_some() && id != target {
                        target = id;
                        if let Some(old) = link.take() {
                            old.task.abort();
                            apply(&tell, &link, stack.disconnected());
                        }
                    } else {
                        target = id;
                    }
                    if link.is_none() {
                        tell.proxy(ProxyState::Looking, None, "Looking for a proxy of the network");
                    }
                }
                Some(MeshCommand::Disconnect) => {
                    auto = false;
                    if let Some(old) = link.take() {
                        old.task.abort();
                        apply(&tell, &link, stack.disconnected());
                    }
                    tell.proxy(ProxyState::Off, None, "Not connected to the network");
                }
                Some(MeshCommand::Send { dst, key, ttl, payload }) => {
                    if !link.as_ref().is_some_and(|link| link.connected) {
                        tell.failed("send", "No proxy is connected: the message cannot reach the network");
                        continue;
                    }
                    match stack.send(dst, key, ttl, &payload, Instant::now()) {
                        Ok((id, output)) => {
                            tell.emit(MeshEvent::Sent { id, dst, key, payload, at_ms: tell.at_ms() });
                            apply(&tell, &link, output);
                        }
                        Err(problem) => tell.failed("send", problem),
                    }
                }
                Some(MeshCommand::Rename { address, name }) => {
                    let changed = stack.change(|network| {
                        if let Some(node) = network.node_mut(address) {
                            node.name = name.trim().to_string();
                        }
                    });
                    if let Err(problem) = changed {
                        tell.failed("save the network", problem);
                    }
                    tell.network(&stack, &file);
                }
                Some(MeshCommand::Forget { address }) => {
                    if let Err(problem) = stack.change(|network| network.remove_node(address)) {
                        tell.failed("save the network", problem);
                    }
                    tell.network(&stack, &file);
                }
            },
            Some(advert) = adverts.recv() => heard.take(&advert, &stack),
            _ = lists.tick() => {
                heard.forget_old();
                if heard.changed {
                    heard.changed = false;
                    let (unprovisioned, proxies) = heard.rows();
                    tell.emit(MeshEvent::Heard { unprovisioned, proxies });
                }
                // Connect by itself, when nothing else is going on.
                if link.is_none() && auto && Instant::now() >= retry_at {
                    let busy = provisioning.as_ref().map(|provisioning| provisioning.id.clone());
                    let id = target.clone().or_else(|| heard.best_proxy());
                    if let Some(id) = id.filter(|id| Some(id) != busy.as_ref()) {
                        links += 1;
                        let (writes, receiver) = unbounded_channel();
                        let task = tokio::spawn(proxy_link(
                            radio.clone(),
                            id.clone(),
                            receiver,
                            news_sender.clone(),
                            links,
                        ));
                        tell.proxy(ProxyState::Connecting, Some(&id), format!("Connecting to {id}"));
                        link = Some(ProxyLink {
                            number: links,
                            id,
                            writes,
                            task,
                            connected: false,
                            setup_at: None,
                        });
                    }
                }
            },
            Some((number, what)) = news.recv() => {
                if link.as_ref().map(|link| link.number) != Some(number) {
                    continue;
                }
                match what {
                    ProxyNews::Connected => {
                        if let Some(link) = link.as_mut() {
                            link.connected = true;
                            link.setup_at = Some(Instant::now() + BEACON_WAIT);
                            tell.proxy(ProxyState::Connected, Some(&link.id), format!("Connected to {}", link.id));
                        }
                    }
                    ProxyNews::Message(kind, data) => {
                        let beacon = kind == proxy_type::MESH_BEACON;
                        let output = stack.receive(kind, &data, Instant::now());
                        let iv_changed = output.events.iter().any(|event| {
                            matches!(event, StackEvent::Iv { .. } | StackEvent::NetworkChanged)
                        });
                        apply(&tell, &link, output);
                        if iv_changed {
                            tell.network(&stack, &file);
                        }
                        // The proxy's first beacon gives the IV index the filter is set with.
                        if beacon && link.as_ref().is_some_and(|link| link.setup_at.is_some()) {
                            set_filter(&tell, &mut link, &mut stack);
                        }
                    }
                    ProxyNews::Lost(problem) => {
                        let id = link.take().map(|link| link.id).unwrap_or_default();
                        apply(&tell, &None, stack.disconnected());
                        tell.log(format!("Proxy {id}: {problem}"));
                        retry_at = Instant::now() + RETRY;
                        tell.proxy(ProxyState::Looking, None, "Looking for a proxy of the network");
                    }
                }
            },
            result = async {
                match provisioning.as_mut() {
                    Some(provisioning) => (&mut provisioning.task).await,
                    None => std::future::pending().await,
                }
            } => {
                let id = provisioning.take().map(|provisioning| provisioning.id).unwrap_or_default();
                match result {
                    Ok(Ok(node)) => {
                        let (address, elements) = (node.address, node.elements);
                        if let Err(problem) = stack.change(|network| network.add_node(node)) {
                            tell.failed("save the network", problem);
                        }
                        heard.devices.remove(&id);
                        heard.changed = true;
                        tell.network(&stack, &file);
                        tell.emit(MeshEvent::Provisioned { address, elements });
                        // It comes back as a proxy, most often: it may be connected to at once.
                        retry_at = Instant::now();
                    }
                    Ok(Err(problem)) => tell.emit(MeshEvent::ProvisionFailed { problem }),
                    Err(_) => tell.emit(MeshEvent::ProvisionFailed { problem: "Provisioning stopped".into() }),
                }
            },
            _ = async {
                match wake {
                    Some(at) => tokio::time::sleep_until(at.into()).await,
                    None => std::future::pending().await,
                }
            } => {
                let now = Instant::now();
                if link.as_ref().and_then(|link| link.setup_at).is_some_and(|at| at <= now) {
                    set_filter(&tell, &mut link, &mut stack);
                }
                apply(&tell, &link, stack.tick(now));
            },
        }
    }
    if let Some(provisioning) = provisioning {
        provisioning.task.abort();
    }
    if let Some(link) = link {
        // Dropping its channel ends it, and it disconnects.
        drop(link.writes);
        let _ = tokio::time::timeout(Duration::from_secs(3), link.task).await;
    }
}

/// Asks the proxy for what is addressed to bleNIMA.
fn set_filter(tell: &Teller, link: &mut Option<ProxyLink>, stack: &mut Stack) {
    if let Some(open) = link.as_mut() {
        open.setup_at = None;
    }
    match stack.proxy_setup() {
        Ok(output) => apply(tell, link, output),
        Err(problem) => tell.failed("set the proxy's filter", problem),
    }
}

/// Writes what the stack gave to the proxy, and tells the tab what happened.
fn apply(tell: &Teller, link: &Option<ProxyLink>, output: Output) {
    if let Some(link) = link {
        for write in output.writes {
            let _ = link.writes.send(write);
        }
    }
    for event in output.events {
        match event {
            StackEvent::Received(received) => tell.emit(MeshEvent::Received {
                src: received.src,
                dst: received.dst,
                key: received.key,
                ttl: received.ttl,
                payload: received.payload,
                at_ms: tell.at_ms(),
            }),
            StackEvent::Acked { id } => tell.emit(MeshEvent::Acked {
                id,
                at_ms: tell.at_ms(),
            }),
            StackEvent::NotAcked { id, problem } => tell.emit(MeshEvent::NotAcked {
                id,
                problem,
                at_ms: tell.at_ms(),
            }),
            StackEvent::Iv {
                iv_index,
                iv_update,
            } => tell.log(format!(
                "IV index {iv_index}{}",
                if iv_update { ", in an IV update" } else { "" }
            )),
            StackEvent::Filter { accept_list, size } => tell.log(format!(
                "Proxy filter: {} of {size} address{}",
                if accept_list {
                    "an accept list"
                } else {
                    "a reject list"
                },
                if size == 1 { "" } else { "es" }
            )),
            StackEvent::NetworkChanged => {}
        }
    }
}

fn characteristic(peripheral: &Peripheral, service: u16, uuid: u16) -> Option<Characteristic> {
    let (service, uuid) = (uuid_from_u16(service), uuid_from_u16(uuid));
    peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.service_uuid == service && c.uuid == uuid)
}

/// Finds, connects to and reads the GATT table of a device, and subscribes
/// to its mesh service's Data Out characteristic. Gives the device, its Data
/// In characteristic, and how many bytes a write may take.
async fn open_mesh_link(
    radio: &Radio,
    id: &str,
    service: u16,
    data_in: u16,
    data_out: u16,
) -> Result<
    (
        Peripheral,
        Characteristic,
        usize,
        impl Stream<Item = ValueNotification> + use<>,
    ),
    String,
> {
    let peripheral = radio
        .find(id, FIND)
        .await?
        .ok_or_else(|| format!("{id} was not heard"))?;
    match tokio::time::timeout(CONNECT, peripheral.connect()).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(crate::gatt::explain(&error.to_string())),
        Err(_) => return Err("Connecting timed out".into()),
    }
    let ready = async {
        match tokio::time::timeout(DISCOVER, peripheral.discover_services()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(crate::gatt::explain(&error.to_string())),
            Err(_) => return Err("Reading the GATT table timed out".into()),
        }
        let name = if service == PROVISIONING_SERVICE {
            "Mesh Provisioning Service"
        } else {
            "Mesh Proxy Service"
        };
        let input = characteristic(&peripheral, service, data_in)
            .ok_or_else(|| format!("It has no {name}"))?;
        let output = characteristic(&peripheral, service, data_out)
            .ok_or_else(|| format!("It has no {name}"))?;
        let notifications = peripheral
            .notifications()
            .await
            .map_err(|error| error.to_string())?;
        peripheral
            .subscribe(&output)
            .await
            .map_err(|error| crate::gatt::explain(&error.to_string()))?;
        // A write takes the ATT MTU less its 3-byte header.
        let room = usize::from(peripheral.mtu()).saturating_sub(3).max(20);
        Ok((input, room, notifications))
    }
    .await;
    match ready {
        Ok((input, room, notifications)) => Ok((peripheral, input, room, notifications)),
        Err(problem) => {
            let _ = peripheral.disconnect().await;
            Err(problem)
        }
    }
}

/// Writes one message to a mesh service's Data In, in as many pieces as it takes.
async fn write_message(
    peripheral: &Peripheral,
    input: &Characteristic,
    kind: u8,
    message: &[u8],
    room: usize,
) -> Result<(), String> {
    for piece in proxy_pieces(kind, message, room) {
        peripheral
            .write(input, &piece, WriteType::WithoutResponse)
            .await
            .map_err(|error| crate::gatt::explain(&error.to_string()))?;
    }
    Ok(())
}

/// A connection to a proxy, for as long as the session wants it.
async fn proxy_link(
    radio: Arc<Radio>,
    id: String,
    mut writes: UnboundedReceiver<(u8, Vec<u8>)>,
    news: UnboundedSender<(u32, ProxyNews)>,
    number: u32,
) {
    let tell = |what: ProxyNews| {
        let _ = news.send((number, what));
    };
    let (peripheral, input, room, mut notifications) =
        match open_mesh_link(&radio, &id, PROXY_SERVICE, PROXY_IN, PROXY_OUT).await {
            Ok(opened) => opened,
            Err(problem) => return tell(ProxyNews::Lost(problem)),
        };
    tell(ProxyNews::Connected);
    let out = uuid_from_u16(PROXY_OUT);
    let mut joiner = ProxyJoiner::default();
    let mut check = tokio::time::interval(Duration::from_secs(2));
    let problem = loop {
        tokio::select! {
            write = writes.recv() => match write {
                None => break None,
                Some((kind, message)) => {
                    if let Err(problem) = write_message(&peripheral, &input, kind, &message, room).await {
                        break Some(problem);
                    }
                }
            },
            notification = notifications.next() => match notification {
                Some(notification) if notification.uuid == out => {
                    if let Some((kind, message)) = joiner.push(&notification.value) {
                        tell(ProxyNews::Message(kind, message));
                    }
                }
                Some(_) => {}
                None => break Some("The connection was lost".to_string()),
            },
            _ = check.tick() => {
                if !peripheral.is_connected().await.unwrap_or(false) {
                    break Some("The connection was lost".to_string());
                }
            }
        }
    };
    let _ = peripheral.disconnect().await;
    if let Some(problem) = problem {
        tell(ProxyNews::Lost(problem));
    }
}

/// What provisioning one device takes.
struct Job {
    radio: Arc<Radio>,
    id: String,
    attention: u8,
    name: String,
    uuid: String,
    /// The network as it was when provisioning started: its keys, and the
    /// addresses taken.
    network: Network,
    iv_update: bool,
}

/// Provisions one device over PB-GATT, asking the tab what it must. Gives
/// the node it became.
async fn provision_device(
    job: Job,
    mut answers: UnboundedReceiver<MeshCommand>,
    tell: Teller,
) -> Result<Node, String> {
    tell.log(format!("Provisioning {}: connecting", job.id));
    let (peripheral, input, room, notifications) = open_mesh_link(
        &job.radio,
        &job.id,
        PROVISIONING_SERVICE,
        PROVISIONING_IN,
        PROVISIONING_OUT,
    )
    .await?;
    let mut link = Bearer {
        peripheral,
        input,
        room,
        notifications: Box::pin(notifications),
        joiner: ProxyJoiner::default(),
        out: uuid_from_u16(PROVISIONING_OUT),
    };
    let result = provision_steps(&job, &mut link, &mut answers, &tell).await;
    let _ = link.peripheral.disconnect().await;
    result
}

/// PB-GATT on one device.
struct Bearer<S> {
    peripheral: Peripheral,
    input: Characteristic,
    room: usize,
    notifications: std::pin::Pin<Box<S>>,
    joiner: ProxyJoiner,
    out: Uuid,
}

impl<S: Stream<Item = ValueNotification>> Bearer<S> {
    async fn send(&mut self, pdu: &[u8], tell: &Teller) -> Result<(), String> {
        tell.log(format!(
            "Provisioning: sent {}",
            provision::pdu_name(pdu[0])
        ));
        write_message(
            &self.peripheral,
            &self.input,
            proxy_type::PROVISIONING,
            pdu,
            self.room,
        )
        .await
    }

    /// The next provisioning PDU from the device; stops early when the tab cancels.
    async fn receive(
        &mut self,
        answers: &mut UnboundedReceiver<MeshCommand>,
        tell: &Teller,
    ) -> Result<Vec<u8>, String> {
        let deadline = tokio::time::sleep(PROVISIONING_TIMEOUT);
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                _ = &mut deadline => return Err("The device did not answer in time".into()),
                answer = answers.recv() => if matches!(answer, None | Some(MeshCommand::Cancel)) {
                    return Err("Cancelled".into());
                },
                notification = self.notifications.next() => match notification {
                    Some(notification) if notification.uuid == self.out => {
                        if let Some((proxy_type::PROVISIONING, pdu)) = self.joiner.push(&notification.value)
                            && let Some(&first) = pdu.first()
                        {
                            tell.log(format!("Provisioning: received {}", provision::pdu_name(first & 0x3F)));
                            return Ok(pdu);
                        }
                    }
                    Some(_) => {}
                    None => return Err("The device disconnected".into()),
                },
            }
        }
    }
}

/// Waits for the tab's answer: the method or the value.
async fn ask(answers: &mut UnboundedReceiver<MeshCommand>) -> Result<MeshCommand, String> {
    match answers.recv().await {
        None | Some(MeshCommand::Cancel) => Err("Cancelled".into()),
        Some(answer) => Ok(answer),
    }
}

/// Whether the device can do what the method asks.
fn method_fits(method: AuthMethod, capabilities: &Capabilities) -> Result<(), String> {
    match method {
        AuthMethod::NoOob if capabilities.oob_type & 0b10 != 0 => {
            Err("The device takes only OOB authenticated provisioning".into())
        }
        AuthMethod::Static if capabilities.oob_type & 0b01 == 0 => {
            Err("The device has no static OOB value".into())
        }
        AuthMethod::Output { action, size }
            if action > 4
                || capabilities.output_actions & (1 << action) == 0
                || size == 0
                || size > capabilities.output_size =>
        {
            Err("The device cannot put out that value".into())
        }
        AuthMethod::Input { action, size }
            if action > 3
                || capabilities.input_actions & (1 << action) == 0
                || size == 0
                || size > capabilities.input_size =>
        {
            Err("The device cannot take that value".into())
        }
        _ => Ok(()),
    }
}

async fn provision_steps<S: Stream<Item = ValueNotification>>(
    job: &Job,
    link: &mut Bearer<S>,
    answers: &mut UnboundedReceiver<MeshCommand>,
    tell: &Teller,
) -> Result<Node, String> {
    let mut provisioner = Provisioner::new();
    link.send(&provisioner.invite(job.attention), tell).await?;
    let received = link.receive(answers, tell).await?;
    let capabilities = provisioner.capabilities(&received)?;
    let algorithm = capabilities
        .algorithm()
        .ok_or("The device offers no provisioning algorithm bleNIMA knows")?;
    let elements = capabilities.elements.max(1);
    let address = job.network.next_address(elements).ok_or_else(|| {
        format!(
            "The network has no room for {elements} more address{}",
            if elements == 1 { "" } else { "es" }
        )
    })?;
    tell.emit(MeshEvent::Capabilities {
        capabilities,
        algorithm,
        address,
    });
    let method = loop {
        match ask(answers).await? {
            MeshCommand::Method { method } => match method_fits(method, &capabilities) {
                Ok(()) => break method,
                Err(problem) => tell.failed("provision", problem),
            },
            _ => continue,
        }
    };
    link.send(&provisioner.start(algorithm, method), tell)
        .await?;
    link.send(&provisioner.public_key(), tell).await?;
    let received = link.receive(answers, tell).await?;
    provisioner.device_public_key(&received)?;

    let auth = match method {
        AuthMethod::NoOob => vec![0; algorithm.size()],
        AuthMethod::Input { action, size } => {
            let value = provision::input_value(action, size);
            tell.emit(MeshEvent::ShowValue {
                method,
                value: value.clone(),
            });
            let received = link.receive(answers, tell).await?;
            if received.first().map(|first| first & 0x3F)
                != Some(provision::pdu_type::INPUT_COMPLETE)
            {
                return Err(match received.as_slice() {
                    [first, code, ..] if first & 0x3F == provision::pdu_type::FAILED => {
                        format!(
                            "The device stopped provisioning: {}",
                            provision::error_name(*code)
                        )
                    }
                    _ => "The device sent something else where Input Complete was due".into(),
                });
            }
            provision::auth_value(algorithm, method, &value)?
        }
        AuthMethod::Static | AuthMethod::Output { .. } => {
            tell.emit(MeshEvent::AskValue { method });
            loop {
                match ask(answers).await? {
                    MeshCommand::Value { value } => {
                        match provision::auth_value(algorithm, method, &value) {
                            Ok(auth) => break auth,
                            Err(problem) => {
                                tell.failed("provision", problem);
                                tell.emit(MeshEvent::AskValue { method });
                            }
                        }
                    }
                    _ => continue,
                }
            }
        }
    };

    link.send(&provisioner.confirmation(&auth), tell).await?;
    let received = link.receive(answers, tell).await?;
    let random = provisioner.device_confirmation(&received)?;
    link.send(&random, tell).await?;
    let received = link.receive(answers, tell).await?;
    provisioner.device_random(&received)?;
    let flags = u8::from(job.iv_update) << 1;
    let data = provisioner.data(
        &job.network.net_key(),
        job.network.net_key_index,
        flags,
        job.network.iv_index,
        address,
    )?;
    link.send(&data, tell).await?;
    let received = link.receive(answers, tell).await?;
    provisioner.complete(&received)?;
    let device_key = provisioner.device_key().ok_or("No device key was made")?;
    tell.log(format!("Provisioning: {} is node 0x{address:04X}", job.id));
    Ok(Node {
        address,
        elements,
        device_key: to_hex(&device_key),
        uuid: job.uuid.clone(),
        name: job.name.clone(),
        composition: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::crypto::tests::bytes;

    #[test]
    fn the_specs_provisioning_service_data_is_read() {
        // 8.5.1: a number, inside the manual.
        let structures = vec![AdStructure::new(
            ad::SERVICE_DATA16,
            bytes("271870cf7c9732a345b691494810d2e9cbf42040"),
        )];
        let data = service_data(&structures, PROVISIONING_SERVICE).unwrap();
        assert_eq!(to_hex(&data[..16]), "70cf7c9732a345b691494810d2e9cbf4");
        assert_eq!(u16::from_le_bytes([data[16], data[17]]), 0x4020);
        assert_eq!(service_data(&structures, PROXY_SERVICE), None);
    }

    #[test]
    fn a_method_must_be_one_the_device_has() {
        let capabilities = Capabilities::parse(&bytes("0100030001020009010004")).unwrap();
        // A static value; Blink and Output Numeric of 2 digits; Input Numeric of 1.
        assert!(method_fits(AuthMethod::NoOob, &capabilities).is_ok());
        assert!(method_fits(AuthMethod::Static, &capabilities).is_ok());
        assert!(method_fits(AuthMethod::Output { action: 3, size: 2 }, &capabilities).is_ok());
        assert!(method_fits(AuthMethod::Output { action: 3, size: 3 }, &capabilities).is_err());
        assert!(method_fits(AuthMethod::Output { action: 1, size: 1 }, &capabilities).is_err());
        assert!(method_fits(AuthMethod::Input { action: 2, size: 1 }, &capabilities).is_ok());
        assert!(method_fits(AuthMethod::Input { action: 0, size: 1 }, &capabilities).is_err());
        let oob_only = Capabilities::parse(&bytes("0100020002000000000000")).unwrap();
        assert!(method_fits(AuthMethod::NoOob, &oob_only).is_err());
        assert!(method_fits(AuthMethod::Static, &oob_only).is_err());
    }
}
