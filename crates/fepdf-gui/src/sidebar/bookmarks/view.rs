//! Drawing the bookmark panel.

use super::BookmarkPanel;
use super::edit::{self, Move, Path};
use crate::app::icons::{glyph, icon_action};
use crate::app::theme::{size, space};
use fepdf::OutlineNode;

/// What the reader asked the window to do with the draft.
pub enum Asked {
    /// Nothing this frame.
    Nothing,
    /// Write the draft to the document as one operation.
    Write,
    /// Show this page, because a bookmark was chosen.
    GoTo(usize),
}

/// The whole panel: the tree, the fields for the chosen bookmark, and the two buttons.
pub fn show(
    panel: &mut BookmarkPanel,
    ui: &mut egui::Ui,
    pages: usize,
    tr: &dyn Fn(&str) -> String,
) -> Asked {
    if pages == 0 {
        ui.label(tr("tools_no_document"));
        return Asked::Nothing;
    }
    let mut asked = Asked::Nothing;
    summary(panel, ui, tr);
    ui.add_space(space::GROUP);

    if panel.draft.items.is_empty() {
        ui.label(tr("marks_none"));
    }
    let mut clicked = None;
    let height = (ui.clip_rect().height() * size::TREE_SHARE).max(size::TREE_MIN_H);
    egui::ScrollArea::vertical().id_salt("marks_tree").max_height(height).show(ui, |ui| {
        rows(&panel.draft.items.clone(), &mut Vec::new(), panel, ui, &mut clicked);
    });
    if let Some(path) = clicked {
        if let Some(node) = edit::at(&panel.draft.items, &path) {
            asked = Asked::GoTo(node.destination_page);
        }
        panel.chosen = Some(path);
    }

    ui.add_space(space::GROUP);
    ui.separator();
    ui.add_space(space::GROUP);
    fields(panel, ui, pages, tr);
    ui.add_space(space::GROUP);
    if commit_row(panel, ui, tr) {
        asked = Asked::Write;
    }
    asked
}

/// How many bookmarks there are, and whether any of them points nowhere.
fn summary(panel: &BookmarkPanel, ui: &mut egui::Ui, tr: &dyn Fn(&str) -> String) {
    ui.label(format!("{} {}", panel.report.items, tr("marks_count")));
    if panel.report.placeless > 0 {
        ui.colored_label(
            crate::app::theme::colors::note::WARN,
            format!("{} {}", panel.report.placeless, tr("marks_placeless")),
        );
    }
    if panel.report.looped {
        ui.colored_label(crate::app::theme::colors::note::WARN, tr("marks_looped"));
    }
}

/// One level of the tree, and the levels under the bookmarks that are not folded.
fn rows(
    items: &[OutlineNode],
    here: &mut Path,
    panel: &mut BookmarkPanel,
    ui: &mut egui::Ui,
    clicked: &mut Option<Path>,
) {
    for (index, node) in items.iter().enumerate() {
        here.push(index);
        let folded = panel.folded.contains(here);
        ui.horizontal(|ui| {
            fold_control(node, here, folded, panel, ui);
            let chosen = panel.chosen.as_ref() == Some(here);
            let label = if node.title.is_empty() { "—" } else { node.title.as_str() };
            if ui.selectable_label(chosen, label).clicked() {
                *clicked = Some(here.clone());
            }
        });
        if !folded && !node.children.is_empty() {
            ui.indent(index, |ui| rows(&node.children, here, panel, ui, clicked));
        }
        here.pop();
    }
}

/// The chevron that hides a bookmark's children, or the space where one would be.
fn fold_control(
    node: &OutlineNode,
    here: &Path,
    folded: bool,
    panel: &mut BookmarkPanel,
    ui: &mut egui::Ui,
) {
    if node.children.is_empty() {
        // The same width as the button, so every title at a level starts in one column.
        ui.add_space(crate::app::theme::size::ICON);
        return;
    }
    let face = if folded { glyph::TREE_SHUT } else { glyph::TREE_OPEN };
    let name = format!("{} ({})", node.title, node.children.len());
    if icon_action(ui, face, false, true, &name).clicked() {
        if folded {
            panel.folded.remove(here);
        } else {
            panel.folded.insert(here.clone());
        }
    }
}

