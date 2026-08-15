//! Pass 0 on the arena: unlocking an encrypted document before anything reads it.
//!
//! ISO 32000-2 clause 7.6. Strings and streams are encrypted with a key derived from
//! the object's own number and generation, so this runs over the arena once the reader
//! has placed every object at its number. The trailer and the `/Encrypt` dictionary
//! itself are never encrypted and are therefore left alone.
//!
//! `/Encrypt` is removed once the document is unlocked. Acrobat reports error 135 for a
//! file whose objects are plain but whose trailer still claims encryption.

use crate::arena::PdfArena;
use crate::error::PdfResult;
use crate::handle::Handle;
use crate::interpretation::{Decision, DecisionLog};
use crate::object::{Object, PdfName, SublimatedData};
use crate::reader::DictHandle;
use bytes::Bytes;
use fepdf_syntax::security::{Cipher, SecurityHandler, StandardSpec};
use std::collections::BTreeMap;

/// What the document said about its own protection.
#[derive(Debug, Clone)]
pub struct Security {
    /// A human-readable name for the handler, as the inspectors report it.
    pub method: String,
    /// The `/P` permission bits, if the document declared any.
    pub permissions: Option<i32>,
}

impl Default for Security {
    fn default() -> Self {
        Self { method: "No Security".to_string(), permissions: None }
    }
}

/// Decrypts every object in place, returning what protection the file used.
///
/// A document with no `/Encrypt` is left untouched and reports "No Security". A
/// document whose handler cannot be built — an unsupported revision, or a wrong
/// password — is also left untouched, and that is recorded rather than raised: the
/// caller can still inspect the file's structure.
pub fn unlock(
    arena: &PdfArena,
    trailer: Option<DictHandle>,
    password: &str,
    decisions: &mut DecisionLog,
) -> PdfResult<Security> {
    let Some(trailer) = trailer else { return Ok(Security::default()) };
    let Some((encrypt, exclude)) = encryption_dict(arena, trailer) else {
        return Ok(Security::default());
    };

    let security = describe(arena, &encrypt);
    let Some(handler) = build_handler(arena, trailer, &encrypt, password) else {
        decisions.push(Decision::violation(
            "7.6.1",
            format!("{} could not be unlocked", security.method),
            "left the document encrypted; its structure is readable but its content is not",
        ));
        return Ok(security);
    };

    for number in 0..arena.object_count() {
        if Some(number) == exclude {
            continue;
        }
        decrypt_object(arena, number, &handler)?;
    }
    remove_encrypt(arena, trailer);
    Ok(security)
}

/// A dictionary read out of the arena, keyed by interned name.
type Dict = BTreeMap<Handle<PdfName>, Object>;

/// The `/Encrypt` dictionary and, when it is indirect, the object number to skip.
fn encryption_dict(arena: &PdfArena, trailer: DictHandle) -> Option<(Dict, Option<u32>)> {
    let entry = entry(arena, trailer, "Encrypt")?;
    match entry {
        Object::Dictionary(h) => Some((arena.get_dict(h)?, None)),
        Object::Reference(h) => match arena.get_object(h)? {
            Object::Dictionary(d) => Some((arena.get_dict(d)?, Some(h.index()))),
            _ => None,
        },
        _ => None,
    }
}

/// Names the handler for reporting, without attempting to unlock anything.
fn describe(arena: &PdfArena, encrypt: &Dict) -> Security {
    let method = match integer(arena, encrypt, "V").unwrap_or(0) {
        5 => "Password Security (AES-256)",
        4 => "Password Security (AES-128)",
        _ => "Password Security (Standard)",
    };
    Security { method: method.to_string(), permissions: permissions(arena, encrypt) }
}

/// The `/P` bits, reinterpreted as the signed 32-bit field 7.6.4.2 defines.
///
/// Producers write these bits either way: Table 22 calls `/P` an integer whose high
/// bit is set for every reserved-and-unused flag, so `-1036` and `4294966260` are the
/// same 32 bits and both appear in the wild. `samples/unicode_16.pdf` writes the
/// unsigned form.
///
/// This was `i32::try_from(value).ok()`, which rejected the unsigned form, and the one
/// caller that matters resolved the `None` with `unwrap_or(0)`. Algorithm 2 hashes
/// `/P` into the file encryption key, so a `/P` of 0 in place of `-1036` produced a
/// different key and the document decrypted to noise — silently, since nothing
/// validates the result.
fn permissions(arena: &PdfArena, encrypt: &Dict) -> Option<i32> {
    let value = integer(arena, encrypt, "P")?;
    i32::try_from(value).ok().or_else(|| u32::try_from(value).ok().map(|bits| bits as i32))
}

