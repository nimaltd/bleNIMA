//! The provisioning protocol, as the Provisioner (5.4): a device is invited,
//! says what it can do, swaps public keys with bleNIMA, proves it is the
//! device the user means with the OOB method chosen, and is given the network
//! key and its address. Both algorithms are here, and both are checked against
//! the spec's sample data (8.17).

use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::{PublicKey, SecretKey, ecdh};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};

use super::crypto::{Key, Mic, Nonce, aes_cmac, ccm_encrypt, hmac_sha256, k1, k5, s1, s2};

/// The provisioning PDUs' types (5.4.1).
pub mod pdu_type {
    pub const INVITE: u8 = 0x00;
    pub const CAPABILITIES: u8 = 0x01;
    pub const START: u8 = 0x02;
    pub const PUBLIC_KEY: u8 = 0x03;
    pub const INPUT_COMPLETE: u8 = 0x04;
    pub const CONFIRMATION: u8 = 0x05;
    pub const RANDOM: u8 = 0x06;
    pub const DATA: u8 = 0x07;
    pub const COMPLETE: u8 = 0x08;
    pub const FAILED: u8 = 0x09;
}

/// A provisioning PDU's name, for the log.
pub fn pdu_name(kind: u8) -> &'static str {
    match kind {
        pdu_type::INVITE => "Invite",
        pdu_type::CAPABILITIES => "Capabilities",
        pdu_type::START => "Start",
        pdu_type::PUBLIC_KEY => "Public Key",
        pdu_type::INPUT_COMPLETE => "Input Complete",
        pdu_type::CONFIRMATION => "Confirmation",
        pdu_type::RANDOM => "Random",
        pdu_type::DATA => "Data",
        pdu_type::COMPLETE => "Complete",
        pdu_type::FAILED => "Failed",
        _ => "an unknown PDU",
    }
}

/// What a Provisioning Failed PDU's error code means (5.4.4).
pub fn error_name(code: u8) -> &'static str {
    match code {
        0x01 => "Invalid PDU",
        0x02 => "Invalid Format",
        0x03 => "Unexpected PDU",
        0x04 => "Confirmation Failed: the OOB value was not the device's",
        0x05 => "Out of Resources",
        0x06 => "Decryption Failed",
        0x07 => "Unexpected Error",
        0x08 => "Cannot Assign Addresses",
        0x09 => "Invalid Data",
        _ => "an unknown error",
    }
}

/// What a device says it can do, in its Provisioning Capabilities PDU.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    /// How many elements it has: it takes that many addresses.
    pub elements: u8,
    /// Bit 0: BTM_ECDH_P256_CMAC_AES128_AES_CCM; bit 1: BTM_ECDH_P256_HMAC_SHA256_AES_CCM.
    pub algorithms: u16,
    /// Bit 0: its public key is to be had out of band.
    pub public_key_type: u8,
    /// Bit 0: it has a static OOB value; bit 1: it takes only OOB authenticated provisioning.
    pub oob_type: u8,
    /// How many digits or letters it can blink, beep, vibrate or show: 0 when none.
    pub output_size: u8,
    /// Bit 0 Blink, 1 Beep, 2 Vibrate, 3 Output Numeric, 4 Output Alphanumeric.
    pub output_actions: u16,
    /// How many digits or letters can be put into it: 0 when none.
    pub input_size: u8,
    /// Bit 0 Push, 1 Twist, 2 Input Numeric, 3 Input Alphanumeric.
    pub input_actions: u16,
}

impl Capabilities {
    /// Reads the PDU's 11 bytes of parameters.
    pub fn parse(params: &[u8]) -> Option<Self> {
        let p: &[u8; 11] = params.try_into().ok()?;
        Some(Self {
            elements: p[0],
            algorithms: u16::from_be_bytes([p[1], p[2]]),
            public_key_type: p[3],
            oob_type: p[4],
            output_size: p[5],
            output_actions: u16::from_be_bytes([p[6], p[7]]),
            input_size: p[8],
            input_actions: u16::from_be_bytes([p[9], p[10]]),
        })
    }

