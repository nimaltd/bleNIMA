//! The mesh stack between the proxy and the access messages. What bleNIMA
//! sends is given its sequence number, encrypted and cut into segments; what
//! comes in is checked, decrypted, put back together and acknowledged. It does
//! no I/O: the session hands it what the proxy sends, and writes out what it
//! gives back.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::crypto::{self, Key, Mic, NetworkKeys, aes_ecb, k2, k4};
use super::network::{Network, UNICAST_MAX};
use super::pdu::{
    self, AccessKey, AccessPlace, Lower, NetworkPdu, NonceKind, SEGMENT_SIZE, SEGMENTS_MAX,
    UNSEGMENTED_MAX, proxy_config, proxy_type,
};

/// How many sequence numbers are written ahead to `mesh.toml`.
const SEQ_BLOCK: u32 = 256;
const SEQ_MAX: u32 = 0xFF_FFFF;
/// How many times the segments not acknowledged are sent again.
const RETRIES: u8 = 4;
/// How long the segments of a message coming in may take.
const INCOMPLETE: Duration = Duration::from_secs(10);
/// How many messages that came in whole are remembered, to acknowledge again
/// when their segments come again because the sender missed the first ack.
const COMPLETED: usize = 16;

/// Which key an access message is sent with or came with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KeyRef {
    /// The device key of the node the message is to or from.
    Device,
    App {
        index: u16,
    },
}

/// An access message that came in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Received {
    pub src: u16,
    pub dst: u16,
    pub ttl: u8,
    pub key: KeyRef,
    /// The opcode and its parameters.
    pub payload: Vec<u8>,
}

/// What happened, for the session to tell the tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StackEvent {
    Received(Received),
    /// The node acknowledged every segment of a message.
    Acked {
        id: u32,
    },
    /// A segmented message the node did not take.
    NotAcked {
        id: u32,
        problem: String,
    },
    /// The proxy told the network's IV index.
    Iv {
        iv_index: u32,
        iv_update: bool,
    },
    /// The proxy said what its filter is now.
    Filter {
        accept_list: bool,
        size: u16,
    },
    /// Something the network remembers changed: a node's composition, or a node reset.
    NetworkChanged,
}

/// What to write to the proxy, as whole proxy messages with their type, and what happened.
#[derive(Debug, Default)]
pub struct Output {
    pub writes: Vec<(u8, Vec<u8>)>,
    pub events: Vec<StackEvent>,
}

/// How a proxy's advert says it belongs to this network.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProxyOf {
    /// It gives the network's Network ID.
    Network,
    /// It gives the identity of the node with this address.
    Node { address: u16 },
}

/// A segmented message waiting for its turn: only one goes out at a time.
struct Queued {
    id: u32,
    dst: u16,
    ttl: u8,
    key: KeyRef,
    payload: Vec<u8>,
}

/// The segmented message going out.
struct Outgoing {
    id: u32,
    dst: u16,
    ttl: u8,
    seq_zero: u16,
    segments: Vec<Vec<u8>>,
    acked: u32,
    tries: u8,
    deadline: Instant,
}

impl Outgoing {
    fn all(&self) -> u32 {
        all_segments(self.segments.len())
    }
}

/// A segmented message coming in.
struct Incoming {
    dst: u16,
    ttl: u8,
    iv_index: u32,
    seq_auth: u32,
    seq_zero: u16,
    akf: bool,
    aid: u8,
    szmic: bool,
    segments: Vec<Option<Vec<u8>>>,
    received: u32,
    deadline: Instant,
}

fn all_segments(count: usize) -> u32 {
    if count >= 32 {
        u32::MAX
    } else {
        (1u32 << count) - 1
    }
}

/// How long a node waits for segments to be acknowledged: 200 ms and 50 ms a hop.
fn segment_timer(ttl: u8) -> Duration {
    Duration::from_millis(200 + 50 * u64::from(ttl))
}

pub fn is_unicast(address: u16) -> bool {
    address != 0 && address <= UNICAST_MAX
}

pub struct Stack {
    network: Network,
    /// Where the network is saved; none in tests.
    path: Option<PathBuf>,
    keys: NetworkKeys,
    network_id: [u8; 8],
    beacon_key: Key,
    identity_key: Key,
    /// Whether the network is in an IV update: messages then go with the IV index before.
    iv_update: bool,
    seq: u32,
    next_id: u32,
    /// The newest IV index and sequence number heard from each address.
    replay: HashMap<u16, u64>,
    incoming: HashMap<u16, Incoming>,
    completed: VecDeque<(u16, u16, u32)>,
    outgoing: Option<Outgoing>,
    queue: VecDeque<Queued>,
}

