//! The mesh's security functions, as the Mesh Protocol spec defines them (3.9):
//! AES-128, AES-CMAC, AES-CCM and HMAC-SHA-256, and the key derivations s1,
//! s2, k1, k2, k3, k4 and k5 made from them. Each is checked against the
//! spec's own sample data (8.1, 8.2 and, for s2 and k5, 8.17.2).

use aes::Aes128;
use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
use ccm::Ccm;
use ccm::aead::AeadInPlace;
use ccm::consts::{U4, U8, U13};
use cmac::{Cmac, Mac};
use hmac::Hmac;
use sha2::Sha256;

/// A 128-bit key, or any 16-byte block.
pub type Key = [u8; 16];

/// A CCM nonce: 13 bytes, as every mesh nonce is.
pub type Nonce = [u8; 13];

/// AES-CMAC of `data` under `key` (RFC 4493).
pub fn aes_cmac(key: &Key, data: &[u8]) -> Key {
    let mut mac = <Cmac<Aes128> as Mac>::new_from_slice(key).expect("a 16-byte key");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// One block through AES-128: the spec's e().
pub fn aes_ecb(key: &Key, block: &Key) -> Key {
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut out = GenericArray::clone_from_slice(block);
    cipher.encrypt_block(&mut out);
    out.into()
}

/// The salt generation function: AES-CMAC under a zero key.
pub fn s1(m: &[u8]) -> Key {
    aes_cmac(&[0; 16], m)
}

/// The key derivation function k1: N, a salt and P.
pub fn k1(n: &[u8], salt: &Key, p: &[u8]) -> Key {
    let t = aes_cmac(salt, n);
    aes_cmac(&t, p)
}

/// HMAC-SHA-256 of `data` under `key`.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC takes any key");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// The 256-bit salt generation function: HMAC-SHA-256 under a zero key.
pub fn s2(m: &[u8]) -> [u8; 32] {
    hmac_sha256(&[0; 32], m)
}

/// The provisioning material derivation function k5: N, a 256-bit salt and P.
pub fn k5(n: &[u8], salt: &[u8; 32], p: &[u8]) -> [u8; 32] {
    let t = hmac_sha256(salt, n);
    hmac_sha256(&t, p)
}

/// What k2 makes of a network key: the NID and the keys a network PDU is sent with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetworkKeys {
    /// 7 bits, at the start of every network PDU, to tell the network apart.
    pub nid: u8,
    pub encryption: Key,
    pub privacy: Key,
}

/// The network key material derivation function k2, with P 0x00 for the
/// managed flooding credentials.
pub fn k2(n: &Key, p: &[u8]) -> NetworkKeys {
    let t = aes_cmac(&s1(b"smk2"), n);
    let mut input = p.to_vec();
    input.push(0x01);
    let t1 = aes_cmac(&t, &input);
    let mut input = t1.to_vec();
    input.extend_from_slice(p);
    input.push(0x02);
    let t2 = aes_cmac(&t, &input);
    let mut input = t2.to_vec();
    input.extend_from_slice(p);
    input.push(0x03);
    let t3 = aes_cmac(&t, &input);
    NetworkKeys {
        nid: t1[15] & 0x7F,
        encryption: t2,
        privacy: t3,
    }
}

/// The derivation function k3: a network's 64-bit Network ID.
pub fn k3(n: &Key) -> [u8; 8] {
    let t = aes_cmac(&s1(b"smk3"), n);
    let out = aes_cmac(&t, b"id64\x01");
    out[8..].try_into().expect("8 bytes")
}

/// The derivation function k4: an application key's 6-bit AID.
pub fn k4(n: &Key) -> u8 {
    let t = aes_cmac(&s1(b"smk4"), n);
    aes_cmac(&t, b"id6\x01")[15] & 0x3F
}

/// The identity key, for Node Identity in a proxy's advert.
pub fn identity_key(net_key: &Key) -> Key {
    k1(net_key, &s1(b"nkik"), b"id128\x01")
}

/// The beacon key, that secure network beacons are authenticated with.
pub fn beacon_key(net_key: &Key) -> Key {
    k1(net_key, &s1(b"nkbk"), b"id128\x01")
}

/// How long a message integrity check is: 32 bits, or 64.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mic {
    Short,
    Long,
}

impl Mic {
    pub fn size(self) -> usize {
        match self {
            Mic::Short => 4,
            Mic::Long => 8,
        }
    }
}

/// AES-CCM: `data` encrypted under `key`, with its MIC after it.
pub fn ccm_encrypt(key: &Key, nonce: &Nonce, data: &[u8], aad: &[u8], mic: Mic) -> Vec<u8> {
    let mut out = data.to_vec();
    let nonce = GenericArray::from_slice(nonce);
    let tag: Vec<u8> = match mic {
        Mic::Short => Ccm::<Aes128, U4, U13>::new(GenericArray::from_slice(key))
            .encrypt_in_place_detached(nonce, aad, &mut out)
            .expect("CCM takes any message a mesh sends")
            .to_vec(),
        Mic::Long => Ccm::<Aes128, U8, U13>::new(GenericArray::from_slice(key))
            .encrypt_in_place_detached(nonce, aad, &mut out)
            .expect("CCM takes any message a mesh sends")
            .to_vec(),
    };
    out.extend_from_slice(&tag);
    out
}

