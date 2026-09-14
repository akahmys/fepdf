//! The document operations this window did not reach.
//!
//! **Measured 2026-09-09: `fepdf-mcp` constructs all thirty `Operation` variants,
//! `fepdf-cli` eight and this crate five.** The five are page manipulation — rotate,
//! remove, reorder, duplicate, and a structure-element edit — so a reader who wanted the
//! front matter numbered in roman had to reach for the command line. These are the seven
//! `fepdf-cli` had and this did not.
//!
//! Each form builds an `Operation` and hands it to the worker, which is what Rule D asks
//! of a frontend: translate, and let `fepdf-doc` decide what the operation means.

use crate::app::FepdfApp;
use crate::worker::WorkerRequest;

/// Which form the tools window is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tool {
    /// Nothing chosen; the window lists what there is.
    #[default]
    None,
    PageLabels,
    Bates,
    Retag,
    Upgrade,
    Attach,
    Geospatial,
    Portfolio,
    /// The sheet the pages are on, and what happens to what is on them (14.11.2).
    Resize,
}

/// What the forms are filling in, kept between frames.
#[derive(Debug, Clone)]
pub struct ToolState {
    pub open: Tool,
    pub label_style: fepdf::PageLabelStyle,
    pub label_prefix: String,
    pub label_start_page: usize,
    pub label_start_number: u32,
    pub bates_prefix: String,
    pub bates_start: u64,
    pub bates_digits: usize,
    pub standard: fepdf::PdfStandard,
    pub attach_path: Option<std::path::PathBuf>,
    pub attach_relationship: fepdf::AFRelationship,
    pub geo_page: usize,
    pub geo_latitude: f64,
    pub geo_longitude: f64,
    pub portfolio_paths: Vec<std::path::PathBuf>,
    /// The sheet chosen by name, or `None` while the width and height are being typed.
    pub sheet: Option<&'static str>,
    /// The sheet turned on its side.
    pub landscape: bool,
    /// What the width and height fields say, in points — the sheet when one is named.
    pub sheet_size: (f64, f64),
    /// What happens to what is already drawn on the pages.
    pub fit: fepdf::ContentFit,
    /// The factor `ContentFit::Scale` uses, kept while another fit is chosen.
    pub scale: f64,
    /// Whether the selection is resized rather than the whole document.
    pub resize_selection: bool,
}

impl Default for ToolState {
    fn default() -> Self {
        Self {
            open: Tool::None,
            label_style: fepdf::PageLabelStyle::LowerRoman,
            label_prefix: String::new(),
            label_start_page: 0,
            label_start_number: 1,
            bates_prefix: "DOC-".to_string(),
            bates_start: 1,
            bates_digits: 6,
            standard: fepdf::PdfStandard::UA2,
            attach_path: None,
            attach_relationship: fepdf::AFRelationship::Supplement,
            geo_page: 0,
            geo_latitude: 0.0,
            geo_longitude: 0.0,
            portfolio_paths: Vec::new(),
            // A4, because this is the sheet most of the world's documents are on and the
            // one every sample in this corpus but two is already using.
            sheet: Some("A4"),
            landscape: false,
            sheet_size: (595.0, 842.0),
            fit: fepdf::ContentFit::Fit,
            scale: 1.0,
            resize_selection: false,
        }
    }
}

/// Draws the tools, into the drawer that holds them.
pub fn show(app: &mut FepdfApp, ui: &mut egui::Ui) {
    body(app, ui);
}

fn body(app: &mut FepdfApp, ui: &mut egui::Ui) {
    if app.total_pages == 0 {
        ui.label(app.locale_mgr.tr(&app.active_language, "tools_no_document"));
        return;
    }
    picker(app, ui);
    ui.separator();
    match app.tools.open {
        Tool::None => {
            ui.label(app.locale_mgr.tr(&app.active_language, "tools_none"));
        }
        Tool::PageLabels => page_labels_form(app, ui),
        Tool::Bates => bates_form(app, ui),
        Tool::Retag => retag_form(app, ui),
        Tool::Upgrade => upgrade_form(app, ui),
        Tool::Attach => attach_form(app, ui),
        Tool::Geospatial => geospatial_form(app, ui),
        Tool::Portfolio => portfolio_form(app, ui),
        Tool::Resize => resize_form(app, ui),
    }
}

