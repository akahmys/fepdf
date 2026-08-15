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

/// The `/Encrypt` values an AES-256 document derives its key from (7.6.4.4).
#[derive(Debug, Clone)]
pub struct AesV5Spec<'a> {
    /// `/U`: 32-byte hash, 8-byte validation salt, 8-byte key salt.
    pub u: &'a [u8],
    /// `/UE`: the file key, wrapped under the user password.
    pub ue: &'a [u8],
    /// `/O`, laid out like `/U` but hashed with the 48-byte `/U` appended.
    pub o: &'a [u8],
    /// `/OE`: the file key, wrapped under the owner password.
    pub oe: &'a [u8],
    /// `/R`: 5 for the Adobe extension, 6 for what PDF 2.0 standardised.
    pub revision: i32,
    /// `/EncryptMetadata`.
    pub encrypt_metadata: bool,
}

/// The hash Algorithm 2.A calls for at this revision.
///
/// Revision 5 is Adobe's original extension and hashes once. Revision 6 is what ISO
/// 32000-2 standardised and runs Algorithm 2.B, which exists so the derivation cannot
/// be parallelised: each round picks its next hash function from the round before.
fn hash_2a(revision: i32, password: &[u8], salt: &[u8], udata: &[u8]) -> Vec<u8> {
    let mut first = Sha256::new();
    first.update(password);
    first.update(salt);
    first.update(udata);
    let seed: [u8; 32] = first.finalize().into();
    if revision < 6 {
        return seed.to_vec();
    }
    hash_2b(password, udata, seed)
}

/// Algorithm 2.B, 7.6.4.3.4, transcribed from the clause.
fn hash_2b(password: &[u8], udata: &[u8], seed: [u8; 32]) -> Vec<u8> {
    let mut k: Vec<u8> = seed.to_vec();
    let mut round: u64 = 0;
    loop {
        // (a) K1 is 64 repetitions of the password, K, and — only when checking an
        // owner password — the 48-byte /U.
        let mut k1 = Vec::with_capacity(64 * (password.len() + k.len() + udata.len()));
        for _ in 0..64 {
            k1.extend_from_slice(password);
            k1.extend_from_slice(&k);
            k1.extend_from_slice(udata);
        }

        // (b) AES-128 in CBC with no padding, keyed and initialised by the halves of K.
        let Some(e) = aes128_cbc_encrypt_no_padding(&k[..16], &k[16..32], &k1) else {
            return k;
        };

        // (c) The first 16 bytes of E as a big-endian integer, modulo 3. 256 is 1 mod 3,
        // so the byte sum has the same remainder and needs no wide arithmetic.
        let remainder = e[..16].iter().map(|b| u32::from(*b)).sum::<u32>() % 3;
        k = match remainder {
            0 => Sha256::digest(&e).to_vec(),
            1 => sha2::Sha384::digest(&e).to_vec(),
            _ => sha2::Sha512::digest(&e).to_vec(),
        };

        round += 1;
        // (e), (f): from round 64, stop once the last byte of E is at most round - 32.
        if round >= 64 && u64::from(e[e.len() - 1]) <= round - 32 {
            k.truncate(32);
            return k;
        }
    }
}

/// AES-128 CBC encryption with no padding, for Algorithm 2.B step (b).
fn aes128_cbc_encrypt_no_padding(key: &[u8], iv: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if !data.len().is_multiple_of(16) {
        return None;
    }
    let cipher = Aes128::new_from_slice(key).ok()?;
    let mut previous: [u8; 16] = iv.try_into().ok()?;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(16) {
        let mut block = [0u8; 16];
        for (i, byte) in chunk.iter().enumerate() {
            block[i] = byte ^ previous[i];
        }
        cipher.encrypt_block(Block::from_mut_slice(&mut block));
        previous = block;
        out.extend_from_slice(&block);
    }
    Some(out)
}

/// AES-256 CBC decryption with a zero initialisation vector and no padding, which is
/// how `/UE` and `/OE` wrap the file key (2.A steps (d) and (e)).
fn aes256_cbc_decrypt_no_padding(key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if !data.len().is_multiple_of(16) {
        return None;
    }
    let cipher = Aes256::new_from_slice(key).ok()?;
    let mut previous = [0u8; 16];
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(16) {
        let mut block: [u8; 16] = chunk.try_into().ok()?;
        let saved = block;
        cipher.decrypt_block(Block::from_mut_slice(&mut block));
        for (i, byte) in block.iter_mut().enumerate() {
            *byte ^= previous[i];
        }
        previous = saved;
        out.extend_from_slice(&block);
    }
    Some(out)
}

