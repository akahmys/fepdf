//! Encryption and decryption of PDF strings and streams (ISO 32000-2 Clause 7.6).

use crate::{SyntaxError, SyntaxResult};
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use aes::{Aes128, Aes256, Block};
use md5;
use sha2::{Digest, Sha256};

/// The 32-byte padding string of ISO 32000-2, Table 24.
const PAD: [u8; 32] = [
    0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01, 0x08,
    0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53, 0x69, 0x7a,
];

/// RC4, as clause 7.6.3.2 requires for the standard handler's earlier revisions.
///
/// Written out because no crate in this workspace provides it and the algorithm is
/// twenty lines. It is used here to *validate* a password — recomputing `/U` and
/// comparing — which is a check the standard specifies, not a security choice.
fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }
    let mut s: [u8; 256] = core::array::from_fn(|i| u8::try_from(i).unwrap_or(0));
    let mut j = 0usize;
    for i in 0..256 {
        j = (j + s[i] as usize + key[i % key.len()] as usize) % 256;
        s.swap(i, j);
    }
    let (mut i, mut j) = (0usize, 0usize);
    data.iter()
        .map(|&byte| {
            i = (i + 1) % 256;
            j = (j + s[i] as usize) % 256;
            s.swap(i, j);
            byte ^ s[(s[i] as usize + s[j] as usize) % 256]
        })
        .collect()
}

#[derive(Clone)]
struct V4Inputs {
    pub password: String,
    pub o: Vec<u8>,
    pub u: Vec<u8>,
    pub p: i32,
    pub file_id: Vec<u8>,
}

/// A security handler for PDF encryption.
#[derive(Clone)]
pub struct SecurityHandler {
    encryption_key: Vec<u8>,
    revision: i32,
    is_aes: bool,
    encrypt_metadata: bool,
    v4_inputs: Option<V4Inputs>,
}

impl SecurityHandler {
    /// Creates a new security handler for AES-128 (Revision 4).
    pub fn new_v4(
        user_password: &str,
        o_string: &[u8],
        u_string: &[u8],
        p_value: i32,
        file_id: &[u8],
        encrypt_metadata: bool,
    ) -> SyntaxResult<Self> {
        let key = Self::derive_v4_key(
            user_password,
            o_string,
            u_string,
            p_value,
            file_id,
            encrypt_metadata,
        )?;

        Ok(Self {
            encryption_key: key,
            revision: 4,
            is_aes: true,
            encrypt_metadata,
            v4_inputs: Some(V4Inputs {
                password: user_password.to_string(),
                o: o_string.to_vec(),
                u: u_string.to_vec(),
                p: p_value,
                file_id: file_id.to_vec(),
            }),
        })
    }

    fn derive_v4_key(
        user_password: &str,
        o_string: &[u8],
        _u_string: &[u8],
        p_value: i32,
        file_id: &[u8],
        encrypt_metadata: bool,
    ) -> SyntaxResult<Vec<u8>> {
        let mut pad = [0u8; 32];
        let pw_bytes = user_password.as_bytes();
        let len = pw_bytes.len().min(32);
        pad[..len].copy_from_slice(&pw_bytes[..len]);
        if len < 32 {
            let padding = [
                0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa,
                0x01, 0x08, 0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe,
                0x64, 0x53, 0x69, 0x7a,
            ];
            pad[len..].copy_from_slice(&padding[..32 - len]);
        }

        let mut hasher = md5::Context::new();
        hasher.consume(pad);
        hasher.consume(o_string);
        hasher.consume(p_value.to_le_bytes());
        hasher.consume(file_id);

        if !encrypt_metadata {
            hasher.consume([0xFF, 0xFF, 0xFF, 0xFF]);
        }

        let mut hash = hasher.finalize().0;
        for _ in 0..50 {
            let mut h2 = md5::Context::new();
            h2.consume(&hash[..16]);
            hash = h2.finalize().0;
        }

        Ok(hash[..16].to_vec())
    }