    /// The strongest algorithm both sides know: the spec has the CMAC one used
    /// only when the device has no other.
    pub fn algorithm(&self) -> Option<Algorithm> {
        if self.algorithms & 0b10 != 0 {
            Some(Algorithm::HmacSha256)
        } else if self.algorithms & 0b01 != 0 {
            Some(Algorithm::Cmac)
        } else {
            None
        }
    }
}

/// The provisioning algorithm: how big the random numbers, the AuthValue and
/// the confirmations are, and what makes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Algorithm {
    /// BTM_ECDH_P256_CMAC_AES128_AES_CCM: 128 bits.
    Cmac,
    /// BTM_ECDH_P256_HMAC_SHA256_AES_CCM: 256 bits.
    HmacSha256,
}

impl Algorithm {
    fn value(self) -> u8 {
        match self {
            Algorithm::Cmac => 0x00,
            Algorithm::HmacSha256 => 0x01,
        }
    }

    /// How many bytes its random numbers, AuthValue and confirmations have.
    pub fn size(self) -> usize {
        match self {
            Algorithm::Cmac => 16,
            Algorithm::HmacSha256 => 32,
        }
    }
}

/// How the device proves it is the one the user means (5.4.2.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum AuthMethod {
    /// Nothing is checked: any device in range could have answered.
    NoOob,
    /// A value both sides know already, such as one printed on the device.
    Static,
    /// The device blinks, beeps, vibrates or shows a value, and the user types it into bleNIMA.
    /// `action`: 0 Blink, 1 Beep, 2 Vibrate, 3 Output Numeric, 4 Output Alphanumeric.
    Output { action: u8, size: u8 },
    /// bleNIMA shows a value, and the user puts it into the device.
    /// `action`: 0 Push, 1 Twist, 2 Input Numeric, 3 Input Alphanumeric.
    Input { action: u8, size: u8 },
}

impl AuthMethod {
    /// The Start PDU's Authentication Method, Action and Size fields.
    fn fields(self) -> [u8; 3] {
        match self {
            AuthMethod::NoOob => [0x00, 0, 0],
            AuthMethod::Static => [0x01, 0, 0],
            AuthMethod::Output { action, size } => [0x02, action, size],
            AuthMethod::Input { action, size } => [0x03, action, size],
        }
    }

    /// Whether the value is letters and digits, rather than a number.
    fn alphanumeric(self) -> bool {
        matches!(
            self,
            AuthMethod::Output { action: 4, .. } | AuthMethod::Input { action: 3, .. }
        )
    }
}

/// A number as an AuthValue: big-endian, in the last bytes.
pub fn numeric_auth(algorithm: Algorithm, number: u64) -> Vec<u8> {
    let mut out = vec![0u8; algorithm.size()];
    let at = out.len() - 8;
    out[at..].copy_from_slice(&number.to_be_bytes());
    out
}

/// Bytes, or the ASCII of letters, as an AuthValue: zeros after them, or cut short.
pub fn octets_auth(algorithm: Algorithm, bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out.resize(algorithm.size(), 0);
    out
}

/// The AuthValue from what the user typed: hex for a static value, a number,
/// or letters and digits, as the method has it.
pub fn auth_value(
    algorithm: Algorithm,
    method: AuthMethod,
    typed: &str,
) -> Result<Vec<u8>, String> {
    let typed = typed.trim();
    match method {
        AuthMethod::NoOob => Ok(vec![0; algorithm.size()]),
        AuthMethod::Static => {
            let bytes =
                super::crypto::from_hex(typed).ok_or("The static OOB value is not hex digits")?;
            if bytes.is_empty() {
                return Err("The static OOB value is empty".into());
            }
            Ok(octets_auth(algorithm, &bytes))
        }
        _ if method.alphanumeric() => {
            if typed.is_empty() || !typed.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Err("The value is letters and digits".into());
            }
            Ok(octets_auth(
                algorithm,
                typed.to_ascii_uppercase().as_bytes(),
            ))
        }
        _ => typed
            .parse::<u64>()
            .map(|number| numeric_auth(algorithm, number))
            .map_err(|_| "The value is a number".into()),
    }
}