impl Stack {
    pub fn new(network: Network, path: Option<PathBuf>) -> Self {
        let net_key = network.net_key();
        Self {
            keys: k2(&net_key, &[0x00]),
            network_id: crypto::k3(&net_key),
            beacon_key: crypto::beacon_key(&net_key),
            identity_key: crypto::identity_key(&net_key),
            seq: network.seq,
            network,
            path,
            iv_update: false,
            next_id: 0,
            replay: HashMap::new(),
            incoming: HashMap::new(),
            completed: VecDeque::new(),
            outgoing: None,
            queue: VecDeque::new(),
        }
    }

    pub fn network(&self) -> &Network {
        &self.network
    }

    /// Changes the network, and saves it. The keys stay as they were: a new
    /// network key takes a new stack.
    pub fn change(&mut self, change: impl FnOnce(&mut Network)) -> Result<(), String> {
        change(&mut self.network);
        self.save()
    }

    pub fn save(&self) -> Result<(), String> {
        match &self.path {
            Some(path) => self
                .network
                .save(path)
                .map_err(|error| format!("could not save {}: {error}", path.display())),
            None => Ok(()),
        }
    }

    pub fn iv_update(&self) -> bool {
        self.iv_update
    }

    /// The IV index messages are sent with: the one before, during an IV update.
    fn tx_iv(&self) -> u32 {
        self.network
            .iv_index
            .saturating_sub(u32::from(self.iv_update))
    }

    fn next_seq(&mut self) -> Result<u32, String> {
        if self.seq > SEQ_MAX {
            return Err("bleNIMA has used every sequence number of this IV index: the network needs an IV update".into());
        }
        if self.seq >= self.network.seq {
            self.network.seq = (self.seq + SEQ_BLOCK).min(SEQ_MAX + 1);
            // Should it fail, the worst is a number sent again after a crash.
            let _ = self.save();
        }
        let seq = self.seq;
        self.seq += 1;
        Ok(seq)
    }

    fn network_pdu(
        &self,
        ctl: bool,
        ttl: u8,
        seq: u32,
        dst: u16,
        transport: Vec<u8>,
    ) -> (u8, Vec<u8>) {
        let pdu = NetworkPdu {
            ctl,
            ttl,
            seq,
            src: self.network.address,
            dst,
            transport,
        };
        (
            proxy_type::NETWORK_PDU,
            pdu::encode_network(&self.keys, self.tx_iv(), &pdu, NonceKind::Network),
        )
    }

    /// The proxy configuration messages that ask the proxy to send bleNIMA
    /// what is addressed to it: an accept list with its own address.
    pub fn proxy_setup(&mut self) -> Result<Output, String> {
        let address = self.network.address.to_be_bytes();
        let mut output = Output::default();
        for message in [
            vec![proxy_config::SET_FILTER_TYPE, proxy_config::ACCEPT_LIST],
            vec![proxy_config::ADD_ADDRESSES, address[0], address[1]],
        ] {
            let seq = self.next_seq()?;
            let pdu = pdu::proxy_config_pdu(
                &self.keys,
                self.tx_iv(),
                seq,
                self.network.address,
                &message,
            );
            output.writes.push((proxy_type::CONFIGURATION, pdu));
        }
        Ok(output)
    }

    /// The key a message to `dst` goes with, with AKF and AID.
    fn access_key(&self, dst: u16, key: KeyRef) -> Result<(Key, bool, u8), String> {
        match key {
            KeyRef::Device => {
                let node = self.network.node(dst).ok_or_else(|| {
                    format!("bleNIMA has no device key for 0x{dst:04X}: it did not provision it")
                })?;
                let key = node.device_key().ok_or("The node's device key is broken")?;
                Ok((key, false, 0))
            }
            KeyRef::App { index } => {
                let key = self
                    .network
                    .app_key(index)
                    .ok_or_else(|| format!("The network has no application key {index}"))?;
                Ok((key, true, k4(&key)))
            }
        }
    }

    /// Sends an access message, its opcode and parameters, to `dst`. Gives the
    /// message an id, which the events about it carry.
    pub fn send(
        &mut self,
        dst: u16,
        key: KeyRef,
        ttl: Option<u8>,
        payload: &[u8],
        now: Instant,
    ) -> Result<(u32, Output), String> {
        if dst == 0 {
            return Err("0x0000 is not an address".into());
        }
        if payload.is_empty() {
            return Err("A message has an opcode at least".into());
        }
        if payload.len() + 4 > SEGMENT_SIZE * SEGMENTS_MAX {
            return Err(format!(
                "A message is {} bytes at most",
                SEGMENT_SIZE * SEGMENTS_MAX - 4
            ));
        }
        let (access_key, akf, aid) = self.access_key(dst, key)?;
        let ttl = ttl.unwrap_or(self.network.ttl).min(127);
        self.next_id += 1;
        let id = self.next_id;
        let mut output = Output::default();
        if payload.len() + 4 <= UNSEGMENTED_MAX {
            let seq = self.next_seq()?;
            let upper = self.seal(access_key, akf, seq, dst, payload);
            let lower = pdu::unsegmented_access(akf, aid, &upper);
            output
                .writes
                .push(self.network_pdu(false, ttl, seq, dst, lower));
        } else {
            self.queue.push_back(Queued {
                id,
                dst,
                ttl,
                key,
                payload: payload.to_vec(),
            });
            self.start_next(now, &mut output);
        }
        Ok((id, output))
    }

