//! One attribute of a structure element, written in its owner's attribute object (14.7.6).
//!
//! **WTPDF asks for attributes the vocabulary could not write** (8.2.6): a header cell's
//! `Scope`, a list's `ListNumbering`, a note's `NoteType`, the `Layout` keys. Only
//! `/UserProperties` could be added; `SetStructAttribute` writes any owner's key.

use fepdf::{
    AttributeValue, IngestionOptions, Operation, Outcome, PdfDocument, StructAttribute,
    UserProperty, UserPropertyValue,
};
use fepdf_model::Handle;
use fepdf_model::object::Object;

/// A tagged page whose table's one header cell, object 7, states no `Scope`, and whose
/// object 8 is a paragraph with its `/A` written in place.
fn table() -> PdfDocument {
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 4 0 R /Lang (en) >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        "<< /Type /StructTreeRoot /K [5 0 R 8 0 R] >>",
        "<< /Type /StructElem /S /Table /P 4 0 R /K [6 0 R] >>",
        "<< /Type /StructElem /S /TR /P 5 0 R /K [7 0 R] >>",
        "<< /Type /StructElem /S /TH /P 6 0 R >>",
        "<< /Type /StructElem /S /P /P 4 0 R /A << /O /Layout /Placement /Block >> >>",
    ]);
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default()).expect("it opens")
}

/// `SetStructAttribute` of `key` in `owner` on object `element`.
fn set(owner: &str, key: &str, element: u32, value: AttributeValue) -> Operation {
    Operation::SetStructAttribute(StructAttribute {
        handle_index: element,
        owner: owner.to_string(),
        key: key.to_string(),
        value,
    })
}

/// The attribute objects an element's `/A` holds, each as its owner and its entries' keys,
/// sorted.
fn attributes(doc: &PdfDocument, element: u32) -> Vec<(String, Vec<String>)> {
    let arena = doc.inner().arena();
    let dict = arena.get_object(Handle::new(element)).and_then(|o| o.as_dict_handle());
    let a = dict.and_then(|d| arena.dict_entry(d, arena.name("A"))).map(|a| a.resolve(arena));
    let items = match a {
        Some(Object::Array(array)) => arena.get_array(array).unwrap_or_default(),
        Some(single) => vec![single],
        None => Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| {
            let entries = arena.get_dict(item.resolve(arena).as_dict_handle()?)?;
            let name = |key: &str| {
                entries
                    .get(&arena.name(key))
                    .and_then(Object::as_name)
                    .and_then(|n| arena.get_name(n))
                    .map(|n| n.as_str().to_string())
            };
            let keys =
                entries.keys().filter_map(|k| arena.get_name(*k)).map(|k| k.as_str().to_string());
            let mut keys: Vec<String> = keys.filter(|k| k != "O").collect();
            keys.sort();
            Some((name("O")?, keys))
        })
        .collect()
}

/// **A header cell given its `Scope` meets 15-003**, which the same cell without one breaks.
#[test]
fn a_header_cell_given_its_scope_meets_15_003() {
    let mut doc = table();
    let outcome = |doc: &PdfDocument| {
        let report = doc.audit_ua2_report().expect("it audits");
        report
            .findings
            .iter()
            .filter(|f| f.checkpoint == "15-003")
            .map(|f| f.outcome)
            .collect::<Vec<_>>()
    };
    assert_eq!(outcome(&doc), vec![Outcome::Broken], "the cell with no Scope");
    doc.apply(set("Table", "Scope", 7, AttributeValue::Name("Column".into()))).expect("it is set");
    assert_eq!(outcome(&doc), vec![Outcome::Sound], "the cell with Scope Column");
}

/// **One attribute object per owner**: a second key for `Table` joins the first, and a key
/// for another owner gets an object of its own.
#[test]
fn a_second_key_for_an_owner_joins_its_object() {
    let mut doc = table();
    doc.apply(set("Table", "Scope", 7, AttributeValue::Name("Row".into()))).expect("set");
    doc.apply(set("Table", "Headers", 7, AttributeValue::Strings(vec!["h1".into()]))).expect("set");
    doc.apply(set("Layout", "TextAlign", 7, AttributeValue::Name("Center".into()))).expect("set");
    assert_eq!(
        attributes(&doc, 7),
        vec![
            ("Table".to_string(), vec!["Headers".to_string(), "Scope".to_string()]),
            ("Layout".to_string(), vec!["TextAlign".to_string()]),
        ]
    );
}