/// A value for Input OOB, for bleNIMA to show and the user to put into the
/// device: how many pushes or twists, a number of `size` digits at most, or
/// `size` letters and digits.
pub fn input_value(action: u8, size: u8) -> String {
    let size = u32::from(size.clamp(1, 8));
    let mut random = [0u8; 8];
    OsRng.fill_bytes(&mut random);
    let random = u64::from_le_bytes(random);
    let max = 10u64.pow(size);
    match action {
        // At least one push or twist.
        0 | 1 => (1 + random % (max - 1)).to_string(),
        3 => {
            const SIGNS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
            let mut bytes = [0u8; 8];
            OsRng.fill_bytes(&mut bytes);
            bytes[..size as usize]
                .iter()
                .map(|byte| char::from(SIGNS[usize::from(*byte) % SIGNS.len()]))
                .collect()
        }
        _ => (random % max).to_string(),
    }
}

/// The keys the session ends with.
struct SessionKeys {
    key: Key,
    nonce: Nonce,
    device_key: Key,
}

/// One device being provisioned, as the Provisioner. Each step takes the
/// device's PDU, type byte and all, and gives the next PDU to send.
pub struct Provisioner {
    secret: SecretKey,
    public: [u8; 64],
    random: Vec<u8>,
    algorithm: Algorithm,
    /// The Invite, Capabilities and Start PDUs' parameters and both public
    /// keys, in that order, as the confirmations take them.
    inputs: Vec<u8>,
    ecdh: [u8; 32],
    confirmation_salt: Vec<u8>,
    confirmation_key: Vec<u8>,
    auth: Vec<u8>,
    confirmation: Vec<u8>,
    device_confirmation: Vec<u8>,
    session: Option<SessionKeys>,
}

impl Default for Provisioner {
    fn default() -> Self {
        Self::new()
    }
}

/// The parameters of a PDU of the type expected, or why the device stopped.
fn expect(pdu: &[u8], kind: u8) -> Result<&[u8], String> {
    let (&first, params) = pdu.split_first().ok_or("The device sent an empty PDU")?;
    match first & 0x3F {
        pdu_type::FAILED => Err(format!(
            "The device stopped provisioning: {}",
            error_name(params.first().copied().unwrap_or(0))
        )),
        found if found == kind => Ok(params),
        found => Err(format!(
            "The device sent {} where {} was due",
            pdu_name(found),
            pdu_name(kind)
        )),
    }
}

fn pdu(kind: u8, params: &[u8]) -> Vec<u8> {
    let mut out = vec![kind];
    out.extend_from_slice(params);
    out
}

impl Provisioner {
    /// A new key pair and a new random number, for one device.
    pub fn new() -> Self {
        let mut random = vec![0u8; 32];
        OsRng.fill_bytes(&mut random);
        Self::from_parts(SecretKey::random(&mut OsRng), random)
    }

    fn from_parts(secret: SecretKey, random: Vec<u8>) -> Self {
        let point = secret.public_key().to_encoded_point(false);
        let mut public = [0u8; 64];
        public.copy_from_slice(&point.as_bytes()[1..]);
        Self {
            secret,
            public,
            random,
            algorithm: Algorithm::Cmac,
            inputs: Vec::new(),
            ecdh: [0; 32],
            confirmation_salt: Vec::new(),
            confirmation_key: Vec::new(),
            auth: Vec::new(),
            confirmation: Vec::new(),
            device_confirmation: Vec::new(),
            session: None,
        }
    }

    /// The Invite PDU: the device is asked to show itself for `attention` seconds.
    pub fn invite(&mut self, attention: u8) -> Vec<u8> {
        self.inputs = vec![attention];
        pdu(pdu_type::INVITE, &[attention])
    }

    /// Takes the device's Capabilities PDU.
    pub fn capabilities(&mut self, received: &[u8]) -> Result<Capabilities, String> {
        let params = expect(received, pdu_type::CAPABILITIES)?;
        let capabilities =
            Capabilities::parse(params).ok_or("The device's capabilities are not 11 bytes")?;
        self.inputs.extend_from_slice(params);
        Ok(capabilities)
    }

    /// The Start PDU: the algorithm and the OOB method chosen, with the
    /// public keys swapped in the open.
    pub fn start(&mut self, algorithm: Algorithm, method: AuthMethod) -> Vec<u8> {
        self.algorithm = algorithm;
        self.random.truncate(algorithm.size());
        let [kind, action, size] = method.fields();
        let params = [algorithm.value(), 0x00, kind, action, size];
        self.inputs.extend_from_slice(&params);
        pdu(pdu_type::START, &params)
    }

