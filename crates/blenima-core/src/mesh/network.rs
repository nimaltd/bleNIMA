//! The mesh network as bleNIMA keeps it, in `mesh.toml` in the config folder:
//! its keys, its IV index, bleNIMA's own address and sequence number, and the
//! nodes it provisioned, with their device keys.

use std::io;
use std::path::Path;

use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};

use super::crypto::{Key, key_from_hex, to_hex};
use crate::tomlfile;

const HEADER: &str = "# bleNIMA's mesh network: its keys and the nodes it provisioned.\n\
                      # Keep it to yourself: the keys let anyone into the network.\n\n";

/// The last unicast address: the ones above it are group and virtual addresses.
pub const UNICAST_MAX: u16 = 0x7FFF;

/// A network key or an application key that bleNIMA made, at random.
fn random_key() -> String {
    let mut key = [0u8; 16];
    OsRng.fill_bytes(&mut key);
    to_hex(&key)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppKey {
    pub index: u16,
    /// 32 hex digits.
    pub key: String,
}

/// A node bleNIMA provisioned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    /// Its primary element's address: the others follow it.
    pub address: u16,
    pub elements: u8,
    /// 32 hex digits: what its configuration is sent with.
    pub device_key: String,
    /// Its Device UUID, from its advert: 32 hex digits.
    #[serde(default)]
    pub uuid: String,
    /// What the user calls it.
    #[serde(default)]
    pub name: String,
    /// Its Composition Data page 0, as hex, once it was read.
    #[serde(default)]
    pub composition: String,
}

impl Node {
    /// Whether the address is one of its elements'.
    pub fn has(&self, address: u16) -> bool {
        address >= self.address
            && u32::from(address) < u32::from(self.address) + u32::from(self.elements.max(1))
    }

    pub fn device_key(&self) -> Option<Key> {
        key_from_hex(&self.device_key)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Network {
    /// The primary network key, 32 hex digits.
    pub net_key: String,
    pub net_key_index: u16,
    pub app_keys: Vec<AppKey>,
    pub iv_index: u32,
    /// bleNIMA's own unicast address, as the provisioner and the client.
    pub address: u16,
    /// The sequence number bleNIMA may go on from. It is written ahead of
    /// use, so one is never sent twice, even after a crash.
    pub seq: u32,
    /// The TTL messages are sent with.
    pub ttl: u8,
    pub nodes: Vec<Node>,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            net_key: String::new(),
            net_key_index: 0,
            app_keys: Vec::new(),
            iv_index: 0,
            address: 0x0001,
            seq: 0,
            ttl: 7,
            nodes: Vec::new(),
        }
    }
}

impl Network {
    /// A new network: new keys, no nodes.
    pub fn random() -> Self {
        Self {
            net_key: random_key(),
            app_keys: vec![AppKey {
                index: 0,
                key: random_key(),
            }],
            ..Self::default()
        }
    }

    /// The network in `path`; a new one, saved there, when there is none yet.
    pub fn load(path: &Path) -> io::Result<Self> {
        if !path.exists() {
            let network = Self::random();
            network.save(path)?;
            return Ok(network);
        }
        let mut network: Self = tomlfile::load(path);
        if network.fix() {
            network.save(path)?;
        }
        Ok(network)
    }

    /// Makes good what a hand edit broke: a key that is not 32 hex digits is
    /// made anew. Says whether it changed anything.
    fn fix(&mut self) -> bool {
        let mut changed = false;
        if key_from_hex(&self.net_key).is_none() {
            self.net_key = random_key();
            changed = true;
        }
        for app_key in &mut self.app_keys {
            if key_from_hex(&app_key.key).is_none() {
                app_key.key = random_key();
                changed = true;
            }
        }
        if self.app_keys.is_empty() {
            self.app_keys.push(AppKey {
                index: 0,
                key: random_key(),
            });
            changed = true;
        }
        if self.address == 0 || self.address > UNICAST_MAX {
            self.address = 0x0001;
            changed = true;
        }
        let before = self.nodes.len();
        self.nodes
            .retain(|node| node.device_key().is_some() && node.address != 0);
        changed || self.nodes.len() != before
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        tomlfile::save(self, path, HEADER)
    }

    pub fn net_key(&self) -> Key {
        key_from_hex(&self.net_key).unwrap_or_default()
    }

