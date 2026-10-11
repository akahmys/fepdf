//! XFDF's form fields (ISO 19444-1 6.3): `<fields>`, a `<field>` for each partial name,
//! nested as the names are, and a `<value>` — several for a choice with several chosen.
//! What they mean is `form_data`'s, shared with FDF.

use super::write::escape;
use crate::form_data::{FieldNode, Flags, Given, Incoming};
use fepdf_model::{Document, PdfResult};
use roxmltree::Node;
use std::fmt::Write as _;

/// The document's form values as an XFDF file. A line break in a value is written as one
/// line feed (6.3.3); a signature's value is not exported.
///
/// # Errors
/// Never, as yet; the signature matches `export`'s.
pub fn export_form(doc: &Document) -> PdfResult<String> {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xfdf xmlns=\"{}\" xml:space=\"preserve\">\n<fields>\n",
        super::NAMESPACE
    );
    for node in crate::form_data::tree(crate::form_data::values(doc)) {
        write_field(&mut out, &node);
    }
    out.push_str("</fields>\n</xfdf>\n");
    Ok(out)
}

fn write_field(out: &mut String, node: &FieldNode) {
    let _ = write!(out, "<field name=\"{}\">", escape(&node.name));
    let values: Vec<&String> = match &node.value {
        Some(Given::Text(text) | Given::Name(text)) => vec![text],
        Some(Given::Several(all)) => all.iter().collect(),
        None => Vec::new(),
    };
    for value in values {
        let _ = write!(
            out,
            "<value>{}</value>",
            escape(&value.replace("\r\n", "\n").replace('\r', "\n"))
        );
    }
    for kid in &node.kids {
        write_field(out, kid);
    }
    out.push_str("</field>\n");
}

/// The fields of `root`'s `<fields>`, by fully qualified name, each that has a value: one
/// `<value>` is a text, several a choice of several. `<value-richtext>` is not imported.
pub(super) fn incoming(root: Node<'_, '_>) -> Vec<Incoming> {
    let mut found = Vec::new();
    let tops = root.children().filter(|n| {
        n.has_tag_name((super::NAMESPACE, "fields")) || n.tag_name().name() == "fields"
    });
    let mut pending: Vec<(Node<'_, '_>, Option<String>, usize)> = tops
        .flat_map(|fields| fields.children().filter(|n| n.tag_name().name() == "field"))
        .map(|n| (n, None, 0))
        .collect();
    pending.reverse();
    while let Some((node, parent, depth)) = pending.pop() {
        let Some(own) = node.attribute("name") else { continue };
        let name = parent.map_or_else(|| own.to_owned(), |p| format!("{p}.{own}"));
        let values: Vec<String> = node
            .children()
            .filter(|n| n.tag_name().name() == "value")
            .map(|n| n.text().unwrap_or_default().to_owned())
            .collect();
        let rich = node.children().any(|n| n.tag_name().name() == "value-richtext");
        let value = match values.as_slice() {
            [] => None,
            [one] => Some(Given::Text(one.clone())),
            _ => Some(Given::Several(values.clone())),
        };
        if value.is_some() {
            let unread = if rich { vec!["<value-richtext>".to_owned()] } else { Vec::new() };
            found.push(Incoming { name: name.clone(), value, flags: Flags::default(), unread });
        }
        if depth < 64 {
            let kids = node.children().filter(|n| n.tag_name().name() == "field");
            pending.extend(kids.rev().map(|k| (k, Some(name.clone()), depth + 1)));
        }
    }
    found
}
