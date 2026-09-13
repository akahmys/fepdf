//! The icon vocabulary, and the one constructor that draws it.
//!
//! **Every codepoint here is checked against `assets/lucide.ttf`.** Two were not, and
//! neither drew: `U+E8E8` is past the end of the font (its last glyph is `U+E6FD`), and
//! `U+E0FF` was taken from `Ubuntu-Light` — see [`super::theme::icon_family`]. Two more
//! drew the wrong thing: the caliper was `shield-ban` and single-page was `layout`.
//!
//! ```bash
//! python3 scripts/audit/icon_glyphs.py   # UI-1
//! ```

use super::theme::{colors, icon_family, size};

/// The glyphs, by what they mean here rather than by what Lucide calls them.
///
/// The name Lucide gives each one is in the comment, because that is what a search of
/// the font's `post` table returns and how the next one will be found.
pub mod glyph {
    /// `file-input` — bring a document in.
    pub const OPEN: &str = "\u{e0c5}";
    /// `file-output` — write a document out.
    pub const EXPORT: &str = "\u{e0c8}";
    /// `file-text` — what this document declares.
    pub const INFO: &str = "\u{e0cc}";
    /// `scan-eye` — what it does when opened. Was `U+E8E8`, which the font does not have.
    pub const SURVEY: &str = "\u{e536}";
    /// `folder-tree` — the structure tree.
    pub const STRUCTURE: &str = "\u{e33c}";
    /// `eraser` — redaction.
    pub const REDACT: &str = "\u{e28f}";
    /// `ruler` — the caliper. Was `shield-ban`.
    pub const CALIPER: &str = "\u{e14b}";
    /// `file-sliders` — the operations that act on the document as a whole.
    pub const TOOLS: &str = "\u{e5a0}";
    /// `settings`.
    pub const SETTINGS: &str = "\u{e154}";
    /// `circle-help`.
    pub const ABOUT: &str = "\u{e082}";
    /// `command` — the palette.
    pub const PALETTE: &str = "\u{e09a}";
    /// `search` — a field that filters what is below it.
    pub const SEARCH: &str = "\u{e151}";

    /// `undo` — take back the last operation.
    pub const UNDO: &str = "\u{e19b}";
    /// `redo` — put it back.
    pub const REDO: &str = "\u{e143}";
    /// `rotate-cw` — a quarter turn clockwise.
    pub const ROTATE: &str = "\u{e149}";
    /// `file` — one page at a time. Was `layout`, which is a panel arrangement.
    pub const PAGE_SINGLE: &str = "\u{e0c0}";
    /// `rows-2` — pages stacked, scrolling.
    pub const PAGE_CONTINUOUS: &str = "\u{e439}";
    /// `book-open` — two pages facing.
    pub const PAGE_SPREAD: &str = "\u{e05f}";
    /// `loader` — a page that has not finished drawing.
    pub const LOADING: &str = "\u{e109}";
    /// `pin` — the controls are held open.
    pub const PIN: &str = "\u{e259}";
    /// `pin-off` — they come and go with the pointer.
    pub const PIN_OFF: &str = "\u{e2b6}";
    /// `zoom-in`.
    pub const ZOOM_IN: &str = "\u{e1b6}";
    /// `zoom-out`.
    pub const ZOOM_OUT: &str = "\u{e1b7}";
    /// `chevrons-left` — the first page.
    pub const PAGE_FIRST: &str = "\u{e072}";
    /// `chevron-left` — the page before.
    pub const PAGE_PREV: &str = "\u{e06e}";
    /// `chevron-right` — the page after.
    pub const PAGE_NEXT: &str = "\u{e06f}";
    /// `chevrons-right` — the last page.
    pub const PAGE_LAST: &str = "\u{e073}";
    /// `x` — close. Lucide maps this one to ASCII `x` rather than to a private-use
    /// codepoint, which is harmless inside a family that holds nothing else.
    pub const CLOSE: &str = "\u{0078}";
}

/// An icon button, its tooltip, and the name a screen reader is given for it.
///
/// **egui takes a widget's text for its accessible name**, and the text of every button
/// in this window is a private-use codepoint — so twenty-six controls announced
/// themselves as `U+E0CC`. That is the one this product has least excuse for: it audits
/// documents against the Matterhorn protocol, and principle P4 says the checks it makes
/// of a file apply to its own window.
///
/// The name is the tooltip. They were always the same sentence; only one of them was
/// reaching anybody.
pub fn icon_action(
    ui: &mut egui::Ui,
    glyph: &'static str,
    is_active: bool,
    enabled: bool,
    name: &str,
) -> egui::Response {
    let button = if enabled { icon_button(glyph, is_active) } else { icon_button_disabled(glyph) };
    // **`add_enabled`, not `add`.** A control drawn as unavailable used to be fully
    // clickable: `enabled` picked the colour and nothing else, so every caller with a
    // reason to disable one had to repeat that reason at the call site — `undo` and `redo`
    // each carried `&& self.can_undo` behind `.clicked()`, and a caller that forgot would
    // have had a grey button that worked.
    named(ui.add_enabled(enabled, button), enabled, name)
}

/// Gives `response` a name — to a screen reader and to the pointer alike.
///
/// **A control whose text is not its name needs this too.** The zoom control's text is
/// `100%` and the binding control's is one character; both are labels for something, and
/// neither says what the control does.
pub fn named(response: egui::Response, enabled: bool, name: &str) -> egui::Response {
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
    response.on_hover_text(name)
}

/// An icon button: [`size::ICON`] square, the glyph drawn from the icon family.
///
/// **One constructor, because there was more than one.** A rail button and a status-bar
/// button were built by different functions with different resting states — one painted
/// a background only on hover, the other always — so the same row of controls read as two
/// kinds of thing. The `VectorIcon` pair this replaced drew two Lucide glyphs by hand in
/// `egui::Painter` calls, which is a third way of making the same object.
pub fn icon_button(glyph: &'static str, is_active: bool) -> egui::Button<'static> {
    let colour = if is_active { colors::rust::ACCENT } else { colors::steel::MUTED };
    let text = egui::RichText::new(glyph).size(size::GLYPH).family(icon_family()).color(colour);
    egui::Button::new(text)
        .min_size(egui::vec2(size::ICON, size::ICON))
        .corner_radius(super::theme::radius::CONTROL)
        .selected(is_active)
}

/// The same button, drawn as present but unavailable.
///
/// **[`colors::steel::EDGE`] rather than the lightest slate available.** The disabled
/// export button measured 1.23:1 against the panel, which is not a greyed control but an
/// absent one — and an entry point the reader cannot see does not tell them the feature
/// exists (principle P3). This clears 3:1 on every surface a control can sit on.
pub fn icon_button_disabled(glyph: &'static str) -> egui::Button<'static> {
    let text = egui::RichText::new(glyph)
        .size(size::GLYPH)
        .family(icon_family())
        .color(colors::steel::EDGE);
    egui::Button::new(text)
        .min_size(egui::vec2(size::ICON, size::ICON))
        .corner_radius(super::theme::radius::CONTROL)
}
