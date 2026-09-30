//! A form field created here, reopened, and reported as what it was given.
//!
//! **The check is the round trip, not the screen**
//! ([ADR-0087](../../../docs/adr/0087-a-form-field-is-created-here-not-only-filled.md)):
//! a form created here, written out and opened again, reports every field through the
//! same reader that reports somebody else's document.
//!
//! The engine read forms well and wrote them barely. `inspect interactive` reported every
//! field a document carried and `SetFormFieldValue` changed values in a form that already
//! existed; no operation created one, so a document this engine declares PDF/UA-2
//! conforming could have fields it could only complain about.

use fepdf::{FieldKind, IngestionOptions, NewField, Operation, PdfDocument, SaveOptions};

/// A one-page document with no form at all, so that what a field needs is created too.
fn blank() -> PdfDocument {
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] >>",
        ])
        .into_iter()
        .collect::<Vec<u8>>()
        .into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// The document through a file and back, which is where a created field has to survive.
fn round_trip(doc: &PdfDocument, name: &str) -> PdfDocument {
    let path =
        std::env::temp_dir().join(format!("fepdf_create_field_{name}_{}.pdf", std::process::id()));
    doc.save_with_options(&path, "2.0", &SaveOptions::default()).expect("it writes");
    let written = std::fs::read(&path).expect("the output is there");
    let _ = std::fs::remove_file(&path);
    PdfDocument::open_with_options(written.into(), &IngestionOptions::default())
        .expect("what this engine wrote, this engine opens")
}

fn field(name: &str, kind: FieldKind) -> NewField {
    NewField {
        page: 0,
        rect: (20.0, 300.0, 220.0, 330.0),
        name: name.to_string(),
        tooltip: format!("what {name} is for"),
        kind,
    }
}

/// **Every one of the nine kinds is created and read back as the type it was given.**
///
/// The types are `/FT` and the kinds are nine, because `/Ff` decides the rest: bit 13 is
/// `Multiline` on a text field and nothing on any other.
#[test]
fn all_nine_kinds_are_created_and_reported() {
    let kinds = [
        ("plain", FieldKind::Text { value: "x".to_string() }, "Tx"),
        ("many", FieldKind::TextArea { value: "y".to_string() }, "Tx"),
        ("secret", FieldKind::Password, "Tx"),
        ("tick", FieldKind::CheckBox { on: true }, "Btn"),
        ("one_of", FieldKind::RadioButton { group: "g".to_string(), on: false }, "Btn"),
        ("press", FieldKind::PushButton { caption: "Go".to_string() }, "Btn"),
        (
            "drop",
            FieldKind::ComboBox { options: vec!["a".into(), "b".into()], value: "b".into() },
            "Ch",
        ),
        (
            "list",
            FieldKind::ListBox { options: vec!["a".into(), "b".into()], value: "a".into() },
            "Ch",
        ),
        ("sign", FieldKind::Signature, "Sig"),
    ];

    let mut doc = blank();
    for (name, kind, _) in &kinds {
        doc.apply(Operation::AddFormField(field(name, kind.clone())))
            .unwrap_or_else(|why| panic!("creating {name} failed: {why}"));
    }
    let reopened = round_trip(&doc, "nine");
    let form = fepdf::form_of(reopened.inner());

    assert!(form.declared, "the document this engine wrote declares no form");
    assert_eq!(form.terminal.len(), kinds.len(), "not every field came back");
    for (field, (name, kind, wanted_type)) in form.terminal.iter().zip(kinds.iter()) {
        // A radio button is a widget of its group's field, which is what is reported.
        let reported =
            if let FieldKind::RadioButton { group, .. } = kind { group.as_str() } else { name };
        assert_eq!(
            field.qualified_name.as_deref(),
            Some(reported),
            "the fields came back in a different order"
        );
        assert_eq!(
            field.field_type.as_deref(),
            Some(*wanted_type),
            "{name} came back as a different type"
        );
        // The check ADR-0087 named: `inspect audit` finding a field without a `/TU` is
        // the failure, and a creator that wrote the defect it reports would be an
        // auditor writing its own findings.
        assert_eq!(
            field.tooltip.as_deref(),
            Some(format!("what {name} is for").as_str()),
            "{name} came back without the /TU it was given"
        );
    }
}

