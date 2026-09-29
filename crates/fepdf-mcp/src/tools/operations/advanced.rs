//! Advanced domain operations: GIS, page labels, mesh shading, public key crypto, unencrypted wrappers.

use super::page::execute_single_op;
use fepdf::{
    ArticleBead, ArticleThread, GeoSpatialAnchor, Operation, PageLabelSpec, PageLabelStyle,
    PdfAction, UnencryptedWrapperSpec,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::fs;

/// Arguments for setting page labels.
#[derive(Deserialize, JsonSchema)]
pub struct PageLabelArg {
    /// 0-indexed page start.
    pub start_page: usize,
    /// Numbering style ("decimal", "lower_roman", "upper_roman", "lower_alpha", "upper_alpha").
    pub style: String,
    /// Optional prefix (e.g. "A-").
    pub prefix: Option<String>,
    /// Starting number (defaults to 1).
    pub start_number: Option<u32>,
}

/// Arguments for the set_page_labels tool.
#[derive(Deserialize, JsonSchema)]
pub struct SetPageLabelsArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Page label scheme definitions.
    pub labels: Vec<PageLabelArg>,
}

/// Arguments for article thread bead.
#[derive(Deserialize, JsonSchema)]
pub struct ArticleBeadArg {
    /// 0-based page index.
    pub page: usize,
    /// Bounding rectangle `[x0, y0, x1, y1]`.
    pub rect: [f32; 4],
}

/// Arguments for an article thread.
#[derive(Deserialize, JsonSchema)]
pub struct ArticleThreadArg {
    /// Title of the article thread.
    pub title: String,
    /// List of beads in the thread.
    pub beads: Vec<ArticleBeadArg>,
}

/// Arguments for updating article threads.
#[derive(Deserialize, JsonSchema)]
pub struct UpdateArticleThreadsArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// List of article threads to write.
    pub threads: Vec<ArticleThreadArg>,
}

/// Arguments for setting a GIS geospatial anchor.
#[derive(Deserialize, JsonSchema)]
pub struct SetGeospatialAnchorArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// 0-based page index.
    pub page: usize,
    /// Latitude in decimal degrees.
    pub latitude: f64,
    /// Longitude in decimal degrees.
    pub longitude: f64,
    /// Coordinate Reference System in WKT format.
    pub crs_wkt: String,
}

/// Arguments for setting an unencrypted wrapper document.
#[derive(Deserialize, JsonSchema)]
pub struct SetUnencryptedWrapperArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Path to the encrypted payload: a PDF file encrypted by a custom security handler.
    pub payload_file_path: String,
    /// The cryptographic filter that decrypts it, as a name: Table 28's `/Subtype`.
    pub crypto_filter: String,
    /// That filter's version, integers with a period between them: Table 28's `/Version`.
    pub filter_version: Option<String>,
    /// What a reader without the filter is told, such as which handler to install.
    pub notice_message: Option<String>,
}

/// Arguments for executing an action.
#[derive(Deserialize, JsonSchema)]
pub struct SetOpenActionArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Action type ("named", "gotor", "gotoe").
    pub action_type: String,
    /// Target argument for the action.
    pub target: String,
}

/// Implementation of the set_page_labels tool.
pub fn set_page_labels_impl(args: SetPageLabelsArgs) -> Result<String, String> {
    let specs = args
        .labels
        .into_iter()
        .map(|l| {
            let style = match l.style.to_lowercase().as_str() {
                "lower_roman" => PageLabelStyle::LowerRoman,
                "upper_roman" => PageLabelStyle::UpperRoman,
                "lower_alpha" => PageLabelStyle::LowerAlpha,
                "upper_alpha" => PageLabelStyle::UpperAlpha,
                _ => PageLabelStyle::Decimal,
            };
            PageLabelSpec {
                start_page: l.start_page,
                style,
                prefix: l.prefix,
                start_number: l.start_number.unwrap_or(1),
            }
        })
        .collect();

    let op = Operation::SetPageLabels(specs);
    execute_single_op(&args.input_path, &args.output_path, op, "Page labels set")
}

/// Implementation of the update_article_threads tool.
pub fn update_article_threads_impl(args: UpdateArticleThreadsArgs) -> Result<String, String> {
    let threads = args
        .threads
        .into_iter()
        .map(|t| {
            let beads =
                t.beads.into_iter().map(|b| ArticleBead { page: b.page, rect: b.rect }).collect();
            ArticleThread { title: t.title, beads }
        })
        .collect();

    let op = Operation::UpdateArticleThreads(threads);
    execute_single_op(&args.input_path, &args.output_path, op, "Article threads updated")
}

/// Implementation of the set_geospatial_anchor tool.
pub fn set_geospatial_anchor_impl(args: SetGeospatialAnchorArgs) -> Result<String, String> {
    let anchor = GeoSpatialAnchor {
        page: args.page,
        latitude: args.latitude,
        longitude: args.longitude,
        altitude_meters: None,
        crs_wkt: args.crs_wkt,
    };
    let op = Operation::SetGeospatialAnchor(anchor);
    execute_single_op(&args.input_path, &args.output_path, op, "Geospatial anchor (/Geo) set")
}

/// Implementation of the set_unencrypted_wrapper tool.
pub fn set_unencrypted_wrapper_impl(args: SetUnencryptedWrapperArgs) -> Result<String, String> {
    let payload = fs::read(&args.payload_file_path)
        .map_err(|e| format!("Failed to read wrapper payload '{}': {e}", args.payload_file_path))?;

    // The payload keeps its own file name, which is what a reader lists it by.
    let payload_name = std::path::Path::new(&args.payload_file_path)
        .file_name()
        .map_or_else(|| "payload.pdf".to_string(), |n| n.to_string_lossy().into_owned());
    let spec = UnencryptedWrapperSpec {
        notice_message: args.notice_message.unwrap_or_else(|| {
            format!("This document is encrypted with the {} security handler.", args.crypto_filter)
        }),
        encrypted_payload_bytes: payload,
        payload_name,
        crypto_filter: args.crypto_filter,
        filter_version: args.filter_version,
    };
    let op = Operation::SetUnencryptedWrapper(spec);
    execute_single_op(&args.input_path, &args.output_path, op, "Unencrypted wrapper payload set")
}

/// Implementation of the set_open_action tool.
pub fn set_open_action_impl(args: SetOpenActionArgs) -> Result<String, String> {
    let action = match args.action_type.to_lowercase().as_str() {
        "named" => PdfAction::Named(args.target),
        "gotor" => PdfAction::GoToRemote { file_path: args.target, page: 0 },
        "gotoe" => PdfAction::GoToEmbedded { embedded_name: args.target, page: 0 },
        _ => return Err(format!("Unsupported action type: {}", args.action_type)),
    };
    let op = Operation::SetOpenAction(action);
    execute_single_op(&args.input_path, &args.output_path, op, "Action executed")
}
