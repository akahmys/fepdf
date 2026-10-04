//! Matterhorn 31-006 to 31-008: the CMap a Type 0 font names (ISO 14289-1 7.21.3.3,
//! ROADMAP Y-F16).
//!
//! **Read as the file wrote it.** Ingestion leaves a Type 0 font's `/Encoding` alone
//! ([ADR-0105](../../../docs/adr/0105-ingestion-never-rewrote-a-real-type-0-cmap.md)), so a
//! name is the file's name and a stream the file's program.

use crate::structure::{AuditFinding, broken};
use fepdf_model::access::{entry, name_in};
use fepdf_model::{Document, Object};

/// Whether `name` is one of ISO 32000-1 Table 118's predefined CMaps, `Identity-H` and
/// `Identity-V` with them.
fn predefined(name: &str) -> bool {
    crate::unicode_map::TABLE_118.contains(&name) || matches!(name, "Identity-H" | "Identity-V")
}

/// 31-006 to 31-008 of the Type 0 font `font`, named `name` in a finding.
///
/// 31-006: the CMap is a Table 118 name or an embedded stream. 31-007: an embedded one's
/// `/WMode` is its program's. 31-008: an embedded one uses no CMap but a Table 118 one,
/// whether by its dictionary's `/UseCMap` or by its program's `usecmap`.
pub fn cmap(doc: &Document, font: &Object, name: &str, findings: &mut Vec<AuditFinding>) {
    let arena = doc.arena();
    let encoding = entry(arena, font, "Encoding");
    let Some(stream @ Object::Stream(..)) = encoding else {
        let named = name_in(arena, font, "Encoding");
        if !named.as_deref().is_some_and(predefined) {
            let what = named.map_or_else(|| "no CMap".to_owned(), |n| format!("the CMap /{n}"));
            findings.push(broken(
                "31-006",
                format!("/{name} names {what}, neither one Table 118 lists nor an embedded one"),
            ));
        }
        return;
    };
    let program = doc.decode_stream(&stream).map(|b| b.to_vec()).unwrap_or_default();
    let words: Vec<&[u8]> =
        program.split(u8::is_ascii_whitespace).filter(|w| !w.is_empty()).collect();
    let stated = entry(arena, &stream, "WMode").and_then(|w| w.as_integer()).unwrap_or(0);
    let written = words
        .windows(2)
        .find(|pair| pair[0] == b"/WMode")
        .and_then(|pair| std::str::from_utf8(pair[1]).ok()?.parse::<i64>().ok())
        .unwrap_or(0);
    if stated != written {
        findings.push(broken(
            "31-007",
            format!("/{name}'s CMap states /WMode {stated}, and its program {written}"),
        ));
    }
    used_cmaps(arena, &stream, &words, name, findings);
}

/// 31-008: the CMaps an embedded one uses, by `/UseCMap` and by `usecmap`.
fn used_cmaps(
    arena: &fepdf_model::PdfArena,
    stream: &Object,
    words: &[&[u8]],
    name: &str,
    findings: &mut Vec<AuditFinding>,
) {
    let mut unlisted: Vec<String> = words
        .windows(2)
        .filter(|pair| pair[1] == b"usecmap")
        .filter_map(|pair| pair[0].strip_prefix(b"/"))
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .chain(name_in(arena, stream, "UseCMap"))
        .filter(|used| !predefined(used))
        .map(|used| format!("/{used}"))
        .collect();
    if let Some(Object::Stream(..)) = entry(arena, stream, "UseCMap") {
        unlisted.push("an embedded CMap".to_owned());
    }
    for other in unlisted {
        findings.push(broken(
            "31-008",
            format!("/{name}'s CMap uses {other}, which Table 118 does not list"),
        ));
    }
}