    fn seal(&self, key: Key, akf: bool, seq: u32, dst: u16, payload: &[u8]) -> Vec<u8> {
        let place = AccessPlace {
            seq,
            src: self.network.address,
            dst,
            iv_index: self.tx_iv(),
            label: None,
        };
        let key = if akf {
            AccessKey::Application(&key)
        } else {
            AccessKey::Device(&key)
        };
        pdu::encrypt_access(key, place, payload, Mic::Short)
    }

    /// Starts the next segmented message, when none is going out.
    fn start_next(&mut self, now: Instant, output: &mut Output) {
        while self.outgoing.is_none() {
            let Some(queued) = self.queue.pop_front() else {
                return;
            };
            match self.start(queued, now, output) {
                Ok(outgoing) => self.outgoing = Some(outgoing),
                Err((id, problem)) => output.events.push(StackEvent::NotAcked { id, problem }),
            }
        }
    }

    fn start(
        &mut self,
        queued: Queued,
        now: Instant,
        output: &mut Output,
    ) -> Result<Outgoing, (u32, String)> {
        let failed = |problem: String| (queued.id, problem);
        let (key, akf, aid) = self.access_key(queued.dst, queued.key).map_err(failed)?;
        let seq_auth = self.next_seq().map_err(failed)?;
        let upper = self.seal(key, akf, seq_auth, queued.dst, &queued.payload);
        let seq_zero = (seq_auth & 0x1FFF) as u16;
        let segments = pdu::segmented_access(akf, aid, false, seq_zero, &upper);
        for (at, segment) in segments.iter().enumerate() {
            let seq = if at == 0 {
                seq_auth
            } else {
                self.next_seq().map_err(failed)?
            };
            output.writes.push(self.network_pdu(
                false,
                queued.ttl,
                seq,
                queued.dst,
                segment.clone(),
            ));
        }
        Ok(Outgoing {
            id: queued.id,
            dst: queued.dst,
            ttl: queued.ttl,
            seq_zero,
            segments,
            acked: 0,
            tries: 0,
            deadline: now + segment_timer(queued.ttl),
        })
    }

    /// When `tick` has something to do next.
    pub fn deadline(&self) -> Option<Instant> {
        let outgoing = self.outgoing.as_ref().map(|outgoing| outgoing.deadline);
        let incoming = self
            .incoming
            .values()
            .map(|incoming| incoming.deadline)
            .min();
        outgoing.into_iter().chain(incoming).min()
    }

    /// Sends again what was not acknowledged in time, and drops what came in
    /// part and was never finished.
    pub fn tick(&mut self, now: Instant) -> Output {
        let mut output = Output::default();
        self.incoming.retain(|_, incoming| incoming.deadline > now);
        let Some(mut outgoing) = self.outgoing.take() else {
            return output;
        };
        if outgoing.deadline > now {
            self.outgoing = Some(outgoing);
            return output;
        }
        let unicast = is_unicast(outgoing.dst);
        // A group hears every segment twice, and acknowledges none.
        let tries = if unicast { RETRIES } else { 1 };
        if outgoing.tries >= tries {
            if unicast {
                output.events.push(StackEvent::NotAcked {
                    id: outgoing.id,
                    problem: format!("0x{:04X} did not acknowledge the message", outgoing.dst),
                });
            }
            self.start_next(now, &mut output);
            return output;
        }
        outgoing.tries += 1;
        for (at, segment) in outgoing.segments.iter().enumerate() {
            if outgoing.acked & (1 << at) != 0 {
                continue;
            }
            match self.next_seq() {
                Ok(seq) => output.writes.push(self.network_pdu(
                    false,
                    outgoing.ttl,
                    seq,
                    outgoing.dst,
                    segment.clone(),
                )),
                Err(problem) => {
                    output.events.push(StackEvent::NotAcked {
                        id: outgoing.id,
                        problem,
                    });
                    self.start_next(now, &mut output);
                    return output;
                }
            }
        }
        outgoing.deadline = now + segment_timer(outgoing.ttl);
        self.outgoing = Some(outgoing);
        output
    }