    /// Creates a new security handler for AES-256 (Revision 5).
    ///
    /// # Compliance Warning
    /// This is a simplified Revision 5 key derivation logic primarily used for internal validation,
    /// and does not fully conform to the multi-stage key hashing and AES-decryption requirements
    /// specified in ISO 32000-2:2020 Clause 7.6.4.3.3 (Algorithm 2.A/Algorithm 3.A).
    ///
    /// TODO(RR-15-EXT): Transition to full multi-stage key verification using validation salts,
    /// key salts, and owner password checking as detailed in ISO 32000-2 Algorithms 8, 9, 2.A, and 3.A.
    pub fn new_v5(
        user_password: &str,
        _owner_password: &str,
        file_id: &[u8],
    ) -> SyntaxResult<Self> {
        // Derive deterministic validation and key salts using SHA-256 to comply with Rule 10 (Determinism)
        let mut ue_hasher = Sha256::new();
        ue_hasher.update(file_id);
        ue_hasher.update(b"UserKeySalt");
        let ue_salt: [u8; 32] = ue_hasher.finalize().into();

        // 50-round SHA-256 multi-stage key derivation (ISO 32000-2:2020 Clause 7.6.4.3.3)
        let mut hasher = Sha256::new();
        hasher.update(user_password.as_bytes());
        hasher.update(ue_salt);
        let mut hash: [u8; 32] = hasher.finalize().into();

        for _ in 0..50 {
            let mut h = Sha256::new();
            h.update(hash);
            h.update(user_password.as_bytes());
            h.update(ue_salt);
            hash = h.finalize().into();
        }

        Ok(Self {
            encryption_key: hash.to_vec(),
            revision: 5,
            is_aes: true,
            encrypt_metadata: true,
            v4_inputs: None,
        })
    }

    /// The file encryption key, for cross-checking key derivation against an
    /// independent implementation of the standard's algorithms.
    #[doc(hidden)]
    #[must_use]
    pub fn file_key(&self) -> &[u8] {
        &self.encryption_key
    }

    /// Whether the password this handler was built from is the document's user
    /// password — Algorithm 6, by way of Algorithm 4 or 5 (7.6.4.4).
    ///
    /// Without this a wrong password derives a wrong key, every string and stream
    /// decrypts to noise, and the document still "opens": `samples/unicode_16.pdf`
    /// reported 1,140 pages and 29,438 font failures rather than refusing.
    ///
    /// Revisions 5 and 6 validate differently (Algorithm 11, against `/U`'s validation
    /// salt) and are not covered here; `new_v5` does not derive a conforming key to
    /// begin with, so there is nothing yet to validate against.
    #[must_use]
    pub fn user_password_matches(&self, u_string: &[u8], file_id: &[u8]) -> bool {
        if self.revision >= 5 || u_string.len() < 16 {
            return true;
        }
        let computed = self.compute_u(file_id);
        if self.revision == 2 {
            // Algorithm 4: /U is the padding string encrypted with the file key.
            return computed == u_string[..computed.len().min(u_string.len())];
        }
        // Algorithm 5: only the first 16 bytes are defined; the rest is arbitrary.
        computed[..16] == u_string[..16]
    }

    /// `/U` as Algorithm 4 (revision 2) or Algorithm 5 (revision 3 and later) defines.
    fn compute_u(&self, file_id: &[u8]) -> Vec<u8> {
        if self.revision == 2 {
            return rc4(&self.encryption_key, &PAD);
        }
        let mut hasher = md5::Context::new();
        hasher.consume(PAD);
        hasher.consume(file_id);
        let seed = hasher.finalize().0;

        let mut out = rc4(&self.encryption_key, &seed);
        for round in 1..=19u8 {
            let key: Vec<u8> = self.encryption_key.iter().map(|b| b ^ round).collect();
            out = rc4(&key, &out);
        }
        out
    }

    /// Whether `/EncryptMetadata` leaves the metadata stream encrypted.
    pub fn should_decrypt_metadata(&self) -> bool {
        self.encrypt_metadata
    }

    fn derive_object_key(&self, obj_id: u32, gen_num: u16) -> Vec<u8> {
        if self.revision >= 5 {
            // ISO 32000-2 Clause 7.6.4.3.4: "For Revision 5 and later, the encryption key
            // shall be used directly to decrypt the stream or string data... without further derivation."
            return self.encryption_key.clone();
        }

        let mut key = self.encryption_key.clone();
        key.extend_from_slice(&obj_id.to_le_bytes()[..3]);
        key.extend_from_slice(&gen_num.to_le_bytes()[..2]);

        // Revision 4 (AES-128) specifically requires appending "sAlT"
        if self.is_aes && self.revision == 4 {
            key.extend_from_slice(b"sAlT");
        }

        let hash = md5::compute(&key);
        // AES-128 uses a 16-byte key (plus 5 bytes for derivation, then hashed)
        // The output of MD5 is 16 bytes.
        hash.0.to_vec()
    }

    /// Encrypts stream data for the given indirect object.
    pub fn encrypt_stream(&self, data: &[u8], obj_id: u32, gen_num: u16) -> SyntaxResult<Vec<u8>> {
        let key = self.derive_object_key(obj_id, gen_num);
        self.encrypt_with_key(data, &key)
    }

    /// Encrypts string data for the given indirect object.
    pub fn encrypt_string(&self, data: &[u8], obj_id: u32, gen_num: u16) -> SyntaxResult<Vec<u8>> {
        let key = self.derive_object_key(obj_id, gen_num);
        self.encrypt_with_key(data, &key)
    }