/// The list of tools, each with the sentence that says when it is the one wanted.
fn picker(app: &mut FepdfApp, ui: &mut egui::Ui) {
    const TOOLS: [(Tool, &str, &str); 8] = [
        (Tool::PageLabels, "tools_page_labels", "tools_page_labels_desc"),
        (Tool::Bates, "tools_bates", "tools_bates_desc"),
        (Tool::Retag, "tools_retag", "tools_retag_desc"),
        (Tool::Upgrade, "tools_upgrade", "tools_upgrade_desc"),
        (Tool::Attach, "tools_attach", "tools_attach_desc"),
        (Tool::Geospatial, "tools_geo", "tools_geo_desc"),
        (Tool::Portfolio, "tools_portfolio", "tools_portfolio_desc"),
        (Tool::Resize, "tools_resize", "tools_resize_desc"),
    ];
    for (tool, name, description) in TOOLS {
        let label = app.locale_mgr.tr(&app.active_language, name);
        if ui.selectable_label(app.tools.open == tool, label).clicked() {
            app.tools.open = tool;
        }
        if app.tools.open == tool {
            let note = app.locale_mgr.tr(&app.active_language, description);
            ui.label(egui::RichText::new(note).size(crate::app::theme::text::SMALL).weak());
        }
    }
}

/// Sends `operation`, and says so in the reader's language.
fn apply(app: &FepdfApp, operation: fepdf::Operation, done_key: &str) {
    let done = app.locale_mgr.tr(&app.active_language, done_key);
    let _ = app.tx_worker.send(WorkerRequest::Apply { operation: Box::new(operation), done });
}

/// 12.4.2: what the page numbers say, which need not be what the pages are.
fn page_labels_form(app: &mut FepdfApp, ui: &mut egui::Ui) {
    let tr = |k: &str| app.locale_mgr.tr(&app.active_language, k);
    ui.horizontal(|ui| {
        ui.label(tr("tools_style"));
        for (style, name) in STYLES {
            ui.selectable_value(&mut app.tools.label_style, style, name);
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("tools_prefix"));
        ui.text_edit_singleline(&mut app.tools.label_prefix);
    });
    ui.horizontal(|ui| {
        ui.label(tr("tools_start_page"));
        ui.add(egui::DragValue::new(&mut app.tools.label_start_page).range(0..=app.total_pages));
    });
    ui.horizontal(|ui| {
        ui.label(tr("tools_start_number"));
        ui.add(egui::DragValue::new(&mut app.tools.label_start_number).range(1..=u32::MAX));
    });
    if ui.button(tr("tools_apply")).clicked() {
        let prefix = app.tools.label_prefix.clone();
        apply(
            app,
            fepdf::Operation::SetPageLabels(vec![fepdf::PageLabelSpec {
                start_page: app.tools.label_start_page,
                style: app.tools.label_style,
                // An empty box is no prefix, not a prefix of nothing: `/P ()` is a
                // label that reads as its number with an empty string in front.
                prefix: (!prefix.is_empty()).then_some(prefix),
                start_number: app.tools.label_start_number,
            }]),
            "tools_page_labels",
        );
    }
}

const STYLES: [(fepdf::PageLabelStyle, &str); 5] = [
    (fepdf::PageLabelStyle::Decimal, "1"),
    (fepdf::PageLabelStyle::LowerRoman, "i"),
    (fepdf::PageLabelStyle::UpperRoman, "I"),
    (fepdf::PageLabelStyle::LowerAlpha, "a"),
    (fepdf::PageLabelStyle::UpperAlpha, "A"),
];

/// A filing sequence stamped on every page.
fn bates_form(app: &mut FepdfApp, ui: &mut egui::Ui) {
    let tr = |k: &str| app.locale_mgr.tr(&app.active_language, k);
    ui.horizontal(|ui| {
        ui.label(tr("tools_prefix"));
        ui.text_edit_singleline(&mut app.tools.bates_prefix);
    });
    ui.horizontal(|ui| {
        ui.label(tr("tools_start_number"));
        ui.add(egui::DragValue::new(&mut app.tools.bates_start));
    });
    ui.horizontal(|ui| {
        ui.label(tr("tools_digits"));
        ui.add(egui::DragValue::new(&mut app.tools.bates_digits).range(1..=12));
    });
    if ui.button(tr("tools_apply")).clicked() {
        apply(
            app,
            fepdf::Operation::ApplyBatesNumbering {
                pages: fepdf::PageSelection::All,
                prefix: app.tools.bates_prefix.clone(),
                start_number: app.tools.bates_start,
                digits: app.tools.bates_digits,
                position: fepdf::DecorationPosition::BottomRight,
            },
            "tools_bates",
        );
    }
}

