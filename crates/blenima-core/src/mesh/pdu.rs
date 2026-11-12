//! The mesh's PDUs, layer by layer, as the Mesh Protocol spec gives them: the
//! network PDU, encrypted and obfuscated (3.4.4); the lower transport, with
//! its segments and their acknowledgements (3.5.2); the upper transport, an
//! access message encrypted with an application or a device key (3.6.4); the
//! proxy PDU that carries them over GATT (6.3); proxy configuration messages
//! (6.5); and the secure network beacon (3.10.3). Each is checked against the
//! spec's sample data (8.3, 8.4, 8.8 and 8.9).

use super::crypto::{self, Key, Mic, NetworkKeys, Nonce, aes_ecb, ccm_decrypt, ccm_encrypt};

/// What a network PDU says before it is encrypted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkPdu {
    /// A control message, rather than an access message.
    pub ctl: bool,
    pub ttl: u8,
    /// The sequence number, 24 bits.
    pub seq: u32,
    pub src: u16,
    pub dst: u16,
    /// The lower transport PDU, or a proxy configuration message.
    pub transport: Vec<u8>,
}

/// The nonce a network PDU is encrypted with: a network one, or a proxy one
/// for proxy configuration messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonceKind {
    Network,
    Proxy,
}

fn seq_bytes(seq: u32) -> [u8; 3] {
    let bytes = seq.to_be_bytes();
    [bytes[1], bytes[2], bytes[3]]
}

/// The network nonce (3.9.5.1), or the proxy nonce (3.9.5.4).
pub fn network_nonce(
    kind: NonceKind,
    ctl: bool,
    ttl: u8,
    seq: u32,
    src: u16,
    iv_index: u32,
) -> Nonce {
    let mut nonce = [0u8; 13];
    match kind {
        NonceKind::Network => {
            nonce[0] = 0x00;
            nonce[1] = (u8::from(ctl) << 7) | (ttl & 0x7F);
        }
        NonceKind::Proxy => nonce[0] = 0x03,
    }
    nonce[2..5].copy_from_slice(&seq_bytes(seq));
    nonce[5..7].copy_from_slice(&src.to_be_bytes());
    nonce[9..13].copy_from_slice(&iv_index.to_be_bytes());
    nonce
}

/// The block the privacy key encrypts to hide a network PDU's header: zeros,
/// the IV index, and the first 7 bytes after the header.
fn pecb(privacy: &Key, iv_index: u32, privacy_random: &[u8]) -> Key {
    let mut block = [0u8; 16];
    block[5..9].copy_from_slice(&iv_index.to_be_bytes());
    block[9..16].copy_from_slice(&privacy_random[..7]);
    aes_ecb(privacy, &block)
}

/// A network PDU, encrypted with the network's encryption key and its header
/// obfuscated with its privacy key.
pub fn encode_network(
    keys: &NetworkKeys,
    iv_index: u32,
    pdu: &NetworkPdu,
    kind: NonceKind,
) -> Vec<u8> {
    let nonce = network_nonce(kind, pdu.ctl, pdu.ttl, pdu.seq, pdu.src, iv_index);
    let mut plain = pdu.dst.to_be_bytes().to_vec();
    plain.extend_from_slice(&pdu.transport);
    let mic = if pdu.ctl { Mic::Long } else { Mic::Short };
    let sealed = ccm_encrypt(&keys.encryption, &nonce, &plain, &[], mic);
    let pecb = pecb(&keys.privacy, iv_index, &sealed);
    let mut header = [0u8; 6];
    header[0] = (u8::from(pdu.ctl) << 7) | (pdu.ttl & 0x7F);
    header[1..4].copy_from_slice(&seq_bytes(pdu.seq));
    header[4..6].copy_from_slice(&pdu.src.to_be_bytes());
    let mut out = vec![(((iv_index & 1) as u8) << 7) | keys.nid];
    out.extend(header.iter().zip(pecb.iter()).map(|(a, b)| a ^ b));
    out.extend_from_slice(&sealed);
    out
}

/// The IV index a network PDU was sent with, from its first byte's IVI bit:
/// the one in use, or the one before it, as during an IV update.
pub fn rx_iv_index(iv_index: u32, first: u8) -> Option<u32> {
    if iv_index & 1 == u32::from(first >> 7) {
        Some(iv_index)
    } else {
        iv_index.checked_sub(1)
    }
}