    /// The Public Key PDU with bleNIMA's public key.
    pub fn public_key(&mut self) -> Vec<u8> {
        self.inputs.extend_from_slice(&self.public);
        pdu(pdu_type::PUBLIC_KEY, &self.public)
    }

    /// Takes the device's Public Key PDU, and works out the secret both share.
    pub fn device_public_key(&mut self, received: &[u8]) -> Result<(), String> {
        let params = expect(received, pdu_type::PUBLIC_KEY)?;
        if params.len() != 64 {
            return Err("The device's public key is not 64 bytes".into());
        }
        if params == self.public {
            return Err("The device sent back bleNIMA's own public key".into());
        }
        let mut sec1 = vec![0x04];
        sec1.extend_from_slice(params);
        let device = PublicKey::from_sec1_bytes(&sec1)
            .map_err(|_| "The device's public key is not on the P-256 curve")?;
        let shared = ecdh::diffie_hellman(self.secret.to_nonzero_scalar(), device.as_affine());
        self.ecdh.copy_from_slice(shared.raw_secret_bytes());
        self.inputs.extend_from_slice(params);
        self.confirmation_salt = match self.algorithm {
            Algorithm::Cmac => s1(&self.inputs).to_vec(),
            Algorithm::HmacSha256 => s2(&self.inputs).to_vec(),
        };
        Ok(())
    }

    /// A confirmation of a random number, with the AuthValue.
    fn confirm(&self, random: &[u8]) -> Vec<u8> {
        match self.algorithm {
            Algorithm::Cmac => {
                let key: Key = self.confirmation_key[..].try_into().expect("16 bytes");
                aes_cmac(&key, &[random, &self.auth].concat()).to_vec()
            }
            Algorithm::HmacSha256 => hmac_sha256(&self.confirmation_key, random).to_vec(),
        }
    }

    /// The Confirmation PDU, once the AuthValue is known.
    pub fn confirmation(&mut self, auth: &[u8]) -> Vec<u8> {
        self.auth = octets_auth(self.algorithm, auth);
        self.confirmation_key = match self.algorithm {
            Algorithm::Cmac => {
                let salt: Key = self.confirmation_salt[..].try_into().expect("16 bytes");
                k1(&self.ecdh, &salt, b"prck").to_vec()
            }
            Algorithm::HmacSha256 => {
                let salt: [u8; 32] = self.confirmation_salt[..].try_into().expect("32 bytes");
                k5(&[&self.ecdh[..], &self.auth].concat(), &salt, b"prck256").to_vec()
            }
        };
        self.confirmation = self.confirm(&self.random);
        pdu(pdu_type::CONFIRMATION, &self.confirmation)
    }

    /// Takes the device's Confirmation PDU; gives the Random PDU.
    pub fn device_confirmation(&mut self, received: &[u8]) -> Result<Vec<u8>, String> {
        let params = expect(received, pdu_type::CONFIRMATION)?;
        if params.len() != self.algorithm.size() {
            return Err("The device's confirmation is not as long as the algorithm's".into());
        }
        if params == self.confirmation {
            return Err("The device sent back bleNIMA's own confirmation".into());
        }
        self.device_confirmation = params.to_vec();
        Ok(pdu(pdu_type::RANDOM, &self.random))
    }

    /// Takes the device's Random PDU and checks it against its confirmation:
    /// when they match, the device knew the AuthValue.
    pub fn device_random(&mut self, received: &[u8]) -> Result<(), String> {
        let params = expect(received, pdu_type::RANDOM)?;
        if params.len() != self.algorithm.size() {
            return Err("The device's random number is not as long as the algorithm's".into());
        }
        if self.confirm(params) != self.device_confirmation {
            return Err(
                "The device's confirmation does not match: the OOB value was wrong, or another device answered".into(),
            );
        }
        let salt = s1(&[&self.confirmation_salt[..], &self.random, params].concat());
        let nonce = k1(&self.ecdh, &salt, b"prsn");
        self.session = Some(SessionKeys {
            key: k1(&self.ecdh, &salt, b"prsk"),
            nonce: nonce[3..].try_into().expect("13 bytes"),
            device_key: k1(&self.ecdh, &salt, b"prdk"),
        });
        Ok(())
    }

