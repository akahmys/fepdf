//! What an interactive processor presents and permits for optional content
//! (ISO 32000-2, 6.3.2.3 and 8.11.4.3).
//!
//! Three of the rules here are `shall`s about the *user interface* rather than about
//! what is drawn, which is why the engine had parsed `/Order`, `/RBGroups` and `/Locked`
//! for phases without applying any of them — there was nothing to apply them to.
//!
//! - "Any groups not listed in this array **shall not be presented** in any user
//!   interface that uses the configuration", and `/Order` defaults to an **empty** array
//!   in the default configuration. A document with groups and no `/Order` presents none.
//! - "The state of a locked group **cannot be changed** through the user interface."
//! - `/RBGroups`: "the state of at most one optional content group in each array shall be
//!   ON at a time. If one group is turned ON, all others shall be turned OFF. However,
//!   turning a group from ON to OFF does not force any other group to be turned ON."

use fepdf_model::optional_content::{LayerId, LayerPanel, LayerRow, OptionalContentState};
use fepdf_model::{Document, Handle, Object};

/// Assembles a one-page file from object bodies, numbered from 1.
fn assemble(bodies: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-2.0\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let table_at = out.len();
    let size = bodies.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{table_at}\n%%EOF\n")
            .as_bytes(),
    );
    out
}

/// A file whose catalogue carries `/OCProperties` with two groups, objects 5 and 6.
fn document(configuration: &str) -> Document {
    let bodies = vec![
        format!(
            "<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [5 0 R 6 0 R] {configuration} >> >>"
        ),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_string(),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
        "<< /Type /OCG /Name (First) >>".to_string(),
        "<< /Type /OCG /Name (Second) >>".to_string(),
    ];
    fepdf_model::document::Document::open(
        assemble(&bodies).into(),
        &fepdf_model::ingest::IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

fn panel(doc: &Document) -> LayerPanel {
    LayerPanel::read(doc, &OptionalContentState::read(doc))
}

/// The groups a panel would present, flattened, as `(name, on, locked)`.
fn presented(rows: &[LayerRow]) -> Vec<(String, bool, bool)> {
    let mut out = Vec::new();
    for row in rows {
        match row {
            LayerRow::Group { name, on, locked, .. } => out.push((name.clone(), *on, *locked)),
            LayerRow::Nested(inner) => out.extend(presented(inner)),
            LayerRow::Label(_) => {}
        }
    }
    out
}

fn handle_of(doc: &Document, name: &str) -> Handle<Object> {
    fepdf_model::optional_content::group_named(doc, name)
        .expect("lookup succeeds")
        .unwrap_or_else(|| panic!("no group named {name}"))
}

fn id_of(doc: &Document, name: &str) -> LayerId {
    LayerPanel::id_of(handle_of(doc, name))
}

#[test]
fn a_configuration_without_order_presents_nothing() {
    // "In the default configuration dictionary, the default value shall be an empty
    // array", and "any groups not listed in this array shall not be presented". Two
    // groups exist and are drawn; neither is offered to the user.
    let doc = document("/D << /BaseState /ON >>");
    assert!(panel(&doc).rows.is_empty(), "no /Order means no rows");
}

#[test]
fn order_decides_which_groups_are_presented_and_in_what_sequence() {
    let doc = document("/D << /BaseState /ON /Order [6 0 R 5 0 R] >>");
    let rows = presented(&panel(&doc).rows);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "Second", "/Order's sequence, not /OCGs'");
    assert_eq!(rows[1].0, "First");

    // A group left out of /Order is drawn and not offered.
    let partial = document("/D << /BaseState /ON /Order [5 0 R] >>");
    let rows = presented(&panel(&partial).rows);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "First");
}

#[test]
fn a_nested_array_becomes_a_label_and_a_subtree() {
    // "Each nested array may optionally have as its first element a text string to be
    // used as a non-selectable label."
    let doc = document("/D << /BaseState /ON /Order [(Anatomy) [5 0 R 6 0 R]] >>");
    let rows = panel(&doc).rows;
    assert!(matches!(&rows[0], LayerRow::Label(text) if text == "Anatomy"));
    assert_eq!(presented(&rows).len(), 2, "the groups are still presented, one level down");
}

#[test]
fn the_state_shown_is_the_state_in_force() {
    let doc = document("/D << /BaseState /ON /OFF [6 0 R] /Order [5 0 R 6 0 R] >>");
    let rows = presented(&panel(&doc).rows);
    assert_eq!(rows[0], ("First".to_string(), true, false));
    assert_eq!(rows[1], ("Second".to_string(), false, false), "/OFF reaches the panel");
}

#[test]
fn a_locked_group_cannot_be_changed_through_the_panel() {
    let doc = document("/D << /BaseState /ON /Order [5 0 R 6 0 R] /Locked [5 0 R] >>");
    let panel = panel(&doc);
    let first = id_of(&doc, "First");
    assert!(panel.is_locked(first));
    assert!(!panel.set(&doc, first, false), "the panel refuses");
    assert!(
        OptionalContentState::read(&doc).is_on(handle_of(&doc, "First")),
        "and nothing changed: a locked group's state is not the user's to set"
    );
    // The unlocked one still moves, so the refusal is about /Locked and not about the
    // toggle being broken.
    let second = id_of(&doc, "Second");
    assert!(panel.set(&doc, second, false));
    assert!(!OptionalContentState::read(&doc).is_on(handle_of(&doc, "Second")));
}

#[test]
fn a_radio_set_turns_its_others_off_but_never_on() {
    let doc = document("/D << /BaseState /ON /Order [5 0 R 6 0 R] /RBGroups [[5 0 R 6 0 R]] >>");
    let panel = panel(&doc);
    let first = id_of(&doc, "First");

    // Turning one ON turns the other OFF.
    assert!(panel.set(&doc, first, true));
    let state = OptionalContentState::read(&doc);
    assert!(state.is_on(handle_of(&doc, "First")));
    assert!(!state.is_on(handle_of(&doc, "Second")), "at most one ON in a radio set");

    // "However, turning a group from ON to OFF does not force any other group to be
    // turned ON." Both off is a legal state and the asymmetry is the whole rule.
    assert!(panel.set(&doc, first, false));
    let state = OptionalContentState::read(&doc);
    assert!(!state.is_on(handle_of(&doc, "First")));
    assert!(!state.is_on(handle_of(&doc, "Second")), "the other stays off");
}

#[test]
fn a_viewer_toggle_is_not_a_document_change() {
    let doc = document("/D << /BaseState /ON /Order [5 0 R] >>");
    let first = id_of(&doc, "First");
    panel(&doc).set(&doc, first, false);
    assert!(!OptionalContentState::read(&doc).is_on(handle_of(&doc, "First")));

    // The override lives beside the document, not in it, so forgetting it returns to
    // what the configuration says without anything having been rewritten.
    doc.reset_layer_visibility();
    assert!(OptionalContentState::read(&doc).is_on(handle_of(&doc, "First")));
}