/// **An `/A` written in place is kept**: a user property added beside it, and a `Layout` key
/// set in it rather than in a second `Layout` object.
#[test]
fn an_attribute_dictionary_written_in_place_is_kept() {
    let mut doc = table();
    doc.apply(Operation::AddUserProperties {
        target_handle: 8,
        properties: vec![UserProperty {
            name: "Owner".into(),
            value: UserPropertyValue::Text("Ops".into()),
            formatted: None,
        }],
    })
    .expect("the property is added");
    doc.apply(set("Layout", "SpaceBefore", 8, AttributeValue::Number(6.0))).expect("set");
    assert_eq!(
        attributes(&doc, 8),
        vec![
            ("Layout".to_string(), vec!["Placement".to_string(), "SpaceBefore".to_string()]),
            ("UserProperties".to_string(), vec!["P".to_string()]),
        ]
    );
}

/// **An attribute of an element that is not there is refused.**
#[test]
fn an_attribute_of_nothing_is_refused() {
    let mut doc = table();
    assert!(doc.apply(set("Table", "Scope", 9999, AttributeValue::Name("Row".into()))).is_err());
}

/// **An element's `/Ref` names the elements it refers to, and survives a file** (Table 355,
/// WTPDF 8.8): the paragraph object 8 refers to the table and its header cell, in order; an
/// empty list removes the entry; and a target that is not an element is refused.
#[test]
fn an_element_refers_to_others_by_ref() {
    let mut doc = table();
    doc.apply(Operation::SetStructRefs { handle_index: 8, targets: vec![5, 7] }).expect("set");
    let refs = |doc: &PdfDocument| -> Vec<u32> {
        let arena = doc.inner().arena();
        let dict = arena.get_object(Handle::new(8)).and_then(|o| o.as_dict_handle());
        match dict.and_then(|d| arena.dict_entry(d, arena.name("Ref"))) {
            Some(Object::Array(a)) => arena
                .get_array(a)
                .unwrap_or_default()
                .iter()
                .filter_map(|r| r.as_reference().map(|h| h.index()))
                .collect(),
            _ => Vec::new(),
        }
    };
    assert_eq!(refs(&doc), vec![5, 7]);
    let path = std::env::temp_dir().join("fepdf-struct-refs.pdf");
    doc.save_as_version(&path, "2.0").expect("it writes");
    let bytes = std::fs::read(&path).expect("it is on disk");
    let reopened =
        PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default()).expect("opens");
    // Written out, the objects are numbered afresh: the paragraph is found by its tag.
    let arena = reopened.inner().arena();
    let paragraph = arena.all_dict_handles().into_iter().find(|d| {
        arena
            .dict_entry(*d, arena.name("S"))
            .and_then(|s| s.as_name())
            .and_then(|n| arena.get_name(n))
            .is_some_and(|n| n.as_str() == "P")
    });
    let written = paragraph.and_then(|d| arena.dict_entry(d, arena.name("Ref")));
    let written = match written.map(|r| r.resolve(arena)) {
        Some(Object::Array(a)) => arena.get_array(a).unwrap_or_default().len(),
        _ => 0,
    };
    assert_eq!(written, 2, "the references did not survive the file");
    assert!(doc.apply(Operation::SetStructRefs { handle_index: 8, targets: vec![9999] }).is_err());
    assert_eq!(refs(&doc), vec![5, 7], "a refused update changed the entry");
    doc.apply(Operation::SetStructRefs { handle_index: 8, targets: Vec::new() }).expect("cleared");
    assert!(refs(&doc).is_empty());
}

/// The structure tree root's `/Namespaces`, each as its `NS` and its `RoleMapNS` keys.
fn namespaces(doc: &PdfDocument) -> Vec<(String, Vec<String>)> {
    let arena = doc.inner().arena();
    let root = doc.inner().get_structure_root().ok().flatten().expect("a tree");
    let root = arena.get_object(root).and_then(|o| o.as_dict_handle()).expect("a dictionary");
    let listed = match arena.dict_entry(root, arena.name("Namespaces")).map(|n| n.resolve(arena)) {
        Some(Object::Array(a)) => arena.get_array(a).unwrap_or_default(),
        _ => Vec::new(),
    };
    listed
        .iter()
        .filter_map(|n| {
            let dict = n.resolve(arena).as_dict_handle()?;
            let name = match arena.dict_entry(dict, arena.name("NS"))? {
                Object::Text(t) => t,
                _ => return None,
            };
            let map = arena
                .dict_entry(dict, arena.name("RoleMapNS"))
                .and_then(|m| m.resolve(arena).as_dict_handle())
                .and_then(|m| arena.get_dict(m))
                .unwrap_or_default();
            let mut keys: Vec<String> = map
                .keys()
                .filter_map(|k| arena.get_name(*k))
                .map(|k| k.as_str().to_string())
                .collect();
            keys.sort();
            Some((name, keys))
        })
        .collect()
}