    /// The Data PDU: the network key and its index, the flags (bit 0 Key
    /// Refresh, bit 1 IV Update), the IV index and the device's first address,
    /// encrypted with the session key.
    pub fn data(
        &self,
        net_key: &Key,
        key_index: u16,
        flags: u8,
        iv_index: u32,
        address: u16,
    ) -> Result<Vec<u8>, String> {
        let session = self
            .session
            .as_ref()
            .ok_or("The device was not checked yet")?;
        let mut data = net_key.to_vec();
        data.extend_from_slice(&key_index.to_be_bytes());
        data.push(flags);
        data.extend_from_slice(&iv_index.to_be_bytes());
        data.extend_from_slice(&address.to_be_bytes());
        let sealed = ccm_encrypt(&session.key, &session.nonce, &data, &[], Mic::Long);
        Ok(pdu(pdu_type::DATA, &sealed))
    }

    /// Takes the device's last PDU: Complete, or why it failed.
    pub fn complete(&self, received: &[u8]) -> Result<(), String> {
        expect(received, pdu_type::COMPLETE).map(|_| ())
    }

    /// The device key, known once the device is checked.
    pub fn device_key(&self) -> Option<Key> {
        self.session.as_ref().map(|session| session.device_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::crypto::tests::{bytes, key};
    use crate::mesh::crypto::to_hex;

    const PRIVATE_KEY: &str = "06a516693c9aa31a6084545d0c5db641b48572b97203ddffb7ac73f7d0457663";
    const PUBLIC_KEY: &str = "2c31a47b5779809ef44cb5eaaf5c3e43d5f8faad4a8794cb987e9b03745c78dd\
                              919512183898dfbecd52e2408e43871fd021109117bd3ed4eaf8437743715d4f";
    const DEVICE_PUBLIC_KEY: &str = "f465e43ff23d3f1b9dc7dfc04da8758184dbc966204796eccf0d6cf5e16500cc\
                                     0201d048bcbbd899eeefc424164e33c201c2b010ca6b4d43a8a155cad8ecb279";
    const ECDH: &str = "ab85843a2f6d883f62e5684b38e307335fe6e1945ecd19604105c6f23221eb69";

    fn provisioner(random: &str) -> Provisioner {
        let secret = SecretKey::from_slice(&bytes(PRIVATE_KEY)).unwrap();
        Provisioner::from_parts(secret, bytes(random))
    }

    fn with_type(kind: u8, params: &str) -> Vec<u8> {
        pdu(kind, &bytes(params))
    }

    #[test]
    fn the_cmac_algorithm_gives_the_specs_keys() {
        let mut p = provisioner("8b19ac31d58b124c946209b5db1021b9");
        assert_eq!(to_hex(&p.invite(0)), "0000");
        let caps = p
            .capabilities(&with_type(pdu_type::CAPABILITIES, "0100010000000000000000"))
            .unwrap();
        assert_eq!(caps.algorithm(), Some(Algorithm::Cmac));
        assert_eq!(
            to_hex(&p.start(Algorithm::Cmac, AuthMethod::NoOob)),
            "020000000000"
        );
        assert_eq!(
            to_hex(&p.public_key()),
            format!("03{PUBLIC_KEY}").replace(' ', "")
        );
        p.device_public_key(&with_type(pdu_type::PUBLIC_KEY, DEVICE_PUBLIC_KEY))
            .unwrap();
        assert_eq!(to_hex(&p.ecdh), ECDH);
        assert_eq!(
            to_hex(&p.confirmation_salt),
            "5faabe187337c71cc6c973369dcaa79a"
        );
        let auth = auth_value(Algorithm::Cmac, AuthMethod::NoOob, "").unwrap();
        assert_eq!(
            to_hex(&p.confirmation(&auth)),
            "05b38a114dfdca1fe153bd2c1e0dc46ac2"
        );
        assert_eq!(
            to_hex(&p.confirmation_key),
            "e31fe046c68ec339c425fc6629f0336f"
        );
        let random = p
            .device_confirmation(&with_type(
                pdu_type::CONFIRMATION,
                "eeba521c196b52cc2e37aa40329f554e",
            ))
            .unwrap();
        assert_eq!(to_hex(&random), "068b19ac31d58b124c946209b5db1021b9");
        p.device_random(&with_type(
            pdu_type::RANDOM,
            "55a2a2bca04cd32ff6f346bd0a0c1a3a",
        ))
        .unwrap();
        let session = p.session.as_ref().unwrap();
        assert_eq!(to_hex(&session.key), "c80253af86b33dfa450bbdb2a191fea3");
        assert_eq!(to_hex(&session.nonce), "da7ddbe78b5f62b81d6847487e");
        let data = p
            .data(
                &key("efb2255e6422d330088e09bb015ed707"),
                0x0567,
                0,
                0x0102_0304,
                0x0B0C,
            )
            .unwrap();
        assert_eq!(
            to_hex(&data),
            "07d0bd7f4a89a2ff6222af59a90a60ad58acfe3123356f5cec2973e0ec50783b10c7"
        );
        assert_eq!(
            p.device_key().map(|k| to_hex(&k)),
            Some("0520adad5e0142aa3e325087b4ec16d8".into())
        );
        assert_eq!(p.complete(&[pdu_type::COMPLETE]), Ok(()));
    }

    #[test]
    fn the_hmac_algorithm_with_a_static_value_gives_the_specs_keys() {
        let mut p = provisioner("36f968b94a13000e64b223576390db6bcc6d62f02617c369ee3f5b3e89df7e1f");
        p.invite(0);
        let caps = p
            .capabilities(&with_type(pdu_type::CAPABILITIES, "0100030001000000000000"))
            .unwrap();
        // Both algorithms: the stronger one is taken.
        assert_eq!(caps.algorithm(), Some(Algorithm::HmacSha256));
        assert_eq!(caps.oob_type, 0x01);
        assert_eq!(
            to_hex(&p.start(Algorithm::HmacSha256, AuthMethod::Static)),
            "020100010000"
        );
        p.public_key();
        p.device_public_key(&with_type(pdu_type::PUBLIC_KEY, DEVICE_PUBLIC_KEY))
            .unwrap();
        assert_eq!(
            to_hex(&p.confirmation_salt),
            "a71141ba8cb6b40f4f52b622e1c091614c73fc308f871b78ca775e769bc3ae69"
        );
        let auth = auth_value(
            Algorithm::HmacSha256,
            AuthMethod::Static,
            "906d73a3c7a7cb3ff730dca68a46b9c18d673f50e078202311473ebbe253669f",
        )
        .unwrap();
        assert_eq!(
            to_hex(&p.confirmation(&auth)),
            "05c99b54617ae646f5f32cf7e1ea6fcc49fd69066078eba9580fa6c7031833e6c8"
        );
        assert_eq!(
            to_hex(&p.confirmation_key),
            "210c3c448152e8d59ef742aa7d22ee5ba59a38648bda6bf05c74f3e46fc2c0bb"
        );
        p.device_confirmation(&with_type(
            pdu_type::CONFIRMATION,
            "56e3722d291373d38c995d6f942c02928c96abb015c233557d7974b6e2df662b",
        ))
        .unwrap();
        p.device_random(&with_type(
            pdu_type::RANDOM,
            "5b9b1fc6a64b2de8bece53187ee989c6566db1fc7dc8580a73dafdd6211d56a5",
        ))
        .unwrap();
        let session = p.session.as_ref().unwrap();
        assert_eq!(to_hex(&session.key), "df4a494da3d45405e402f1d6a6cea338");
        assert_eq!(to_hex(&session.nonce), "11b987db2ae41fbb9e96b80446");
        let data = p
            .data(
                &key("efb2255e6422d330088e09bb015ed707"),
                0x0567,
                0,
                0x0102_0304,
                0x0B0C,
            )
            .unwrap();
        assert_eq!(
            to_hex(&data),
            "07f9df98cbb736be1f600659ac4c37821a82db31e410a03de7693a2a0428fbdaf321"
        );
        assert_eq!(
            p.device_key().map(|k| to_hex(&k)),
            Some("2770852a737cf05d8813768f22af3a2d".into())
        );
    }

    #[test]
    fn a_wrong_oob_value_is_caught_at_the_random() {
        let mut p = provisioner("8b19ac31d58b124c946209b5db1021b9");
        p.invite(0);
        p.capabilities(&with_type(pdu_type::CAPABILITIES, "0100010000000000000000"))
            .unwrap();
        p.start(Algorithm::Cmac, AuthMethod::NoOob);
        p.public_key();
        p.device_public_key(&with_type(pdu_type::PUBLIC_KEY, DEVICE_PUBLIC_KEY))
            .unwrap();
        // Not the zeros the device's confirmation was made with.
        p.confirmation(&numeric_auth(Algorithm::Cmac, 1));
        p.device_confirmation(&with_type(
            pdu_type::CONFIRMATION,
            "eeba521c196b52cc2e37aa40329f554e",
        ))
        .unwrap();
        let checked = p.device_random(&with_type(
            pdu_type::RANDOM,
            "55a2a2bca04cd32ff6f346bd0a0c1a3a",
        ));
        assert!(checked.unwrap_err().contains("does not match"));
        assert_eq!(p.device_key(), None);
    }

    #[test]
    fn the_device_is_held_to_the_protocol() {
        let mut p = provisioner("8b19ac31d58b124c946209b5db1021b9");
        p.invite(5);
        assert_eq!(
            p.capabilities(&[pdu_type::FAILED, 0x02]).unwrap_err(),
            "The device stopped provisioning: Invalid Format"
        );
        assert_eq!(
            p.capabilities(&[pdu_type::RANDOM]).unwrap_err(),
            "The device sent Random where Capabilities was due"
        );
        p.start(Algorithm::Cmac, AuthMethod::NoOob);
        let own = p.public_key();
        assert!(
            p.device_public_key(&own)
                .unwrap_err()
                .contains("own public key")
        );
        let mut off_curve = with_type(pdu_type::PUBLIC_KEY, DEVICE_PUBLIC_KEY);
        off_curve[64] ^= 1;
        assert!(
            p.device_public_key(&off_curve)
                .unwrap_err()
                .contains("P-256")
        );
        assert!(p.data(&[0; 16], 0, 0, 0, 1).is_err());
    }

    #[test]
    fn auth_values_are_made_as_the_spec_shows() {
        // 5.4.2.4.1's own examples.
        let blink = AuthMethod::Output { action: 0, size: 1 };
        assert_eq!(
            to_hex(&auth_value(Algorithm::Cmac, blink, "5").unwrap()),
            "00000000000000000000000000000005"
        );
        let numeric = AuthMethod::Output { action: 3, size: 6 };
        assert_eq!(
            to_hex(&auth_value(Algorithm::Cmac, numeric, "019655").unwrap()),
            "00000000000000000000000000004cc7"
        );
        let text = AuthMethod::Output { action: 4, size: 6 };
        assert_eq!(
            to_hex(&auth_value(Algorithm::Cmac, text, "123abc").unwrap()),
            "31323341424300000000000000000000"
        );
        assert_eq!(
            to_hex(&auth_value(Algorithm::Cmac, AuthMethod::Static, "123456").unwrap()),
            "12345600000000000000000000000000"
        );
        assert_eq!(
            to_hex(
                &auth_value(
                    Algorithm::Cmac,
                    AuthMethod::Static,
                    "000102030405060708090a0b0c0d0e0f10111213"
                )
                .unwrap()
            ),
            "000102030405060708090a0b0c0d0e0f"
        );
        assert_eq!(
            auth_value(Algorithm::HmacSha256, AuthMethod::NoOob, "").unwrap(),
            vec![0; 32]
        );
        assert!(auth_value(Algorithm::Cmac, numeric, "12a").is_err());
        assert!(auth_value(Algorithm::Cmac, text, "12-").is_err());
        assert!(auth_value(Algorithm::Cmac, AuthMethod::Static, "xyz").is_err());
    }

    #[test]
    fn input_values_fit_the_device() {
        for _ in 0..200 {
            let push: u64 = input_value(0, 1).parse().unwrap();
            assert!((1..=9).contains(&push));
            let number = input_value(2, 4);
            assert!(number.parse::<u64>().unwrap() < 10_000);
            let text = input_value(3, 6);
            assert_eq!(text.len(), 6);
            assert!(
                text.chars()
                    .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
            );
        }
    }
}