    /// The proxy went away: what was going out is not acknowledged now.
    pub fn disconnected(&mut self) -> Output {
        let mut output = Output::default();
        let waiting = self.outgoing.take().map(|outgoing| outgoing.id).into_iter();
        for id in waiting.chain(self.queue.drain(..).map(|queued| queued.id)) {
            output.events.push(StackEvent::NotAcked {
                id,
                problem: "The proxy was disconnected".into(),
            });
        }
        self.incoming.clear();
        output
    }

    /// Takes a whole proxy message: a network PDU, a beacon or a proxy configuration message.
    pub fn receive(&mut self, kind: u8, data: &[u8], now: Instant) -> Output {
        let mut output = Output::default();
        match kind {
            proxy_type::NETWORK_PDU => self.network_message(data, now, &mut output),
            proxy_type::MESH_BEACON => self.beacon(data, &mut output),
            proxy_type::CONFIGURATION => {
                if let Some(pdu) =
                    pdu::decode_network(&self.keys, self.network.iv_index, data, NonceKind::Proxy)
                    && pdu.transport.len() >= 4
                    && pdu.transport[0] == proxy_config::FILTER_STATUS
                {
                    output.events.push(StackEvent::Filter {
                        accept_list: pdu.transport[1] == proxy_config::ACCEPT_LIST,
                        size: u16::from_be_bytes([pdu.transport[2], pdu.transport[3]]),
                    });
                }
            }
            _ => {}
        }
        output
    }

    fn beacon(&mut self, data: &[u8], output: &mut Output) {
        let Some(beacon) = pdu::parse_secure_beacon(&self.beacon_key, &self.network_id, data)
        else {
            return;
        };
        // An IV index older than the one known is of no use.
        if beacon.iv_index < self.network.iv_index
            || (beacon.iv_index == self.network.iv_index && beacon.iv_update == self.iv_update)
        {
            return;
        }
        let before = self.tx_iv();
        self.network.iv_index = beacon.iv_index;
        self.iv_update = beacon.iv_update;
        if self.tx_iv() != before {
            // A new IV index starts the sequence numbers over.
            self.seq = 0;
            self.network.seq = 0;
        }
        let _ = self.save();
        output.events.push(StackEvent::Iv {
            iv_index: beacon.iv_index,
            iv_update: beacon.iv_update,
        });
    }

    fn network_message(&mut self, data: &[u8], now: Instant, output: &mut Output) {
        let Some(iv_index) = data
            .first()
            .and_then(|&first| pdu::rx_iv_index(self.network.iv_index, first))
        else {
            return;
        };
        let Some(pdu) =
            pdu::decode_network(&self.keys, self.network.iv_index, data, NonceKind::Network)
        else {
            return;
        };
        if pdu.src == self.network.address || !is_unicast(pdu.src) {
            return;
        }
        match pdu::parse_lower(pdu.ctl, &pdu.transport) {
            Some(Lower::Ack {
                seq_zero,
                block_ack,
                ..
            }) => self.ack(pdu.src, seq_zero, block_ack, now, output),
            Some(Lower::Access { akf, aid, upper }) => {
                let replay = (u64::from(iv_index) << 24) | u64::from(pdu.seq);
                if self
                    .replay
                    .get(&pdu.src)
                    .is_some_and(|&last| last >= replay)
                {
                    return;
                }
                if let Some(received) = self.open(&pdu, iv_index, pdu.seq, akf, aid, false, &upper)
                {
                    self.replay.insert(pdu.src, replay);
                    self.deliver(received, output);
                }
            }
            Some(Lower::Segment {
                akf,
                aid,
                szmic,
                seq_zero,
                seg_o,
                seg_n,
                data,
            }) => {
                let segment = Segment {
                    akf,
                    aid,
                    szmic,
                    seq_zero,
                    seg_o,
                    seg_n,
                };
                self.segment(&pdu, iv_index, segment, data, now, output);
            }
            _ => {}
        }
    }

    /// A Segment Acknowledgment for the message going out.
    fn ack(&mut self, src: u16, seq_zero: u16, block_ack: u32, now: Instant, output: &mut Output) {
        let Some(outgoing) = self.outgoing.as_mut() else {
            return;
        };
        if outgoing.dst != src || outgoing.seq_zero != seq_zero {
            return;
        }
        if block_ack == 0 {
            let id = outgoing.id;
            self.outgoing = None;
            output.events.push(StackEvent::NotAcked {
                id,
                problem: format!("0x{src:04X} is busy, and dropped the message"),
            });
        } else {
            outgoing.acked |= block_ack;
            if outgoing.acked & outgoing.all() != outgoing.all() {
                return;
            }
            let id = outgoing.id;
            self.outgoing = None;
            output.events.push(StackEvent::Acked { id });
        }
        self.start_next(now, output);
    }