/// A network PDU taken apart: none when it is not of this network, or its
/// NetMIC does not match.
pub fn decode_network(
    keys: &NetworkKeys,
    iv_index: u32,
    data: &[u8],
    kind: NonceKind,
) -> Option<NetworkPdu> {
    // Header, the shortest transport PDU and the shortest NetMIC.
    if data.len() < 7 + 2 + 1 + 4 || data[0] & 0x7F != keys.nid {
        return None;
    }
    let iv_index = rx_iv_index(iv_index, data[0])?;
    let sealed = &data[7..];
    let pecb = pecb(&keys.privacy, iv_index, sealed);
    let header: Vec<u8> = data[1..7]
        .iter()
        .zip(pecb.iter())
        .map(|(a, b)| a ^ b)
        .collect();
    let ctl = header[0] & 0x80 != 0;
    let ttl = header[0] & 0x7F;
    let seq = u32::from_be_bytes([0, header[1], header[2], header[3]]);
    let src = u16::from_be_bytes([header[4], header[5]]);
    let nonce = network_nonce(kind, ctl, ttl, seq, src, iv_index);
    let mic = if ctl { Mic::Long } else { Mic::Short };
    let plain = ccm_decrypt(&keys.encryption, &nonce, sealed, &[], mic)?;
    if plain.len() < 3 {
        return None;
    }
    Some(NetworkPdu {
        ctl,
        ttl,
        seq,
        src,
        dst: u16::from_be_bytes([plain[0], plain[1]]),
        transport: plain[2..].to_vec(),
    })
}

/// The most an unsegmented access message's upper transport PDU holds.
pub const UNSEGMENTED_MAX: usize = 15;

/// The most one segment of a segmented access message holds.
pub const SEGMENT_SIZE: usize = 12;

/// The most segments a message can have: SegN is 5 bits.
pub const SEGMENTS_MAX: usize = 32;

/// A lower transport PDU, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lower {
    /// An access message in one PDU.
    Access { akf: bool, aid: u8, upper: Vec<u8> },
    /// One segment of a segmented access message.
    Segment {
        akf: bool,
        aid: u8,
        /// A 64-bit TransMIC.
        szmic: bool,
        seq_zero: u16,
        seg_o: u8,
        seg_n: u8,
        data: Vec<u8>,
    },
    /// A Segment Acknowledgment: which segments of a message arrived.
    Ack {
        obo: bool,
        seq_zero: u16,
        block_ack: u32,
    },
    /// Any other control message in one PDU.
    Control { opcode: u8, params: Vec<u8> },
    /// A segment of a segmented control message, which bleNIMA does not use.
    ControlSegment,
}

/// The header byte of an access message: SEG, AKF and AID.
fn access_header(segmented: bool, akf: bool, aid: u8) -> u8 {
    (u8::from(segmented) << 7) | (u8::from(akf) << 6) | (aid & 0x3F)
}

/// An unsegmented access message's lower transport PDU.
pub fn unsegmented_access(akf: bool, aid: u8, upper: &[u8]) -> Vec<u8> {
    let mut out = vec![access_header(false, akf, aid)];
    out.extend_from_slice(upper);
    out
}

/// A segmented access message's lower transport PDUs, one a segment.
pub fn segmented_access(
    akf: bool,
    aid: u8,
    szmic: bool,
    seq_zero: u16,
    upper: &[u8],
) -> Vec<Vec<u8>> {
    let pieces: Vec<&[u8]> = upper.chunks(SEGMENT_SIZE).collect();
    let seg_n = (pieces.len() - 1) as u32;
    pieces
        .iter()
        .enumerate()
        .map(|(seg_o, piece)| {
            let fields = (u32::from(szmic) << 23)
                | (u32::from(seq_zero & 0x1FFF) << 10)
                | ((seg_o as u32 & 0x1F) << 5)
                | (seg_n & 0x1F);
            let mut out = vec![access_header(true, akf, aid)];
            out.extend_from_slice(&fields.to_be_bytes()[1..]);
            out.extend_from_slice(piece);
            out
        })
        .collect()
}

