//! MCP Server implementation and tool routing for fepdf.

#![allow(missing_docs)]

use crate::tools::operations::vocabulary::{
    AddLtvInfoArgs, DuplicatePagesArgs, InsertFromArgs, ReorderBatchArgs, RetagArgs, UpgradeArgs,
    add_ltv_info_impl, duplicate_pages_impl, insert_from_impl, reorder_batch_impl, retag_impl,
    upgrade_impl,
};
use crate::tools::{
    AddAnnotationArgs, AddMeshShadingArgs, AddPageDecorationArgs, AddPublicKeyRecipientArgs,
    AddUserPropertiesArgs, ApplyBatesNumberingArgs, ApplyOperationArgs, AttachAssociatedFileArgs,
    AuditArgs, CreatePortfolioArgs, DeleteStructElemArgs, ExecuteActionArgs, ExtractTextArgs,
    RedactDocumentArgs, RemovePagesArgs, RenderArgs, ReorderPagesArgs, RotatePagesArgs,
    SetFormFieldValueArgs, SetGeospatialAnchorArgs, SetMeasurementScaleArgs, SetOutputIntentArgs,
    SetPageLabelsArgs, SetPronunciationLexiconArgs, SetUnencryptedWrapperArgs,
    UpdateArticleThreadsArgs, UpdateLayersArgs, UpdateOutlinesArgs, UpdateStructElemArgs,
    VerifySignaturesArgs, add_annotation_impl, add_mesh_shading_impl, add_page_decoration_impl,
    add_public_key_recipient_impl, add_user_properties_impl, apply_bates_numbering_impl,
    apply_operation_impl, apply_redaction_impl, attach_associated_file_impl, audit_document_impl,
    create_portfolio_impl, delete_struct_elem_impl, execute_action_impl, extract_text_impl,
    remove_pages_impl, render_page_impl, reorder_pages_impl, rotate_pages_impl,
    set_form_field_value_impl, set_geospatial_anchor_impl, set_measurement_scale_impl,
    set_output_intent_impl, set_page_labels_impl, set_pronunciation_lexicon_impl,
    set_unencrypted_wrapper_impl, update_article_threads_impl, update_layers_impl,
    update_outlines_impl, update_struct_elem_impl, verify_signatures_impl,
};
use rmcp::{
    ServiceExt,
    handler::server::{ServerHandler, router::Router, wrapper::Parameters},
    tool, tool_handler, tool_router,
};

/// The fepdf MCP Server implementation.
///
/// It provides comprehensive PDF operations, structural auditing, rendering,
/// and accessibility tools via the Model Context Protocol.
pub struct FepdfServer;

#[tool_handler]
impl ServerHandler for FepdfServer {}

#[tool_router]
impl FepdfServer {
    /// Creates a new instance of the fepdf MCP server.
    pub fn new() -> Self {
        Self
    }