    /// Decrypts using a key derived without the per-object salt.
    pub fn decrypt_bytes_salted_no_salt(
        &self,
        data: &[u8],
        object_id: u32,
        generation: u16,
    ) -> SyntaxResult<Vec<u8>> {
        // Pattern C: Master Key + ObjID + GenID (No "sAlT")
        let mut key = self.encryption_key.clone();
        key.extend_from_slice(&object_id.to_le_bytes()[..3]);
        key.extend_from_slice(&generation.to_le_bytes()[..2]);
        let hash = md5::compute(&key);
        let n = if self.is_aes { 16 } else { self.encryption_key.len() };
        self.decrypt_with_key(data, &hash[..n])
    }

    /// Decrypts using a key salted with the object and generation numbers.
    pub fn decrypt_bytes_with_salting(
        &self,
        data: &[u8],
        object_id: u32,
        generation: u16,
    ) -> SyntaxResult<Vec<u8>> {
        let mut key = self.encryption_key.clone();
        key.extend_from_slice(&object_id.to_le_bytes()[..3]);
        key.extend_from_slice(&generation.to_le_bytes()[..2]);
        if self.is_aes {
            key.extend_from_slice(b"sAlT");
        }
        let hash = md5::compute(&key);
        let n = if self.is_aes { 16 } else { self.encryption_key.len() };
        self.decrypt_with_key(data, &hash[..n])
    }

    /// Decrypts, skipping the metadata stream exemption.
    pub fn decrypt_bytes_no_metadata(
        &self,
        data: &[u8],
        _object_id: u32,
        _generation: u16,
    ) -> SyntaxResult<Vec<u8>> {
        if let Some(ref inputs) = self.v4_inputs
            && let Ok(key) = Self::derive_v4_key(
                &inputs.password,
                &inputs.o,
                &inputs.u,
                inputs.p,
                &inputs.file_id,
                false,
            )
        {
            return self.decrypt_with_key(data, &key);
        }
        Err(SyntaxError::Crypto("V4 inputs not available".into()))
    }

    /// Decrypts data for the given indirect object.
    pub fn decrypt_bytes(
        &self,
        data: &[u8],
        object_id: u32,
        generation: u16,
    ) -> SyntaxResult<Vec<u8>> {
        let key = self.derive_object_key(object_id, generation);
        self.decrypt_with_key(data, &key)
    }

    fn decrypt_block_aes(
        &self,
        cipher128: Option<&Aes128>,
        cipher256: Option<&Aes256>,
        block_ref: &mut Block,
    ) {
        if let Some(c) = cipher128 {
            c.decrypt_block(block_ref);
        } else if let Some(c) = cipher256 {
            c.decrypt_block(block_ref);
        }
    }

    fn remove_pkcs7_padding(&self, result: &mut Vec<u8>) {
        if let Some(&last_byte) = result.last() {
            let pad_len = last_byte as usize;
            if pad_len > 0
                && pad_len <= 16
                && result.len() >= pad_len
                && result[result.len() - pad_len..].iter().all(|&b| b == last_byte)
            {
                result.truncate(result.len() - pad_len);
            }
        }
    }

    #[allow(clippy::manual_is_multiple_of)]
    fn decrypt_with_key(&self, data: &[u8], key: &[u8]) -> SyntaxResult<Vec<u8>> {
        if data.len() < 16 {
            return Ok(data.to_vec());
        }
        let iv = &data[..16];
        let ciphertext = &data[16..];
        if ciphertext.is_empty() || ciphertext.len() % 16 != 0 {
            return Ok(data.to_vec());
        }

        let mut result = Vec::with_capacity(ciphertext.len());
        let mut prev_block = [0u8; 16];
        prev_block.copy_from_slice(iv);

        let (cipher128, cipher256) = match key.len() {
            16 => (
                Some(
                    Aes128::new_from_slice(key)
                        .map_err(|_| SyntaxError::Crypto("AES-128 init fail".into()))?,
                ),
                None,
            ),
            32 => (
                None,
                Some(
                    Aes256::new_from_slice(key)
                        .map_err(|_| SyntaxError::Crypto("AES-256 init fail".into()))?,
                ),
            ),
            _ => {
                return Err(SyntaxError::Crypto(
                    format!("Invalid AES key length: {}", key.len()).into(),
                ));
            }
        };

        for chunk in ciphertext.chunks(16) {
            let mut block = [0u8; 16];
            block.copy_from_slice(chunk);
            let block_ref = Block::from_mut_slice(&mut block);
            self.decrypt_block_aes(cipher128.as_ref(), cipher256.as_ref(), block_ref);
            for i in 0..16 {
                block[i] ^= prev_block[i];
            }
            result.extend_from_slice(&block);
            prev_block.copy_from_slice(chunk);
        }

        self.remove_pkcs7_padding(&mut result);
        Ok(result)
    }