/// Rebuilds the logical structure tree from what is drawn.
fn retag_form(app: &mut FepdfApp, ui: &mut egui::Ui) {
    if ui.button(app.locale_mgr.tr(&app.active_language, "tools_apply")).clicked() {
        apply(app, fepdf::Operation::Retag, "tools_retag");
    }
}

/// Records conformance with a standard (6.3.1).
fn upgrade_form(app: &mut FepdfApp, ui: &mut egui::Ui) {
    const STANDARDS: [(fepdf::PdfStandard, &str); 4] = [
        (fepdf::PdfStandard::A4, "PDF/A-4"),
        (fepdf::PdfStandard::X6, "PDF/X-6"),
        (fepdf::PdfStandard::UA2, "PDF/UA-2"),
        (fepdf::PdfStandard::ISO32000_2, "ISO 32000-2"),
    ];
    for (standard, name) in STANDARDS {
        ui.radio_value(&mut app.tools.standard, standard, name);
    }
    if ui.button(app.locale_mgr.tr(&app.active_language, "tools_apply")).clicked() {
        apply(app, fepdf::Operation::Upgrade { standard: app.tools.standard }, "tools_upgrade");
    }
}

/// 14.13: another file carried inside this one, with the relationship it stands in.
fn attach_form(app: &mut FepdfApp, ui: &mut egui::Ui) {
    const RELATIONSHIPS: [(fepdf::AFRelationship, &str); 4] = [
        (fepdf::AFRelationship::Source, "Source"),
        (fepdf::AFRelationship::Data, "Data"),
        (fepdf::AFRelationship::Supplement, "Supplement"),
        (fepdf::AFRelationship::Alternative, "Alternative"),
    ];
    let tr = |k: &str| app.locale_mgr.tr(&app.active_language, k);

    ui.horizontal(|ui| {
        if ui.button(tr("tools_pick_files")).clicked() {
            app.tools.attach_path = rfd::FileDialog::new().pick_file();
        }
        if let Some(path) = &app.tools.attach_path {
            ui.label(path.file_name().unwrap_or(path.as_os_str()).to_string_lossy());
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("tools_relationship"));
        for (relationship, name) in RELATIONSHIPS {
            ui.selectable_value(&mut app.tools.attach_relationship, relationship, name);
        }
    });

    let Some(path) = app.tools.attach_path.clone() else { return };
    if ui.button(tr("tools_apply")).clicked() {
        // Read here rather than in the worker: a file the reader cannot read is a thing
        // to say now, beside the button they pressed.
        match std::fs::read(&path) {
            Ok(data) => apply(
                app,
                fepdf::Operation::AttachAssociatedFile(fepdf::AssociatedFile {
                    filename: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                    relationship: app.tools.attach_relationship,
                    mime_type: mime_of(&path),
                    data,
                }),
                "tools_attach",
            ),
            Err(e) => {
                app.notice = Some(
                    crate::app::Notice::failed("notice_attach_failed")
                        .about(format!("{}: {e}", path.display())),
                );
            }
        }
    }
}