/// The title and page of the chosen bookmark, and the buttons that move it.
fn fields(panel: &mut BookmarkPanel, ui: &mut egui::Ui, pages: usize, tr: &dyn Fn(&str) -> String) {
    let Some(path) = panel.chosen.clone() else {
        ui.label(tr("marks_pick"));
        ui.add_space(space::ITEM);
        add_row(panel, ui, tr);
        return;
    };
    let Some(node) = edit::at_mut(&mut panel.draft.items, &path) else {
        // The draft changed under the selection — a delete, or an undo of one.
        panel.chosen = None;
        return;
    };
    ui.horizontal(|ui| {
        ui.label(tr("marks_title_field"));
        ui.add(egui::TextEdit::singleline(&mut node.title).desired_width(f32::INFINITY));
    });
    ui.horizontal(|ui| {
        ui.label(tr("tools_page"));
        // One-based for the reader, zero-based in the file (12.3.2.2). The clamp is the
        // field's, not the engine's: a bookmark pointing past the last page is written
        // without a `/Dest` and comes back pointing at page 1.
        let mut shown = node.destination_page + 1;
        if ui.add(egui::DragValue::new(&mut shown).range(1..=pages)).changed() {
            node.destination_page = shown - 1;
        }
    });
    ui.add_space(space::ITEM);
    move_row(panel, ui, &path, tr);
    ui.add_space(space::ITEM);
    add_row(panel, ui, tr);
}

/// Up, down, in, out, and delete.
fn move_row(
    panel: &mut BookmarkPanel,
    ui: &mut egui::Ui,
    path: &Path,
    tr: &dyn Fn(&str) -> String,
) {
    let moves = [
        (glyph::MARK_UP, Move::Up, "marks_up"),
        (glyph::MARK_DOWN, Move::Down, "marks_down"),
        (glyph::MARK_IN, Move::In, "marks_in"),
        (glyph::MARK_OUT, Move::Out, "marks_out"),
    ];
    ui.horizontal(|ui| {
        for (face, how, key) in moves {
            if icon_action(ui, face, false, true, &tr(key)).clicked() {
                // The panel follows the bookmark rather than the position: after a move,
                // the path that was selected names a different bookmark.
                panel.chosen = edit::shift(&mut panel.draft.items, path, how);
                panel.folded.clear();
            }
        }
        ui.separator();
        if icon_action(ui, glyph::MARK_DELETE, false, true, &tr("marks_delete")).clicked() {
            edit::take(&mut panel.draft.items, path);
            panel.chosen = None;
            panel.folded.clear();
        }
    });
}

/// Adding a bookmark, either beside the chosen one or at the end.
fn add_row(panel: &mut BookmarkPanel, ui: &mut egui::Ui, tr: &dyn Fn(&str) -> String) {
    if !icon_action(ui, glyph::MARK_ADD, false, true, &tr("marks_add")).clicked() {
        return;
    }
    // It takes the chosen bookmark's page, because a reader adding one is almost always
    // adding it near where they are — and a page is easier to change than to find.
    let page = panel
        .chosen
        .as_ref()
        .and_then(|p| edit::at(&panel.draft.items, p))
        .map_or(0, |n| n.destination_page);
    let fresh = OutlineNode { title: tr("marks_new"), destination_page: page, children: vec![] };
    let landing = match panel.chosen.as_ref() {
        Some(path) => {
            let mut next = path.clone();
            if let Some(last) = next.last_mut() {
                *last += 1;
            }
            next
        }
        None => vec![panel.draft.items.len()],
    };
    if edit::put(&mut panel.draft.items, &landing, fresh) {
        panel.chosen = Some(landing);
    }
}

/// Write the draft, or throw it away. Both are disabled while it matches the file.
fn commit_row(panel: &mut BookmarkPanel, ui: &mut egui::Ui, tr: &dyn Fn(&str) -> String) -> bool {
    let edited = panel.edited();
    let mut write = false;
    ui.horizontal(|ui| {
        if icon_action(ui, glyph::MARK_WRITE, false, edited, &tr("tools_apply")).clicked() {
            write = true;
        }
        if icon_action(ui, glyph::MARK_REVERT, false, edited, &tr("marks_revert")).clicked() {
            panel.draft = panel.filed.clone();
            panel.chosen = None;
            panel.folded.clear();
        }
        if edited {
            ui.label(tr("marks_unwritten"));
        }
    });
    write
}