    pub fn app_key(&self, index: u16) -> Option<Key> {
        self.app_keys
            .iter()
            .find(|app_key| app_key.index == index)
            .and_then(|app_key| key_from_hex(&app_key.key))
    }

    /// The node one of whose elements has this address.
    pub fn node(&self, address: u16) -> Option<&Node> {
        self.nodes.iter().find(|node| node.has(address))
    }

    pub fn node_mut(&mut self, address: u16) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|node| node.has(address))
    }

    /// The first address with room for a node of `elements` elements after
    /// bleNIMA's own and the nodes', or none when the network is full.
    pub fn next_address(&self, elements: u8) -> Option<u16> {
        let size = u32::from(elements.max(1));
        let mut taken: Vec<(u32, u32)> = self
            .nodes
            .iter()
            .map(|node| (u32::from(node.address), u32::from(node.elements.max(1))))
            .collect();
        taken.push((u32::from(self.address), 1));
        taken.sort();
        let mut candidate = 1;
        for (start, length) in taken {
            if candidate + size <= start {
                break;
            }
            candidate = candidate.max(start + length);
        }
        (candidate + size - 1 <= u32::from(UNICAST_MAX)).then_some(candidate as u16)
    }

    /// Adds a node, in place of any that had its addresses.
    pub fn add_node(&mut self, node: Node) {
        let end = u32::from(node.address) + u32::from(node.elements.max(1));
        self.nodes.retain(|other| {
            let other_end = u32::from(other.address) + u32::from(other.elements.max(1));
            other_end <= u32::from(node.address) || u32::from(other.address) >= end
        });
        self.nodes.push(node);
        self.nodes.sort_by_key(|node| node.address);
    }

    pub fn remove_node(&mut self, address: u16) {
        self.nodes.retain(|node| !node.has(address));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tomlfile::tests::TempDir;

    fn node(address: u16, elements: u8) -> Node {
        Node {
            address,
            elements,
            device_key: "9d6dd0e96eb25dc19a40ed9914f8f03f".into(),
            uuid: String::new(),
            name: String::new(),
            composition: String::new(),
        }
    }

    #[test]
    fn a_new_network_has_its_own_keys_and_is_saved() {
        let dir = TempDir::new("mesh-new");
        let path = dir.file("mesh.toml");
        let network = Network::load(&path).unwrap();
        assert!(key_from_hex(&network.net_key).is_some());
        assert_eq!(network.app_keys.len(), 1);
        assert_eq!(network.address, 0x0001);
        assert_eq!(Network::load(&path).unwrap(), network);
        assert_ne!(Network::random().net_key, network.net_key);
    }

    #[test]
    fn a_broken_key_is_made_anew_and_the_rest_kept() {
        let dir = TempDir::new("mesh-fix");
        let path = dir.file("mesh.toml");
        std::fs::write(
            &path,
            "net_key = \"12\"\niv_index = 5\n[[nodes]]\naddress = 2\nelements = 1\ndevice_key = \"zz\"\n",
        )
        .unwrap();
        let network = Network::load(&path).unwrap();
        assert!(key_from_hex(&network.net_key).is_some());
        assert_eq!(network.iv_index, 5);
        assert_eq!(network.ttl, 7);
        // A node without its device key cannot be talked to.
        assert!(network.nodes.is_empty());
    }

    #[test]
    fn addresses_are_given_in_the_first_gap_that_fits() {
        let mut network = Network::random();
        assert_eq!(network.next_address(1), Some(0x0002));
        network.add_node(node(0x0002, 3));
        network.add_node(node(0x0008, 1));
        assert_eq!(network.next_address(3), Some(0x0005));
        assert_eq!(network.next_address(4), Some(0x0009));
        assert_eq!(network.node(0x0004).map(|n| n.address), Some(0x0002));
        assert_eq!(network.node(0x0005), None);
        network.remove_node(0x0003);
        assert_eq!(network.next_address(4), Some(0x0002));
        // A node put where another one was takes its place.
        network.add_node(node(0x0007, 2));
        assert_eq!(
            network.nodes.iter().map(|n| n.address).collect::<Vec<_>>(),
            [0x0007]
        );
        // Up to the last unicast address, and no further.
        network.address = 0x7FFF;
        network.nodes.clear();
        for address in (0x0001..0x7F81).step_by(255) {
            network.add_node(node(address, 255));
        }
        assert_eq!(network.next_address(126), Some(0x7F81));
        assert_eq!(network.next_address(127), None);
    }
}
