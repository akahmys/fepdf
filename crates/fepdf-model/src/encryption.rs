//! What protects a document, and how far the engine conforms in handling it (7.6).
//!
//! The interesting column is the last one. Clause 7.6 is the area this engine has been
//! weakest in, and two of the three defects found there were invisible because the
//! file *opened* — so a report that only named the handler would have said "AES-128"
//! about a document decrypting to noise. Conformance is therefore stated per file,
//! against what the code actually implements rather than what the dictionary declares.

use crate::access::{self, Dict};
use crate::arena::PdfArena;
use crate::decrypt;
use crate::error::{PdfError, PdfResult};
use crate::handle::DictHandle;
use crate::interpretation::Decision;
use crate::object::Object;
use crate::reader;
use serde::{Deserialize, Serialize};

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

/// An encrypted payload carried by an unencrypted wrapper document (7.6.7).
///
/// The wrapper itself is plain; the real document is embedded, encrypted by a handler
/// this standard does not define. There is therefore nothing to decrypt here and no
/// pretence of it — the point of the clause is that a reader without the filter can
/// still tell the user *which* filter is missing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedPayload {
    /// `/EP /Subtype`: the name of the cryptographic filter needed (Table 28).
    pub filter: String,
    /// `/EP /Version`, if the producer gave one. Read as text, not a number: the
    /// standard's own note says the periods separate integers.
    pub filter_version: Option<String>,
    /// `/F` or `/UF` from the file specification.
    pub file_name: Option<String>,
    /// `/Desc`, which is where a producer puts the instructions the clause asks for.
    pub description: Option<String>,
    /// Which of 7.6.7's conditions the file actually meets. A file failing some of
    /// them is still worth reporting, and saying which is more use than a verdict.
    pub conditions_met: Vec<String>,
    /// Conditions the clause requires that this file does not meet.
    pub conditions_unmet: Vec<String>,
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
    /// An encrypted payload, when the document is an unencrypted wrapper (7.6.7).
    pub payload: Option<EncryptedPayload>,
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
///
/// Each row is the bit, the keyword `--permissions` accepts for it, and what it means.
/// One table, both directions: reading a `/P` and writing one could not disagree while
/// only one of them existed, and now that both do, a keyword naming a different bit
/// from the one `inspect encryption` reports would be a vocabulary to learn twice.
const PERMISSION_BITS: &[(u8, &str, &str)] = &[
    (3, "print", "print"),
    (4, "modify", "modify contents"),
    (5, "copy", "copy or extract text and graphics"),
    (6, "annotate", "add or modify annotations and fill form fields"),
    (9, "forms", "fill form fields, even with bit 6 clear"),
    (10, "accessibility", "extract for accessibility (deprecated in PDF 2.0)"),
    (11, "assemble", "assemble: insert, rotate or delete pages"),
    (12, "print-high", "print at high resolution"),
];

/// The keywords `--permissions` takes, for a caller that has to list them.
#[must_use]
pub fn permission_keywords() -> Vec<&'static str> {
    PERMISSION_BITS.iter().map(|(_, keyword, _)| *keyword).collect()
}

/// Builds a `/P` from a comma-separated list of keywords: what is named is granted.
///
/// Everything not named is denied, which is the direction that makes an empty list
/// mean something — `--permissions ""` grants nothing, rather than silently granting
/// all. The reserved bits follow Table 22: 1 and 2 are cleared, and the rest are set,
/// because a reserved bit written the wrong way is a `/P` other readers disagree about.
///
/// # Errors
/// If a keyword is not one of [`permission_keywords`], naming the ones that are.
pub fn permissions_from_keywords(list: &str) -> PdfResult<i32> {
    // Bits 1 and 2 shall be 0; bits 7, 8 and 13 upward shall be 1. Start from that, with
    // all eight meaningful bits denied, and grant what was asked for.
    let mut bits: u32 = !0b11;
    for (bit, _, _) in PERMISSION_BITS {
        bits &= !(1 << (bit - 1));
    }

    for keyword in list.split(',').map(str::trim).filter(|k| !k.is_empty()) {
        let found = PERMISSION_BITS.iter().find(|(_, k, _)| *k == keyword).ok_or_else(|| {
            PdfError::refused(
                "set permissions",
                format!(
                    "{keyword:?} is not a permission; the ones Table 22 defines are {}",
                    permission_keywords().join(", ")
                ),
            )
        })?;
        bits |= 1 << (found.0 - 1);
    }
    Ok(bits.cast_signed())
}

