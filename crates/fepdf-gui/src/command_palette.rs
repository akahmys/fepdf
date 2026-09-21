// RR-15 Limit: GUI - Command Palette window declaration and dispatch
pub struct CommandPalette;

/// What the palette can do, and what each one is called.
///
/// **An enum, not nine strings.** The action was matched on its own English name —
/// `"Load PDF"`, `"Redaction Studio"` — which made the dispatch string-typed, gave the
/// `match` a `_ => {}` arm that could silently swallow a typo, and put nine pieces of
/// English in the source that read exactly like the prose UI-5 looks for. The names the
/// reader sees were always locale keys; only the dispatch was not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Open a document.
    Load,
    /// Put the view back where it started.
    ResetView,
    /// The redaction brush.
    RedactBrush,
    /// The tagging brush.
    TagBrush,
    /// The caliper.
    Caliper,
    /// The export wizard.
    Export,
    /// The reading-order overlay.
    ReadingOrder,
    /// The redaction drawer.
    RedactionStudio,
    /// The document-tools drawer.
    Tools,
    /// The runs of the page, and editing one.
    EditText,
    /// The document's form, and filling it.
    FillForm,
    /// A rectangle of the page, copied as a picture.
    Snapshot,
}

impl Command {
    /// Every command, in the order the palette lists them.
    pub const ALL: [Self; 12] = [
        Self::Load,
        Self::ResetView,
        Self::RedactBrush,
        Self::TagBrush,
        Self::Caliper,
        Self::Export,
        Self::ReadingOrder,
        Self::RedactionStudio,
        Self::Tools,
        Self::EditText,
        Self::FillForm,
        Self::Snapshot,
    ];

    /// The locale keys naming it and describing it.
    ///
    /// No wildcard arm, so a tenth command does not compile until it has both (Rule 5).
    pub const fn keys(self) -> (&'static str, &'static str) {
        match self {
            Self::Load => ("cmd_load_pdf", "cmd_load_pdf_desc"),
            Self::ResetView => ("cmd_reset_view", "cmd_reset_view_desc"),
            Self::RedactBrush => ("cmd_redact_brush", "cmd_redact_brush_desc"),
            Self::TagBrush => ("cmd_tagging_brush", "cmd_tagging_brush_desc"),
            Self::Caliper => ("cmd_caliper_brush", "cmd_caliper_brush_desc"),
            Self::Export => ("cmd_export_pdf", "cmd_export_pdf_desc"),
            Self::ReadingOrder => ("cmd_reading_order", "cmd_reading_order_desc"),
            Self::RedactionStudio => ("cmd_redaction_studio", "cmd_redaction_studio_desc"),
            Self::Tools => ("cmd_document_tools", "cmd_document_tools_desc"),
            Self::EditText => ("cmd_edit_text", "cmd_edit_text_desc"),
            Self::FillForm => ("cmd_fill_form", "cmd_fill_form_desc"),
            Self::Snapshot => ("cmd_snapshot", "cmd_snapshot_desc"),
        }
    }

    /// Does it, which is the only thing this type is for.
    fn run(self, app: &mut crate::app::FepdfApp, ctx: &egui::Context) {
        match self {
            Self::Load => {
                if let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file() {
                    app.open_file(p, ctx);
                }
            }
            Self::ResetView => app.reset_view(),
            Self::RedactBrush => {
                app.redaction_manager.is_active = !app.redaction_manager.is_active;
                if app.redaction_manager.is_active {
                    app.selection_manager.clear();
                    app.selection_manager.is_tagging_brush_active = false;
                    app.caliper_tool.is_active = false;
                }
            }
            Self::TagBrush => {
                app.selection_manager.is_tagging_brush_active =
                    !app.selection_manager.is_tagging_brush_active;
                if app.selection_manager.is_tagging_brush_active {
                    app.selection_manager.clear();
                    app.redaction_manager.is_active = false;
                    app.caliper_tool.is_active = false;
                }
            }
            Self::Export => app.open_export_wizard(),
            Self::ReadingOrder => app.show_reading_order = !app.show_reading_order,
            Self::Caliper
            | Self::RedactionStudio
            | Self::Tools
            | Self::EditText
            | Self::FillForm
            | Self::Snapshot => self.open_drawer(app),
        }
    }