/// A Segment Acknowledgment's transport PDU.
pub fn segment_ack(obo: bool, seq_zero: u16, block_ack: u32) -> Vec<u8> {
    let fields = (u16::from(obo) << 15) | ((seq_zero & 0x1FFF) << 2);
    let mut out = vec![0x00];
    out.extend_from_slice(&fields.to_be_bytes());
    out.extend_from_slice(&block_ack.to_be_bytes());
    out
}

/// Reads a lower transport PDU; none when it is too short to be one.
pub fn parse_lower(ctl: bool, transport: &[u8]) -> Option<Lower> {
    let (&first, rest) = transport.split_first()?;
    let segmented = first & 0x80 != 0;
    if !ctl {
        let akf = first & 0x40 != 0;
        let aid = first & 0x3F;
        if !segmented {
            return Some(Lower::Access {
                akf,
                aid,
                upper: rest.to_vec(),
            });
        }
        if rest.len() < 4 {
            return None;
        }
        let fields = u32::from_be_bytes([0, rest[0], rest[1], rest[2]]);
        return Some(Lower::Segment {
            akf,
            aid,
            szmic: fields & 0x80_0000 != 0,
            seq_zero: ((fields >> 10) & 0x1FFF) as u16,
            seg_o: ((fields >> 5) & 0x1F) as u8,
            seg_n: (fields & 0x1F) as u8,
            data: rest[3..].to_vec(),
        });
    }
    let opcode = first & 0x7F;
    if segmented {
        return Some(Lower::ControlSegment);
    }
    if opcode == 0x00 {
        if rest.len() < 6 {
            return None;
        }
        let fields = u16::from_be_bytes([rest[0], rest[1]]);
        return Some(Lower::Ack {
            obo: fields & 0x8000 != 0,
            seq_zero: (fields >> 2) & 0x1FFF,
            block_ack: u32::from_be_bytes([rest[2], rest[3], rest[4], rest[5]]),
        });
    }
    Some(Lower::Control {
        opcode,
        params: rest.to_vec(),
    })
}

/// The sequence number a segmented message's nonce takes, SeqAuth's lowest 24
/// bits, from one segment's sequence number and its SeqZero.
pub fn seq_auth(seq: u32, seq_zero: u16) -> u32 {
    seq.wrapping_sub(seq.wrapping_sub(u32::from(seq_zero)) & 0x1FFF) & 0xFF_FFFF
}

/// Which key an access message is encrypted with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessKey<'a> {
    Application(&'a Key),
    Device(&'a Key),
}

/// The application nonce (3.9.5.2), or the device nonce (3.9.5.3).
pub fn access_nonce(
    device: bool,
    aszmic: bool,
    seq: u32,
    src: u16,
    dst: u16,
    iv_index: u32,
) -> Nonce {
    let mut nonce = [0u8; 13];
    nonce[0] = if device { 0x02 } else { 0x01 };
    nonce[1] = u8::from(aszmic) << 7;
    nonce[2..5].copy_from_slice(&seq_bytes(seq));
    nonce[5..7].copy_from_slice(&src.to_be_bytes());
    nonce[7..9].copy_from_slice(&dst.to_be_bytes());
    nonce[9..13].copy_from_slice(&iv_index.to_be_bytes());
    nonce
}

/// Where an access message goes and from where, for its nonce: `seq` is its
/// SeqAuth's for a segmented one. `label` is a virtual address's Label UUID.
#[derive(Clone, Copy, Debug)]
pub struct AccessPlace<'a> {
    pub seq: u32,
    pub src: u16,
    pub dst: u16,
    pub iv_index: u32,
    pub label: Option<&'a [u8; 16]>,
}

/// An access message encrypted into an upper transport PDU, with its TransMIC.
pub fn encrypt_access(key: AccessKey, place: AccessPlace, access: &[u8], mic: Mic) -> Vec<u8> {
    let (key, device) = match key {
        AccessKey::Application(key) => (key, false),
        AccessKey::Device(key) => (key, true),
    };
    let nonce = access_nonce(
        device,
        mic == Mic::Long,
        place.seq,
        place.src,
        place.dst,
        place.iv_index,
    );
    ccm_encrypt(
        key,
        &nonce,
        access,
        place.label.map_or(&[][..], |label| &label[..]),
        mic,
    )
}