/// Builds a handler for the revisions the syntax layer implements.
fn build_handler(
    arena: &PdfArena,
    trailer: DictHandle,
    encrypt: &Dict,
    password: &str,
) -> Option<SecurityHandler> {
    let version = integer(arena, encrypt, "V").unwrap_or(0);
    let revision = integer(arena, encrypt, "R").unwrap_or(0);
    let id = first_file_id(arena, trailer);

    if (version, revision) == (5, 5) || (version, revision) == (5, 6) {
        return SecurityHandler::new_v5(password, "", &id).ok();
    }

    // 7.6.4.2 Table 20: `/V` decides how the key is sized and whether crypt filters
    // apply; `/CFM` decides the cipher. `/V 4` is AES only when `/CFM` says `/AESV2`.
    let (key_len, cipher) = match version {
        1 => (5, Cipher::Rc4),
        2 => (key_bytes(arena, encrypt).unwrap_or(5), Cipher::Rc4),
        4 => (key_bytes(arena, encrypt).unwrap_or(16), crypt_filter_method(arena, encrypt)),
        _ => return None,
    };
    if !(2..=4).contains(&revision) {
        return None;
    }

    let u = string(arena, encrypt, "U").unwrap_or_default();
    let handler = SecurityHandler::new_standard(
        password,
        &StandardSpec {
            owner: &string(arena, encrypt, "O").unwrap_or_default(),
            permissions: permissions(arena, encrypt)?,
            file_id: &id,
            encrypt_metadata: boolean(arena, encrypt, "EncryptMetadata").unwrap_or(true),
            revision: i32::try_from(revision).ok()?,
            key_len,
            cipher,
        },
    )
    .ok()?;

    // Algorithm 6. A wrong password otherwise derives a wrong key and every object
    // decrypts to noise, which the engine reported as thousands of font failures
    // rather than as a refusal.
    handler.user_password_matches(&u, &id).then_some(handler)
}

/// `/Length` in bytes. It is written in bits, and defaults to 40.
fn key_bytes(arena: &PdfArena, encrypt: &Dict) -> Option<usize> {
    let bits = integer(arena, encrypt, "Length")?;
    usize::try_from(bits).ok().map(|b| b / 8).filter(|b| (5..=16).contains(b))
}

/// The cipher named by the default stream crypt filter (7.6.5).
///
/// `/V 4` files exist with `/CFM /V2`, which is RC4 under a crypt filter. Assuming AES
/// from `/V` alone decrypts those to noise, which is what the handler did: `is_aes`
/// was set `true` at both construction sites and no code path could clear it.
fn crypt_filter_method(arena: &PdfArena, encrypt: &Dict) -> Cipher {
    let Some(cf) = entry_in(arena, encrypt, "CF").and_then(|o| as_dict(arena, &o)) else {
        return Cipher::Rc4;
    };
    let name = name_of(arena, encrypt, "StmF").unwrap_or_else(|| "Identity".to_string());
    let Some(filter) = cf.get(&arena.name(&name)).and_then(|o| as_dict(arena, o)) else {
        return Cipher::Rc4;
    };
    match filter.get(&arena.name("CFM")).and_then(|o| match o {
        Object::Name(h) => arena.get_name_str(*h),
        _ => None,
    }) {
        Some(m) if m == "AESV2" || m == "AESV3" => Cipher::Aes,
        Some(_) | None => Cipher::Rc4,
    }
}

fn entry_in(arena: &PdfArena, dict: &Dict, key: &str) -> Option<Object> {
    dict.get(&arena.name(key)).cloned()
}

fn as_dict(arena: &PdfArena, object: &Object) -> Option<Dict> {
    match object {
        Object::Dictionary(h) => arena.get_dict(*h),
        Object::Reference(h) => match arena.get_object(*h)? {
            Object::Dictionary(d) => arena.get_dict(d),
            _ => None,
        },
        _ => None,
    }
}

fn name_of(arena: &PdfArena, dict: &Dict, key: &str) -> Option<String> {
    match dict.get(&arena.name(key))? {
        Object::Name(h) => arena.get_name_str(*h),
        _ => None,
    }
}

/// The first element of the trailer's `/ID`, which keys the encryption.
fn first_file_id(arena: &PdfArena, trailer: DictHandle) -> Vec<u8> {
    let Some(Object::Array(h)) = entry(arena, trailer, "ID") else { return Vec::new() };
    let Some(items) = arena.get_array(h) else { return Vec::new() };
    match items.first() {
        Some(Object::String(b) | Object::Hex(b)) => b.to_vec(),
        _ => Vec::new(),
    }
}

