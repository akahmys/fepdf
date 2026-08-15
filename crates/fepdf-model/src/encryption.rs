//! What protects a document, and how far the engine conforms in handling it (7.6).
//!
//! The interesting column is the last one. Clause 7.6 is the area this engine has been
//! weakest in, and two of the three defects found there were invisible because the
//! file *opened* — so a report that only named the handler would have said "AES-128"
//! about a document decrypting to noise. Conformance is therefore stated per file,
//! against what the code actually implements rather than what the dictionary declares.

use crate::arena::PdfArena;
use crate::decrypt;
use crate::error::PdfResult;
use crate::handle::Handle;
use crate::interpretation::Decision;
use crate::object::{Object, PdfName};
use crate::reader::{self, DictHandle};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How far the engine conforms in handling a given security handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Conformance {
    /// Implemented against the standard's algorithms and cross-checked against an
    /// independent implementation.
    Implemented,
    /// Implemented, but with a documented departure from the clause. The handler may
    /// fail on input a conforming reader accepts.
    NonConformant,
    /// Not implemented. The document is left encrypted and its content unreadable.
    Unsupported,
}

/// One permission bit of Table 22, and whether the document grants it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    /// The bit position the standard numbers it by, counting from 1.
    pub bit: u8,
    /// What the bit governs.
    pub meaning: &'static str,
    /// Whether the document allows it.
    pub granted: bool,
}

/// A crypt filter (7.6.5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptFilter {
    /// The name it is registered under in `/CF`.
    pub name: String,
    /// `/CFM` — `V2`, `AESV2`, `AESV3` or `None`.
    pub method: String,
    /// Whether `/StmF` names it for streams.
    pub for_streams: bool,
    /// Whether `/StrF` names it for strings.
    pub for_strings: bool,
}

/// The document's security, as declared and as handled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionReport {
    /// Whether the trailer declares `/Encrypt` at all.
    pub encrypted: bool,
    /// `/Filter`, naming the security handler.
    pub handler: Option<String>,
    /// `/V`, the algorithm version.
    pub version: Option<i64>,
    /// `/R`, the handler revision.
    pub revision: Option<i64>,
    /// Key length in bits, as `/Length` gives it or as `/V` implies.
    pub key_bits: Option<usize>,
    /// The cipher applied to streams, from `/CFM` where a crypt filter names one.
    pub cipher: Option<String>,
    /// `/EncryptMetadata`.
    pub encrypt_metadata: bool,
    /// Crypt filters, if the file defines any.
    pub crypt_filters: Vec<CryptFilter>,
    /// `/P` as written, and decoded bit by bit.
    pub permission_bits: Option<i32>,
    /// What each Table 22 bit permits.
    pub permissions: Vec<Permission>,
    /// How far the engine conforms in handling this handler.
    pub conformance: Conformance,
    /// Why, in one phrase.
    pub conformance_note: &'static str,
    /// Whether the supplied password unlocked it.
    pub unlocked: bool,
    /// Which password authenticated: `"user"`, `"owner"`, or absent. 7.6.4.1 restricts
    /// only user access by `/P`, so `unlocked` alone does not say enough.
    pub access: Option<String>,
    /// Decisions taken reading and unlocking.
    pub decisions: Vec<Decision>,
}

/// Table 22, for revision 3 and later. Bits 1, 2, 7 and 8 are reserved.
///
/// These are the bits as the file writes them, not a viewer's policy. Cross-checking
/// `samples/unicode_16.pdf` against PDFKit agrees on five of six — printing, copying,
/// commenting, form entry and assembly — and disagrees on one: PDFKit reports
/// `allowsDocumentChanges` true where bit 4 is clear. `/P` is `0xFFFFFBF4` and bit 4 is
/// worth 8, so the bit really is clear; PDFKit's property is a higher-level judgement
/// that does not map to it one for one. Report the bit.
const PERMISSION_BITS: &[(u8, &str)] = &[
    (3, "print"),
    (4, "modify contents"),
    (5, "copy or extract text and graphics"),
    (6, "add or modify annotations and fill form fields"),
    (9, "fill form fields, even with bit 6 clear"),
    (10, "extract for accessibility (deprecated in PDF 2.0)"),
    (11, "assemble: insert, rotate or delete pages"),
    (12, "print at high resolution"),
];