/// An upper transport PDU decrypted into its access message; none when its
/// TransMIC does not match.
pub fn decrypt_access(
    key: AccessKey,
    place: AccessPlace,
    upper: &[u8],
    mic: Mic,
) -> Option<Vec<u8>> {
    let (key, device) = match key {
        AccessKey::Application(key) => (key, false),
        AccessKey::Device(key) => (key, true),
    };
    let nonce = access_nonce(
        device,
        mic == Mic::Long,
        place.seq,
        place.src,
        place.dst,
        place.iv_index,
    );
    ccm_decrypt(
        key,
        &nonce,
        upper,
        place.label.map_or(&[][..], |label| &label[..]),
        mic,
    )
}

/// What a proxy PDU carries (6.3.1).
pub mod proxy_type {
    pub const NETWORK_PDU: u8 = 0x00;
    pub const MESH_BEACON: u8 = 0x01;
    pub const CONFIGURATION: u8 = 0x02;
    pub const PROVISIONING: u8 = 0x03;
}

/// A message as proxy PDUs, each `room` bytes at most (the MTU less 3): one
/// whole, or in a first piece, middle ones and a last.
pub fn proxy_pieces(kind: u8, message: &[u8], room: usize) -> Vec<Vec<u8>> {
    let room = room.max(2) - 1;
    if message.len() <= room {
        let mut out = vec![kind & 0x3F];
        out.extend_from_slice(message);
        return vec![out];
    }
    let chunks: Vec<&[u8]> = message.chunks(room).collect();
    let last = chunks.len() - 1;
    chunks
        .iter()
        .enumerate()
        .map(|(at, chunk)| {
            let sar = match at {
                0 => 0b01,
                n if n == last => 0b11,
                _ => 0b10,
            };
            let mut out = vec![(sar << 6) | (kind & 0x3F)];
            out.extend_from_slice(chunk);
            out
        })
        .collect()
}

/// Puts proxy PDUs back together into whole messages.
#[derive(Default)]
pub struct ProxyJoiner {
    kind: Option<u8>,
    buffer: Vec<u8>,
}

impl ProxyJoiner {
    /// Takes one proxy PDU; gives a message, with its type, once it is whole.
    pub fn push(&mut self, piece: &[u8]) -> Option<(u8, Vec<u8>)> {
        let (&first, rest) = piece.split_first()?;
        let kind = first & 0x3F;
        match first >> 6 {
            0b00 => {
                self.kind = None;
                self.buffer.clear();
                Some((kind, rest.to_vec()))
            }
            0b01 => {
                self.kind = Some(kind);
                self.buffer = rest.to_vec();
                None
            }
            sar => {
                // A piece of another message, or with no first piece, is dropped.
                if self.kind != Some(kind) {
                    self.kind = None;
                    self.buffer.clear();
                    return None;
                }
                self.buffer.extend_from_slice(rest);
                if sar == 0b11 {
                    self.kind = None;
                    return Some((kind, std::mem::take(&mut self.buffer)));
                }
                None
            }
        }
    }
}

/// Proxy configuration messages, from a proxy client to its proxy (6.5).
pub mod proxy_config {
    pub const SET_FILTER_TYPE: u8 = 0x00;
    pub const ADD_ADDRESSES: u8 = 0x01;
    pub const REMOVE_ADDRESSES: u8 = 0x02;
    pub const FILTER_STATUS: u8 = 0x03;
    /// Only the addresses in the list are forwarded to the client.
    pub const ACCEPT_LIST: u8 = 0x00;
    /// Everything but the addresses in the list is forwarded.
    pub const REJECT_LIST: u8 = 0x01;
}

/// A proxy configuration message as a network PDU: CTL set, TTL 0, to the
/// unassigned address, with the proxy nonce.
pub fn proxy_config_pdu(
    keys: &NetworkKeys,
    iv_index: u32,
    seq: u32,
    src: u16,
    message: &[u8],
) -> Vec<u8> {
    let pdu = NetworkPdu {
        ctl: true,
        ttl: 0,
        seq,
        src,
        dst: 0x0000,
        transport: message.to_vec(),
    };
    encode_network(keys, iv_index, &pdu, NonceKind::Proxy)
}