/// Decrypts one indirect object's strings and streams, nested containers included.
fn decrypt_object(arena: &PdfArena, number: u32, handler: &SecurityHandler) -> PdfResult<()> {
    let handle = Handle::new(number);
    let Some(object) = arena.get_object(handle) else { return Ok(()) };
    let generation = 0;
    let replaced = decrypt_value(arena, object, number, generation, handler)?;
    arena.set_object(handle, replaced);
    Ok(())
}

/// Rewrites one value, recursing through arrays and dictionaries.
///
/// Recursion follows the object graph as written, which the parser builds acyclically
/// within a single indirect object; references are not followed, so a cycle between
/// objects cannot be reached from here.
fn decrypt_value(
    arena: &PdfArena,
    object: Object,
    number: u32,
    generation: u16,
    handler: &SecurityHandler,
) -> PdfResult<Object> {
    Ok(match object {
        Object::String(b) => {
            Object::String(Bytes::from(handler.decrypt_bytes(&b, number, generation)?))
        }
        Object::Hex(b) => Object::Hex(Bytes::from(handler.decrypt_bytes(&b, number, generation)?)),
        Object::Array(h) => {
            decrypt_array(arena, h, number, generation, handler)?;
            Object::Array(h)
        }
        Object::Dictionary(h) => {
            decrypt_dict(arena, h, number, generation, handler)?;
            Object::Dictionary(h)
        }
        Object::Stream(h, data) => {
            decrypt_dict(arena, h, number, generation, handler)?;
            Object::Stream(h, decrypt_payload(&data, number, generation, handler)?)
        }
        other => other,
    })
}

/// Decrypts a stream's bytes, leaving anything already sublimated alone.
fn decrypt_payload(
    data: &std::sync::Arc<SublimatedData>,
    number: u32,
    generation: u16,
    handler: &SecurityHandler,
) -> PdfResult<std::sync::Arc<SublimatedData>> {
    let SublimatedData::Raw(bytes) = data.as_ref() else { return Ok(std::sync::Arc::clone(data)) };
    let plain = handler.decrypt_bytes(bytes, number, generation)?;
    Ok(std::sync::Arc::new(SublimatedData::Raw(Bytes::from(plain))))
}

/// Decrypts every element of an array in place.
fn decrypt_array(
    arena: &PdfArena,
    handle: Handle<Vec<Object>>,
    number: u32,
    generation: u16,
    handler: &SecurityHandler,
) -> PdfResult<()> {
    let Some(items) = arena.get_array(handle) else { return Ok(()) };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(decrypt_value(arena, item, number, generation, handler)?);
    }
    arena.set_array(handle, out);
    Ok(())
}

/// Decrypts every value of a dictionary in place, keys untouched.
fn decrypt_dict(
    arena: &PdfArena,
    handle: DictHandle,
    number: u32,
    generation: u16,
    handler: &SecurityHandler,
) -> PdfResult<()> {
    let Some(dict) = arena.get_dict(handle) else { return Ok(()) };
    let mut out = BTreeMap::new();
    for (key, value) in dict {
        out.insert(key, decrypt_value(arena, value, number, generation, handler)?);
    }
    arena.set_dict(handle, out);
    Ok(())
}

/// Drops `/Encrypt` now that the objects behind it are plain.
fn remove_encrypt(arena: &PdfArena, trailer: DictHandle) {
    let Some(mut dict) = arena.get_dict(trailer) else { return };
    dict.remove(&arena.name("Encrypt"));
    arena.set_dict(trailer, dict);
}

/// One entry of a dictionary held in the arena.
fn entry(arena: &PdfArena, dict: DictHandle, key: &str) -> Option<Object> {
    arena.get_dict(dict)?.get(&arena.name(key)).cloned()
}

/// An integer entry, following one level of indirection.
fn integer(arena: &PdfArena, dict: &Dict, key: &str) -> Option<i64> {
    match dict.get(&arena.name(key))? {
        Object::Integer(v) => Some(*v),
        Object::Reference(h) => match arena.get_object(*h)? {
            Object::Integer(v) => Some(v),
            _ => None,
        },
        _ => None,
    }
}

/// A string entry, in either of the two string syntaxes.
fn string(arena: &PdfArena, dict: &Dict, key: &str) -> Option<Vec<u8>> {
    match dict.get(&arena.name(key))? {
        Object::String(b) | Object::Hex(b) => Some(b.to_vec()),
        _ => None,
    }
}

/// A boolean entry.
fn boolean(arena: &PdfArena, dict: &Dict, key: &str) -> Option<bool> {
    match dict.get(&arena.name(key))? {
        Object::Boolean(v) => Some(*v),
        _ => None,
    }
}
