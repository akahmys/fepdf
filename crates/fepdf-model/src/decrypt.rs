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
use fepdf_syntax::security::{Access, AesV5Spec, Cipher, SecurityHandler, StandardSpec};
use std::collections::BTreeMap;

/// What the document said about its own protection.
#[derive(Debug, Clone)]
pub struct Security {
    /// A human-readable name for the handler, as the inspectors report it.
    pub method: String,
    /// The `/P` permission bits, if the document declared any.
    pub permissions: Option<i32>,
    /// Which password authenticated. `None` when the document is not encrypted, or
    /// when no password opened it.
    pub access: Option<Access>,
}

impl Default for Security {
    fn default() -> Self {
        Self { method: "No Security".to_string(), permissions: None, access: None }
    }
}

/// What a reader offers a document to get in.
///
/// A password for the standard handler, and a certificate with its private key for a
/// public-key one (7.6.5). The two are not interchangeable and a document takes exactly
/// one kind, so this is a struct rather than a choice: a caller supplies what it has
/// and the document decides which it needed.
#[derive(Default, Clone, Copy)]
pub struct Credentials<'a> {
    /// The password to try. Every reader starts with the empty one.
    pub password: &'a str,
    /// The identity a public-key encrypted document may have been addressed to.
    pub recipient: Option<&'a crate::cms::RecipientIdentity>,
}

impl<'a> Credentials<'a> {
    /// Just a password, which is what every caller but ingestion has.
    #[must_use]
    pub fn password(password: &'a str) -> Self {
        Self { password, recipient: None }
    }
}