    fn segment(
        &mut self,
        pdu: &NetworkPdu,
        iv_index: u32,
        segment: Segment,
        data: Vec<u8>,
        now: Instant,
        output: &mut Output,
    ) {
        let src = pdu.src;
        let seq_auth = pdu::seq_auth(pdu.seq, segment.seq_zero);
        let replay = (u64::from(iv_index) << 24) | u64::from(seq_auth);
        // Whole already: the sender missed the ack, so it is sent again.
        if let Some(&(_, _, block)) = self
            .completed
            .iter()
            .find(|(from, zero, _)| *from == src && *zero == segment.seq_zero)
        {
            if is_unicast(pdu.dst) {
                output
                    .writes
                    .push(self.ack_pdu(src, pdu.ttl, segment.seq_zero, block));
            }
            return;
        }
        if self.replay.get(&src).is_some_and(|&last| last >= replay)
            || segment.seg_o > segment.seg_n
        {
            return;
        }
        let fresh = self
            .incoming
            .get(&src)
            .is_none_or(|incoming| incoming.seq_auth != seq_auth || incoming.iv_index != iv_index);
        if fresh {
            self.incoming.insert(
                src,
                Incoming {
                    dst: pdu.dst,
                    ttl: pdu.ttl,
                    iv_index,
                    seq_auth,
                    seq_zero: segment.seq_zero,
                    akf: segment.akf,
                    aid: segment.aid,
                    szmic: segment.szmic,
                    segments: vec![None; usize::from(segment.seg_n) + 1],
                    received: 0,
                    deadline: now + INCOMPLETE,
                },
            );
        }
        let Some(incoming) = self.incoming.get_mut(&src) else {
            return;
        };
        if incoming.segments.len() != usize::from(segment.seg_n) + 1 {
            return;
        }
        incoming.segments[usize::from(segment.seg_o)] = Some(data);
        incoming.received |= 1 << segment.seg_o;
        incoming.deadline = now + INCOMPLETE;
        let all = all_segments(incoming.segments.len());
        if incoming.received != all {
            return;
        }
        let Some(incoming) = self.incoming.remove(&src) else {
            return;
        };
        let upper: Vec<u8> = incoming.segments.into_iter().flatten().flatten().collect();
        let whole = NetworkPdu {
            ctl: false,
            ttl: incoming.ttl,
            seq: seq_auth,
            src,
            dst: incoming.dst,
            transport: Vec::new(),
        };
        let Some(received) = self.open(
            &whole,
            incoming.iv_index,
            incoming.seq_auth,
            incoming.akf,
            incoming.aid,
            incoming.szmic,
            &upper,
        ) else {
            return;
        };
        self.replay.insert(src, replay);
        if self.completed.len() >= COMPLETED {
            self.completed.pop_front();
        }
        self.completed.push_back((src, incoming.seq_zero, all));
        if is_unicast(incoming.dst) {
            output
                .writes
                .push(self.ack_pdu(src, incoming.ttl, incoming.seq_zero, all));
        }
        self.deliver(received, output);
    }

    /// A Segment Acknowledgment to `dst`: sent with TTL 0 when the message came with it.
    fn ack_pdu(&mut self, dst: u16, ttl: u8, seq_zero: u16, block: u32) -> (u8, Vec<u8>) {
        let ttl = if ttl == 0 { 0 } else { self.network.ttl };
        let seq = self.next_seq().unwrap_or(SEQ_MAX);
        self.network_pdu(
            true,
            ttl,
            seq,
            dst,
            pdu::segment_ack(false, seq_zero, block),
        )
    }

    /// An upper transport PDU decrypted: with the sender's device key, or the
    /// application key its AID points to.
    #[allow(clippy::too_many_arguments)]
    fn open(
        &self,
        pdu: &NetworkPdu,
        iv_index: u32,
        seq: u32,
        akf: bool,
        aid: u8,
        szmic: bool,
        upper: &[u8],
    ) -> Option<Received> {
        let mic = if szmic { Mic::Long } else { Mic::Short };
        let place = AccessPlace {
            seq,
            src: pdu.src,
            dst: pdu.dst,
            iv_index,
            label: None,
        };
        let (key, payload) = if akf {
            self.network.app_keys.iter().find_map(|app_key| {
                let key = crypto::key_from_hex(&app_key.key)?;
                if k4(&key) != aid {
                    return None;
                }
                let payload = pdu::decrypt_access(AccessKey::Application(&key), place, upper, mic)?;
                Some((
                    KeyRef::App {
                        index: app_key.index,
                    },
                    payload,
                ))
            })?
        } else {
            let key = self.network.node(pdu.src)?.device_key()?;
            let payload = pdu::decrypt_access(AccessKey::Device(&key), place, upper, mic)?;
            (KeyRef::Device, payload)
        };
        Some(Received {
            src: pdu.src,
            dst: pdu.dst,
            ttl: pdu.ttl,
            key,
            payload,
        })
    }