    /// Renders a specific page of a PDF document to a PNG image for visual inspection.
    #[tool(
        name = "render_page",
        description = "Renders a specific page of a PDF document to a PNG image for visual inspection."
    )]
    pub async fn render_page(
        &self,
        Parameters(args): Parameters<RenderArgs>,
    ) -> Result<String, String> {
        render_page_impl(args)
    }

    /// Performs a structural compliance audit of a PDF document.
    #[tool(
        name = "audit_document",
        description = "Performs a structural compliance audit of a PDF document, checking Catalog, XRef, and Page Tree integrity."
    )]
    pub async fn audit_document(
        &self,
        Parameters(args): Parameters<AuditArgs>,
    ) -> Result<String, String> {
        audit_document_impl(args)
    }

    /// Analyzes and verifies all digital signatures in a PDF.
    #[tool(
        name = "verify_signatures",
        description = "Analyzes and verifies all digital signatures in a PDF, including integrity checks (MD5/SHA) and signer certificate validation."
    )]
    pub async fn verify_signatures(
        &self,
        Parameters(args): Parameters<VerifySignaturesArgs>,
    ) -> Result<String, String> {
        verify_signatures_impl(args)
    }

    /// Extracts plain text from a PDF document by page range or for all pages.
    #[tool(
        name = "extract_text",
        description = "Extracts plain text from a PDF document by page range or for all pages."
    )]
    pub async fn extract_text(
        &self,
        Parameters(args): Parameters<ExtractTextArgs>,
    ) -> Result<String, String> {
        extract_text_impl(args)
    }

    /// Physically sanitizes and scrubs content streams inside specified bounding rectangles on designated pages.
    #[tool(
        name = "apply_redaction",
        description = "Physically sanitizes and scrubs content streams inside specified bounding rectangles on designated pages."
    )]
    pub async fn apply_redaction(
        &self,
        Parameters(args): Parameters<RedactDocumentArgs>,
    ) -> Result<String, String> {
        apply_redaction_impl(args)
    }

    /// Applies any canonical fepdf Operation in JSON format to mutate a PDF document.
    #[tool(
        name = "apply_operation",
        description = "Applies any canonical fepdf Operation in JSON format to mutate a PDF document."
    )]
    pub async fn apply_operation(
        &self,
        Parameters(args): Parameters<ApplyOperationArgs>,
    ) -> Result<String, String> {
        apply_operation_impl(args)
    }

    // --- Page Operations ---
    /// Rotates specified pages by a 90-degree multiple.
    #[tool(
        name = "rotate_pages",
        description = "Rotates specified pages (all, even, odd, or range) by a 90-degree multiple."
    )]
    pub async fn rotate_pages(
        &self,
        Parameters(args): Parameters<RotatePagesArgs>,
    ) -> Result<String, String> {
        rotate_pages_impl(args)
    }

    /// Reorders pages in a PDF document by moving a page from one index to another.
    #[tool(
        name = "reorder_pages",
        description = "Reorders pages in a PDF document by moving a page from one index to another."
    )]
    pub async fn reorder_pages(
        &self,
        Parameters(args): Parameters<ReorderPagesArgs>,
    ) -> Result<String, String> {
        reorder_pages_impl(args)
    }

    /// Removes specified pages or page selections from a PDF document.
    #[tool(
        name = "remove_pages",
        description = "Removes specified pages or page selections from a PDF document."
    )]
    pub async fn remove_pages(
        &self,
        Parameters(args): Parameters<RemovePagesArgs>,
    ) -> Result<String, String> {
        remove_pages_impl(args)
    }

    /// Moves several pages at once, preserving their relative order.
    #[tool(
        name = "reorder_pages_batch",
        description = "Moves several pages at once to a target index, preserving their relative order."
    )]
    pub async fn reorder_pages_batch(
        &self,
        Parameters(args): Parameters<ReorderBatchArgs>,
    ) -> Result<String, String> {
        reorder_batch_impl(args)
    }

    /// Duplicates a selection of pages in place.
    #[tool(
        name = "duplicate_pages",
        description = "Duplicates a selection of pages, inserting each copy after its original."
    )]
    pub async fn duplicate_pages(
        &self,
        Parameters(args): Parameters<DuplicatePagesArgs>,
    ) -> Result<String, String> {
        duplicate_pages_impl(args)
    }

    /// Inserts every page of another document at an index.
    #[tool(
        name = "insert_from",
        description = "Inserts every page of another PDF document at a given 0-based index."
    )]
    pub async fn insert_from(
        &self,
        Parameters(args): Parameters<InsertFromArgs>,
    ) -> Result<String, String> {
        insert_from_impl(args)
    }

    /// Declares conformance with a PDF standard.
    #[tool(
        name = "upgrade_standard",
        description = "Declares conformance with a PDF standard: A4, X6, UA2 or ISO32000-2."
    )]
    pub async fn upgrade_standard(
        &self,
        Parameters(args): Parameters<UpgradeArgs>,
    ) -> Result<String, String> {
        upgrade_impl(args)
    }

    // --- Tag Structure & Accessibility Operations ---
    /// Rebuilds the document's logical structure.
    #[tool(
        name = "retag_document",
        description = "Rebuilds the document's logical structure tree from its page content."
    )]
    pub async fn retag_document(
        &self,
        Parameters(args): Parameters<RetagArgs>,
    ) -> Result<String, String> {
        retag_impl(args)
    }

    /// Embeds long-term validation material beside a signature.
    #[tool(
        name = "add_ltv_info",
        description = "Embeds DER-encoded certificates in the DSS for long-term signature validation."
    )]
    pub async fn add_ltv_info(
        &self,
        Parameters(args): Parameters<AddLtvInfoArgs>,
    ) -> Result<String, String> {
        add_ltv_info_impl(args)
    }

    /// Updates a Tagged PDF structural element's tag type, alternate text (Alt), language, or actual text.
    #[tool(
        name = "update_struct_elem",
        description = "Updates a Tagged PDF structural element's tag type, alternate text (Alt), language, or actual text."
    )]
    pub async fn update_struct_elem(
        &self,
        Parameters(args): Parameters<UpdateStructElemArgs>,
    ) -> Result<String, String> {
        update_struct_elem_impl(args)
    }

    /// Deletes a structural element from the PDF/UA logical structure tree.
    #[tool(
        name = "delete_struct_elem",
        description = "Deletes a structural element from the PDF/UA logical structure tree."
    )]
    pub async fn delete_struct_elem(
        &self,
        Parameters(args): Parameters<DeleteStructElemArgs>,
    ) -> Result<String, String> {
        delete_struct_elem_impl(args)
    }

    /// Attaches user-defined properties (/UserProperties) to a Tagged PDF element.
    #[tool(
        name = "add_user_properties",
        description = "Attaches user-defined properties (/UserProperties) to a Tagged PDF element."
    )]
    pub async fn add_user_properties(
        &self,
        Parameters(args): Parameters<AddUserPropertiesArgs>,
    ) -> Result<String, String> {
        add_user_properties_impl(args)
    }

    // --- Metadata & Structure Domain Operations ---
    /// Updates the PDF document bookmarks and outline hierarchy tree.
    #[tool(
        name = "update_outlines",
        description = "Updates the PDF document bookmarks and outline hierarchy tree."
    )]
    pub async fn update_outlines(
        &self,
        Parameters(args): Parameters<UpdateOutlinesArgs>,
    ) -> Result<String, String> {
        update_outlines_impl(args)
    }

    /// Configures Optional Content Groups (OCG layers) and default visibility states.
    #[tool(
        name = "update_layers",
        description = "Configures Optional Content Groups (OCG layers) and default visibility states."
    )]
    pub async fn update_layers(
        &self,
        Parameters(args): Parameters<UpdateLayersArgs>,
    ) -> Result<String, String> {
        update_layers_impl(args)
    }

    /// Embeds an Associated File (/AF) with relationship metadata compliant with PDF 2.0 / PDF/A-3.
    #[tool(
        name = "attach_associated_file",
        description = "Embeds an Associated File (/AF) with relationship metadata compliant with PDF 2.0 / PDF/A-3."
    )]
    pub async fn attach_associated_file(
        &self,
        Parameters(args): Parameters<AttachAssociatedFileArgs>,
    ) -> Result<String, String> {
        attach_associated_file_impl(args)
    }

    /// Creates a PDF Portfolio / Collection embedding multiple files with specified view modes.
    #[tool(
        name = "create_portfolio",
        description = "Creates a PDF Portfolio / Collection embedding multiple files with specified view modes."
    )]
    pub async fn create_portfolio(
        &self,
        Parameters(args): Parameters<CreatePortfolioArgs>,
    ) -> Result<String, String> {
        create_portfolio_impl(args)
    }

    /// Sets or updates color management OutputIntents (PDF/X, PDF/A, PDF/E).
    #[tool(
        name = "set_output_intent",
        description = "Sets or updates color management OutputIntents (PDF/X, PDF/A, PDF/E)."
    )]
    pub async fn set_output_intent(
        &self,
        Parameters(args): Parameters<SetOutputIntentArgs>,
    ) -> Result<String, String> {
        set_output_intent_impl(args)
    }

    /// Embeds a W3C Pronunciation Lexicon Specification (PLS) XML dictionary (/PL).
    #[tool(
        name = "set_pronunciation_lexicon",
        description = "Embeds a W3C Pronunciation Lexicon Specification (PLS) XML dictionary (/PL)."
    )]
    pub async fn set_pronunciation_lexicon(
        &self,
        Parameters(args): Parameters<SetPronunciationLexiconArgs>,
    ) -> Result<String, String> {
        set_pronunciation_lexicon_impl(args)
    }

    // --- Decorations, Annotations & Forms ---
    /// Adds headers, footers, or watermarks to specified pages.
    #[tool(
        name = "add_page_decoration",
        description = "Adds headers, footers, or watermarks to specified pages."
    )]
    pub async fn add_page_decoration(
        &self,
        Parameters(args): Parameters<AddPageDecorationArgs>,
    ) -> Result<String, String> {
        add_page_decoration_impl(args)
    }

    /// Applies legal Bates numbering sequences to document pages.
    #[tool(
        name = "apply_bates_numbering",
        description = "Applies legal Bates numbering sequences to document pages."
    )]
    pub async fn apply_bates_numbering(
        &self,
        Parameters(args): Parameters<ApplyBatesNumberingArgs>,
    ) -> Result<String, String> {
        apply_bates_numbering_impl(args)
    }

    /// Adds interactive annotations (highlights, underlines, notes, stamps, links) to a page.
    #[tool(
        name = "add_annotation",
        description = "Adds interactive annotations (highlights, underlines, notes, stamps, links) to a page."
    )]
    pub async fn add_annotation(
        &self,
        Parameters(args): Parameters<AddAnnotationArgs>,
    ) -> Result<String, String> {
        add_annotation_impl(args)
    }

    /// Configures drawing measurement scale dictionary (/Measure) for CAD and technical drawings.
    #[tool(
        name = "set_measurement_scale",
        description = "Configures drawing measurement scale dictionary (/Measure) for CAD and technical drawings."
    )]
    pub async fn set_measurement_scale(
        &self,
        Parameters(args): Parameters<SetMeasurementScaleArgs>,
    ) -> Result<String, String> {
        set_measurement_scale_impl(args)
    }

    /// Fills or updates values in AcroForm interactive form fields.
    #[tool(
        name = "set_form_field_value",
        description = "Fills or updates values in AcroForm interactive form fields."
    )]
    pub async fn set_form_field_value(
        &self,
        Parameters(args): Parameters<SetFormFieldValueArgs>,
    ) -> Result<String, String> {
        set_form_field_value_impl(args)
    }

    // --- Advanced Navigation & Security ---
    /// Configures page numbering schemes (/PageLabels) such as Roman numerals, decimals, or custom prefixes.
    #[tool(
        name = "set_page_labels",
        description = "Configures page numbering schemes (/PageLabels) such as Roman numerals, decimals, or custom prefixes."
    )]
    pub async fn set_page_labels(
        &self,
        Parameters(args): Parameters<SetPageLabelsArgs>,
    ) -> Result<String, String> {
        set_page_labels_impl(args)
    }

    /// Updates article threads (/Threads) and reading beads for multi-column navigation.
    #[tool(
        name = "update_article_threads",
        description = "Updates article threads (/Threads) and reading beads for multi-column navigation."
    )]
    pub async fn update_article_threads(
        &self,
        Parameters(args): Parameters<UpdateArticleThreadsArgs>,
    ) -> Result<String, String> {
        update_article_threads_impl(args)
    }

    /// Triggers or embeds PDF Actions (GoToR, GoToE, Named actions).
    #[tool(
        name = "execute_action",
        description = "Triggers or embeds PDF Actions (GoToR, GoToE, Named actions)."
    )]
    pub async fn execute_action(
        &self,
        Parameters(args): Parameters<ExecuteActionArgs>,
    ) -> Result<String, String> {
        execute_action_impl(args)
    }

    /// Sets geospatial GIS metadata anchor (/Geo) with coordinates (latitude, longitude) and CRS.
    #[tool(
        name = "set_geospatial_anchor",
        description = "Sets geospatial GIS metadata anchor (/Geo) with coordinates (latitude, longitude) and CRS."
    )]
    pub async fn set_geospatial_anchor(
        &self,
        Parameters(args): Parameters<SetGeospatialAnchorArgs>,
    ) -> Result<String, String> {
        set_geospatial_anchor_impl(args)
    }

    /// Adds Type 4-7 mesh shading gradient specification.
    #[tool(
        name = "add_mesh_shading",
        description = "Adds Type 4-7 mesh shading gradient specification."
    )]
    pub async fn add_mesh_shading(
        &self,
        Parameters(args): Parameters<AddMeshShadingArgs>,
    ) -> Result<String, String> {
        add_mesh_shading_impl(args)
    }

    /// Configures an unencrypted wrapper document payload conforming to ISO 32000-2 Section 7.6.7.
    #[tool(
        name = "set_unencrypted_wrapper",
        description = "Configures an unencrypted wrapper document payload conforming to ISO 32000-2 Section 7.6.7."
    )]
    pub async fn set_unencrypted_wrapper(
        &self,
        Parameters(args): Parameters<SetUnencryptedWrapperArgs>,
    ) -> Result<String, String> {
        set_unencrypted_wrapper_impl(args)
    }

    /// Adds a public key recipient certificate for certificate-based document encryption.
    #[tool(
        name = "add_public_key_recipient",
        description = "Adds a public key recipient certificate for certificate-based document encryption."
    )]
    pub async fn add_public_key_recipient(
        &self,
        Parameters(args): Parameters<AddPublicKeyRecipientArgs>,
    ) -> Result<String, String> {
        add_public_key_recipient_impl(args)
    }
}

impl Default for FepdfServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Entry point for running the fepdf MCP server over stdio.
pub async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    let server = FepdfServer::new();
    let router = Router::new(server).with_tools(FepdfServer::tool_router());

    let transport = rmcp::transport::stdio();

    println!(
        "fepdf MCP Server starting on stdio with 24 Operation tools and Resource/Prompt support..."
    );
    router.serve(transport).await.map_err(|e| format!("Server error: {e}"))?;

    Ok(())
}
