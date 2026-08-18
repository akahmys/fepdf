//! Metadata, outlines, layers, and portfolio domain operation tools.

use super::page::execute_single_op;
use fepdf::{
    AFRelationship, AssociatedFile, CollectionViewMode, LayerGroup, Operation,
    OptionalContentProperties, OutlineNode, OutlineTree, OutputIntent, PortfolioCollection,
    PortfolioItem, VisibilityState,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Node definition for outline / bookmarks tree.
#[derive(Deserialize, JsonSchema)]
pub struct OutlineNodeArg {
    /// Bookmark title string.
    pub title: String,
    /// Destination 0-based page index.
    pub dest_page: Option<usize>,
    /// Child bookmarks.
    pub children: Option<Vec<OutlineNodeArg>>,
}

/// Arguments for updating document outlines / bookmarks.
#[derive(Deserialize, JsonSchema)]
pub struct UpdateOutlinesArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Root bookmark nodes.
    pub roots: Vec<OutlineNodeArg>,
}

/// Layer definition for OCG layers.
#[derive(Deserialize, JsonSchema)]
pub struct LayerArg {
    /// Unique identifier of the layer.
    pub id: Option<String>,
    /// User-visible name of the layer.
    pub name: String,
    /// Default visibility state ("on", "off").
    pub default_state: Option<String>,
}

/// Arguments for updating optional content properties / layers.
#[derive(Deserialize, JsonSchema)]
pub struct UpdateLayersArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// List of layers to configure.
    pub layers: Vec<LayerArg>,
}

/// Arguments for attaching an associated file (/AF).
#[derive(Deserialize, JsonSchema)]
pub struct AttachAssociatedFileArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Path to the local file to attach.
    pub file_path: String,
    /// Embedded filename (defaults to file_path basename).
    pub filename: Option<String>,
    /// Relationship type ("source", "supplement", "alternative", "data").
    pub relationship: Option<String>,
    /// MIME type (e.g. "application/pdf", "text/csv").
    pub mime_type: Option<String>,
}

/// Arguments for creating a PDF Portfolio / Collection.
#[derive(Deserialize, JsonSchema)]
pub struct CreatePortfolioArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// View layout ("detail", "tile", "hidden").
    pub view_mode: Option<String>,
    /// Paths to files to embed in the collection.
    pub files: Vec<String>,
}

/// Arguments for setting an OutputIntent.
#[derive(Deserialize, JsonSchema)]
pub struct SetOutputIntentArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Output intent subtype (e.g. "GTS_PDFX").
    pub subtype: Option<String>,
    /// Output condition identifier (e.g. "FOGRA39").
    pub identifier: String,
    /// Human-readable info string.
    pub info: Option<String>,
}

/// Arguments for embedding a pronunciation lexicon (PLS XML).
#[derive(Deserialize, JsonSchema)]
pub struct SetPronunciationLexiconArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Path to the W3C PLS XML file.
    pub lexicon_xml_path: String,
}

fn convert_outline_node(node: OutlineNodeArg) -> OutlineNode {
    let kids = node.children.unwrap_or_default().into_iter().map(convert_outline_node).collect();
    OutlineNode { title: node.title, destination_page: node.dest_page.unwrap_or(0), children: kids }
}

/// Implementation of the update_outlines tool.
pub fn update_outlines_impl(args: UpdateOutlinesArgs) -> Result<String, String> {
    let tree = OutlineTree { items: args.roots.into_iter().map(convert_outline_node).collect() };
    let op = Operation::UpdateOutlines(tree);
    execute_single_op(&args.input_path, &args.output_path, op, "Outlines/Bookmarks updated")
}

/// Implementation of the update_layers tool.
pub fn update_layers_impl(args: UpdateLayersArgs) -> Result<String, String> {
    let layers = args
        .layers
        .into_iter()
        .enumerate()
        .map(|(idx, l)| {
            let state = match l.default_state.as_deref() {
                Some("off") => VisibilityState::Off,
                _ => VisibilityState::On,
            };
            LayerGroup {
                id: l.id.unwrap_or_else(|| format!("Layer_{idx}")),
                name: l.name,
                default_state: state,
                printable: true,
            }
        })
        .collect();

    let oc_props = OptionalContentProperties { layers };
    let op = Operation::UpdateLayers(oc_props);
    execute_single_op(&args.input_path, &args.output_path, op, "Optional Content Layers updated")
}

/// Implementation of the attach_associated_file tool.
pub fn attach_associated_file_impl(args: AttachAssociatedFileArgs) -> Result<String, String> {
    let data = fs::read(&args.file_path)
        .map_err(|e| format!("Failed to read attachment file '{}': {e}", args.file_path))?;
    let filename = args.filename.unwrap_or_else(|| {
        Path::new(&args.file_path).file_name().unwrap_or_default().to_string_lossy().to_string()
    });

    let relationship = match args.relationship.as_deref() {
        Some("source") => AFRelationship::Source,
        Some("supplement") => AFRelationship::Supplement,
        Some("alternative") => AFRelationship::Alternative,
        _ => AFRelationship::Data,
    };

    let af = AssociatedFile {
        filename,
        relationship,
        mime_type: args.mime_type.unwrap_or_else(|| "application/octet-stream".to_string()),
        data,
    };

    let op = Operation::AttachAssociatedFile(af);
    execute_single_op(&args.input_path, &args.output_path, op, "Associated file attached (/AF)")
}

/// Implementation of the create_portfolio tool.
pub fn create_portfolio_impl(args: CreatePortfolioArgs) -> Result<String, String> {
    let view_mode = match args.view_mode.as_deref() {
        Some("tile") => CollectionViewMode::Tile,
        Some("hidden") => CollectionViewMode::Hidden,
        _ => CollectionViewMode::Details,
    };

    let mut items = Vec::new();
    for f in args.files {
        let data = fs::read(&f).map_err(|e| format!("Failed to read portfolio item '{f}': {e}"))?;
        let name = Path::new(&f).file_name().unwrap_or_default().to_string_lossy().to_string();
        items.push(PortfolioItem {
            filename: name,
            mime_type: None,
            description: None,
            size_bytes: data.len() as u64,
            data,
        });
    }

    let coll = PortfolioCollection { view_mode, initial_document: None, items };
    let op = Operation::CreatePortfolio(coll);
    execute_single_op(&args.input_path, &args.output_path, op, "Portfolio/Collection created")
}

/// Implementation of the set_output_intent tool.
pub fn set_output_intent_impl(args: SetOutputIntentArgs) -> Result<String, String> {
    let intent = OutputIntent {
        subtype: args.subtype.unwrap_or_else(|| "GTS_PDFX".to_string()),
        identifier: args.identifier,
        info: args.info,
        icc_profile_bytes: None,
    };
    let op = Operation::SetOutputIntent(intent);
    execute_single_op(&args.input_path, &args.output_path, op, "OutputIntent set")
}

/// Implementation of the set_pronunciation_lexicon tool.
pub fn set_pronunciation_lexicon_impl(args: SetPronunciationLexiconArgs) -> Result<String, String> {
    let bytes = fs::read(&args.lexicon_xml_path)
        .map_err(|e| format!("Failed to read lexicon XML '{}': {e}", args.lexicon_xml_path))?;
    let op = Operation::SetPronunciationLexicon { lexicon_xml_bytes: bytes };
    execute_single_op(&args.input_path, &args.output_path, op, "Pronunciation lexicon (/PL) set")
}