/// Decrypts every object in place, returning what protection the file used.
///
/// A document with no `/Encrypt` is left untouched and reports "No Security". A
/// document whose handler cannot be built — an unsupported revision, a wrong password,
/// or a certificate it was not addressed to — is also left untouched, and that is
/// recorded rather than raised: the caller can still inspect the file's structure.
/// `/Encrypt` is removed once the document is unlocked.
///
/// # Errors
/// If an object cannot be rewritten after decryption.
pub fn unlock(
    arena: &PdfArena,
    trailer: Option<DictHandle>,
    credentials: Credentials<'_>,
    decisions: &mut DecisionLog,
) -> PdfResult<Security> {
    let Some(trailer) = trailer else { return Ok(Security::default()) };
    let Some((encrypt, exclude)) = encryption_dict(arena, trailer) else {
        return Ok(Security::default());
    };

    let mut security = describe(arena, &encrypt);
    let Some(handler) = build_handler(arena, trailer, &encrypt, credentials, decisions) else {
        decisions.push(Decision::violation(
            "7.6.1",
            format!("{} could not be unlocked", security.method),
            "left the document encrypted; its structure is readable but its content is not",
        ));
        return Ok(security);
    };

    security.access = Some(handler.access());

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
    // The handler comes first: `/V` says how the key is sized, not what unlocks the
    // document. Reading only `/V` reported a certificate-encrypted file as "Password
    // Security (AES-256) could not be unlocked", which names the wrong credential and
    // sends the reader looking for a password that does not exist.
    let method = if name_of(arena, encrypt, "Filter").as_deref() == Some("Adobe.PubSec") {
        match integer(arena, encrypt, "V").unwrap_or(0) {
            5 => "Certificate Security (AES-256)",
            4 => "Certificate Security (AES-128)",
            _ => "Certificate Security",
        }
    } else {
        match integer(arena, encrypt, "V").unwrap_or(0) {
            5 => "Password Security (AES-256)",
            4 => "Password Security (AES-128)",
            _ => "Password Security (Standard)",
        }
    };
    Security { method: method.to_string(), permissions: permissions(arena, encrypt), access: None }
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
    credentials: Credentials<'_>,
    decisions: &mut DecisionLog,
) -> Option<SecurityHandler> {
    let version = integer(arena, encrypt, "V").unwrap_or(0);
    let revision = integer(arena, encrypt, "R").unwrap_or(0);
    let id = first_file_id(arena, trailer);
    let password = credentials.password;

    // 7.6.5: a public-key handler names itself in /Filter and derives its key from a
    // seed in /Recipients rather than from a password. It has no /R and no /O or /U,
    // so the checks below would all read absent values.
    if name_of(arena, encrypt, "Filter").as_deref() == Some("Adobe.PubSec") {
        return build_public_key(arena, encrypt, credentials.recipient, decisions);
    }

    if version == 5 && (revision == 5 || revision == 6) {
        return build_aes256(arena, encrypt, &saslprep(password), revision, decisions);
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
    let o = string(arena, encrypt, "O").unwrap_or_default();
    let spec = StandardSpec {
        owner: &o,
        permissions: permissions(arena, encrypt)?,
        file_id: &id,
        encrypt_metadata: boolean(arena, encrypt, "EncryptMetadata").unwrap_or(true),
        revision: i32::try_from(revision).ok()?,
        key_len,
        cipher,
    };

    // Algorithm 6. A wrong password otherwise derives a wrong key and every object
    // decrypts to noise, which the engine reported as thousands of font failures
    // rather than as a refusal.
    let handler = SecurityHandler::new_standard(password, &spec).ok()?;
    if handler.user_password_matches(&u, &id) {
        return Some(handler);
    }

    // Algorithm 7. 7.6.4.1 says either password should open the document, and `/O`
    // holds the user password wrapped under the owner's — so recover it, and derive
    // the key from *that*, since the owner password is not what Algorithm 2 hashes.
    let recovered = SecurityHandler::recover_user_password(password, &o, spec.revision, key_len)?;
    let recovered = String::from_utf8_lossy(&recovered).into_owned();
    let mut owner = SecurityHandler::new_standard(recovered.trim_end_matches('\0'), &spec).ok()?;
    if owner.user_password_matches(&u, &id) {
        owner.set_access(Access::Owner);
        return Some(owner);
    }
    None
}

/// Algorithm 2.A: the AES-256 handler, whose key comes from `/U`, `/UE`, `/O` and
/// `/OE` and from no part of `/ID` — which is what lets an incremental update leave it
/// valid, and why 7.6.4.3.3 encourages it over Algorithm 2.
fn build_aes256(
    arena: &PdfArena,
    encrypt: &Dict,
    password: &str,
    revision: i64,
    decisions: &mut DecisionLog,
) -> Option<SecurityHandler> {
    let (u, ue) = (string(arena, encrypt, "U")?, string(arena, encrypt, "UE")?);
    let (o, oe) = (string(arena, encrypt, "O")?, string(arena, encrypt, "OE")?);
    let handler = SecurityHandler::new_aes256(
        password,
        &AesV5Spec {
            u: &u,
            ue: &ue,
            o: &o,
            oe: &oe,
            revision: i32::try_from(revision).ok()?,
            encrypt_metadata: boolean(arena, encrypt, "EncryptMetadata").unwrap_or(true),
        },
    )?;

    // Step (f). A `/Perms` that does not decrypt to this file's own `/P` means the
    // permissions were edited without the key, which the standard makes detectable so
    // that stripping them cannot be silent.
    if let (Some(perms), Some(declared)) =
        (string(arena, encrypt, "Perms"), permissions(arena, encrypt))
        && !handler.perms_agree(&perms, declared)
    {
        decisions.push(Decision::violation(
            "7.6.4.3.3",
            "/Perms does not decrypt to the /P this file declares",
            "unlocked the document anyway; its permissions have been altered without the key",
        ));
    }
    Some(handler)
}

/// SASLprep (RFC 4013), as 7.6.4.3.3 step (a) requires before the UTF-8 conversion.
///
/// Applied here rather than in `fepdf-syntax` on purpose: that crate is the byte layer,
/// and keeping Unicode tables out of it is what lets the cryptography be read on its
/// own (`ARCHITECTURE.md` §3). A password is a string until the moment it is hashed.
///
/// **Partial, and knowingly so.** The substantive half of the profile is the NFKC
/// normalisation, which is done. Also done are the two mappings that change what a user
/// can type: RFC 3454 table B.1 characters map to nothing, and table C.1.2 non-ASCII
/// spaces map to `U+0020`. Not done: the prohibited-output tables and the bidi checks
/// of RFC 3454 §6, which *reject* passwords rather than transform them — refusing a
/// password a conforming reader would also refuse gains nothing here, and refusing one
/// it would accept is the failure this function exists to remove.
///
/// Measured: `target/encrypted/aes256_saslprep.pdf` stores `/U` for the NFKC form of
/// `\u{FB01}re` — the fi ligature. PDFKit opens it when that ligature is typed; without
/// this, so did nothing here.
fn saslprep(password: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    password
        .chars()
        .filter(|c| !is_mapped_to_nothing(*c))
        .map(|c| if is_non_ascii_space(c) { ' ' } else { c })
        .nfkc()
        .collect()
}

/// RFC 3454 table B.1: soft hyphen, zero-width joiners, variation selectors and the
/// other characters stringprep deletes outright.
fn is_mapped_to_nothing(c: char) -> bool {
    matches!(c,
        '\u{00AD}' | '\u{034F}' | '\u{1806}'
        | '\u{180B}'..='\u{180D}'
        | '\u{200B}'..='\u{200D}'
        | '\u{2060}'
        | '\u{FE00}'..='\u{FE0F}'
        | '\u{FEFF}')
}

/// RFC 3454 table C.1.2: the spaces that are not `U+0020`.
fn is_non_ascii_space(c: char) -> bool {
    matches!(
        c,
        '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
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
/// Builds a handler for a public-key encrypted document (7.6.5).
///
/// Records rather than fails when no credential was offered: a reader that was given
/// only a password has not got this wrong, it has been handed a document of a kind it
/// was not asked to open.
fn build_public_key(
    arena: &PdfArena,
    encrypt: &Dict,
    recipient: Option<&crate::cms::RecipientIdentity>,
    decisions: &mut DecisionLog,
) -> Option<SecurityHandler> {
    let Some(identity) = recipient else {
        decisions.push(Decision::violation(
            "7.6.5",
            "the document is encrypted to a certificate",
            "no certificate and key were offered, so it stays encrypted",
        ));
        return None;
    };

    let recipients = recipients(arena, encrypt);
    if recipients.is_empty() {
        decisions.push(Decision::violation(
            "7.6.5",
            "a public-key handler with no /Recipients",
            "left the document encrypted; there is no seed to derive a key from",
        ));
        return None;
    }

    let key_len = key_bytes(arena, encrypt).unwrap_or(32);
    let encrypt_metadata = boolean(arena, encrypt, "EncryptMetadata").unwrap_or(true);
    match SecurityHandler::open_public_key(&recipients, identity, key_len, encrypt_metadata) {
        Ok(handler) => handler,
        Err(why) => {
            decisions.push(Decision::violation(
                "7.6.5",
                format!("a /Recipients entry could not be opened: {why}"),
                "left the document encrypted",
            ));
            None
        }
    }
}

/// The `/Recipients` entries, wherever this version keeps them.
///
/// 7.6.5: for `/V` 4 and 5 they sit in the crypt filter that `/StmF` names, because a
/// crypt filter may address a different set of people from the document as a whole.
/// Earlier versions put one array at the top of `/Encrypt`. A file that uses crypt
/// filters and also carries a top-level array is not this engine's problem to reconcile
/// — the filter's own list is the one governing its streams.
fn recipients(arena: &PdfArena, encrypt: &Dict) -> Vec<Vec<u8>> {
    let from_filter = entry_in(arena, encrypt, "CF")
        .and_then(|o| as_dict(arena, &o))
        .and_then(|cf| {
            let named = name_of(arena, encrypt, "StmF").unwrap_or_else(|| "Identity".to_string());
            cf.get(&arena.name(&named)).and_then(|o| as_dict(arena, o))
        })
        .map(|filter| strings(arena, &filter, "Recipients"))
        .unwrap_or_default();
    if from_filter.is_empty() { strings(arena, encrypt, "Recipients") } else { from_filter }
}

/// A dictionary entry that is an array of byte strings, or a single one.
fn strings(arena: &PdfArena, dict: &Dict, key: &str) -> Vec<Vec<u8>> {
    match dict.get(&arena.name(key)).map(|o| o.resolve(arena)) {
        Some(Object::Array(h)) => arena
            .get_array(h)
            .unwrap_or_default()
            .iter()
            .filter_map(|o| match o.resolve(arena) {
                Object::String(b) | Object::Hex(b) => Some(b.to_vec()),
                _ => None,
            })
            .collect(),
        // A crypt filter that is not the document default carries one entry, not an
        // array of them.
        Some(Object::String(b) | Object::Hex(b)) => vec![b.to_vec()],
        _ => Vec::new(),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    /// The transformations 7.6.4.3.3 step (a) asks for, which is what lets a user type
    /// a password the way their keyboard produces it.
    #[test]
    fn saslprep_normalises_what_a_keyboard_produces() {
        // NFKC compatibility folding: the fi ligature is the case the fixture uses.
        assert_eq!(saslprep("\u{FB01}re"), "fire");
        // Full-width Latin, which a Japanese input method emits.
        assert_eq!(saslprep("\u{FF50}\u{FF41}\u{FF53}\u{FF53}"), "pass");
        // Canonical composition: "e" plus a combining acute is one character after.
        assert_eq!(saslprep("cafe\u{0301}"), "caf\u{00E9}");
    }

    #[test]
    fn saslprep_deletes_and_folds_the_two_mapping_tables() {
        // RFC 3454 B.1: a soft hyphen or a zero-width joiner is invisible, and a
        // password that differs from another only by one must not be a different
        // password.
        assert_eq!(saslprep("pass\u{00AD}word"), "password");
        assert_eq!(saslprep("pass\u{200B}word"), "password");
        // C.1.2: every other space becomes U+0020.
        assert_eq!(saslprep("a\u{00A0}b"), "a b");
        assert_eq!(saslprep("a\u{3000}b"), "a b");
    }

    #[test]
    fn saslprep_leaves_an_ascii_password_alone() {
        // The overwhelming case, and the one a regression here would break silently.
        for password in ["", "secret", "P@ssw0rd!", "a b c"] {
            assert_eq!(saslprep(password), password);
        }
    }

    #[test]
    fn permissions_read_either_form_of_the_same_thing() {
        // ADR-0009. `-1036` and `4294966260` are the same 32 bits, and producers write
        // both; rejecting the unsigned form fed a wrong key into Algorithm 2.
        let arena = PdfArena::new();
        let mut dict = BTreeMap::new();
        dict.insert(arena.name("P"), Object::Integer(4_294_966_260));
        assert_eq!(permissions(&arena, &dict), Some(-1036));

        let mut signed = BTreeMap::new();
        signed.insert(arena.name("P"), Object::Integer(-1036));
        assert_eq!(permissions(&arena, &signed), Some(-1036));

        // Beyond 32 bits it is neither form, and guessing is what caused the defect.
        let mut nonsense = BTreeMap::new();
        nonsense.insert(arena.name("P"), Object::Integer(1 << 40));
        assert_eq!(permissions(&arena, &nonsense), None);
    }
}
