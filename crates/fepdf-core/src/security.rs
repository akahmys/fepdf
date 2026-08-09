use crate::{PdfError, PdfResult};
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use aes::{Aes128, Aes256, Block};
use md5;
use sha2::{Digest, Sha256};

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
    ) -> PdfResult<Self> {
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
    ) -> PdfResult<Vec<u8>> {
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
    pub fn new_v5(user_password: &str, _owner_password: &str, file_id: &[u8]) -> PdfResult<Self> {
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

    pub fn encrypt_stream(&self, data: &[u8], obj_id: u32, gen_num: u16) -> PdfResult<Vec<u8>> {
        let key = self.derive_object_key(obj_id, gen_num);
        self.encrypt_with_key(data, &key)
    }

    pub fn encrypt_string(&self, data: &[u8], obj_id: u32, gen_num: u16) -> PdfResult<Vec<u8>> {
        let key = self.derive_object_key(obj_id, gen_num);
        self.encrypt_with_key(data, &key)
    }

    pub fn decrypt_bytes_salted_no_salt(
        &self,
        data: &[u8],
        object_id: u32,
        generation: u16,
    ) -> PdfResult<Vec<u8>> {
        // Pattern C: Master Key + ObjID + GenID (No "sAlT")
        let mut key = self.encryption_key.clone();
        key.extend_from_slice(&object_id.to_le_bytes()[..3]);
        key.extend_from_slice(&generation.to_le_bytes()[..2]);
        let hash = md5::compute(&key);
        let n = if self.is_aes { 16 } else { self.encryption_key.len() };
        self.decrypt_with_key(data, &hash[..n])
    }

    pub fn decrypt_bytes_with_salting(
        &self,
        data: &[u8],
        object_id: u32,
        generation: u16,
    ) -> PdfResult<Vec<u8>> {
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

    pub fn decrypt_bytes_no_metadata(
        &self,
        data: &[u8],
        _object_id: u32,
        _generation: u16,
    ) -> PdfResult<Vec<u8>> {
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
        Err(PdfError::Other("V4 inputs not available".into()))
    }

    pub fn decrypt_bytes(
        &self,
        data: &[u8],
        object_id: u32,
        generation: u16,
    ) -> PdfResult<Vec<u8>> {
        let key = self.derive_object_key(object_id, generation);
        self.decrypt_with_key(data, &key)
    }

    fn decrypt_block_aes(
        &self,
        cipher128: &Option<Aes128>,
        cipher256: &Option<Aes256>,
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
    fn decrypt_with_key(&self, data: &[u8], key: &[u8]) -> PdfResult<Vec<u8>> {
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

        let (cipher128, cipher256) = if key.len() == 16 {
            (
                Some(
                    Aes128::new_from_slice(key)
                        .map_err(|_| PdfError::Other("AES-128 init fail".into()))?,
                ),
                None,
            )
        } else {
            (
                None,
                Some(
                    Aes256::new_from_slice(key)
                        .map_err(|_| PdfError::Other("AES-256 init fail".into()))?,
                ),
            )
        };

        for chunk in ciphertext.chunks(16) {
            let mut block = [0u8; 16];
            block.copy_from_slice(chunk);
            let block_ref = Block::from_mut_slice(&mut block);
            self.decrypt_block_aes(&cipher128, &cipher256, block_ref);
            for i in 0..16 {
                block[i] ^= prev_block[i];
            }
            result.extend_from_slice(&block);
            prev_block.copy_from_slice(chunk);
        }

        self.remove_pkcs7_padding(&mut result);
        Ok(result)
    }

    fn encrypt_with_key(&self, data: &[u8], key: &[u8]) -> PdfResult<Vec<u8>> {
        use rand::RngCore;
        let mut iv = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut iv);

        let mut result = Vec::with_capacity(iv.len() + data.len() + 16);
        result.extend_from_slice(&iv);

        // PKCS#7 Padding
        let pad_len = 16 - (data.len() % 16);
        let mut padded_data = data.to_vec();
        padded_data.extend(vec![pad_len as u8; pad_len]);

        let (cipher128, cipher256) = if key.len() == 16 {
            (
                Some(
                    Aes128::new_from_slice(key)
                        .map_err(|_| PdfError::Other("AES-128 init fail".into()))?,
                ),
                None,
            )
        } else {
            (
                None,
                Some(
                    Aes256::new_from_slice(key)
                        .map_err(|_| PdfError::Other("AES-256 init fail".into()))?,
                ),
            )
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
