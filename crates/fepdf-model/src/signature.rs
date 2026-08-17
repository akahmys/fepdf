//! Checking the signatures a file carries (12.8).
//!
//! This works at the **byte layer** — `load_document` and Pass 0, not `Document::open`
//! ([ADR-0013]) — and it has to. `/ByteRange` names offsets into the file as it exists
//! on disk; a `Document` has merged the revision chain, settled the metadata and thrown
//! the source bytes away, so by the time one exists the offsets point at nothing.
//!
//! The signature itself is read out of the bytes rather than out of the parsed
//! `/Contents`, for the same reason and one more: 7.6.2 exempts `/Contents` from
//! encryption, so the decryption pass would corrupt it in an encrypted file. The gap
//! between the two ranges *is* the `/Contents` string, so taking the signature from
//! there makes reading it and checking that the range is honest the same act.
//!
//! **What a pass means.** That the signature verifies over the bytes `/ByteRange` names,
//! and that it is bound to the certificate it carries. Not that the certificate is
//! trusted, not that it was valid when it signed, and not that it has not been revoked —
//! none of which this asks. `covers_whole_file` is reported separately because a
//! signature can verify perfectly over a range that stops short of the end of the file,
//! with anything at all appended after it.
//!
//! [ADR-0013]: ../../../../docs/adr/0013-a-document-is-one-normalised-state.md

use crate::arena::PdfArena;
use crate::decrypt::Credentials;
use crate::error::{PdfError, PdfResult};
use crate::interactive::{array_of, dict_of, name_of};
use crate::object::Object;
use crate::reader;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

type Dict = BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>;

/// What checking one signature showed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureCheck {
    /// The field's `/T`, when it has one.
    pub field: Option<String>,
    /// `/SubFilter` — the form the signature takes.
    pub sub_filter: Option<String>,
    /// `/M`, the time the signature states it was made. The document's word, not a fact.
    pub signed_at: Option<String>,
    /// Whether `/ByteRange` accounts for every byte of the file but the signature.
    ///
    /// A signature that verifies over less than the whole file is still a valid
    /// signature — over less than the whole file. This is the difference.
    pub covers_whole_file: bool,
    /// How many bytes the signature covers, of how many the file has.
    pub covered: (usize, usize),
    /// The signer's common name, when the signature verified and the certificate says.
    pub signer: Option<String>,
    /// Why the signature was refused, when it was.
    pub refused: Option<String>,
}

impl SignatureCheck {
    /// Whether the signature verified over the bytes it names.
    #[must_use]
    pub fn verified(&self) -> bool {
        self.refused.is_none()
    }
}

/// Every signature in one file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureReport {
    /// One per signature field carrying a `/V`, in the order the form lists them.
    pub signatures: Vec<SignatureCheck>,
    /// Signature fields with no value — a place for a signature, not a signature.
    pub unsigned_fields: usize,
    /// What the engine decided while reading this file (§5.3).
    pub decisions: Vec<crate::interpretation::Decision>,
}

impl SignatureReport {
    /// Reads `bytes` and checks every signature it carries.
    ///
    /// # Errors
    /// Fails when the file cannot be read or names no catalogue. A signature that does
    /// not verify is a result, not an error: the question was asked and answered.
    pub fn survey(bytes: &[u8]) -> PdfResult<Self> {
        let raw = reader::load_document(bytes)?;
        let mut decisions = raw.decisions.clone();
        crate::decrypt::unlock(&raw.arena, raw.trailer, Credentials::default(), &mut decisions)?;
        let arena = &raw.arena;
        let catalog = raw
            .trailer
            .and_then(|t| arena.get_dict(t))
            .and_then(|d| d.get(&arena.name("Root")).cloned())
            .and_then(|r| dict_of(arena, &r))
            .ok_or_else(|| PdfError::Arena("the file names no catalogue".into()))?;

        let mut signatures = Vec::new();
        let mut unsigned_fields = 0;
        for field in signature_fields(arena, &catalog) {
            match field.get(&arena.name("V")).and_then(|v| dict_of(arena, v)) {
                Some(signature) => {
                    signatures.push(check(arena, bytes, &field, &signature));
                }
                None => unsigned_fields += 1,
            }
        }

        Ok(Self { signatures, unsigned_fields, decisions: decisions.entries().to_vec() })
    }
}