/// **A field is created holding what it was given to hold.**
///
/// A password is the exception and is exempt by name: writing one into a document puts it
/// in the document, and this engine will not.
#[test]
fn a_created_field_holds_the_value_it_was_given() {
    let mut doc = blank();
    doc.apply(Operation::AddFormField(field(
        "plain",
        FieldKind::Text { value: "typed".to_string() },
    )))
    .expect("the field is created");
    doc.apply(Operation::AddFormField(field(
        "drop",
        FieldKind::ComboBox { options: vec!["a".into(), "b".into()], value: "b".into() },
    )))
    .expect("the field is created");
    doc.apply(Operation::AddFormField(field("secret", FieldKind::Password)))
        .expect("the field is created");

    let form = fepdf::form_of(round_trip(&doc, "values").inner());
    let held = |name: &str| {
        form.terminal
            .iter()
            .find(|f| f.qualified_name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("{name} is not in the form"))
            .value
            .clone()
    };
    assert_eq!(held("plain").as_deref(), Some("typed"), "the text field lost its value");
    assert_eq!(held("drop").as_deref(), Some("b"), "the choice field lost its value");
    assert_eq!(held("secret"), None, "a password was written into the document");
}

/// **Every created field carries a `/TU`**, because a field without one is a Matterhorn
/// failure this engine already reports — and an auditor that created the defect it names
/// would be writing its own findings.
#[test]
fn a_field_with_no_tooltip_is_refused() {
    let mut doc = blank();
    let mut asked = field("anon", FieldKind::Text { value: String::new() });
    asked.tooltip = String::new();
    let error = doc.apply(Operation::AddFormField(asked)).expect_err("it refuses");
    assert!(error.to_string().contains("/TU"), "the refusal does not say what is missing: {error}");
}

/// A field with no name cannot be filled, and saying so beats writing one that cannot.
#[test]
fn a_field_with_no_name_is_refused() {
    let mut doc = blank();
    let error = doc
        .apply(Operation::AddFormField(field("  ", FieldKind::Text { value: String::new() })))
        .expect_err("it refuses");
    assert!(error.to_string().contains("no name"), "the refusal does not say why: {error}");
}

/// **A field this engine creates can be filled by the part of it that fills fields.**
///
/// Creating and filling are two halves of one thing, and a field created in a shape
/// `SetFormFieldValue` cannot reach would be a form only its maker can use.
#[test]
fn a_created_field_can_then_be_filled() {
    let mut doc = blank();
    doc.apply(Operation::AddFormField(field("who", FieldKind::Text { value: String::new() })))
        .expect("the field is created");
    doc.apply(Operation::SetFormFieldValue(fepdf::FormFieldSpec {
        name: "who".to_string(),
        value: fepdf::FormValue::Text("someone".to_string()),
    }))
    .expect("the value applies");

    let form = fepdf::form_of(round_trip(&doc, "filled").inner());
    assert_eq!(
        form.terminal[0].value.as_deref(),
        Some("someone"),
        "the field this engine created did not keep what was put in it"
    );
}