/// AES-256 ECB decryption, which `/Perms` uses (2.A step (f)).
fn aes256_ecb_decrypt(key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes256::new_from_slice(key).ok()?;
    let mut block: [u8; 16] = data.get(..16)?.try_into().ok()?;
    cipher.decrypt_block(Block::from_mut_slice(&mut block));
    Some(block.to_vec())
}

/// A security handler for PDF encryption.
#[derive(Clone)]
pub struct SecurityHandler {
    encryption_key: Vec<u8>,
    revision: i32,
    is_aes: bool,
    encrypt_metadata: bool,
}

/// Which cipher the standard security handler applies to strings and streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cipher {
    /// RC4, for `/V` 1 and 2, and for `/V 4` under `/CFM /V2`.
    Rc4,
    /// AES in CBC mode, for `/CFM /AESV2` and `/AESV3`.
    Aes,
}

/// The `/Encrypt` dictionary values the standard handler derives its key from.
///
/// A struct rather than nine arguments, because they arrive together, from one
/// dictionary, and getting two of them the same type in the wrong order is a defect
/// with no compiler to catch it.
#[derive(Debug, Clone)]
pub struct StandardSpec<'a> {
    /// `/O`.
    pub owner: &'a [u8],
    /// `/P`, as the signed 32-bit value 7.6.4.2 defines.
    pub permissions: i32,
    /// The first element of the trailer's `/ID`.
    pub file_id: &'a [u8],
    /// `/EncryptMetadata`, which defaults to true.
    pub encrypt_metadata: bool,
    /// `/R`.
    pub revision: i32,
    /// `/Length` in **bytes**: 5 for the 40-bit default of `/V 1`, `/Length / 8` else.
    pub key_len: usize,
    /// From `/CFM` where a crypt filter is present, [`Cipher::Rc4`] otherwise.
    ///
    /// Independent of `/V`: reading the cipher from `/V` alone takes `/V 4 /CFM /V2`
    /// for AES, and that file is RC4.
    pub cipher: Cipher,
}

impl SecurityHandler {
    /// Creates a handler for the standard security handler's password revisions.
    pub fn new_standard(user_password: &str, spec: &StandardSpec<'_>) -> SyntaxResult<Self> {
        if !(1..=16).contains(&spec.key_len) {
            return Err(SyntaxError::Crypto(
                format!(
                    "key length {} is outside the 40..128-bit range 7.6.4.2 allows",
                    spec.key_len
                )
                .into(),
            ));
        }
        Ok(Self {
            encryption_key: Self::derive_file_key(user_password, spec),
            revision: spec.revision,
            is_aes: spec.cipher == Cipher::Aes,
            encrypt_metadata: spec.encrypt_metadata,
        })
    }

    /// Creates a handler for AES-128 (`/V 4`, `/R 4`, `/CFM /AESV2`).
    pub fn new_v4(
        user_password: &str,
        o_string: &[u8],
        _u_string: &[u8],
        p_value: i32,
        file_id: &[u8],
        encrypt_metadata: bool,
    ) -> SyntaxResult<Self> {
        Self::new_standard(
            user_password,
            &StandardSpec {
                owner: o_string,
                permissions: p_value,
                file_id,
                encrypt_metadata,
                revision: 4,
                key_len: 16,
                cipher: Cipher::Aes,
            },
        )
    }

    /// The file encryption key: ISO 32000-2, Algorithm 2.
    fn derive_file_key(user_password: &str, spec: &StandardSpec<'_>) -> Vec<u8> {
        let mut pad = PAD;
        let pw_bytes = user_password.as_bytes();
        let len = pw_bytes.len().min(32);
        pad[..len].copy_from_slice(&pw_bytes[..len]);
        pad[len..].copy_from_slice(&PAD[..32 - len]);

        let mut hasher = md5::Context::new();
        hasher.consume(pad);
        hasher.consume(spec.owner);
        hasher.consume(spec.permissions.to_le_bytes());
        hasher.consume(spec.file_id);

        if spec.revision >= 4 && !spec.encrypt_metadata {
            hasher.consume([0xFF, 0xFF, 0xFF, 0xFF]);
        }

        let mut hash = hasher.finalize().0;
        // Step (h): revision 2 stops here; 3 and later iterate over the first n bytes,
        // which is where `key_len` first matters — hashing all 16 regardless produces
        // the right answer only for a 128-bit key.
        if spec.revision >= 3 {
            for _ in 0..50 {
                let mut h2 = md5::Context::new();
                h2.consume(&hash[..spec.key_len]);
                hash = h2.finalize().0;
            }
        }

        hash[..spec.key_len].to_vec()
    }