    /// Opens the drawer this command names, or closes it if it was already showing.
    ///
    /// **Six arms of `run` were these three lines with a different name in them.** The
    /// tool that belongs to a drawer is turned by `show_drawer`, so this does not say it
    /// again — that was the second of two places for one rule.
    fn open_drawer(self, app: &mut crate::app::FepdfApp) {
        let Some(drawer) = self.drawer() else { return };
        let showing = app.active_drawer == drawer;
        app.show_drawer(Self::toggled(showing, drawer));
    }

    /// The drawer a command opens, when opening one is what it does.
    ///
    /// No wildcard arm, so a thirteenth command does not compile until it has said
    /// whether it opens one (Rule 5).
    const fn drawer(self) -> Option<crate::sidebar::ActiveDrawer> {
        use crate::sidebar::ActiveDrawer;
        match self {
            Self::Caliper => Some(ActiveDrawer::Caliper),
            Self::RedactionStudio => Some(ActiveDrawer::Redaction),
            Self::Tools => Some(ActiveDrawer::Tools),
            Self::EditText => Some(ActiveDrawer::TextRuns),
            Self::FillForm => Some(ActiveDrawer::Form),
            Self::Snapshot => Some(ActiveDrawer::Snapshot),
            Self::Load
            | Self::ResetView
            | Self::RedactBrush
            | Self::TagBrush
            | Self::Export
            | Self::ReadingOrder => None,
        }
    }

    /// The act a command cannot be run without, if it belongs to one view.
    ///
    /// **The palette is a shortcut, not a second door** (UI-4), so it offers what the
    /// window offers and no more: the brushes and the caliper draw on a page, and the
    /// redaction studio opens onto a brush that does. They stay listed where they cannot
    /// run, greyed, because a reader searching for one wants to be told it exists and
    /// where — not to find the list a word shorter.
    const fn needs(self) -> Option<crate::view::Act> {
        match self {
            Self::RedactBrush
            | Self::Caliper
            | Self::RedactionStudio
            | Self::EditText
            | Self::Snapshot => Some(crate::view::Act::DrawOnPage),
            Self::TagBrush => Some(crate::view::Act::SelectText),
            Self::Load
            | Self::ResetView
            | Self::Export
            | Self::ReadingOrder
            | Self::Tools
            // A form is filled in a panel and its fields are wherever they are.
            | Self::FillForm => None,
        }
    }

    /// The drawer a toggle lands on: closed if it was already showing.
    const fn toggled(
        showing: bool,
        drawer: crate::sidebar::ActiveDrawer,
    ) -> crate::sidebar::ActiveDrawer {
        if showing { crate::sidebar::ActiveDrawer::None } else { drawer }
    }
}

impl CommandPalette {
    pub fn show(app: &mut crate::app::FepdfApp, ctx: &egui::Context) {
        // RR-15 Limit: GUI - Command Palette window declaration and dispatch
        let mut show_palette = app.show_command_palette;
        if !show_palette {
            return;
        }

        let mut close_palette = false;
        let title = app.locale_mgr.tr(&app.active_language, "cmd_palette_title");
        let mut chosen = None;
        egui::Window::new(title)
            .open(&mut show_palette)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 100.0))
            .default_width(crate::app::theme::size::TABLE_W)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(crate::app::icons::glyph::SEARCH)
                            .family(crate::app::theme::icon_family())
                            .color(crate::app::theme::colors::steel::MUTED),
                    );
                    let text_resp = ui.text_edit_singleline(&mut app.command_palette_search);
                    text_resp.request_focus();
                });
                ui.separator();

                let query = app.command_palette_search.to_lowercase();
                for command in Command::ALL {
                    let (name_key, desc_key) = command.keys();
                    let name = app.locale_mgr.tr(&app.active_language, name_key);
                    let desc = app.locale_mgr.tr(&app.active_language, desc_key);
                    let matches = query.is_empty()
                        || name.to_lowercase().contains(&query)
                        || desc.to_lowercase().contains(&query);
                    let here = command.needs().is_none_or(|act| app.view.does(act));
                    if matches
                        && ui
                            .add_enabled(
                                here,
                                egui::Button::selectable(false, format!("{name} — {desc}")),
                            )
                            .clicked()
                    {
                        chosen = Some(command);
                        close_palette = true;
                    }
                }
            });

        // **Outside the window's closure**, which borrows `app` for as long as it is open.
        if let Some(command) = chosen {
            command.run(app, ctx);
        }

        if close_palette {
            app.show_command_palette = false;
            app.command_palette_search.clear();
        } else {
            app.show_command_palette = show_palette;
        }
    }
}
