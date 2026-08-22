//! The layer list an interactive PDF processor must have (ISO 32000-2, 6.3.2.3).
//!
//! 6.3.1 makes anything that interacts with a person while processing a file an
//! interactive processor, and 6.3.2.3 then requires it to "support all of the interactive
//! aspects of optional content (8.11)". This engine could already read a configuration
//! and hide what it turned off; what it could not do was let a reader change their mind.
//!
//! Three of 8.11.4.3's rules are about this panel rather than about what is drawn, and the
//! engine enforces all three — the UI shows them rather than implementing them:
//!
//! * `/Order` decides **which** groups appear, not merely their sequence. A group left
//!   out of it "shall not be presented", and `/Order` defaults to an empty array, so a
//!   document with layers and no `/Order` correctly shows none.
//! * `/Locked` groups are shown greyed and cannot be clicked.
//! * `/RBGroups` gives radio behaviour, applied by the engine when the toggle is made.
//!
//! The checkbox is drawn from the state the engine reports, never from what the UI last
//! sent, so a toggle the engine refuses simply does not move.

use crate::locale::LocaleManager;
use crate::worker::WorkerRequest;
use std::sync::mpsc::Sender;

/// Draws the layer panel.
// RR-15 Limit: GUI - egui layout tree for a nested, checkable list
pub fn show_layers(
    ui: &mut egui::Ui,
    layers: &[fepdf::LayerRow],
    tx_worker: &Sender<WorkerRequest>,
    locale_mgr: &LocaleManager,
    active_lang: &str,
) {
    ui.heading(locale_mgr.tr(active_lang, "tab_layers"));
    ui.separator();

    if layers.is_empty() {
        ui.add_space(4.0);
        ui.label(locale_mgr.tr(active_lang, "layers_none"));
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        show_rows(ui, layers, tx_worker, locale_mgr, active_lang);
    });
}

fn show_rows(
    ui: &mut egui::Ui,
    rows: &[fepdf::LayerRow],
    tx_worker: &Sender<WorkerRequest>,
    locale_mgr: &LocaleManager,
    active_lang: &str,
) {
    for row in rows {
        match row {
            // "a text string to be used as a **non-selectable** label" — so it is a
            // label and not a disabled checkbox, which would imply a state it has none of.
            fepdf::LayerRow::Label(text) => {
                ui.add_space(2.0);
                ui.label(egui::RichText::new(text).strong());
            }
            fepdf::LayerRow::Group { id, name, on, locked } => {
                show_group(ui, *id, name, *on, *locked, tx_worker, locale_mgr, active_lang);
            }
            fepdf::LayerRow::Nested(inner) => {
                ui.indent(inner.as_ptr(), |ui| {
                    show_rows(ui, inner, tx_worker, locale_mgr, active_lang);
                });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn show_group(
    ui: &mut egui::Ui,
    id: fepdf::LayerId,
    name: &str,
    on: bool,
    locked: bool,
    tx_worker: &Sender<WorkerRequest>,
    locale_mgr: &LocaleManager,
    active_lang: &str,
) {
    let mut checked = on;
    // `add_enabled` and not a hidden row: a locked layer is still *presented*, and a
    // reader who cannot see that a layer exists cannot tell it from one that is off.
    let response = ui.add_enabled(!locked, egui::Checkbox::new(&mut checked, name));
    if locked {
        response.on_hover_text(locale_mgr.tr(active_lang, "layers_locked"));
        return;
    }
    if response.changed() {
        // Sent, not applied here. The engine owns `/RBGroups` and `/Locked`, and the
        // next `LayersChanged` redraws every checkbox from what it actually did — a
        // radio set turns its siblings off without this panel knowing they exist.
        let _ = tx_worker.send(WorkerRequest::SetLayerVisible { layer: id, on: checked });
    }
}