/// A secure network beacon, checked against the network's beacon key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecureBeacon {
    pub key_refresh: bool,
    pub iv_update: bool,
    pub iv_index: u32,
}

/// Reads a secure network beacon: none when it is not one, or not of this network.
pub fn parse_secure_beacon(
    beacon_key: &Key,
    network_id: &[u8; 8],
    data: &[u8],
) -> Option<SecureBeacon> {
    if data.len() != 22 || data[0] != 0x01 || &data[2..10] != network_id {
        return None;
    }
    let auth = crypto::aes_cmac(beacon_key, &data[1..14]);
    if auth[..8] != data[14..22] {
        return None;
    }
    Some(SecureBeacon {
        key_refresh: data[1] & 0x01 != 0,
        iv_update: data[1] & 0x02 != 0,
        iv_index: u32::from_be_bytes([data[10], data[11], data[12], data[13]]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::crypto::tests::{bytes, key};
    use crate::mesh::crypto::{k2, k4, to_hex};

    const IV_INDEX: u32 = 0x1234_5678;

    fn keys() -> NetworkKeys {
        k2(&key("7dd7364cd842ad18c17c2b820c84c3d6"), &[0x00])
    }

    #[test]
    fn a_control_message_is_the_specs_message_1() {
        // A Friend Request: CTL set, so a 64-bit NetMIC.
        let pdu = NetworkPdu {
            ctl: true,
            ttl: 0,
            seq: 1,
            src: 0x1201,
            dst: 0xFFFD,
            transport: bytes("034b50057e400000010000"),
        };
        assert_eq!(
            to_hex(&network_nonce(
                NonceKind::Network,
                true,
                0,
                1,
                0x1201,
                IV_INDEX
            )),
            "00800000011201000012345678"
        );
        let packet = encode_network(&keys(), IV_INDEX, &pdu, NonceKind::Network);
        assert_eq!(
            to_hex(&packet),
            "68eca487516765b5e5bfdacbaf6cb7fb6bff871f035444ce83a670df"
        );
        assert_eq!(
            decode_network(&keys(), IV_INDEX, &packet, NonceKind::Network),
            Some(pdu)
        );
    }

    #[test]
    fn config_appkey_add_goes_in_the_specs_two_segments_message_6() {
        let dev_key = key("9d6dd0e96eb25dc19a40ed9914f8f03f");
        let access = bytes("0056341263964771734fbd76e3b40519d1d94a48");
        let place = AccessPlace {
            seq: 0x3129AB,
            src: 0x0003,
            dst: 0x1201,
            iv_index: IV_INDEX,
            label: None,
        };
        assert_eq!(
            to_hex(&access_nonce(
                true, false, 0x3129AB, 0x0003, 0x1201, IV_INDEX
            )),
            "02003129ab0003120112345678"
        );
        let upper = encrypt_access(AccessKey::Device(&dev_key), place, &access, Mic::Short);
        assert_eq!(
            to_hex(&upper),
            "ee9dddfd2169326d23f3afdfcfdc18c52fdef772e0e17308"
        );
        let segments = segmented_access(false, 0, false, (0x3129AB & 0x1FFF) as u16, &upper);
        assert_eq!(
            segments.iter().map(|s| to_hex(s)).collect::<Vec<_>>(),
            [
                "8026ac01ee9dddfd2169326d23f3afdf",
                "8026ac21cfdc18c52fdef772e0e17308"
            ]
        );
        let packets: Vec<String> = segments
            .iter()
            .zip([0x3129AB, 0x3129AC])
            .map(|(segment, seq)| {
                let pdu = NetworkPdu {
                    ctl: false,
                    ttl: 4,
                    seq,
                    src: 0x0003,
                    dst: 0x1201,
                    transport: segment.clone(),
                };
                to_hex(&encode_network(&keys(), IV_INDEX, &pdu, NonceKind::Network))
            })
            .collect();
        assert_eq!(
            packets,
            [
                "68cab5c5348a230afba8c63d4e686364979deaf4fd40961145939cda0e",
                "681615b5dd4a846cae0c032bf0746f44f1b8cc8ce5edc57e55beed49c0"
            ]
        );
        // And back: the second segment's SeqAuth is the first's sequence number.
        let Some(Lower::Segment {
            seq_zero,
            seg_o,
            seg_n,
            ..
        }) = parse_lower(false, &segments[1])
        else {
            panic!("a segment");
        };
        assert_eq!((seq_zero, seg_o, seg_n), (0x09AB, 1, 1));
        assert_eq!(seq_auth(0x3129AC, seq_zero), 0x3129AB);
        assert_eq!(
            decrypt_access(AccessKey::Device(&dev_key), place, &upper, Mic::Short),
            Some(access)
        );
    }

    #[test]
    fn a_segment_acknowledgment_is_the_specs_message_7() {
        let ack = segment_ack(true, 0x09AB, 0x0000_0002);
        assert_eq!(to_hex(&ack), "00a6ac00000002");
        assert_eq!(
            parse_lower(true, &ack),
            Some(Lower::Ack {
                obo: true,
                seq_zero: 0x09AB,
                block_ack: 2
            })
        );
        let pdu = NetworkPdu {
            ctl: true,
            ttl: 0x0B,
            seq: 0x014835,
            src: 0x2345,
            dst: 0x0003,
            transport: ack,
        };
        assert_eq!(
            to_hex(&encode_network(&keys(), IV_INDEX, &pdu, NonceKind::Network)),
            "68e476b5579c980d0d730f94d7f3509df987bb417eb7c05f"
        );
    }

    #[test]
    fn config_appkey_status_is_read_from_the_specs_message_16() {
        let packet = bytes("68e80e5da5af0e6b9be7f5a642f2f98680e61c3a8b47f228");
        let pdu = decode_network(&keys(), IV_INDEX, &packet, NonceKind::Network).expect("ours");
        assert_eq!(
            (pdu.ttl, pdu.seq, pdu.src, pdu.dst),
            (0x0B, 6, 0x1201, 0x0003)
        );
        let Some(Lower::Access {
            akf: false, upper, ..
        }) = parse_lower(pdu.ctl, &pdu.transport)
        else {
            panic!("an unsegmented access message");
        };
        let place = AccessPlace {
            seq: pdu.seq,
            src: pdu.src,
            dst: pdu.dst,
            iv_index: IV_INDEX,
            label: None,
        };
        let dev_key = key("9d6dd0e96eb25dc19a40ed9914f8f03f");
        let access = decrypt_access(AccessKey::Device(&dev_key), place, &upper, Mic::Short);
        assert_eq!(access.map(|a| to_hex(&a)), Some("800300563412".into()));
    }

    #[test]
    fn health_current_status_with_an_app_key_is_the_specs_message_18() {
        let app_key = key("63964771734fbd76e3b40519d1d94a48");
        let place = AccessPlace {
            seq: 7,
            src: 0x1201,
            dst: 0xFFFF,
            iv_index: IV_INDEX,
            label: None,
        };
        let upper = encrypt_access(
            AccessKey::Application(&app_key),
            place,
            &bytes("0400000000"),
            Mic::Short,
        );
        assert_eq!(to_hex(&upper), "5a8bde6d9106ea078a");
        let lower = unsegmented_access(true, k4(&app_key), &upper);
        assert_eq!(to_hex(&lower), "665a8bde6d9106ea078a");
        let pdu = NetworkPdu {
            ctl: false,
            ttl: 3,
            seq: 7,
            src: 0x1201,
            dst: 0xFFFF,
            transport: lower,
        };
        assert_eq!(
            to_hex(&encode_network(&keys(), IV_INDEX, &pdu, NonceKind::Network)),
            "6848cba437860e5673728a627fb938535508e21a6baf57"
        );
    }

    #[test]
    fn a_virtual_address_puts_its_label_in_the_mic_as_message_22() {
        let app_key = key("63964771734fbd76e3b40519d1d94a48");
        let label: [u8; 16] = bytes("0073e7e4d8b9440faf8415df4c56c0e1")
            .try_into()
            .unwrap();
        let place = AccessPlace {
            seq: 0x07080B,
            src: 0x1234,
            dst: 0xB529,
            iv_index: 0x1234_5677,
            label: Some(&label),
        };
        let upper = encrypt_access(
            AccessKey::Application(&app_key),
            place,
            &bytes("d50a0048656c6c6f"),
            Mic::Short,
        );
        assert_eq!(to_hex(&upper), "3871b904d431526316ca48a0");
        let pdu = NetworkPdu {
            ctl: false,
            ttl: 3,
            seq: 0x07080B,
            src: 0x1234,
            dst: 0xB529,
            transport: unsegmented_access(true, 0x26, &upper),
        };
        let packet = encode_network(&keys(), 0x1234_5677, &pdu, NonceKind::Network);
        assert_eq!(
            to_hex(&packet),
            "e8d85caecef1e3ed31f3fdcf88a411135fea55df730b6b28e255"
        );
        // A PDU of the IV index before is read while the next is in use, as during an update.
        assert_eq!(
            decode_network(&keys(), 0x1234_5678, &packet, NonceKind::Network),
            Some(pdu)
        );
    }

    #[test]
    fn a_proxy_configuration_message_is_the_specs_set_filter_type() {
        let keys = k2(&key("d1aafb2a1a3c281cbdb0e960edfad852"), &[0x00]);
        assert_eq!(keys.nid, 0x10);
        assert_eq!(
            to_hex(&network_nonce(
                NonceKind::Proxy,
                true,
                0,
                1,
                0x0001,
                IV_INDEX
            )),
            "03000000010001000012345678"
        );
        let message = [proxy_config::SET_FILTER_TYPE, proxy_config::ACCEPT_LIST];
        let pdu = proxy_config_pdu(&keys, IV_INDEX, 1, 0x0001, &message);
        assert_eq!(to_hex(&pdu), "10386bd60efbbb8b8c28512e792d3711f4b526");
        let pieces = proxy_pieces(proxy_type::CONFIGURATION, &pdu, 20);
        assert_eq!(pieces.len(), 1);
        assert_eq!(
            to_hex(&pieces[0]),
            "0210386bd60efbbb8b8c28512e792d3711f4b526"
        );
        let back = decode_network(&keys, IV_INDEX, &pdu, NonceKind::Proxy).unwrap();
        assert_eq!(back.transport, message);
    }

    #[test]
    fn a_long_message_goes_over_gatt_in_the_specs_pieces() {
        // The Provisioning Public Key PDU at an MTU of 23: 20 bytes a write.
        let pdu = bytes(
            "032c31a47b5779809ef44cb5eaaf5c3e43d5f8faad4a8794cb987e9b03745c78dd\
             919512183898dfbecd52e2408e43871fd021109117bd3ed4eaf8437743715d4f",
        );
        let pieces = proxy_pieces(proxy_type::PROVISIONING, &pdu, 20);
        assert_eq!(
            pieces.iter().map(|p| to_hex(p)).collect::<Vec<_>>(),
            [
                "43032c31a47b5779809ef44cb5eaaf5c3e43d5f8",
                "83faad4a8794cb987e9b03745c78dd9195121838",
                "8398dfbecd52e2408e43871fd021109117bd3ed4",
                "c3eaf8437743715d4f"
            ]
        );
        let mut joiner = ProxyJoiner::default();
        let mut whole = None;
        for piece in &pieces {
            whole = joiner.push(piece);
        }
        assert_eq!(whole, Some((proxy_type::PROVISIONING, pdu)));
    }

    #[test]
    fn a_secure_network_beacon_is_checked_as_the_specs() {
        let net_key = key("7dd7364cd842ad18c17c2b820c84c3d6");
        let beacon = bytes("01003ecaff672f673370123456788ea261582f364f6f");
        let network_id = crypto::k3(&net_key);
        let read = parse_secure_beacon(&crypto::beacon_key(&net_key), &network_id, &beacon);
        assert_eq!(
            read,
            Some(SecureBeacon {
                key_refresh: false,
                iv_update: false,
                iv_index: 0x1234_5678
            })
        );
        let mut forged = beacon.clone();
        forged[13] ^= 1;
        assert_eq!(
            parse_secure_beacon(&crypto::beacon_key(&net_key), &network_id, &forged),
            None
        );
    }
}