    /// Retrieves the file encryption key from an AES-256 document: Algorithm 2.A.
    ///
    /// Tries the password as the user's, then as the owner's, and returns `None` when
    /// it is neither. Nothing here uses `/ID` — 7.6.4.3.3 deliberately drops it, which
    /// is why an incremental update cannot invalidate the key.
    ///
    /// **SASLprep is not applied.** Step (a) of 2.A calls for RFC 4013 normalisation
    /// before the UTF-8 conversion; this truncates to 127 bytes (step b) and otherwise
    /// takes the string as given. Passwords outside ASCII may therefore fail where a
    /// conforming reader succeeds, and that is a real gap rather than a rounding.
    pub fn new_aes256(password: &str, spec: &AesV5Spec<'_>) -> Option<Self> {
        if spec.u.len() < 48 || spec.o.len() < 48 || spec.ue.len() < 32 || spec.oe.len() < 32 {
            return None;
        }
        let pw = &password.as_bytes()[..password.len().min(127)];

        let key = if hash_2a(spec.revision, pw, &spec.u[32..40], &[]) == spec.u[..32] {
            // Steps (a), (b), (e): the user password.
            let intermediate = hash_2a(spec.revision, pw, &spec.u[40..48], &[]);
            aes256_cbc_decrypt_no_padding(&intermediate, spec.ue)?
        } else if hash_2a(spec.revision, pw, &spec.o[32..40], &spec.u[..48]) == spec.o[..32] {
            // Steps (c), (d): the owner password, which hashes the 48-byte /U with it.
            let intermediate = hash_2a(spec.revision, pw, &spec.o[40..48], &spec.u[..48]);
            aes256_cbc_decrypt_no_padding(&intermediate, spec.oe)?
        } else {
            return None;
        };

        Some(Self {
            encryption_key: key,
            revision: spec.revision,
            is_aes: true,
            encrypt_metadata: spec.encrypt_metadata,
        })
    }