/// **A field's words are text strings (7.9.2.2)**, so a name in Japanese is written as
/// one: UTF-16 or UTF-8 behind its byte order mark. They were the UTF-8 bytes bare, which
/// every other reader takes for PDFDocEncoding, so 電話 was a field named in mojibake
/// everywhere but here. Read back without refinement, which would repair what it reads.
#[test]
fn a_fields_words_are_written_as_text_strings() {
    let mut doc = blank();
    let mut combo = field(
        "電話",
        FieldKind::ComboBox {
            options: vec!["東京".to_string(), "大阪".to_string()],
            value: "東京".to_string(),
        },
    );
    combo.tooltip = "電話番号".to_string();
    doc.apply(Operation::AddFormField(combo)).expect("the field is created");
    let mut push = field("押す", FieldKind::PushButton { caption: "送信".to_string() });
    push.rect = (20.0, 200.0, 220.0, 230.0);
    doc.apply(Operation::AddFormField(push)).expect("the button is created");

    let path = std::env::temp_dir().join(format!("fepdf_field_text_{}.pdf", std::process::id()));
    doc.save_with_options(&path, "2.0", &SaveOptions::default()).expect("it writes");
    let written = std::fs::read(&path).expect("the output is there");
    let _ = std::fs::remove_file(&path);
    let raw = IngestionOptions { active_refinement: false, ..IngestionOptions::default() };
    let back = PdfDocument::open_with_options(written.into(), &raw).expect("it reads back");

    let arena = back.inner().arena();
    let marked =
        |bytes: &[u8]| bytes.starts_with(&[0xFE, 0xFF]) || bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    let mut strings = Vec::new();
    let mut push = |key: &str, value: &fepdf_model::Object| {
        if let fepdf_model::Object::String(b) | fepdf_model::Object::Hex(b) = value {
            strings.push((key.to_string(), b.to_vec()));
        }
    };
    for dict in arena.all_dict_handles() {
        for (key, value) in arena.get_dict(dict).unwrap_or_default() {
            let key = arena.get_name(key).map(|k| k.as_str().to_string()).unwrap_or_default();
            match (key.as_str(), value.resolve(arena)) {
                ("Opt", fepdf_model::Object::Array(options)) => {
                    for option in arena.get_array(options).unwrap_or_default() {
                        push("Opt", &option);
                    }
                }
                ("T" | "TU" | "V" | "CA", _) => push(&key, &value),
                _ => {}
            }
        }
    }
    let bare: Vec<_> =
        strings.iter().filter(|(_, b)| !b.is_ascii() && !marked(b)).map(|(k, _)| k).collect();
    assert!(strings.len() >= 6, "the words were not found to check: {strings:?}");
    assert!(bare.is_empty(), "written as bare bytes: {bare:?}");
}

/// The state each widget of the field `name` shows, in the order of its `/Kids`.
fn shown(doc: &PdfDocument, name: &str) -> Vec<String> {
    let arena = doc.inner().arena();
    let form = fepdf::form_of(doc.inner());
    assert!(form.terminal.iter().any(|f| f.qualified_name.as_deref() == Some(name)));
    let catalog = doc.inner().catalog_handle().and_then(|c| doc.inner().resolve_to_dict(c).ok());
    let entry = |dict, key: &str| arena.dict_entry(dict, arena.name(key)).map(|v| v.resolve(arena));
    let acro = catalog.and_then(|c| entry(c, "AcroForm")).and_then(|a| a.as_dict_handle());
    let fields = match acro.and_then(|a| entry(a, "Fields")) {
        Some(fepdf_model::Object::Array(a)) => arena.get_array(a).unwrap_or_default(),
        _ => Vec::new(),
    };
    let group = fields
        .iter()
        .filter_map(|f| f.resolve(arena).as_dict_handle())
        .find(|f| matches!(entry(*f, "T"), Some(t) if text_of(arena, &t).as_deref() == Some(name)))
        .expect("the group is a field of the form");
    let kids = match entry(group, "Kids") {
        Some(fepdf_model::Object::Array(a)) => arena.get_array(a).unwrap_or_default(),
        _ => Vec::new(),
    };
    kids.iter()
        .filter_map(|k| k.resolve(arena).as_dict_handle())
        .filter_map(|k| entry(k, "AS")?.as_name().and_then(|n| arena.get_name_str(n)))
        .collect()
}

/// A text string entry, as its text.
fn text_of(arena: &fepdf_model::PdfArena, value: &fepdf_model::Object) -> Option<String> {
    match value.resolve(arena) {
        fepdf_model::Object::Text(t) => Some(t),
        fepdf_model::Object::String(b) | fepdf_model::Object::Hex(b) => {
            Some(fepdf_model::refine::text::recover_string(&b))
        }
        _ => None,
    }
}

