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

use crate::app::icons::{glyph, icon_action};
use fepdf::FormField;

/// What the reader asked of the form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// Put `value` in the field named `field`, by its fully qualified name (12.7.4.2).
    Value {
        /// The field.
        field: String,
        /// What to put in it.
        value: fepdf::FormValue,
    },
    /// Recalculate the calculated fields in this order (`/CO`).
    CalculationOrder(Vec<String>),
}

/// What the drawer is holding between frames.
#[derive(Default)]
pub struct FormPanel {
    /// What is being typed, by field name. A draft outlives a repaint and not a document.
    drafts: std::collections::BTreeMap<String, String>,
    /// Which document the drafts belong to, so that opening another one forgets them.
    for_document: Option<String>,
    /// The calculation order being arranged, and the form's order it was taken from — so
    /// that a form which changes under it (the order was applied) starts a fresh one.
    order: Option<(Vec<String>, Vec<String>)>,
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
            self.order = None;
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
        self.calculation(ui, form, words).or(asked)
    }

    /// The order the calculated fields are recalculated in, and a way to change it.
    ///
    /// **Drawn only for a form that calculates.** For one that does, every field that
    /// calculates is listed once, because that is what `/CO` has to hold; a file whose
    /// `/CO` left one out has it added at the end, and the drawer says so.
    fn calculation(
        &mut self,
        ui: &mut egui::Ui,
        form: &fepdf::FormFields,
        words: &dyn Fn(&str) -> String,
    ) -> Option<Asked> {
        let starting = draft_order(form);
        if starting.is_empty() {
            return None;
        }
        if self.order.as_ref().is_none_or(|(_, from)| *from != form.calculation_order) {
            self.order = Some((starting.clone(), form.calculation_order.clone()));
        }
        let (order, _) = self.order.as_mut()?;
        ui.separator();
        ui.heading(words("form_calc_heading"));
        if starting != form.calculation_order {
            ui.label(
                egui::RichText::new(words("form_calc_completed"))
                    .size(crate::app::theme::text::SMALL)
                    .weak(),
            );
        }
        let mut moved = None;
        for (at, name) in order.iter().enumerate() {
            ui.horizontal(|ui| {
                if icon_action(ui, glyph::MARK_UP, false, at > 0, &words("form_calc_up")).clicked()
                {
                    moved = Some((at, at - 1));
                }
                let last = at + 1 < order.len();
                if icon_action(ui, glyph::MARK_DOWN, false, last, &words("form_calc_down"))
                    .clicked()
                {
                    moved = Some((at, at + 1));
                }
                ui.label(format!("{}. {name}", at + 1));
            });
        }
        if let Some((from, to)) = moved {
            order.swap(from, to);
        }
        let changed = *order != form.calculation_order;
        ui.add_enabled(changed, egui::Button::new(words("form_apply")))
            .clicked()
            .then(|| Asked::CalculationOrder(order.clone()))
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
                .map(|on| Asked::Value { field: name, value: fepdf::FormValue::Boolean(on) }),
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
        written.then(|| Asked::Value {
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
                chosen = Some(Asked::Value {
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

/// The calculation order to start arranging from: the form's own, keeping only fields
/// that calculate, then any field that calculates and was left out, in form order.
fn draft_order(form: &fepdf::FormFields) -> Vec<String> {
    let mut calculating: Vec<String> = Vec::new();
    for field in form.terminal.iter().filter(|field| field.calculates) {
        if let Some(name) = &field.qualified_name
            && !calculating.contains(name)
        {
            calculating.push(name.clone());
        }
    }
    let mut order: Vec<String> =
        form.calculation_order.iter().filter(|name| calculating.contains(name)).cloned().collect();
    order.dedup();
    for name in calculating {
        if !order.contains(&name) {
            order.push(name);
        }
    }
    order
}

#[cfg(test)]
mod calculation_order {
    use super::draft_order;

    fn field(name: &str, calculates: bool) -> fepdf::FormField {
        fepdf::FormField {
            name: Some(name.to_string()),
            qualified_name: Some(name.to_string()),
            field_type: Some("Tx".to_string()),
            flags: None,
            value: None,
            tooltip: None,
            has_default_appearance: false,
            is_widget: true,
            options: Vec::new(),
            selected_indices: Vec::new(),
            top_index: None,
            calculates,
        }
    }

    fn form(fields: &[(&str, bool)], order: &[&str]) -> fepdf::FormFields {
        fepdf::FormFields {
            declared: true,
            terminal: fields.iter().map(|(name, on)| field(name, *on)).collect(),
            calculation_order: order.iter().map(ToString::to_string).collect(),
            ..fepdf::FormFields::default()
        }
    }

    #[test]
    fn the_forms_own_order_is_where_arranging_starts() {
        let form = form(&[("a", true), ("b", true)], &["b", "a"]);
        assert_eq!(draft_order(&form), ["b", "a"]);
    }

    /// **A field the file's `/CO` left out is added**, at the end, since the engine will
    /// refuse an order that does not name every field that calculates.
    #[test]
    fn a_calculating_field_left_out_of_co_is_added() {
        let form = form(&[("a", true), ("b", true), ("c", false)], &["b"]);
        assert_eq!(draft_order(&form), ["b", "a"]);
    }

    /// And a name in `/CO` for a field that calculates nothing is dropped, for the same
    /// reason.
    #[test]
    fn a_field_that_does_not_calculate_is_dropped() {
        let form = form(&[("a", true), ("c", false)], &["c", "a"]);
        assert_eq!(draft_order(&form), ["a"]);
    }

    #[test]
    fn a_form_that_calculates_nothing_has_no_order_to_arrange() {
        assert!(draft_order(&form(&[("a", false)], &[])).is_empty());
    }
}