/// The namespace an element's `/NS` names, if it names one.
fn element_namespace(doc: &PdfDocument, element: u32) -> Option<String> {
    let arena = doc.inner().arena();
    let dict = arena.get_object(Handle::new(element))?.as_dict_handle()?;
    let ns = arena.dict_entry(dict, arena.name("NS"))?.resolve(arena).as_dict_handle()?;
    match arena.dict_entry(ns, arena.name("NS"))? {
        Object::Text(t) => Some(t),
        _ => None,
    }
}

/// **Elements put in a namespace share its one dictionary** (14.7.4): two elements in PDF
/// 2.0's standard namespace list it once in `/Namespaces`; an empty name returns an element
/// to the default; and a type mapped in a custom namespace to one of another namespace adds
/// that namespace and writes the pair (Table 356).
#[test]
fn elements_share_a_namespace_and_a_namespace_maps_its_types() {
    const PDF2: &str = "http://iso.org/pdf2/ssn";
    let mut doc = table();
    for element in [5, 8] {
        doc.apply(Operation::SetStructNamespace { handle_index: element, namespace: PDF2.into() })
            .expect("set");
    }
    assert_eq!(element_namespace(&doc, 5).as_deref(), Some(PDF2));
    assert_eq!(element_namespace(&doc, 8).as_deref(), Some(PDF2));
    assert_eq!(namespaces(&doc), vec![(PDF2.to_string(), Vec::new())]);

    doc.apply(Operation::SetStructNamespace { handle_index: 8, namespace: String::new() })
        .expect("removed");
    assert_eq!(element_namespace(&doc, 8), None);

    doc.apply(Operation::MapStructType {
        namespace: "urn:example:tags".into(),
        from: "Chapter".into(),
        to: "Section".into(),
        to_namespace: Some(PDF2.into()),
    })
    .expect("mapped");
    assert_eq!(
        namespaces(&doc),
        vec![
            (PDF2.to_string(), Vec::new()),
            ("urn:example:tags".to_string(), vec!["Chapter".to_string()])
        ]
    );
    assert!(
        doc.apply(Operation::SetStructNamespace { handle_index: 9999, namespace: PDF2.into() })
            .is_err()
    );
}

/// **A file associated with an element is in its `/AF`, after any it had** (14.13), with
/// the relationship given — a formula's MathML as a `Supplement` (WTPDF 8.2.5.29) — and a
/// specification naming it both ways, so 21-001 stays sound.
#[test]
fn a_file_is_associated_with_an_element() {
    let mut doc = table();
    let mathml = fepdf::AssociatedFile {
        filename: "formula.mml".into(),
        relationship: fepdf::AFRelationship::Supplement,
        mime_type: "application/mathml+xml".into(),
        data: b"<math><mi>x</mi></math>".to_vec(),
    };
    for _ in 0..2 {
        doc.apply(Operation::AttachStructAssociatedFile { handle_index: 8, file: mathml.clone() })
            .expect("attached");
    }
    let arena = doc.inner().arena();
    let dict =
        arena.get_object(Handle::new(8)).and_then(|o| o.as_dict_handle()).expect("an element");
    let files = match arena.dict_entry(dict, arena.name("AF")).map(|a| a.resolve(arena)) {
        Some(Object::Array(a)) => arena.get_array(a).unwrap_or_default(),
        _ => Vec::new(),
    };
    assert_eq!(files.len(), 2, "each association is kept");
    let spec = files[0].resolve(arena).as_dict_handle().expect("a file specification");
    let relationship = arena
        .dict_entry(spec, arena.name("AFRelationship"))
        .and_then(|r| r.as_name())
        .and_then(|n| arena.get_name(n))
        .map(|n| n.as_str().to_string());
    assert_eq!(relationship.as_deref(), Some("Supplement"));
    let report = doc.audit_ua2_report().expect("it audits");
    let outcome: Vec<Outcome> =
        report.findings.iter().filter(|f| f.checkpoint == "21-001").map(|f| f.outcome).collect();
    assert_eq!(outcome, vec![Outcome::Sound], "the element's file is named both ways");
    assert!(
        doc.apply(Operation::AttachStructAssociatedFile { handle_index: 9999, file: mathml })
            .is_err()
    );
}