/// A media type from the extension, which is what a `/Subtype` is for (Table 43).
///
/// **Guessed, and it says so by falling back rather than by inventing.** A reader who
/// needs an exact type has a document-preparation tool that asks for one; what this
/// avoids is writing `application/octet-stream` over an XML invoice that a validator will
/// then refuse.
fn mime_of(path: &std::path::Path) -> String {
    match path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
        Some("pdf") => "application/pdf",
        Some("xml") => "application/xml",
        Some("json") => "application/json",
        Some("csv") => "text/csv",
        Some("txt") => "text/plain",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// Where on the Earth a page sits (Table 168).
fn geospatial_form(app: &mut FepdfApp, ui: &mut egui::Ui) {
    let tr = |k: &str| app.locale_mgr.tr(&app.active_language, k);
    ui.horizontal(|ui| {
        ui.label(tr("tools_page"));
        ui.add(egui::DragValue::new(&mut app.tools.geo_page).range(0..=app.total_pages - 1));
    });
    ui.horizontal(|ui| {
        ui.label(tr("tools_latitude"));
        ui.add(egui::DragValue::new(&mut app.tools.geo_latitude).range(-90.0..=90.0).speed(0.01));
    });
    ui.horizontal(|ui| {
        ui.label(tr("tools_longitude"));
        ui.add(
            egui::DragValue::new(&mut app.tools.geo_longitude).range(-180.0..=180.0).speed(0.01),
        );
    });
    if ui.button(tr("tools_apply")).clicked() {
        apply(
            app,
            fepdf::Operation::SetGeospatialAnchor(fepdf::GeoSpatialAnchor {
                page: app.tools.geo_page,
                latitude: app.tools.geo_latitude,
                longitude: app.tools.geo_longitude,
                altitude_meters: None,
                // WGS 84, which is what a latitude and a longitude typed by a person are
                // in unless they say otherwise. Naming it beats leaving `/GCS` absent.
                crs_wkt: WGS84.to_string(),
            }),
            "tools_geo",
        );
    }
}

/// EPSG:4326, the coordinate system a plain latitude and longitude belong to.
const WGS84: &str = "GEOGCS[\"WGS 84\",DATUM[\"WGS_1984\",SPHEROID[\"WGS 84\",6378137,298.257223563]],\
PRIMEM[\"Greenwich\",0],UNIT[\"degree\",0.0174532925199433],AUTHORITY[\"EPSG\",\"4326\"]]";

/// Several files presented as one collection (7.11.6).
fn portfolio_form(app: &mut FepdfApp, ui: &mut egui::Ui) {
    let tr = |k: &str| app.locale_mgr.tr(&app.active_language, k);
    if ui.button(tr("tools_pick_files")).clicked()
        && let Some(paths) = rfd::FileDialog::new().pick_files()
    {
        app.tools.portfolio_paths = paths;
    }
    for path in &app.tools.portfolio_paths {
        ui.label(path.file_name().unwrap_or(path.as_os_str()).to_string_lossy());
    }
    if app.tools.portfolio_paths.is_empty() {
        return;
    }
    if ui.button(tr("tools_apply")).clicked() {
        // Read here rather than in the worker, and refused as a whole rather than in
        // part: a portfolio missing one of the files the reader chose is not the
        // collection they asked for, and finding out later is worse than not making it.
        let mut items = Vec::with_capacity(app.tools.portfolio_paths.len());
        for path in &app.tools.portfolio_paths {
            match std::fs::read(path) {
                Ok(data) => items.push(fepdf::PortfolioItem {
                    filename: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                    mime_type: Some(mime_of(path)),
                    description: None,
                    size_bytes: data.len() as u64,
                    data,
                }),
                Err(e) => {
                    app.notice = Some(
                        crate::app::Notice::failed("notice_attach_failed")
                            .about(format!("{}: {e}", path.display())),
                    );
                    return;
                }
            }
        }
        apply(
            app,
            fepdf::Operation::CreatePortfolio(fepdf::PortfolioCollection {
                view_mode: fepdf::CollectionViewMode::Details,
                initial_document: None,
                items,
            }),
            "tools_portfolio",
        );
    }
}

/// Every string the resize form shows, so they can be read before the pickers borrow the
/// state they draw.
const RESIZE_WORDS: [&str; 12] = [
    "tools_apply",
    "tools_resize_sheet",
    "tools_resize_width",
    "tools_resize_height",
    "tools_resize_points",
    "tools_resize_landscape",
    "tools_resize_content",
    "tools_resize_selection",
    "tools_fit_fit",
    "tools_fit_centre",
    "tools_fit_anchor",
    "tools_fit_scale",
];

/// 14.11.2: the sheet the pages are on, and what happens to what is drawn there.
///
/// **The fit is the question this form exists to ask.** Changing the sheet says nothing
/// about the drawing on it, so the operation refuses to guess and this offers the four
/// answers by name.
fn resize_form(app: &mut FepdfApp, ui: &mut egui::Ui) {
    // **The strings are taken before the pickers run**, not read through a closure that
    // borrows the locale: the pickers take the tool state mutably, and a shared borrow of
    // one of `app`'s fields held across that is a borrow of the whole of it. The drawer
    // above does the same with its title.
    let words: Vec<String> =
        RESIZE_WORDS.iter().map(|key| app.locale_mgr.tr(&app.active_language, key)).collect();
    let tr = |key: &str| -> String {
        RESIZE_WORDS.iter().position(|k| *k == key).map_or_else(String::new, |at| words[at].clone())
    };
    sheet_picker(&mut app.tools, ui, &tr);
    ui.add_space(crate::app::theme::space::GROUP);
    fit_picker(&mut app.tools, ui, &tr);
    ui.add_space(crate::app::theme::space::GROUP);

    let selected = app.selected_pages.len();
    if selected > 0 {
        let label = format!("{} ({selected})", tr("tools_resize_selection"));
        ui.checkbox(&mut app.tools.resize_selection, label);
    }
    if ui.button(tr("tools_apply")).clicked() {
        send_resize(app);
    }
}

/// Builds the resize the form describes and sends it.
///
/// **One home**, because the capture harness presses this too and a second copy of the
/// reading would be a second thing to keep true (UI-12).
pub fn send_resize(app: &FepdfApp) {
    let pages = if app.tools.resize_selection && !app.selected_pages.is_empty() {
        let mut indices: Vec<usize> = app.selected_pages.iter().copied().collect();
        indices.sort_unstable();
        fepdf::PageSelection::Indices(indices)
    } else {
        fepdf::PageSelection::All
    };
    let size = app.tools.sheet_size;
    let size = if app.tools.landscape { fepdf::PageResize::landscape(size) } else { size };
    let resize = fepdf::PageResize { size, content: app.tools.fit };
    apply(app, fepdf::Operation::ResizePages(pages, resize), "tools_resize");
}

/// The sheet, by name or by two numbers.
fn sheet_picker(tools: &mut ToolState, ui: &mut egui::Ui, tr: &dyn Fn(&str) -> String) {
    ui.label(tr("tools_resize_sheet"));
    ui.horizontal_wrapped(|ui| {
        for (name, size) in fepdf::PageResize::SHEETS {
            // The names are identifiers — `A4` is `A4` in every language — so they carry
            // no locale key, for the same reason `PDF/A-4` beside them does not.
            if ui.selectable_label(tools.sheet == Some(name), name).clicked() {
                tools.sheet = Some(name);
                tools.sheet_size = size;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("tools_resize_width"));
        // Typing a number stops the sheet being a named one: it is whatever was typed.
        if ui.add(egui::DragValue::new(&mut tools.sheet_size.0).speed(1.0)).changed() {
            tools.sheet = None;
        }
        ui.label(tr("tools_resize_height"));
        if ui.add(egui::DragValue::new(&mut tools.sheet_size.1).speed(1.0)).changed() {
            tools.sheet = None;
        }
        ui.label(tr("tools_resize_points"));
    });
    ui.checkbox(&mut tools.landscape, tr("tools_resize_landscape"));
}

/// What happens to what is already on the page.
fn fit_picker(tools: &mut ToolState, ui: &mut egui::Ui, tr: &dyn Fn(&str) -> String) {
    const FITS: [(fepdf::ContentFit, &str); 3] = [
        (fepdf::ContentFit::Fit, "tools_fit_fit"),
        (fepdf::ContentFit::Centre, "tools_fit_centre"),
        (fepdf::ContentFit::Anchor, "tools_fit_anchor"),
    ];
    ui.label(tr("tools_resize_content"));
    for (fit, key) in FITS {
        ui.radio_value(&mut tools.fit, fit, tr(key));
    }
    // **The factor is read whether or not this row is the chosen one**, so that dragging
    // it is how a reader chooses it — a radio button that has to be pressed first before
    // the number beside it does anything is two gestures for one intent.
    ui.horizontal(|ui| {
        let chosen = matches!(tools.fit, fepdf::ContentFit::Scale(_));
        if ui.radio(chosen, tr("tools_fit_scale")).clicked() {
            tools.fit = fepdf::ContentFit::Scale(tools.scale);
        }
        let dragged = ui
            .add(egui::DragValue::new(&mut tools.scale).speed(0.01).range(0.05..=10.0).suffix("×"));
        if dragged.changed() {
            tools.fit = fepdf::ContentFit::Scale(tools.scale);
        }
    });
}