impl EncryptionReport {
    /// Reads `bytes` and reports what protects it.
    ///
    /// # Errors
    /// Fails only when the file cannot be read at all.
    pub fn survey(bytes: &[u8], password: &str) -> PdfResult<Self> {
        let raw = reader::load_document(bytes)?;

        // Read the dictionary *before* unlocking. `unlock` drops `/Encrypt` when it
        // succeeds — Acrobat reports error 135 for a file whose trailer still claims
        // encryption over plain objects — so asking afterwards reports every readable
        // encrypted document as having none, which is what this did first.
        let Some(encrypt) = raw.trailer.and_then(|t| encryption_dict(&raw.arena, t)) else {
            let mut decisions = raw.decisions.clone();
            decrypt::unlock(&raw.arena, raw.trailer, password, &mut decisions)?;
            return Ok(Self::unencrypted(decisions.entries().to_vec()));
        };

        let mut decisions = raw.decisions.clone();
        let security = decrypt::unlock(&raw.arena, raw.trailer, password, &mut decisions)?;
        let unlocked = raw
            .trailer
            .and_then(|t| raw.arena.get_dict(t))
            .is_none_or(|d| !d.contains_key(&raw.arena.name("Encrypt")));

        let version = integer(&raw.arena, &encrypt, "V");
        let revision = integer(&raw.arena, &encrypt, "R");
        let crypt_filters = read_crypt_filters(&raw.arena, &encrypt);
        let (conformance, conformance_note) = judge(version, revision);

        Ok(Self {
            encrypted: true,
            handler: name(&raw.arena, &encrypt, "Filter"),
            version,
            revision,
            key_bits: key_bits(&raw.arena, &encrypt, version),
            cipher: crypt_filters
                .iter()
                .find(|f| f.for_streams)
                .map(|f| f.method.clone())
                .or_else(|| version.map(|v| if v >= 4 { "unnamed" } else { "V2" }.to_string())),
            encrypt_metadata: boolean(&raw.arena, &encrypt, "EncryptMetadata").unwrap_or(true),
            crypt_filters,
            permission_bits: security.permissions,
            permissions: decode_permissions(security.permissions),
            conformance,
            conformance_note,
            unlocked,
            access: security.access.map(|a| match a {
                fepdf_syntax::security::Access::User => "user".to_string(),
                fepdf_syntax::security::Access::Owner => "owner".to_string(),
            }),
            decisions: decisions.entries().to_vec(),
        })
    }

    fn unencrypted(decisions: Vec<Decision>) -> Self {
        Self {
            encrypted: false,
            handler: None,
            version: None,
            revision: None,
            key_bits: None,
            cipher: None,
            encrypt_metadata: true,
            crypt_filters: Vec::new(),
            permission_bits: None,
            permissions: Vec::new(),
            conformance: Conformance::Implemented,
            conformance_note: "the document declares no /Encrypt",
            unlocked: true,
            access: None,
            decisions,
        }
    }
}

/// What the engine can actually do with this `/V` and `/R`, as opposed to what the
/// dictionary claims. Stated here so the gaps `ROADMAP.md` lists are visible per file.
fn judge(version: Option<i64>, revision: Option<i64>) -> (Conformance, &'static str) {
    match (version.unwrap_or(0), revision.unwrap_or(0)) {
        (1 | 2, 2..=3) => (
            Conformance::Implemented,
            "RC4 to Algorithms 1 to 6, checked against an independent implementation",
        ),
        (4, 4) => (
            Conformance::Implemented,
            "AES-128 or RC4 by /CFM, to Algorithms 1 to 6, cross-checked against PDFKit",
        ),
        (5, 6) => (
            Conformance::Implemented,
            "AES-256 to Algorithms 2.A and 2.B, with /Perms checked and SASLprep \
             applied to the password (its mappings and NFKC, not its refusals)",
        ),
        (5, 5) => (
            Conformance::Implemented,
            "AES-256 at Adobe's revision 5, which hashes once where revision 6 runs \
             Algorithm 2.B; deprecated by PDF 2.0 but still read",
        ),
        _ => (Conformance::Unsupported, "no handler is implemented for this /V and /R"),
    }
}

/// Table 22, decoded. A cleared bit denies; the reserved bits are not reported.
fn decode_permissions(bits: Option<i32>) -> Vec<Permission> {
    let Some(bits) = bits else { return Vec::new() };
    PERMISSION_BITS
        .iter()
        .map(|(bit, meaning)| Permission {
            bit: *bit,
            meaning,
            granted: bits & (1 << (bit - 1)) != 0,
        })
        .collect()
}

