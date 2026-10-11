//! A form's values, as FDF (12.7.8.3.2) and XFDF (ISO 19444-1 6.3) carry them: out of a
//! document, and into one (ROADMAP AA-3b, [ADR-0125]).
//!
//! **Matched by fully qualified name, and nothing is created.** A value whose field the
//! document does not have is said and left; so is one the field cannot take — a state its
//! buttons do not have. A value is set as filling the form by hand sets it, through
//! `SetFormFieldValue`, so its appearance is drawn the same way.
//!
//! **What the two files share is here; their syntax is in `fdf` and `xfdf`.**
//!
//! [ADR-0125]: ../../../docs/adr/0125-form-values-come-in-by-name-and-make-no-field.md

use crate::operation::{FormFieldSpec, FormValue};
use fepdf_model::interpretation::Decision;
use fepdf_model::{DictHandle, Document, Object, PdfArena, PdfResult};

/// A value as a file gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Given {
    /// A text string: a text or choice field's value.
    Text(String),
    /// A name: a button's state.
    Name(String),
    /// Several: a choice field with more than one chosen.
    Several(Vec<String>),
}

/// Flags an FDF field changes (Table 249): `/Ff` replaced, or bits set and cleared; the
/// same for its widgets' `/F`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    pub ff: Option<i64>,
    pub set_ff: Option<i64>,
    pub clr_ff: Option<i64>,
    pub f: Option<i64>,
    pub set_f: Option<i64>,
    pub clr_f: Option<i64>,
}

/// One field of a file, by its fully qualified name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incoming {
    pub name: String,
    pub value: Option<Given>,
    pub flags: Flags,
    /// Entries of the file's field this does not import: its appearances, options,
    /// actions and rich text.
    pub unread: Vec<String>,
}

/// One terminal field of a document, for a file: its name's parts, and its value.
pub struct Outgoing {
    pub parts: Vec<String>,
    pub value: Option<Given>,
}

/// The document's terminal fields and their values, in the form's order. A signature
/// field's value is not exported (ISO 19444-1 6.3.3).
pub fn values(doc: &Document) -> Vec<Outgoing> {
    let arena = doc.arena();
    let Some(acro) = acro_form(doc) else { return Vec::new() };
    crate::apply::fields::named_fields(arena, acro)
        .into_iter()
        .filter(|(_, _, dict)| terminal(arena, *dict))
        .map(|(name, _, dict)| {
            let signature = crate::apply::annotations::inherited(arena, dict, "FT")
                .and_then(|t| t.as_name())
                .is_some_and(|t| t == arena.name("Sig"));
            let value = crate::apply::annotations::inherited(arena, dict, "V")
                .filter(|_| !signature)
                .and_then(|v| given(arena, &v));
            Outgoing { parts: name.split('.').map(str::to_owned).collect(), value }
        })
        .collect()
}

/// A field of a file being written: its partial name, its value, and its kids.
pub struct FieldNode {
    pub name: String,
    pub value: Option<Given>,
    pub kids: Vec<Self>,
}

/// `values` as a field tree: each under the fields its name's earlier parts name.
pub fn tree(values: Vec<Outgoing>) -> Vec<FieldNode> {
    let mut roots = Vec::new();
    for field in values {
        insert(&mut roots, &field.parts, field.value);
    }
    roots
}

fn insert(nodes: &mut Vec<FieldNode>, parts: &[String], value: Option<Given>) {
    let Some((first, rest)) = parts.split_first() else { return };
    let at = match nodes.iter().position(|n| n.name == *first) {
        Some(at) => at,
        None => {
            nodes.push(FieldNode { name: first.clone(), value: None, kids: Vec::new() });
            nodes.len() - 1
        }
    };
    if let Some(node) = nodes.get_mut(at) {
        if rest.is_empty() {
            node.value = value;
        } else {
            insert(&mut node.kids, rest, value);
        }
    }
}

/// Whether `dict` is a terminal field: none of its kids is a field with a name of its own.
fn terminal(arena: &PdfArena, dict: DictHandle) -> bool {
    let kids = arena
        .dict_entry(dict, arena.name("Kids"))
        .and_then(|k| k.resolve(arena).as_array())
        .and_then(|k| arena.get_array(k))
        .unwrap_or_default();
    !kids.iter().any(|kid| {
        kid.resolve(arena)
            .as_dict_handle()
            .is_some_and(|k| arena.dict_entry(k, arena.name("T")).is_some())
    })
}