impl EncryptionReport {
    /// Reads `bytes` and reports what protects it.
    ///
    /// # Errors
    /// Fails only when the file cannot be read at all.
    pub fn survey(bytes: &[u8], credentials: decrypt::Credentials<'_>) -> PdfResult<Self> {
        // One copy of the file here, instead of one copy of its tail per object inside
        // `parse_indirect_at` (ADR-0074). These entry points take a `&[u8]` from a public
        // API; `Document::open` already holds a `Bytes` and passes it through untouched.
        let raw = reader::load_document(&bytes::Bytes::copy_from_slice(bytes))?;

        // Read the dictionary *before* unlocking. `unlock` drops `/Encrypt` when it
        // succeeds — Acrobat reports error 135 for a file whose trailer still claims
        // encryption over plain objects — so asking afterwards reports every readable
        // encrypted document as having none, which is what this did first.
        // 7.6.7 first, and outside the encrypted branch: a wrapper document is itself
        // *unencrypted*. Reporting it only for files with an `/Encrypt` would miss
        // every one of them, which is the whole population.
        let payload = raw
            .trailer
            .and_then(|t| catalogue(&raw.arena, t))
            .and_then(|c| read_payload(&raw.arena, &c));

        let Some(encrypt) = raw.trailer.and_then(|t| encryption_dict(&raw.arena, t)) else {
            let mut decisions = raw.decisions.clone();
            decrypt::unlock_raw(&raw, credentials, &mut decisions)?;
            return Ok(Self::unencrypted(decisions.entries().to_vec(), payload));
        };

        let mut decisions = raw.decisions.clone();
        let security = decrypt::unlock_raw(&raw, credentials, &mut decisions)?;
        let unlocked = raw
            .trailer
            .and_then(|t| raw.arena.get_dict(t))
            .is_none_or(|d| !d.contains_key(&raw.arena.name("Encrypt")));

        let version = access::integer_at(&raw.arena, &encrypt, "V");
        let revision = access::integer_at(&raw.arena, &encrypt, "R");
        let crypt_filters = read_crypt_filters(&raw.arena, &encrypt);
        let (conformance, conformance_note) = judge(version, revision);

        Ok(Self {
            encrypted: true,
            handler: access::name_at(&raw.arena, &encrypt, "Filter"),
            version,
            revision,
            key_bits: key_bits(&raw.arena, &encrypt, version),
            cipher: crypt_filters
                .iter()
                .find(|f| f.for_streams)
                .map(|f| f.method.clone())
                .or_else(|| version.map(|v| if v >= 4 { "unnamed" } else { "V2" }.to_string())),
            encrypt_metadata: access::boolean_at(&raw.arena, &encrypt, "EncryptMetadata")
                .unwrap_or(true),
            crypt_filters,
            permission_bits: security.permissions,
            permissions: decode_permissions(security.permissions),
            conformance,
            conformance_note,
            payload,
            unlocked,
            access: security.access.map(|a| match a {
                fepdf_syntax::security::Access::User => "user".to_string(),
                fepdf_syntax::security::Access::Owner => "owner".to_string(),
            }),
            decisions: decisions.entries().to_vec(),
        })
    }

    fn unencrypted(decisions: Vec<Decision>, payload: Option<EncryptedPayload>) -> Self {
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
            payload,
            decisions,
        }
    }
}

/// Whether this file is an unencrypted wrapper, and what its payload needs (7.6.7).
///
/// Every condition the clause states is checked and reported individually, met or not.
/// A producer that gets four of five right has still told the reader what filter is
/// missing, which is the whole purpose; a single boolean would discard that.
fn read_payload(arena: &PdfArena, catalog: &Dict) -> Option<EncryptedPayload> {
    let (mut met, mut unmet) = (Vec::new(), Vec::new());
    let mut note = |ok: bool, what: &str| {
        if ok {
            met.push(what.to_string());
        } else {
            unmet.push(what.to_string());
        }
    };

    // "shall include a Collection dictionary ... setting the collection View to H"
    let collection = access::dict_at(arena, catalog, "Collection");
    note(collection.is_some(), "/Collection in the catalogue");
    note(
        collection
            .as_ref()
            .is_some_and(|c| matches!(access::name_at(arena, c, "View").as_deref(), Some("H"))),
        "/Collection /View /H",
    );

    // "the EmbeddedFiles name tree shall contain exactly one entry"
    let embedded = catalog
        .get(&arena.name("Names"))
        .and_then(|n| access::dict_of(arena, n))
        .and_then(|n| access::dict_at(arena, &n, "EmbeddedFiles"))
        .and_then(|e| access::array_at(arena, &e, "Names"));
    note(
        embedded.as_ref().is_some_and(|names| names.len() == 2),
        "exactly one entry in the EmbeddedFiles name tree",
    );

    // "and as an entry in the AF array in the document catalog"
    let af = access::array_at(arena, catalog, "AF");
    note(af.as_ref().is_some_and(|a| !a.is_empty()), "/AF names the payload");

    // The file specification itself, reached through /AF.
    let spec = af.as_ref().and_then(|a| a.first()).and_then(|f| access::dict_of(arena, f))?;
    note(
        matches!(
            access::name_at(arena, &spec, "AFRelationship").as_deref(),
            Some("EncryptedPayload")
        ),
        "/AFRelationship /EncryptedPayload",
    );

    let ep = access::dict_at(arena, &spec, "EP")?;
    let filter = access::name_at(arena, &ep, "Subtype")?;
    note(true, "/EP /Subtype names the filter");

    Some(EncryptedPayload {
        filter,
        filter_version: access::name_at(arena, &ep, "Version"),
        file_name: access::text_at(arena, &spec, "UF")
            .or_else(|| access::text_at(arena, &spec, "F")),
        description: access::text_at(arena, &spec, "Desc"),
        conditions_met: met,
        conditions_unmet: unmet,
    })
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
        .map(|(bit, _, meaning)| Permission {
            bit: *bit,
            meaning,
            granted: bits & (1 << (bit - 1)) != 0,
        })
        .collect()
}