/// Checks one signature, reporting rather than failing.
fn check(arena: &PdfArena, bytes: &[u8], field: &Dict, signature: &Dict) -> SignatureCheck {
    let mut check = SignatureCheck {
        field: field.get(&arena.name("T")).and_then(|t| text_of(arena, t)),
        sub_filter: signature.get(&arena.name("SubFilter")).and_then(|s| name_of(arena, s)),
        signed_at: signature.get(&arena.name("M")).and_then(|m| text_of(arena, m)),
        covers_whole_file: false,
        covered: (0, bytes.len()),
        signer: None,
        refused: Some("not checked".to_string()),
    };

    let ranges = match byte_range(arena, signature, bytes.len()) {
        Ok(ranges) => ranges,
        Err(why) => {
            check.refused = Some(why.to_string());
            return check;
        }
    };
    let [first, gap, second] = ranges;
    check.covered = (first.len() + second.len(), bytes.len());
    check.covers_whole_file = first.start == 0 && second.end == bytes.len();

    let der = match hex_string(&bytes[gap.clone()]) {
        Some(der) => der,
        None => {
            check.refused = Some("the bytes /ByteRange skips are not a hex string".to_string());
            return check;
        }
    };

    let taken = crate::cms::digest(&[&bytes[first], &bytes[second]]);
    match crate::cms::verify_detached(&der, &taken) {
        Ok(verified) => {
            check.signer = verified.signer;
            check.refused = None;
        }
        Err(why) => check.refused = Some(why.to_string()),
    }
    check
}

/// The two covered ranges and the gap between them, checked against the file's length.
fn byte_range(
    arena: &PdfArena,
    signature: &Dict,
    length: usize,
) -> PdfResult<[std::ops::Range<usize>; 3]> {
    let numbers = array_of(arena, signature.get(&arena.name("ByteRange")))
        .ok_or_else(|| PdfError::Other("the signature states no /ByteRange".into()))?;
    let numbers: Vec<usize> = numbers
        .iter()
        .filter_map(|n| n.resolve(arena).as_integer().and_then(|n| usize::try_from(n).ok()))
        .collect();
    let [a, b, c, d] = numbers[..] else {
        return Err(PdfError::Other(
            format!("/ByteRange is not four whole numbers: {numbers:?}").into(),
        ));
    };

    // Every arithmetic here is on numbers the file supplied, so none of it may wrap or
    // run past the end: a /ByteRange is exactly the sort of thing a hostile file lies in.
    let first_end = a.checked_add(b).filter(|&e| e <= length);
    let second_end = c.checked_add(d).filter(|&e| e <= length);
    match (first_end, second_end) {
        (Some(first_end), Some(second_end)) if first_end <= c => {
            Ok([a..first_end, first_end..c, c..second_end])
        }
        _ => Err(PdfError::Other(
            format!("/ByteRange [{a} {b} {c} {d}] does not fit a file of {length} bytes").into(),
        )),
    }
}

/// Decodes `<hex>` as the bytes it stands for, brackets required.
fn hex_string(bytes: &[u8]) -> Option<Vec<u8>> {
    let inner = bytes.strip_prefix(b"<")?.strip_suffix(b">")?;
    if inner.len() % 2 != 0 {
        return None;
    }
    inner
        .chunks(2)
        .map(|pair| {
            let digits = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(digits, 16).ok()
        })
        .collect()
}

/// Every terminal `/FT /Sig` field, walking `/Kids`.
fn signature_fields(arena: &PdfArena, catalog: &Dict) -> Vec<Dict> {
    let mut out = Vec::new();
    let Some(form) = catalog.get(&arena.name("AcroForm")).and_then(|a| dict_of(arena, a)) else {
        return out;
    };
    let mut queue: Vec<(Object, u32)> = array_of(arena, form.get(&arena.name("Fields")))
        .unwrap_or_default()
        .into_iter()
        .map(|f| (f, 0))
        .collect();
    while let Some((node, depth)) = queue.pop() {
        if depth > 64 {
            continue;
        }
        let Some(d) = dict_of(arena, &node) else { continue };
        if let Some(kids) = array_of(arena, d.get(&arena.name("Kids"))) {
            // A field's /Kids may be widgets rather than fields; a widget carries no
            // /FT, so descending costs nothing and missing a nested field would.
            queue.extend(kids.into_iter().map(|k| (k, depth + 1)));
        }
        if d.get(&arena.name("FT")).and_then(|t| name_of(arena, t)).as_deref() == Some("Sig") {
            out.push(d);
        }
    }
    out.reverse();
    out
}

fn text_of(arena: &PdfArena, object: &Object) -> Option<String> {
    match object.resolve(arena) {
        Object::Text(s) => Some(s),
        Object::String(b) | Object::Hex(b) => Some(crate::refine::text::recover_string(&b)),
        _ => None,
    }
}
