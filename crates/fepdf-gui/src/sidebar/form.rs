//! The fields of the document's form, and what a reader puts in them.
//!
//! **The engine could set a field value and nothing in this window asked it to.** A form
//! could be read, audited and not touched
//! ([ADR-0087](../../../../docs/adr/0087-a-form-field-is-created-here-not-only-filled.md)).
//!
//! What is drawn for a field follows its `/FT`, and a type this engine cannot write is
//! shown with what it holds and no way to change it — rather than a text box that writes
//! the wrong shape into the file. A signature field is the case that matters: it is signed
//! rather than filled, and offering to type into one would be offering to forge it.

use fepdf::FormField;

/// A field's value as the reader has left it, and what the caller does with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    /// The fully qualified name of the field (12.7.4.2), which is what an edit names.
    pub field: String,
    /// What to put in it.
    pub value: fepdf::FormValue,
}

/// What the drawer is holding between frames.
#[derive(Default)]
pub struct FormPanel {
    /// What is being typed, by field name. A draft outlives a repaint and not a document.
    drafts: std::collections::BTreeMap<String, String>,
    /// Which document the drafts belong to, so that opening another one forgets them.
    for_document: Option<String>,
}

impl FormPanel {
    /// Draws the drawer, and answers what the reader asked to write.
    pub fn show(
        // RR-15 Limit: GUI - Sequential egui declarations for the form drawer
        &mut self,
        ui: &mut egui::Ui,
        form: &fepdf::FormFields,
        document: Option<&str>,
        words: &dyn Fn(&str) -> String,
    ) -> Option<Asked> {
        if self.for_document.as_deref() != document {
            self.drafts.clear();
            self.for_document = document.map(ToString::to_string);
        }
        if !form.declared {
            ui.label(words("form_none"));
            return None;
        }
        if form.terminal.is_empty() {
            ui.label(words("form_no_fields"));
            return None;
        }

        let mut asked = None;
        for field in &form.terminal {
            if let Some(answer) = self.field(ui, field, words) {
                asked = Some(answer);
            }
            ui.add_space(crate::app::theme::space::ITEM);
        }
        asked
    }

    /// One field: what it is called, what it holds, and what can be done to it.
    fn field(
        &mut self,
        ui: &mut egui::Ui,
        field: &FormField,
        words: &dyn Fn(&str) -> String,
    ) -> Option<Asked> {
        let name = field.qualified_name.clone()?;
        ui.label(if name.is_empty() { words("form_unnamed") } else { name.clone() });
        match field.field_type.as_deref() {
            Some("Tx") => self.text_field(ui, field, &name, words),
            Some("Ch") => self.choice_field(ui, field, &name, words),
            // **When the tick changes, not when a click lands.** A checkbox asked on
            // every release would write the field again on every click anywhere in the
            // drawer, which is a document changed by looking at it.
            Some("Btn") => Self::ticked(ui, field)
                .map(|on| Asked { field: name, value: fepdf::FormValue::Boolean(on) }),
            Some("Sig") => {
                ui.label(words("form_signature_not_filled"));
                None
            }
            _ => {
                ui.label(words("form_unknown_type"));
                None
            }
        }
    }

    /// A text field: a box holding what it holds, and a button that writes it in.
    fn text_field(
        &mut self,
        ui: &mut egui::Ui,
        field: &FormField,
        name: &str,
        words: &dyn Fn(&str) -> String,
    ) -> Option<Asked> {
        let draft = self
            .drafts
            .entry(name.to_string())
            .or_insert_with(|| field.value.clone().unwrap_or_default());
        let typed = ui.text_edit_singleline(draft);
        let written = ui.button(words("form_apply")).clicked()
            || (typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        written.then(|| Asked {
            field: name.to_string(),
            value: fepdf::FormValue::Text(draft.clone()),
        })
    }

    /// A choice field: its own options, because a list that let a reader type anything
    /// would let them write a value `/Opt` does not offer (12.7.4.4).
    fn choice_field(
        &mut self,
        ui: &mut egui::Ui,
        field: &FormField,
        name: &str,
        words: &dyn Fn(&str) -> String,
    ) -> Option<Asked> {
        if field.options.is_empty() {
            return self.text_field(ui, field, name, words);
        }
        let mut chosen = None;
        for option in &field.options {
            // `/Opt` may give one string or a pair; the display one is what a reader
            // reads and the export one is what goes in the file (Table 231).
            let label = if option.display_value.is_empty() {
                option.export_value.clone()
            } else {
                option.display_value.clone()
            };
            let picked = field.value.as_deref() == Some(option.export_value.as_str());
            if ui.selectable_label(picked, label).clicked() {
                chosen = Some(Asked {
                    field: name.to_string(),
                    value: fepdf::FormValue::Choice(option.export_value.clone()),
                });
            }
        }
        chosen
    }

    /// A button: whether it is on, drawn as a tick — and the answer only when it turned.
    ///
    /// `/Off` is the name 12.7.5.2.3 gives the state that is not on; anything else is a
    /// state the widget has an appearance for.
    fn ticked(ui: &mut egui::Ui, field: &FormField) -> Option<bool> {
        let mut on = field.value.as_deref().is_some_and(|value| value != "Off");
        ui.checkbox(&mut on, "").changed().then_some(on)
    }
}