/// `/Length` in bits, defaulting as 7.6.4.2 does.
fn key_bits(arena: &PdfArena, encrypt: &Dict, version: Option<i64>) -> Option<usize> {
    match access::integer_at(arena, encrypt, "Length") {
        Some(bits) => usize::try_from(bits).ok(),
        None => match version? {
            1 => Some(40),
            5 => Some(256),
            _ => None,
        },
    }
}

fn read_crypt_filters(arena: &PdfArena, encrypt: &Dict) -> Vec<CryptFilter> {
    let Some(cf) = access::dict_at(arena, encrypt, "CF") else {
        return Vec::new();
    };
    let stream_filter = access::name_at(arena, encrypt, "StmF");
    let string_filter = access::name_at(arena, encrypt, "StrF");

    let mut out: Vec<CryptFilter> = cf
        .iter()
        .filter_map(|(key, value)| {
            let name = arena.get_name_str(*key)?;
            let filter = access::dict_of(arena, value)?;
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

/// The document catalogue, which 7.6.7's markers hang from.
fn catalogue(arena: &PdfArena, trailer: DictHandle) -> Option<Dict> {
    let root = arena.get_dict(trailer)?.get(&arena.name("Root")).cloned()?;
    access::dict_of(arena, &root)
}

fn encryption_dict(arena: &PdfArena, trailer: DictHandle) -> Option<Dict> {
    let value = arena.get_dict(trailer)?.get(&arena.name("Encrypt")).cloned()?;
    access::dict_of(arena, &value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `/Encrypt` dictionary whose entries are written as indirect references.
    ///
    /// 7.3.10 permits it and the engine decrypts such a file perfectly. What it used to
    /// do was *report* on it as though the entries were absent — measured 2026-09-05
    /// against a real RC4-128 file with `/V`, `/R` and `/Length` written as references:
    ///
    /// | | reported | truth |
    /// | :--- | :--- | :--- |
    /// | `/Filter` | `(absent)` | `/Standard` |
    /// | `/V`, `/R`, `/Length` | `—` | 2, 3, 128 |
    /// | verdict | `UNSUPPORTED` | implemented |
    ///
    /// and `DECISIONS TAKEN READING` said the file departed from nothing while the same
    /// run extracted its Japanese text.
    fn indirect_encrypt_dict() -> (PdfArena, Dict) {
        let arena = PdfArena::new();
        let indirect = |value: Object| Object::Reference(arena.alloc_object(value));
        let mut dict = Dict::new();
        dict.insert(arena.name("Filter"), indirect(Object::Name(arena.name("Standard"))));
        dict.insert(arena.name("V"), indirect(Object::Integer(2)));
        dict.insert(arena.name("R"), indirect(Object::Integer(3)));
        dict.insert(arena.name("Length"), indirect(Object::Integer(128)));
        dict.insert(arena.name("EncryptMetadata"), indirect(Object::Boolean(false)));
        (arena, dict)
    }

    /// The report reads what such a file declares, rather than reporting it as absent.
    #[test]
    fn an_encrypt_dictionary_written_as_references_is_read() {
        let (arena, encrypt) = indirect_encrypt_dict();

        let version = access::integer_at(&arena, &encrypt, "V");
        let revision = access::integer_at(&arena, &encrypt, "R");
        assert_eq!((version, revision), (Some(2), Some(3)));
        assert_eq!(access::name_at(&arena, &encrypt, "Filter").as_deref(), Some("Standard"));
        assert_eq!(key_bits(&arena, &encrypt, version), Some(128), "/Length behind a reference");

        // The default is the trap: `.unwrap_or(true)` over an accessor that could not
        // read the value reports the opposite of what the document says.
        assert!(!access::boolean_at(&arena, &encrypt, "EncryptMetadata").unwrap_or(true));
    }

    /// And the verdict follows from what was read, so it stops saying UNSUPPORTED.
    ///
    /// `judge` matches on `(version.unwrap_or(0), revision.unwrap_or(0))`, so a pair it
    /// could not read falls into the wildcard arm — which answers with a default and
    /// records nothing, the ground RR-15 Rule 20 covers and nothing checks.
    #[test]
    fn the_verdict_follows_what_was_read() {
        let (arena, encrypt) = indirect_encrypt_dict();
        let version = access::integer_at(&arena, &encrypt, "V");
        let revision = access::integer_at(&arena, &encrypt, "R");

        assert_eq!(judge(version, revision).0, Conformance::Implemented);
        assert_eq!(
            judge(None, None).0,
            Conformance::Unsupported,
            "and an /Encrypt that really says nothing is still unsupported"
        );
    }

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

    /// 7.6.7 recognition, on documents assembled to meet or miss each condition.
    mod wrapper {
        use super::*;

        /// A wrapper meeting every condition the clause states.
        fn wrapper(view: &str, relationship: &str, entries: usize) -> Vec<u8> {
            let names: String = (0..entries).map(|i| format!("(p{i}) 6 0 R ")).collect();
            let objects: Vec<String> = vec![
                format!(
                    "<< /Type /Catalog /Pages 2 0 R /Names 8 0 R /AF [6 0 R] \
                     /Collection << /Type /Collection /View /{view} >> >>"
                ),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".into(),
                "null".into(),
                "null".into(),
                format!(
                    "<< /Type /Filespec /F (payload.pdf) /AFRelationship /{relationship} \
                     /EP << /Type /EncryptedPayload /Subtype /AcmeCustomCrypto /Version /1.0 >> >>"
                ),
                "null".into(),
                "<< /EmbeddedFiles 9 0 R >>".into(),
                format!("<< /Names [{names}] >>"),
            ];

            fepdf_fixtures::assemble(&objects)
        }

        #[test]
        fn a_conforming_wrapper_names_its_filter() {
            let r = EncryptionReport::survey(
                &wrapper("H", "EncryptedPayload", 1),
                decrypt::Credentials::default(),
            )
            .expect("reads");
            let p = r.payload.expect("recognised");
            assert_eq!(p.filter, "AcmeCustomCrypto");
            assert_eq!(p.filter_version.as_deref(), Some("1.0"));
            assert_eq!(p.file_name.as_deref(), Some("payload.pdf"));
            assert!(p.conditions_unmet.is_empty(), "{:?}", p.conditions_unmet);
            // The wrapper is itself unencrypted, which is the point of the clause.
            assert!(!r.encrypted);
        }

        #[test]
        fn each_condition_is_reported_separately() {
            // A producer that gets four of five right has still said which filter is
            // needed, and that is the service 7.6.7 exists to provide. A single
            // boolean would throw it away.
            let r = EncryptionReport::survey(
                &wrapper("D", "EncryptedPayload", 1),
                decrypt::Credentials::default(),
            )
            .expect("reads");
            let p = r.payload.expect("still recognised");
            assert_eq!(p.filter, "AcmeCustomCrypto");
            assert!(
                p.conditions_unmet.iter().any(|c| c.contains("/View /H")),
                "{:?}",
                p.conditions_unmet
            );
            assert!(p.conditions_met.iter().any(|c| c.contains("/AFRelationship")));
        }

        #[test]
        fn more_than_one_embedded_file_is_flagged() {
            // "the EmbeddedFiles name tree shall contain exactly one entry".
            let r = EncryptionReport::survey(
                &wrapper("H", "EncryptedPayload", 3),
                decrypt::Credentials::default(),
            )
            .expect("reads");
            let p = r.payload.expect("recognised");
            assert!(
                p.conditions_unmet.iter().any(|c| c.contains("exactly one entry")),
                "{:?}",
                p.conditions_unmet
            );
        }

        #[test]
        fn an_ordinary_document_is_not_a_wrapper() {
            // The check runs on every file, encrypted or not, so a false positive here
            // would appear on the whole corpus.
            let plain = b"%PDF-2.0\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
                          2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n\
                          trailer\n<< /Root 1 0 R >>\n";
            let r = EncryptionReport::survey(plain, decrypt::Credentials::default()).ok();
            assert!(r.is_none_or(|r| r.payload.is_none()));
        }
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