/// A field's `/V` as a file carries it.
fn given(arena: &PdfArena, value: &Object) -> Option<Given> {
    if let Object::Name(name) = value {
        return arena.get_name_str(*name).map(Given::Name);
    }
    if let Object::Array(items) = value {
        let all = arena.get_array(*items)?;
        let texts =
            all.iter().filter_map(|i| crate::apply::fields::text_of(arena, &i.resolve(arena)));
        return Some(Given::Several(texts.collect()));
    }
    crate::apply::fields::text_of(arena, value).map(Given::Text)
}

/// Puts `incoming` into the document's form, each by its name, and says what it left.
///
/// # Errors
/// Never for a value that does not fit, which is said; only when the catalogue will not
/// read.
pub fn put(doc: &Document, incoming: &[Incoming], source: &'static str) -> PdfResult<()> {
    let arena = doc.arena();
    let Some(acro) = acro_form(doc) else {
        if !incoming.is_empty() {
            doc.record(Decision::ambiguity(
                source,
                format!("{} field values, and the document has no form", incoming.len()),
                "imported none of them",
            ));
        }
        return Ok(());
    };
    let fields = crate::apply::fields::named_fields(arena, acro);
    for field in incoming {
        let Some((_, _, dict)) = fields.iter().find(|(name, _, _)| *name == field.name) else {
            doc.record(Decision::ambiguity(
                source,
                format!("a value for {:?}, which the document has no field of", field.name),
                "left it out: an import fills fields and makes none",
            ));
            continue;
        };
        one(doc, field, *dict, source);
    }
    Ok(())
}

/// One field's value and flags, and what was not imported of it.
fn one(doc: &Document, field: &Incoming, dict: DictHandle, source: &'static str) {
    if let Some(value) = &field.value
        && let Some(form_value) = form_value(doc, field, value, source)
        && let Err(why) = crate::apply::annotations::apply_set_form_field_value(
            doc,
            FormFieldSpec { name: field.name.clone(), value: form_value },
        )
    {
        doc.record(Decision::ambiguity(
            source,
            format!("{:?} cannot take the value given: {why}", field.name),
            "left its value as it was",
        ));
    }
    flags(doc.arena(), dict, field.flags);
    if !field.unread.is_empty() {
        doc.record(Decision::ambiguity(
            source,
            format!("{:?} carries {}", field.name, field.unread.join(", ")),
            "imported its value and flags only",
        ));
    }
}

/// The value as `SetFormFieldValue` takes it: a name is a button's state, a text a text,
/// and of several only the first, which is said.
fn form_value(
    doc: &Document,
    field: &Incoming,
    value: &Given,
    source: &'static str,
) -> Option<FormValue> {
    match value {
        Given::Text(text) => Some(FormValue::Text(text.clone())),
        Given::Name(state) => Some(FormValue::Choice(state.clone())),
        Given::Several(all) => {
            if all.len() > 1 {
                doc.record(Decision::ambiguity(
                    source,
                    format!("{:?} is given {} choices", field.name, all.len()),
                    "chose the first: the form is filled with one choice at a time",
                ));
            }
            all.first().map(|first| FormValue::Choice(first.clone()))
        }
    }
}

/// Table 249's flags: `/Ff` onto the field, `/F` onto each of its widgets. A whole value
/// replaces; otherwise bits are set, then cleared.
fn flags(arena: &PdfArena, field: DictHandle, flags: Flags) {
    let changed = |now: i64, whole: Option<i64>, set: Option<i64>, clear: Option<i64>| {
        whole.unwrap_or_else(|| (now | set.unwrap_or(0)) & !clear.unwrap_or(0))
    };
    let change = |dict: DictHandle,
                  key: &str,
                  (whole, set, clear): (Option<i64>, Option<i64>, Option<i64>)| {
        if whole.is_none() && set.is_none() && clear.is_none() {
            return;
        }
        let mut entries = arena.get_dict(dict).unwrap_or_default();
        let now = entries.get(&arena.name(key)).and_then(Object::as_integer).unwrap_or(0);
        entries.insert(arena.name(key), Object::Integer(changed(now, whole, set, clear)));
        arena.set_dict(dict, entries);
    };
    change(field, "Ff", (flags.ff, flags.set_ff, flags.clr_ff));
    for widget in crate::apply::annotations::widgets_of(arena, field) {
        change(widget, "F", (flags.f, flags.set_f, flags.clr_f));
    }
}

/// The document's `/AcroForm`.
fn acro_form(doc: &Document) -> Option<DictHandle> {
    let arena = doc.arena();
    let catalog = doc.resolve_to_dict(doc.catalog_handle()?).ok()?;
    arena.dict_entry(catalog, arena.name("AcroForm"))?.resolve(arena).as_dict_handle()
}
