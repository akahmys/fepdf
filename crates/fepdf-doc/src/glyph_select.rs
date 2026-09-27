//! 31-011, 31-016 and 31-030: what a font's codes select, and the widths it renders.
//!
//! UA1:7.21.4.1, 7.21.5 and 7.21.8.
//!
//! **31-030 of every code shown, 31-011 of every code rendered.** A code selecting
//! `.notdef` breaks 31-030 whether it is drawn or not; a code selecting a glyph the
//! program lacks breaks 31-011 only where the text is rendered, which is what 7.21.4.1
//! asks the program for.

use crate::glyph_map::Glyph;
use crate::structure::{AuditFinding, broken, for_a_reader};
use fepdf_model::{Document, Handle, Object};
use std::collections::BTreeSet;

/// What the pages show in one font: every code, and those rendered, as one-byte codes and
/// as two-byte ones.
pub(crate) struct Shown<'a> {
    pub(crate) codes: &'a BTreeSet<u8>,
    pub(crate) rendered: &'a BTreeSet<u8>,
    pub(crate) pairs: &'a BTreeSet<u32>,
    pub(crate) rendered_pairs: &'a BTreeSet<u32>,
}

/// Asks 31-011 and 31-030 of the codes `shown` in `font`, whose program is `embedded` or
/// not. A font with no map, or codes it cannot settle, is left for a reader.
pub(crate) fn selected(
    doc: &Document,
    font: Handle<Object>,
    name: &str,
    embedded: bool,
    shown: &Shown<'_>,
    findings: &mut Vec<AuditFinding>,
) {
    if shown.codes.is_empty() {
        return;
    }
    let Some(map) = crate::glyph_map::of(doc, font) else {
        unmapped(name, embedded && !shown.rendered.is_empty(), findings);
        return;
    };
    let widen = |set: &BTreeSet<u8>| set.iter().map(|c| u32::from(*c)).collect::<Vec<_>>();
    let (all, rendered) = if map.two_byte() {
        (shown.pairs.iter().copied().collect(), shown.rendered_pairs.iter().copied().collect())
    } else {
        (widen(shown.codes), widen(shown.rendered))
    };
    let of = |codes: &[u32], glyph: Option<Glyph>| -> Vec<u32> {
        codes.iter().copied().filter(|c| map.select(*c) == glyph).collect()
    };
    let say = |condition, codes: Vec<u32>, what: &str, findings: &mut Vec<AuditFinding>| {
        if let Some(first) = codes.first() {
            findings.push(broken(
                condition,
                format!(
                    "/{name}: {} codes its text shows {what}, 0x{first:02X} first",
                    codes.len()
                ),
            ));
        }
    };
    say("31-030", of(&all, Some(Glyph::NotDef)), "select .notdef", findings);
    if embedded {
        say(
            "31-011",
            of(&rendered, Some(Glyph::Missing)),
            "select a glyph its program lacks",
            findings,
        );
    }
    if embedded {
        widths(doc, font, name, &map, &of(&rendered, Some(Glyph::Present)), findings);
    }
    unsettled(name, &of(&all, None), findings);
}

/// Codes the map could not settle — a TrueType name with no code to look up by — are left
/// for a reader: which glyph a reader draws for one is its own choice (9.6.6.4).
fn unsettled(name: &str, codes: &[u32], findings: &mut Vec<AuditFinding>) {
    if let Some(first) = codes.first() {
        findings.push(for_a_reader(
            "31-030",
            format!(
                "/{name}: {} codes its text shows reach no glyph by the lookup ISO 32000-1 \
                 describes, 0x{first:02X} first — which glyph a reader draws is its own choice",
                codes.len()
            ),
        ));
    }
}

/// A font this does not map: which glyph its codes select is left for a reader.
fn unmapped(name: &str, rendered_and_embedded: bool, findings: &mut Vec<AuditFinding>) {
    let what = "which glyph its codes select is not followed here for a font of this kind — \
                its program is not embedded, its CMap is not an Identity one, or its program \
                or encoding does not read";
    findings.push(for_a_reader("31-030", format!("/{name}: {what}")));
    if rendered_and_embedded {
        findings.push(for_a_reader("31-011", format!("/{name}: {what}")));
        findings.push(for_a_reader("31-016", format!("/{name}: {what}")));
    }
}

/// 31-016: each rendered code's width in the dictionary against its glyph's in the
/// program, to 1/1000 unit — the difference 7.21.5 allows. A code whose glyph or widths
/// cannot be read leaves the condition to a reader, unless another code already breaks it.
fn widths(
    doc: &Document,
    font: Handle<Object>,
    name: &str,
    map: &crate::glyph_map::GlyphMap,
    rendered: &[u32],
    findings: &mut Vec<AuditFinding>,
) {
    if rendered.is_empty() {
        return;
    }
    let Some(program) = crate::glyph_widths::program_widths(doc, font) else {
        findings.push(for_a_reader(
            "31-016",
            format!("/{name}: its program's widths are not read here for a font of this kind"),
        ));
        return;
    };
    let (mut differing, mut unread) = (Vec::new(), 0_usize);
    for code in rendered {
        let dictionary = crate::glyph_widths::dictionary_width(doc.arena(), font, *code);
        let held = map.target(*code).and_then(|t| program.of(&t));
        match (dictionary, held) {
            (Some(d), Some(p)) if (d - p).abs() > 1.0 => differing.push((*code, d, p)),
            (Some(_), Some(_)) => {}
            _ => unread += 1,
        }
    }
    if let Some((code, d, p)) = differing.first() {
        findings.push(broken(
            "31-016",
            format!(
                "/{name}: {} codes it renders have a width in the dictionary more than 1/1000 \
                 unit from the program's, 0x{code:02X} first ({d} against {p:.1})",
                differing.len()
            ),
        ));
    } else if unread > 0 {
        findings.push(for_a_reader(
            "31-016",
            format!("/{name}: {unread} codes it renders have a width this does not read"),
        ));
    }
}