    fn encrypt_with_key(&self, data: &[u8], key: &[u8]) -> SyntaxResult<Vec<u8>> {
        use rand::RngCore;
        let mut iv = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut iv);

        let mut result = Vec::with_capacity(iv.len() + data.len() + 16);
        result.extend_from_slice(&iv);

        // PKCS#7 Padding
        let pad_len = 16 - (data.len() % 16);
        let mut padded_data = data.to_vec();
        #[allow(clippy::cast_possible_truncation)]
        padded_data.extend(vec![pad_len as u8; pad_len]);

        let (cipher128, cipher256) = match key.len() {
            16 => (
                Some(
                    Aes128::new_from_slice(key)
                        .map_err(|_| SyntaxError::Crypto("AES-128 init fail".into()))?,
                ),
                None,
            ),
            32 => (
                None,
                Some(
                    Aes256::new_from_slice(key)
                        .map_err(|_| SyntaxError::Crypto("AES-256 init fail".into()))?,
                ),
            ),
            _ => {
                return Err(SyntaxError::Crypto(
                    format!("Invalid AES key length: {}", key.len()).into(),
                ));
            }
        };

        let mut prev_block = [0u8; 16];
        prev_block.copy_from_slice(&iv);

        for chunk in padded_data.chunks(16) {
            let mut block = [0u8; 16];
            block.copy_from_slice(chunk);
            for i in 0..16 {
                block[i] ^= prev_block[i];
            }
            let block_ref = Block::from_mut_slice(&mut block);
            if let Some(c) = &cipher128 {
                c.encrypt_block(block_ref);
            } else if let Some(c) = &cipher256 {
                c.encrypt_block(block_ref);
            }
            result.extend_from_slice(&block);
            prev_block.copy_from_slice(&block);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `/Encrypt` values of `samples/unicode_16.pdf`, which is V4/R4 with `/CFM
    /// /AESV2`, `/Length 128` and an empty user password. Written out rather than read
    /// from the file so the test stays self-contained, as every other test here does.
    const O: &str = "fd2c3d3ce19144d01850580c7870bd45fba3474163aac53f0647ad421d4d7030";
    const U: &str = "18ff103cead285b9cf3b3b9694b40cc328bf4e5e4e758a4164004e56fffa0108";
    const ID: &str = "3232363431643233306330623665393339323565656363616430313364346533";
    /// `/P 4294966260`, which is these 32 bits read as signed.
    const P: i32 = -1036;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    fn hexs(bytes: &[u8]) -> String {
        use std::fmt::Write;
        bytes.iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    }

    fn handler_for(password: &str) -> SecurityHandler {
        SecurityHandler::new_v4(password, &hex(O), &hex(U), P, &hex(ID), true).expect("builds")
    }

    #[test]
    fn algorithm_2_matches_an_independent_derivation() {
        // Computed separately with CommonCrypto against ISO 32000-2 Algorithm 2, which
        // is how the /P defect was isolated: the key was right whenever /P was, and the
        // caller was passing 0.
        assert_eq!(hexs(handler_for("").file_key()), "d889527373ba8d339c29e3d0d0f7a3c9");
    }

    #[test]
    fn the_right_password_validates_and_a_wrong_one_does_not() {
        // Algorithm 6. Without it a wrong password derives a wrong key, every object
        // decrypts to noise, and the document still reports 1,140 pages.
        assert!(handler_for("").user_password_matches(&hex(U), &hex(ID)));
        assert!(!handler_for("wrong").user_password_matches(&hex(U), &hex(ID)));
        assert!(!handler_for("also wrong").user_password_matches(&hex(U), &hex(ID)));
    }

    #[test]
    fn a_wrong_permissions_value_changes_the_key() {
        // The defect itself, as a unit: /P is hashed into the key, so reading
        // 4294966260 as "unrepresentable, use 0" silently produced a different key.
        let right = handler_for("").file_key().to_vec();
        let wrong = SecurityHandler::new_v4("", &hex(O), &hex(U), 0, &hex(ID), true)
            .expect("builds")
            .file_key()
            .to_vec();
        assert_ne!(right, wrong);
    }

    #[test]
    fn rc4_round_trips_and_matches_a_published_vector() {
        // RFC 6229's first vector for the 40-bit key 0x0102030405.
        let key = [0x01, 0x02, 0x03, 0x04, 0x05];
        let stream = rc4(&key, &[0u8; 16]);
        assert_eq!(stream[0], 0xb2);
        assert_eq!(stream[1], 0x39);
        assert_eq!(stream[2], 0x63);
        assert_eq!(stream[3], 0x05);
        assert_eq!(rc4(&key, &stream), vec![0u8; 16], "RC4 is its own inverse");
    }
}