    /// Hands a message on, and keeps what the network remembers of it: a
    /// node's Composition Data page 0, or that the node was reset.
    fn deliver(&mut self, received: Received, output: &mut Output) {
        if received.key == KeyRef::Device {
            let changed = match received.payload.as_slice() {
                // Composition Data Status, page 0.
                [0x02, 0x00, data @ ..] if !data.is_empty() => {
                    let hex = crypto::to_hex(data);
                    self.network.node_mut(received.src).is_some_and(|node| {
                        let changed = node.composition != hex;
                        node.composition = hex;
                        changed
                    })
                }
                // Config Node Reset Status: the node forgot the network.
                [0x80, 0x4A] => {
                    let before = self.network.nodes.len();
                    self.network.remove_node(received.src);
                    self.network.nodes.len() != before
                }
                _ => false,
            };
            if changed {
                let _ = self.save();
                output.events.push(StackEvent::NetworkChanged);
            }
        }
        output.events.push(StackEvent::Received(received));
    }

    /// Whether a proxy's advert, its Mesh Proxy Service data, is of this
    /// network: by its Network ID, or the identity of one of its nodes.
    pub fn proxy_of(&self, service_data: &[u8]) -> Option<ProxyOf> {
        match service_data {
            [0x00, id @ ..] if id == self.network_id => Some(ProxyOf::Network),
            [0x01, hash @ ..] if hash.len() == 16 => {
                let (hash, random) = hash.split_at(8);
                self.network.nodes.iter().find_map(|node| {
                    // The spec pads with zeros; Mesh Profile 1.0 devices may pad with ones.
                    [0x00u8, 0xFF].into_iter().find_map(|padding| {
                        let mut block = [padding; 16];
                        block[6..14].copy_from_slice(random);
                        block[14..].copy_from_slice(&node.address.to_be_bytes());
                        (aes_ecb(&self.identity_key, &block)[8..] == *hash).then_some(
                            ProxyOf::Node {
                                address: node.address,
                            },
                        )
                    })
                })
            }
            _ => None,
        }
    }
}

/// A segment's lower transport fields.
#[derive(Clone, Copy)]
struct Segment {
    akf: bool,
    aid: u8,
    szmic: bool,
    seq_zero: u16,
    seg_o: u8,
    seg_n: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::crypto::tests::bytes;
    use crate::mesh::crypto::to_hex;
    use crate::mesh::network::{AppKey, Node};

    const IV_INDEX: u32 = 0x1234_5678;

    /// The spec's sample network (8.3), as seen from `address`, with a node at
    /// `node` using the spec's device key.
    fn stack(address: u16, node: u16) -> Stack {
        let network = Network {
            net_key: "7dd7364cd842ad18c17c2b820c84c3d6".into(),
            app_keys: vec![AppKey {
                index: 0,
                key: "63964771734fbd76e3b40519d1d94a48".into(),
            }],
            iv_index: IV_INDEX,
            address,
            nodes: vec![Node {
                address: node,
                elements: 1,
                device_key: "9d6dd0e96eb25dc19a40ed9914f8f03f".into(),
                uuid: String::new(),
                name: String::new(),
                composition: String::new(),
            }],
            ..Network::default()
        };
        Stack::new(network, None)
    }