/// AES-CCM backwards: the data, or none when its MIC does not match.
pub fn ccm_decrypt(
    key: &Key,
    nonce: &Nonce,
    sealed: &[u8],
    aad: &[u8],
    mic: Mic,
) -> Option<Vec<u8>> {
    let split = sealed.len().checked_sub(mic.size())?;
    let (data, tag) = sealed.split_at(split);
    let mut out = data.to_vec();
    let nonce = GenericArray::from_slice(nonce);
    let good = match mic {
        Mic::Short => Ccm::<Aes128, U4, U13>::new(GenericArray::from_slice(key))
            .decrypt_in_place_detached(nonce, aad, &mut out, GenericArray::from_slice(tag))
            .is_ok(),
        Mic::Long => Ccm::<Aes128, U8, U13>::new(GenericArray::from_slice(key))
            .decrypt_in_place_detached(nonce, aad, &mut out, GenericArray::from_slice(tag))
            .is_ok(),
    };
    good.then_some(out)
}

/// Hex digits as bytes, for keys typed and for tests: "0953fa..." or with spaces.
pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    let digits: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    (0..digits.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(digits.get(at..at + 2)?, 16).ok())
        .collect()
}

/// Bytes as hex digits, lower case, with nothing between them.
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A key from 32 hex digits.
pub fn key_from_hex(text: &str) -> Option<Key> {
    from_hex(text)?.try_into().ok()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn key(text: &str) -> Key {
        key_from_hex(text).expect("32 hex digits")
    }

    pub fn bytes(text: &str) -> Vec<u8> {
        from_hex(text).expect("hex digits")
    }

    // The spec's sample data, 8.1: the security functions.

    #[test]
    fn s1_is_the_specs() {
        assert_eq!(to_hex(&s1(b"test")), "b73cefbd641ef2ea598c2b6efb62f79c");
    }

    #[test]
    fn k1_is_the_specs() {
        let n = bytes("3216d1509884b533248541792b877f98");
        let salt = s1(b"salt");
        assert_eq!(to_hex(&salt), "2ba14ffa0df84a2831938d57d276cab4");
        assert_eq!(
            to_hex(&aes_cmac(&salt, &n)),
            "c764bea25cf9738b08956ea3c712d5af"
        );
        // The spec's P is 16 bytes: s1("info").
        let p = bytes("5a09d60797eeb4478aada59db3352a0d");
        assert_eq!(s1(b"info").to_vec(), p);
        assert_eq!(
            to_hex(&k1(&n, &salt, &p)),
            "f6ed15a8934afbe7d83e8dcb57fcf5d7"
        );
    }

    #[test]
    fn k2_is_the_specs() {
        let keys = k2(&key("f7a2a44f8e8a8029064f173ddc1e2b00"), &[0x00]);
        assert_eq!(keys.nid, 0x7F);
        assert_eq!(to_hex(&keys.encryption), "9f589181a0f50de73c8070c7a6d27f46");
        assert_eq!(to_hex(&keys.privacy), "4c715bd4a64b938f99b453351653124f");
        // The friendship credentials, with the friendship's addresses and counters as P.
        let friend = k2(
            &key("f7a2a44f8e8a8029064f173ddc1e2b00"),
            &bytes("010203040506070809"),
        );
        assert_eq!(friend.nid, 0x73);
        assert_eq!(
            to_hex(&friend.encryption),
            "11efec0642774992510fb5929646df49"
        );
        assert_eq!(to_hex(&friend.privacy), "d4d7cc0dfa772d836a8df9df5510d7a7");
    }

    #[test]
    fn k3_and_k4_are_the_specs() {
        assert_eq!(
            to_hex(&k3(&key("f7a2a44f8e8a8029064f173ddc1e2b00"))),
            "ff046958233db014"
        );
        assert_eq!(k4(&key("3216d1509884b533248541792b877f98")), 0x38);
    }

    // 8.2: the keys a network and an application key make.

    #[test]
    fn the_keys_derived_are_the_specs() {
        let net_key = key("7dd7364cd842ad18c17c2b820c84c3d6");
        assert_eq!(k4(&key("63964771734fbd76e3b40519d1d94a48")), 0x26);
        let keys = k2(&net_key, &[0x00]);
        assert_eq!(keys.nid, 0x68);
        assert_eq!(to_hex(&keys.encryption), "0953fa93e7caac9638f58820220a398e");
        assert_eq!(to_hex(&keys.privacy), "8b84eedec100067d670971dd2aa700cf");
        assert_eq!(to_hex(&k3(&net_key)), "3ecaff672f673370");
        assert_eq!(
            to_hex(&identity_key(&net_key)),
            "84396c435ac48560b5965385253e210c"
        );
        assert_eq!(
            to_hex(&beacon_key(&net_key)),
            "5423d967da639a99cb02231a83f7d254"
        );
    }

    #[test]
    fn ccm_goes_back_and_forth_and_refuses_a_changed_message() {
        let key = key("0953fa93e7caac9638f58820220a398e");
        let nonce: Nonce = bytes("00800000011201000012345678").try_into().unwrap();
        for mic in [Mic::Short, Mic::Long] {
            let sealed = ccm_encrypt(&key, &nonce, b"hello", &[], mic);
            assert_eq!(sealed.len(), 5 + mic.size());
            assert_eq!(
                ccm_decrypt(&key, &nonce, &sealed, &[], mic).unwrap(),
                b"hello"
            );
            let mut changed = sealed.clone();
            changed[0] ^= 1;
            assert_eq!(ccm_decrypt(&key, &nonce, &changed, &[], mic), None);
        }
    }

    #[test]
    fn hex_is_read_as_people_write_it() {
        assert_eq!(from_hex("01 0a ff"), Some(vec![0x01, 0x0A, 0xFF]));
        assert_eq!(from_hex("abc"), None);
        assert_eq!(from_hex("zz"), None);
        assert_eq!(to_hex(&[0x01, 0xAB]), "01ab");
    }
}