/// **Radio buttons given one group are one field, and one of them is chosen** (12.7.5.2.4).
/// Each was a field of its own under its own name with the on state `/Yes` every other
/// had, so a group was a set of unrelated boxes and choosing one chose nothing else.
/// Choosing by name then turns the rest off, and a name no button has is refused.
#[test]
fn radio_buttons_in_a_group_are_one_field_and_one_is_chosen() {
    let mut doc = blank();
    let button = |name: &str, x: f64, on: bool| {
        let mut made = field(name, FieldKind::RadioButton { group: "size".to_string(), on });
        made.rect = (x, 300.0, x + 20.0, 320.0);
        Operation::AddFormField(made)
    };
    doc.apply(button("S", 20.0, false)).expect("S is added");
    doc.apply(button("M", 60.0, true)).expect("M is added");
    doc.apply(button("L", 100.0, true)).expect("L is added");
    doc.apply(button("S", 140.0, false)).expect_err("a second S is refused");

    let back = round_trip(&doc, "radio");
    let form = fepdf::form_of(back.inner());
    let size = form.terminal.iter().find(|f| f.qualified_name.as_deref() == Some("size"));
    assert_eq!(
        size.and_then(|f| f.value.as_deref()),
        Some("/L"),
        "the last chosen is not the value"
    );
    assert_eq!(shown(&back, "size"), ["Off", "Off", "L"]);

    let mut chosen = back;
    chosen
        .apply(Operation::SetFormFieldValue(fepdf::FormFieldSpec {
            name: "size".to_string(),
            value: fepdf::FormValue::Choice("S".to_string()),
        }))
        .expect("S is chosen");
    assert_eq!(shown(&chosen, "size"), ["S", "Off", "Off"]);
    let refused = chosen
        .apply(Operation::SetFormFieldValue(fepdf::FormFieldSpec {
            name: "size".to_string(),
            value: fepdf::FormValue::Choice("XL".to_string()),
        }))
        .expect_err("a state no button has is refused");
    assert!(refused.to_string().contains("XL"), "the refusal does not name it: {refused}");
}

/// **A field in a group draws in the face its group states** (12.7.4.3): `/DA` is
/// inherited, and the field's own and the form's were read and its group's was not.
#[test]
fn a_field_draws_in_the_appearance_its_group_states() {
    let mut doc = PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] /DA (/Helv 12 Tf 0 g) \
             /DR << /Font << /Helv 6 0 R >> >> >> >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Annots [5 0 R] >>",
            "<< /T (group) /FT /Tx /DA (/Helv 7 Tf 0 g) /Kids [5 0 R] >>",
            "<< /Type /Annot /Subtype /Widget /T (name) /Parent 4 0 R /P 3 0 R \
             /Rect [20 300 220 330] >>",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        ])
        .into_iter()
        .collect::<Vec<u8>>()
        .into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens");
    doc.apply(Operation::SetFormFieldValue(fepdf::FormFieldSpec {
        name: "group.name".to_string(),
        value: fepdf::FormValue::Text("typed".to_string()),
    }))
    .expect("the value is written");
    let arena = doc.inner().arena();
    let widget =
        arena.get_object(arena.handle(5)).and_then(|o| o.as_dict_handle()).expect("widget");
    let normal = arena
        .dict_entry(widget, arena.name("AP"))
        .and_then(|ap| arena.dict_entry(ap.resolve(arena).as_dict_handle()?, arena.name("N")))
        .map(|n| n.resolve(arena))
        .expect("the widget has an appearance");
    let drawn = doc.inner().decode_stream(&normal).expect("it decodes");
    let drawn = String::from_utf8_lossy(&drawn);
    assert!(drawn.contains("/Helv 7 Tf"), "not drawn at the group's size: {drawn}");
}
