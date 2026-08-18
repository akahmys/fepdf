//! Advanced domain operations: GIS, page labels, mesh shading, public key crypto, unencrypted wrappers.

use super::page::execute_single_op;
use fepdf::{
    ArticleBead, ArticleThread, GeoSpatialAnchor, MeshShadingSpec, MeshShadingType, Operation,
    PageLabelSpec, PageLabelStyle, PdfAction, PublicKeyRecipientSpec, UnencryptedWrapperSpec,
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

/// Arguments for adding a mesh shading spec.
#[derive(Deserialize, JsonSchema)]
pub struct AddMeshShadingArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Type of mesh shading (4, 5, 6, 7).
    pub shading_type: u8,
    /// Target color space (defaults to "DeviceRGB").
    pub color_space: Option<String>,
    /// Hex-encoded binary data of the mesh shading stream.
    pub stream_bytes_hex: String,
}

/// Arguments for setting an unencrypted wrapper document.
#[derive(Deserialize, JsonSchema)]
pub struct SetUnencryptedWrapperArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Path to the encrypted payload binary file.
    pub payload_file_path: String,
    /// Notice message for legacy PDF readers.
    pub notice_message: Option<String>,
}

/// Arguments for adding a public key recipient certificate.
#[derive(Deserialize, JsonSchema)]
pub struct AddPublicKeyRecipientArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Path to the recipient X.509 certificate in DER format.
    pub cert_der_path: String,
    /// Hex-encoded encrypted key bytes for this recipient.
    pub encrypted_key_hex: String,
}

/// Arguments for executing an action.
#[derive(Deserialize, JsonSchema)]
pub struct ExecuteActionArgs {
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

/// Implementation of the add_mesh_shading tool.
pub fn add_mesh_shading_impl(args: AddMeshShadingArgs) -> Result<String, String> {
    let st = match args.shading_type {
        4 => MeshShadingType::FreeFormTriangleMesh,
        5 => MeshShadingType::LatticeFormTriangleMesh,
        6 => MeshShadingType::CoonsPatchMesh,
        7 => MeshShadingType::TensorProductPatchMesh,
        _ => return Err("Invalid shading type: must be 4, 5, 6, or 7".into()),
    };

    let data_bytes = (0..args.stream_bytes_hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(
                &args.stream_bytes_hex[i..std::cmp::min(i + 2, args.stream_bytes_hex.len())],
                16,
            )
            .unwrap_or(0)
        })
        .collect();

    let spec = MeshShadingSpec {
        shading_type: st,
        color_space: args.color_space.unwrap_or_else(|| "DeviceRGB".to_string()),
        data_bytes,
    };
    let op = Operation::AddMeshShading(spec);
    execute_single_op(&args.input_path, &args.output_path, op, "Mesh shading spec added")
}

/// Implementation of the set_unencrypted_wrapper tool.
pub fn set_unencrypted_wrapper_impl(args: SetUnencryptedWrapperArgs) -> Result<String, String> {
    let payload = fs::read(&args.payload_file_path)
        .map_err(|e| format!("Failed to read wrapper payload '{}': {e}", args.payload_file_path))?;

    let spec = UnencryptedWrapperSpec {
        notice_message: args
            .notice_message
            .unwrap_or_else(|| "This document is protected.".to_string()),
        encrypted_payload_bytes: payload,
    };
    let op = Operation::SetUnencryptedWrapper(spec);
    execute_single_op(&args.input_path, &args.output_path, op, "Unencrypted wrapper payload set")
}

/// Implementation of the add_public_key_recipient tool.
pub fn add_public_key_recipient_impl(args: AddPublicKeyRecipientArgs) -> Result<String, String> {
    let cert_der_bytes = fs::read(&args.cert_der_path)
        .map_err(|e| format!("Failed to read certificate DER '{}': {e}", args.cert_der_path))?;

    let encrypted_key_bytes = (0..args.encrypted_key_hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(
                &args.encrypted_key_hex[i..std::cmp::min(i + 2, args.encrypted_key_hex.len())],
                16,
            )
            .unwrap_or(0)
        })
        .collect();

    let spec =
        PublicKeyRecipientSpec { certificate_der_bytes: cert_der_bytes, encrypted_key_bytes };
    let op = Operation::AddPublicKeyRecipient(spec);
    execute_single_op(&args.input_path, &args.output_path, op, "Public key recipient added")
}

/// Implementation of the execute_action tool.
pub fn execute_action_impl(args: ExecuteActionArgs) -> Result<String, String> {
    let action = match args.action_type.to_lowercase().as_str() {
        "named" => PdfAction::Named(args.target),
        "gotor" => PdfAction::GoToRemote { file_path: args.target, page: 0 },
        "gotoe" => PdfAction::GoToEmbedded { embedded_name: args.target, page: 0 },
        _ => return Err(format!("Unsupported action type: {}", args.action_type)),
    };
    let op = Operation::ExecuteAction(action);
    execute_single_op(&args.input_path, &args.output_path, op, "Action executed")
}