    fn received(output: &Output) -> Vec<&Received> {
        output
            .events
            .iter()
            .filter_map(|event| match event {
                StackEvent::Received(received) => Some(received),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn config_appkey_status_comes_in_with_the_device_key_as_message_16() {
        let mut stack = stack(0x0003, 0x1201);
        let now = Instant::now();
        let packet = bytes("68e80e5da5af0e6b9be7f5a642f2f98680e61c3a8b47f228");
        let output = stack.receive(proxy_type::NETWORK_PDU, &packet, now);
        let got = received(&output);
        assert_eq!(got.len(), 1);
        assert_eq!(
            (got[0].src, got[0].dst, got[0].key),
            (0x1201, 0x0003, KeyRef::Device)
        );
        assert_eq!(to_hex(&got[0].payload), "800300563412");
        // The same packet again is a replay, and is dropped.
        assert!(received(&stack.receive(proxy_type::NETWORK_PDU, &packet, now)).is_empty());
    }

    #[test]
    fn health_current_status_comes_in_with_the_app_key_as_message_18() {
        let mut stack = stack(0x0003, 0x1201);
        let packet = bytes("6848cba437860e5673728a627fb938535508e21a6baf57");
        let output = stack.receive(proxy_type::NETWORK_PDU, &packet, Instant::now());
        let got = received(&output);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].key, KeyRef::App { index: 0 });
        assert_eq!(got[0].dst, 0xFFFF);
        assert_eq!(to_hex(&got[0].payload), "0400000000");
    }

    #[test]
    fn config_appkey_add_goes_out_as_the_specs_segments_message_6() {
        let mut stack = stack(0x0003, 0x1201);
        stack.seq = 0x3129AB;
        stack.network.seq = 0x3129AB + 100;
        let now = Instant::now();
        let payload = bytes("0056341263964771734fbd76e3b40519d1d94a48");
        let (id, output) = stack
            .send(0x1201, KeyRef::Device, Some(4), &payload, now)
            .unwrap();
        assert_eq!(
            output
                .writes
                .iter()
                .map(|(_, pdu)| to_hex(pdu))
                .collect::<Vec<_>>(),
            [
                "68cab5c5348a230afba8c63d4e686364979deaf4fd40961145939cda0e",
                "681615b5dd4a846cae0c032bf0746f44f1b8cc8ce5edc57e55beed49c0"
            ]
        );
        // The node acknowledges the second segment, then both.
        let mut node = self::stack(0x1201, 0x0003);
        node.seq = 0x014835;
        node.network.seq = 0x014835 + 100;
        let ack = |node: &mut Stack, block: u32| {
            let (kind, pdu) = node.network_pdu(
                true,
                0x0B,
                node.seq,
                0x0003,
                pdu::segment_ack(false, 0x09AB, block),
            );
            node.seq += 1;
            (kind, pdu)
        };
        let (kind, pdu) = ack(&mut node, 0b10);
        assert!(stack.receive(kind, &pdu, now).events.is_empty());
        // Only the first segment goes again when its time is up.
        let again = stack.tick(now + Duration::from_millis(500));
        assert_eq!(again.writes.len(), 1);
        let (kind, pdu) = ack(&mut node, 0b11);
        assert_eq!(
            stack.receive(kind, &pdu, now).events,
            [StackEvent::Acked { id }]
        );
        assert_eq!(stack.deadline(), None);
    }

    #[test]
    fn segments_coming_in_are_put_together_and_acknowledged() {
        // The node's side of message 6: two segments from 0x0003, with its device key.
        let mut node = stack(0x1201, 0x0003);
        node.network.seq = 0x014835;
        node.seq = 0x014835;
        let now = Instant::now();
        let first = bytes("68cab5c5348a230afba8c63d4e686364979deaf4fd40961145939cda0e");
        let second = bytes("681615b5dd4a846cae0c032bf0746f44f1b8cc8ce5edc57e55beed49c0");
        assert!(
            node.receive(proxy_type::NETWORK_PDU, &second, now)
                .events
                .is_empty()
        );
        let output = node.receive(proxy_type::NETWORK_PDU, &first, now);
        let got = received(&output);
        assert_eq!(got.len(), 1);
        assert_eq!(
            to_hex(&got[0].payload),
            "0056341263964771734fbd76e3b40519d1d94a48"
        );
        // The ack is the spec's message 7 for these segments: both, SeqZero 0x09AB.
        assert_eq!(output.writes.len(), 1);
        let ack = pdu::decode_network(
            &node.keys,
            IV_INDEX,
            &output.writes[0].1,
            NonceKind::Network,
        )
        .unwrap();
        assert_eq!((ack.ctl, ack.ttl, ack.dst), (true, 7, 0x0003));
        assert_eq!(
            pdu::parse_lower(true, &ack.transport),
            Some(Lower::Ack {
                obo: false,
                seq_zero: 0x09AB,
                block_ack: 0b11
            })
        );
        // A segment again: the message is not handed on twice, but acknowledged again.
        let again = node.receive(proxy_type::NETWORK_PDU, &second, now);
        assert!(received(&again).is_empty());
        assert_eq!(again.writes.len(), 1);
    }

    #[test]
    fn a_message_not_acknowledged_fails_after_its_tries() {
        let mut stack = stack(0x0003, 0x1201);
        let mut now = Instant::now();
        let (id, output) = stack
            .send(0x1201, KeyRef::Device, Some(0), &[0x80; 20], now)
            .unwrap();
        assert_eq!(output.writes.len(), 2);
        // A second one waits for the first.
        let (second, output) = stack
            .send(0x1201, KeyRef::App { index: 0 }, Some(0), &[0x81; 20], now)
            .unwrap();
        assert!(output.writes.is_empty());
        for _ in 0..RETRIES {
            now += Duration::from_secs(1);
            assert_eq!(stack.tick(now).writes.len(), 2);
        }
        now += Duration::from_secs(1);
        let output = stack.tick(now);
        assert_eq!(
            output.events,
            [StackEvent::NotAcked {
                id,
                problem: "0x1201 did not acknowledge the message".into()
            }]
        );
        assert_eq!(output.writes.len(), 2, "the second message goes out");
        let output = stack.disconnected();
        assert_eq!(
            output.events,
            [StackEvent::NotAcked {
                id: second,
                problem: "The proxy was disconnected".into()
            }]
        );
    }

    #[test]
    fn short_messages_go_whole_and_sequence_numbers_are_kept_ahead() {
        let mut stack = stack(0x0003, 0x1201);
        stack.network.seq = 10;
        stack.seq = 10;
        let (_, output) = stack
            .send(
                0x1201,
                KeyRef::App { index: 0 },
                None,
                &[0x82, 0x01],
                Instant::now(),
            )
            .unwrap();
        assert_eq!(output.writes.len(), 1);
        let pdu = pdu::decode_network(
            &stack.keys,
            IV_INDEX,
            &output.writes[0].1,
            NonceKind::Network,
        )
        .unwrap();
        assert_eq!(
            (pdu.seq, pdu.ttl, pdu.src, pdu.dst),
            (10, 7, 0x0003, 0x1201)
        );
        assert_eq!(stack.network.seq, 10 + SEQ_BLOCK);
        assert!(
            stack
                .send(
                    0x0042,
                    KeyRef::Device,
                    None,
                    &[0x80, 0x08, 0x00],
                    Instant::now()
                )
                .is_err()
        );
        assert!(
            stack
                .send(
                    0x1201,
                    KeyRef::App { index: 3 },
                    None,
                    &[0x82, 0x01],
                    Instant::now()
                )
                .is_err()
        );
        assert!(
            stack
                .send(0x1201, KeyRef::Device, None, &[0; 400], Instant::now())
                .is_err()
        );
    }

    #[test]
    fn a_beacon_brings_the_iv_index_and_starts_the_sequence_over() {
        let mut stack = stack(0x0003, 0x1201);
        stack.network.iv_index = 0x1234_5677;
        stack.seq = 500;
        let output = stack.receive(
            proxy_type::MESH_BEACON,
            &bytes("01003ecaff672f673370123456788ea261582f364f6f"),
            Instant::now(),
        );
        assert_eq!(
            output.events,
            [StackEvent::Iv {
                iv_index: IV_INDEX,
                iv_update: false
            }]
        );
        assert_eq!((stack.network.iv_index, stack.seq), (IV_INDEX, 0));
    }

    #[test]
    fn the_proxy_is_asked_for_what_is_addressed_to_blenima() {
        let mut stack = stack(0x0003, 0x1201);
        let output = stack.proxy_setup().unwrap();
        let messages: Vec<_> = output
            .writes
            .iter()
            .map(|(kind, pdu)| {
                assert_eq!(*kind, proxy_type::CONFIGURATION);
                to_hex(
                    &pdu::decode_network(&stack.keys, IV_INDEX, pdu, NonceKind::Proxy)
                        .unwrap()
                        .transport,
                )
            })
            .collect();
        assert_eq!(messages, ["0000", "010003"]);
    }

    #[test]
    fn a_proxy_of_the_network_is_known_by_its_advert() {
        let stack = stack(0x0003, 0x1201);
        // 8.6.1 and 8.6.2: the Network ID, and node 0x1201's identity.
        assert_eq!(
            stack.proxy_of(&bytes("003ecaff672f673370")),
            Some(ProxyOf::Network)
        );
        assert_eq!(
            stack.proxy_of(&bytes("0100861765aefcc57b34ae608fbbc1f2c6")),
            Some(ProxyOf::Node { address: 0x1201 })
        );
        assert_eq!(
            stack.proxy_of(&bytes("0100861765aefcc57b34ae608fbbc1f2c7")),
            None
        );
        assert_eq!(stack.proxy_of(&bytes("003ecaff672f673371")), None);
    }

    #[test]
    fn composition_data_and_a_reset_are_remembered() {
        let mut stack = stack(0x1201, 0x0003);
        let mut output = Output::default();
        let message = |payload: &str| Received {
            src: 0x0003,
            dst: 0x1201,
            ttl: 5,
            key: KeyRef::Device,
            payload: bytes(payload),
        };
        stack.deliver(message("0200590000000000"), &mut output);
        assert_eq!(stack.network.nodes[0].composition, "590000000000");
        assert!(output.events.contains(&StackEvent::NetworkChanged));
        stack.deliver(message("804a"), &mut output);
        assert!(stack.network.nodes.is_empty());
    }
}