    /// Checks `/Perms` against `/P`: Algorithm 2.A step (f).
    ///
    /// A file whose `/Perms` does not decrypt to its own `/P` has had its permissions
    /// edited without the key, which the standard makes detectable precisely so that
    /// stripping them is not silent.
    #[must_use]
    pub fn perms_agree(&self, perms: &[u8], declared: i32) -> bool {
        if perms.len() < 16 {
            return false;
        }
        let Some(plain) = aes256_ecb_decrypt(&self.encryption_key, &perms[..16]) else {
            return false;
        };
        if &plain[9..12] != b"adb" {
            return false;
        }
        i32::from_le_bytes([plain[0], plain[1], plain[2], plain[3]]) == declared
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
    /// Revisions 5 and 6 are not covered here because they validate as part of
    /// deriving the key: [`Self::new_aes256`] returns `None` when the password matches
    /// neither `/U` nor `/O`, so there is no unvalidated handler to check afterwards.
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

    /// The per-object key: ISO 32000-2, Algorithm 1.
    fn derive_object_key(&self, obj_id: u32, gen_num: u16) -> Vec<u8> {
        if self.revision >= 5 {
            // ISO 32000-2 Clause 7.6.4.3.4: "For Revision 5 and later, the encryption key
            // shall be used directly to decrypt the stream or string data... without further derivation."
            return self.encryption_key.clone();
        }

        let mut key = self.encryption_key.clone();
        key.extend_from_slice(&obj_id.to_le_bytes()[..3]);
        key.extend_from_slice(&gen_num.to_le_bytes()[..2]);

        // Step (b): AES adds the four bytes 73 41 6C 54, whatever the revision.
        if self.is_aes {
            key.extend_from_slice(b"sAlT");
        }

        let hash = md5::compute(&key);
        // Step (d): the object key is n + 5 bytes, to a maximum of 16. Returning all
        // 16 regardless is correct only for a 128-bit file key, and silently wrong for
        // the 40-bit default of `/V 1`.
        let n = (self.encryption_key.len() + 5).min(16);
        hash.0[..n].to_vec()
    }

    /// Encrypts stream data for the given indirect object.
    pub fn encrypt_stream(&self, data: &[u8], obj_id: u32, gen_num: u16) -> SyntaxResult<Vec<u8>> {
        let key = self.derive_object_key(obj_id, gen_num);
        self.encrypt_with_key(data, &key)
    }

    // Three further `decrypt_bytes_*` variants stood here — one without the per-object
    // salt, one with, one recomputing the key as though `/EncryptMetadata` were false.
    // None had a caller. Alternative key derivations sitting in a crypto module are a
    // hazard rather than an option: `decrypt_bytes` below is Algorithm 1, and there is
    // no second answer to pick from.

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
        // RC4 is a stream cipher: no IV, no block alignment, no padding, and the
        // ciphertext is the same length as the plaintext. Falling through to the AES
        // path below would have read its first sixteen bytes as an IV.
        if !self.is_aes {
            return Ok(rc4(key, data));
        }
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

        let (cipher128, cipher256) = Self::aes_ciphers(key)?;

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

    /// The AES cipher for a key of the one length that names it. Shared by both
    /// directions, which had the same twenty lines each.
    fn aes_ciphers(key: &[u8]) -> SyntaxResult<(Option<Aes128>, Option<Aes256>)> {
        match key.len() {
            16 => Ok((
                Some(
                    Aes128::new_from_slice(key)
                        .map_err(|_| SyntaxError::Crypto("AES-128 init fail".into()))?,
                ),
                None,
            )),
            32 => Ok((
                None,
                Some(
                    Aes256::new_from_slice(key)
                        .map_err(|_| SyntaxError::Crypto("AES-256 init fail".into()))?,
                ),
            )),
            other => Err(SyntaxError::Crypto(format!("Invalid AES key length: {other}").into())),
        }
    }

    fn encrypt_with_key(&self, data: &[u8], key: &[u8]) -> SyntaxResult<Vec<u8>> {
        if !self.is_aes {
            return Ok(rc4(key, data));
        }
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

        let (cipher128, cipher256) = Self::aes_ciphers(key)?;

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

    /// Reference values for RC4 revisions, computed by `scripts/test/make_encrypted.py`,
    /// which implements Algorithms 1 to 5 from the standard with only hashlib. Whatever
    /// these assert, they assert independently of the code under test.
    mod rc4_revisions {
        use super::*;

        const FILE_ID: &str = "00112233445566778899aabbccddeeff";
        const P: i32 = -4;

        fn rc4_handler(owner: &[u8], revision: i32, key_len: usize) -> SecurityHandler {
            SecurityHandler::new_standard(
                "",
                &StandardSpec {
                    owner,
                    permissions: P,
                    file_id: &hex(FILE_ID),
                    encrypt_metadata: true,
                    revision,
                    key_len,
                    cipher: Cipher::Rc4,
                },
            )
            .expect("builds")
        }

        #[test]
        fn revision_2_derives_a_forty_bit_key() {
            let o = hex("2055c756c72e1ad702608e8196acad447ad32d17cff583235f6dd15fed7dab67");
            let u = hex("e6f8e044f4f9dc0158c31560c11e2dbda4fe3487341bc7a11c77e297701a83cc");
            let h = rc4_handler(&o, 2, 5);
            assert_eq!(hexs(h.file_key()), "85fe7c5d51", "Algorithm 2, no iteration at R2");
            assert!(h.user_password_matches(&u, &hex(FILE_ID)), "Algorithm 4");
            assert!(!h.file_key().is_empty());
        }

        #[test]
        fn revision_3_derives_a_hundred_and_twenty_eight_bit_key() {
            let o = hex("36451bd39d753b7c1d10922c28e6665aa4f3353fb0348b536893e3b1db5c579b");
            let u = hex("1e901c0cd9f1675370edfbad8a6fe84028bf4e5e4e758a4164004e56fffa0108");
            let h = rc4_handler(&o, 3, 16);
            assert_eq!(hexs(h.file_key()), "6e6c4cba9f2e6bcb27e743d91e5fabc5");
            assert!(h.user_password_matches(&u, &hex(FILE_ID)), "Algorithm 5");
            assert!(!h.user_password_matches(&hex(&"aa".repeat(32)), &hex(FILE_ID)));
        }

        #[test]
        fn an_rc4_stream_round_trips_at_its_own_length() {
            // RC4 adds no IV and no padding, which is why `decrypt_with_key` must not
            // reach the AES path: that reads the first sixteen bytes as an IV.
            let o = hex("36451bd39d753b7c1d10922c28e6665aa4f3353fb0348b536893e3b1db5c579b");
            let h = rc4_handler(&o, 3, 16);
            let plain = b"BT /F1 12 Tf (hello) Tj ET";
            let cipher = h.encrypt_stream(plain, 7, 0).expect("encrypts");
            assert_eq!(cipher.len(), plain.len(), "a stream cipher changes no lengths");
            assert_eq!(h.decrypt_bytes(&cipher, 7, 0).expect("decrypts"), plain);
        }

        #[test]
        fn the_object_key_is_n_plus_five_bytes() {
            // Algorithm 1 step (d). Returning all sixteen is right only for a 128-bit
            // file key, and was what the code did for every key length.
            let o = hex("2055c756c72e1ad702608e8196acad447ad32d17cff583235f6dd15fed7dab67");
            let h = rc4_handler(&o, 2, 5);
            assert_eq!(hexs(&h.derive_object_key(7, 0)), "aff63a256e8c21ed8b5c");
            assert_eq!(h.derive_object_key(7, 0).len(), 10, "5 + 5");
        }
    }

    /// Algorithms 2.A and 2.B against `scripts/test/make_encrypted.py`, which
    /// transcribes them from ISO 32000-2 with only `hashlib` and a pure-Python AES.
    /// The AES there is itself checked against FIPS-197, so a disagreement below means
    /// one of the two transcriptions is wrong rather than both being wrong together.
    mod aes256 {
        use super::*;

        const SALT: &str = "0011223344556677";

        #[test]
        fn algorithm_2b_matches_an_independent_transcription() {
            assert_eq!(
                hexs(&hash_2b(b"", b"", sha256_of(b"", &hex(SALT), b""))),
                "c2e2dc9383748384b0ef05fdbc769d3879a739cffd5a05a7d6bb2958d0f9ccf0"
            );
            assert_eq!(
                hexs(&hash_2a(6, b"password", &hex(SALT), b"")),
                "03ccc9a6b2caf2fa710326f2b867dfe523a6006e711411738233fa1831db58fb"
            );
        }

        #[test]
        fn the_owner_path_folds_the_forty_eight_byte_u_into_the_hash() {
            // 2.A step (c). Omitting /U makes every owner password fail, and nothing
            // else would notice, because the user path is tried first.
            let udata: Vec<u8> = (0..48u8).collect();
            assert_eq!(
                hexs(&hash_2a(6, b"", &hex(SALT), &udata)),
                "c7f2cf304927163bf52398502c32b09dd366a5b0192f927a01a7ba3a98dc9fda"
            );
        }

        #[test]
        fn revision_5_hashes_once_where_revision_6_iterates() {
            // Adobe's extension, which PDF 2.0 deprecates but files still use.
            assert_eq!(
                hexs(&hash_2a(5, b"", &hex(SALT), b"")),
                "d1a5f998fa6ed82da6943127533b412f2286b30c8473a819f70a8fec5913fea7"
            );
            assert_ne!(hash_2a(5, b"", &hex(SALT), b""), hash_2a(6, b"", &hex(SALT), b""));
        }

        #[test]
        fn a_short_or_absent_string_refuses_rather_than_guessing() {
            let spec = AesV5Spec {
                u: &[0u8; 8],
                ue: &[0u8; 32],
                o: &[0u8; 48],
                oe: &[0u8; 32],
                revision: 6,
                encrypt_metadata: true,
            };
            assert!(SecurityHandler::new_aes256("", &spec).is_none(), "/U is 48 bytes");
        }

        fn sha256_of(password: &[u8], salt: &[u8], udata: &[u8]) -> [u8; 32] {
            let mut h = Sha256::new();
            h.update(password);
            h.update(salt);
            h.update(udata);
            h.finalize().into()
        }
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