/// `/Length` in bits, defaulting as 7.6.4.2 does.
fn key_bits(arena: &PdfArena, encrypt: &Dict, version: Option<i64>) -> Option<usize> {
    match integer(arena, encrypt, "Length") {
        Some(bits) => usize::try_from(bits).ok(),
        None => match version? {
            1 => Some(40),
            5 => Some(256),
            _ => None,
        },
    }
}

fn read_crypt_filters(arena: &PdfArena, encrypt: &Dict) -> Vec<CryptFilter> {
    let Some(cf) = encrypt.get(&arena.name("CF")).and_then(|o| as_dict(arena, o)) else {
        return Vec::new();
    };
    let stream_filter = name(arena, encrypt, "StmF");
    let string_filter = name(arena, encrypt, "StrF");

    let mut out: Vec<CryptFilter> = cf
        .iter()
        .filter_map(|(key, value)| {
            let name = arena.get_name_str(*key)?;
            let filter = as_dict(arena, value)?;
            Some(CryptFilter {
                method: filter
                    .get(&arena.name("CFM"))
                    .and_then(|m| match m {
                        Object::Name(h) => arena.get_name_str(*h),
                        _ => None,
                    })
                    .unwrap_or_else(|| "None".to_string()),
                for_streams: stream_filter.as_deref() == Some(name.as_str()),
                for_strings: string_filter.as_deref() == Some(name.as_str()),
                name,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

type Dict = BTreeMap<Handle<PdfName>, Object>;

fn encryption_dict(arena: &PdfArena, trailer: DictHandle) -> Option<Dict> {
    let value = arena.get_dict(trailer)?.get(&arena.name("Encrypt")).cloned()?;
    as_dict(arena, &value)
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

fn integer(arena: &PdfArena, dict: &Dict, key: &str) -> Option<i64> {
    match dict.get(&arena.name(key))? {
        Object::Integer(v) => Some(*v),
        _ => None,
    }
}

fn name(arena: &PdfArena, dict: &Dict, key: &str) -> Option<String> {
    match dict.get(&arena.name(key))? {
        Object::Name(h) => arena.get_name_str(*h),
        _ => None,
    }
}

fn boolean(arena: &PdfArena, dict: &Dict, key: &str) -> Option<bool> {
    match dict.get(&arena.name(key))? {
        Object::Boolean(v) => Some(*v),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `/P` from `samples/unicode_16.pdf`, written in the unsigned form.
    const P_UNICODE_16: i32 = -1036;

    #[test]
    fn table_22_decodes_the_bits_the_file_writes() {
        // Verified against PDFKit on the file this value comes from: printing,
        // copying, commenting, form entry and assembly all agree.
        let p = decode_permissions(Some(P_UNICODE_16));
        let granted = |bit: u8| p.iter().find(|x| x.bit == bit).expect("bit is listed").granted;
        assert!(granted(3), "print");
        assert!(!granted(4), "modify contents: 0xFFFFFBF4 & 8 == 0");
        assert!(granted(5), "copy");
        assert!(granted(6), "annotations");
        assert!(!granted(11), "assemble");
        assert!(granted(12), "print at high resolution");
    }

    #[test]
    fn every_bit_granted_reads_as_such() {
        // `/P -4` clears only the two reserved low bits, which is what
        // `scripts/test/make_encrypted.py` writes.
        let p = decode_permissions(Some(-4));
        assert!(p.iter().all(|x| x.granted), "{p:?}");
        assert_eq!(p.len(), 8, "Table 22 defines eight non-reserved bits");
    }

    #[test]
    fn an_absent_permissions_field_yields_no_claims() {
        assert!(decode_permissions(None).is_empty());
    }

    #[test]
    fn conformance_names_the_gaps_rather_than_the_declaration() {
        // The point of the column: a file can declare AES-256 and be unreadable.
        assert_eq!(judge(Some(1), Some(2)).0, Conformance::Implemented);
        assert_eq!(judge(Some(2), Some(3)).0, Conformance::Implemented);
        assert_eq!(judge(Some(4), Some(4)).0, Conformance::Implemented);
        assert_eq!(judge(Some(5), Some(6)).0, Conformance::Implemented);
        assert_eq!(judge(Some(5), Some(5)).0, Conformance::Implemented);
        // Public-key handlers, and anything else the standard adds.
        assert_eq!(judge(Some(4), Some(9)).0, Conformance::Unsupported);
        assert_eq!(judge(None, None).0, Conformance::Unsupported);
    }
}
