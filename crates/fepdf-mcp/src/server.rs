//! MCP Server implementation and tool routing for fepdf.

#![allow(missing_docs)]

use crate::McpError;
use crate::tools::operations::vocabulary::{
    AddLtvInfoArgs, DeclareConformanceArgs, DuplicatePagesArgs, InsertFromArgs, ReorderBatchArgs,
    RetagArgs, UpgradeArgs, add_ltv_info_impl, declare_conformance_impl, duplicate_pages_impl,
    insert_from_impl, reorder_batch_impl, retag_impl, upgrade_impl,
};
use crate::tools::{
    AddAnnotationArgs, AddFormFieldArgs, AddPageDecorationArgs, AddUserPropertiesArgs,
    ApplyBatesNumberingArgs, ApplyOperationArgs, AttachAssociatedFileArgs, AuditArgs,
    CombinePagesArgs, CreatePortfolioArgs, CropPagesArgs, DeleteRunArgs, DeleteStructElemArgs,
    EditObjectArgs, EditRunArgs, ExtractTextArgs, ListCommentsArgs, ListObjectsArgs, ListRunsArgs,
    MapStructTypeArgs, MarkArtifactArgs, MergeRunsArgs, MoveRunArgs, MoveStructElemArgs,
    RedactDocumentArgs, RemovePagesArgs, ReorderPagesArgs, RotatePagesArgs,
    SetCalculationOrderArgs, SetFormFieldValueArgs, SetGeospatialAnchorArgs,
    SetMeasurementScaleArgs, SetOpenActionArgs, SetOutputIntentArgs, SetPageLabelsArgs,
    SetPronunciationLexiconArgs, SetStructAttributeArgs, SetStructNamespaceArgs, SetStructRefsArgs,
    SetTabOrderArgs, SetUnencryptedWrapperArgs, SplitPageArgs, SplitRunArgs,
    UpdateArticleThreadsArgs, UpdateLayersArgs, UpdateOutlinesArgs, UpdateStructElemArgs,
    VerifySignaturesArgs, WrapStructElemArgs, add_annotation_impl, add_form_field_impl,
    add_page_decoration_impl, add_user_properties_impl, apply_bates_numbering_impl,
    apply_operation_impl, apply_redaction_impl, attach_associated_file_impl, audit_document_impl,
    combine_pages_impl, create_portfolio_impl, crop_pages_impl, delete_run_impl,
    delete_struct_elem_impl, edit_object_impl, edit_run_impl, extract_text_impl,
    list_comments_impl, list_objects_impl, list_runs_impl, map_struct_type_impl,
    mark_artifact_impl, merge_runs_impl, move_run_impl, move_struct_elem_impl, remove_pages_impl,
    reorder_pages_impl, rotate_pages_impl, set_calculation_order_impl, set_form_field_value_impl,
    set_geospatial_anchor_impl, set_measurement_scale_impl, set_open_action_impl,
    set_output_intent_impl, set_page_labels_impl, set_pronunciation_lexicon_impl,
    set_struct_attribute_impl, set_struct_namespace_impl, set_struct_refs_impl, set_tab_order_impl,
    set_unencrypted_wrapper_impl, split_page_impl, split_run_impl, update_article_threads_impl,
    update_layers_impl, update_outlines_impl, update_struct_elem_impl, verify_signatures_impl,
    wrap_struct_elem_impl,
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

#[tool_handler(router = Self::all_tools())]
#[allow(unknown_lints)]
#[allow(clippy::unused_async_trait_impl)]
impl ServerHandler for FepdfServer {
    /// What this server tells a client it has, at `initialize`.
    ///
    /// **`#[tool_handler]` writes this method when nothing else does, and what it writes
    /// depends on which routers the type carries** — a `#[tool_router]` earns
    /// `.enable_tools()`, a `#[prompt_router]` earns `.enable_prompts()`, and there is no
    /// resource router to earn anything. With only the first of those, `prompts` and
    /// `resources` stayed `None` in `ServerCapabilities`, and `serde` drops a `None`
    /// capability from the response entirely: a client read a capability object naming
    /// tools alone and correctly concluded this server had no prompts and no resources.
    /// It was right. Both are served below, so both are declared here.
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .build(),
        )
    }

    async fn list_prompts(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListPromptsResult, rmcp::ErrorData> {
        Ok(rmcp::model::ListPromptsResult {
            prompts: crate::prompts::catalogue(),
            ..Default::default()
        })
    }

    async fn get_prompt(
        &self,
        request: rmcp::model::GetPromptRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::GetPromptResult, rmcp::ErrorData> {
        crate::prompts::get(&request.name, request.arguments.as_ref())
    }

    /// Empty: every resource this server serves names a file, so they are templates.
    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, rmcp::ErrorData> {
        Ok(rmcp::model::ListResourcesResult::default())
    }

    async fn list_resource_templates(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListResourceTemplatesResult, rmcp::ErrorData> {
        Ok(rmcp::model::ListResourceTemplatesResult {
            resource_templates: crate::resources::templates(),
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResult, rmcp::ErrorData> {
        let body = crate::resources::read(&request.uri)?;
        Ok(rmcp::model::ReadResourceResult::new(vec![rmcp::model::ResourceContents::text(
            body,
            &request.uri,
        )]))
    }
}

/// Every tool this build serves.
///
/// **Two routers rather than one**, because `#[tool_router]` collects the methods
/// carrying `#[tool]` without looking at their `#[cfg]`: cfg-ing a tool out of the main
/// block leaves the generated router referring to a method that no longer exists. A
/// second block that is itself behind the feature is what the macro can see correctly.
impl FepdfServer {
    /// Every tool this build registers.
    pub fn all_tools() -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        #[allow(unused_mut)]
        let mut router = Self::tool_router();
        router.merge(Self::review_tool_router());
        #[cfg(feature = "render")]
        router.merge(Self::render_tool_router());
        router
    }
}

/// Reading a page's annotations for review (ROADMAP AA-2a).
///
/// **A block of its own because the main one is at its limit**: `impl FepdfServer` below
/// reached 802 lines with this tool in it, against Rule 1's 800 (ADR-0106). The review
/// operations themselves go through `apply_operation`; what they need from a tool is the
/// index each annotation is named by.
#[tool_router(router = review_tool_router)]
impl FepdfServer {
    /// Lists a page's annotations, so that a caller has an index to name.
    #[tool(
        name = "list_comments",
        description = "Lists the annotations of a page in /Annots order: each one's kind, author (/T), words, dates, what it replies to, and the state each reviewer has given it. Each `at` (page and index) is what RemoveAnnotation, EditAnnotation, ReplyToAnnotation and SetAnnotationState take through apply_operation."
    )]
    pub async fn list_comments(
        &self,
        Parameters(args): Parameters<ListCommentsArgs>,
    ) -> Result<String, McpError> {
        list_comments_impl(args)
    }

    /// Writes a document's comments to an FDF file.
    #[tool(
        name = "export_fdf",
        description = "Writes every markup annotation of a PDF to an FDF file (ISO 32000-2 12.7.8): each with its /Page and an /NM, replies and states included. Links, widgets and pop-ups are not exported."
    )]
    pub async fn export_fdf(
        &self,
        Parameters(args): Parameters<crate::tools::ExportFdfArgs>,
    ) -> Result<String, McpError> {
        crate::tools::export_fdf_impl(args)
    }

    /// Writes a document's comments to an XFDF file.
    #[tool(
        name = "export_xfdf",
        description = "Writes the annotations export_fdf writes as XFDF (ISO 19444-1). What XFDF has no place for (a free text's /CL and /RD, a stamp's picture, /Path) is recorded as a decision."
    )]
    pub async fn export_xfdf(
        &self,
        Parameters(args): Parameters<crate::tools::ExportFdfArgs>,
    ) -> Result<String, McpError> {
        crate::tools::export_xfdf_impl(args)
    }

    /// Puts an XFDF file's comments onto a document.
    #[tool(
        name = "import_xfdf",
        description = "Imports the annotations of an XFDF file (ISO 19444-1, given as fdf_path) onto a PDF and saves the result, as import_fdf does: matched by name, and given an appearance drawn from their entries where they carry none."
    )]
    pub async fn import_xfdf(
        &self,
        Parameters(args): Parameters<crate::tools::ImportFdfArgs>,
    ) -> Result<String, McpError> {
        crate::tools::import_xfdf_impl(args)
    }

    /// Puts an FDF file's comments onto a document.
    #[tool(
        name = "import_fdf",
        description = "Imports the annotations of an FDF file onto a PDF and saves the result. An annotation whose /NM matches one on its page replaces it in place; any other is added; annotations the FDF does not name are left alone."
    )]
    pub async fn import_fdf(
        &self,
        Parameters(args): Parameters<crate::tools::ImportFdfArgs>,
    ) -> Result<String, McpError> {
        crate::tools::import_fdf_impl(args)
    }
}

/// The one tool that rasterises, and the only reason this server links a GPU stack.
///
/// Measured 2026-09-08: 285 crates in the dependency tree with `render`, 246 without.
#[cfg(feature = "render")]
#[tool_router(router = render_tool_router)]
impl FepdfServer {
    /// Renders a specific page of a PDF document to a PNG image for visual inspection.
    #[tool(
        name = "render_page",
        description = "Renders a specific page of a PDF document to a PNG image for visual inspection."
    )]
    pub async fn render_page(
        &self,
        Parameters(args): Parameters<crate::tools::render::RenderArgs>,
    ) -> Result<String, McpError> {
        crate::tools::render::render_page_impl(args)
    }

    /// Hands out a page as a picture for an OCR engine, with what is needed to read its
    /// answer back onto the page.
    #[tool(
        name = "page_for_ocr",
        description = "Writes a page as a PNG for an OCR engine (300 DPI unless `dpi` says otherwise) and answers its size in pixels, `pixel_to_page` — the six numbers of the transform from the picture's pixels (origin top left) to the page's points — and the text the page already has with where it is. Give `pixel_to_page` back to `add_text_layer` with boxes in pixels."
    )]
    pub async fn page_for_ocr(
        &self,
        Parameters(args): Parameters<crate::tools::text_layer::PageForOcrArgs>,
    ) -> Result<String, McpError> {
        crate::tools::text_layer::page_for_ocr_impl(args)
    }
}

#[tool_router]
impl FepdfServer {
    /// Creates a new instance of the fepdf MCP server.
    pub fn new() -> Self {
        Self
    }

    /// Performs a structural compliance audit of a PDF document.
    #[tool(
        name = "audit_document",
        description = "Audits a PDF: whether it opens, what the reader had to repair in its \
                       file structure to open it (7.5: header, cross-reference, trailer — \
                       each repair a Warning), the PDF/UA-2 accessibility audit's findings, \
                       and the engine's ISO 32000-2 compliance issues. status is FAILED \
                       when any finding is an Error."
    )]
    pub async fn audit_document(
        &self,
        Parameters(args): Parameters<AuditArgs>,
    ) -> Result<String, McpError> {
        audit_document_impl(args)
    }

    /// Analyzes and verifies all digital signatures in a PDF.
    #[tool(
        name = "verify_signatures",
        description = "Checks every digital signature a PDF carries: whether it verifies, who the certificate says signed it, and how much of the file the signature covers. No certificate chain is built to a root and no revocation list is checked."
    )]
    pub async fn verify_signatures(
        &self,
        Parameters(args): Parameters<VerifySignaturesArgs>,
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
        extract_text_impl(args)
    }

    /// Physically sanitizes and scrubs content streams inside specified bounding rectangles on designated pages.
    #[tool(
        name = "apply_redaction",
        description = "Removes text from inside rectangles on pages: each show-text operator \
                       of a page's own content stream whose text touches a rectangle has its \
                       strings replaced by [REDACTED] — the whole operator, including what \
                       lies outside the rectangle — and the count answered is how many were \
                       replaced. Only text is removed: images, vector graphics, annotations, \
                       form fields and the text of form XObjects inside the rectangles are \
                       left as they are, so this alone does not make a region safe to share."
    )]
    pub async fn apply_redaction(
        &self,
        Parameters(args): Parameters<RedactDocumentArgs>,
    ) -> Result<String, McpError> {
        apply_redaction_impl(args)
    }

    /// Applies any canonical fepdf Operation in JSON format to mutate a PDF document.
    #[tool(
        name = "apply_operation",
        description = "Applies any canonical fepdf Operation in JSON format to mutate a PDF \
                       document. A SetFormFieldValue operation also runs the form's \
                       calculation order, as set_form_field_value does; no other operation \
                       runs the document's scripts."
    )]
    pub async fn apply_operation(
        &self,
        Parameters(args): Parameters<ApplyOperationArgs>,
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
        insert_from_impl(args)
    }

    /// Declares conformity in a PDF Declaration.
    #[tool(
        name = "declare_conformance",
        description = "Declares that the document conforms to a standard, as a PDF Declaration \
                       (pdfd:conformsTo) in its XMP metadata, beside any it has: WTPDF's \
                       http://pdfa.org/declarations/wtpdf/#reuse1.0 or #accessibility1.0. \
                       The caller's statement; nothing is checked."
    )]
    pub async fn declare_conformance(
        &self,
        Parameters(args): Parameters<DeclareConformanceArgs>,
    ) -> Result<String, McpError> {
        declare_conformance_impl(args)
    }

    /// Declares conformance with a PDF standard.
    #[tool(
        name = "upgrade_standard",
        description = "Declares conformance with a PDF standard in the XMP metadata: A4, UA2 or \
                       ISO32000-2. X6 is refused: its identification is ISO 15930-9's, \
                       which this engine does not have."
    )]
    pub async fn upgrade_standard(
        &self,
        Parameters(args): Parameters<UpgradeArgs>,
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
        delete_struct_elem_impl(args)
    }

    /// Moves a structural element beside or inside another, which is how reading order
    /// is changed (14.7.4).
    #[tool(
        name = "move_struct_elem",
        description = "Moves a structural element before, after or inside another one, \
                       changing the document's reading order."
    )]
    pub async fn move_struct_elem(
        &self,
        Parameters(args): Parameters<MoveStructElemArgs>,
    ) -> Result<String, McpError> {
        move_struct_elem_impl(args)
    }

    /// Attaches user-defined properties (/UserProperties) to a Tagged PDF element.
    #[tool(
        name = "add_user_properties",
        description = "Attaches user-defined properties (/UserProperties) to a Tagged PDF element."
    )]
    pub async fn add_user_properties(
        &self,
        Parameters(args): Parameters<AddUserPropertiesArgs>,
    ) -> Result<String, McpError> {
        add_user_properties_impl(args)
    }

    /// Sets one attribute of a Tagged PDF element, in its owner's attribute object.
    #[tool(
        name = "set_struct_attribute",
        description = "Sets one attribute of a Tagged PDF structure element (14.7.6): the key \
                       in the attribute object its owner (/O) names, such as Table /Scope or \
                       List /ListNumbering. Give exactly one value."
    )]
    pub async fn set_struct_attribute(
        &self,
        Parameters(args): Parameters<SetStructAttributeArgs>,
    ) -> Result<String, McpError> {
        set_struct_attribute_impl(args)
    }

    /// Sets the structure elements a Tagged PDF element refers to (/Ref).
    #[tool(
        name = "set_struct_refs",
        description = "Sets the structure elements a Tagged PDF element refers to (/Ref, \
                       PDF 2.0): a TOCI to its target, a citation to its note and back, a \
                       continued list to its previous part. An empty list removes /Ref."
    )]
    pub async fn set_struct_refs(
        &self,
        Parameters(args): Parameters<SetStructRefsArgs>,
    ) -> Result<String, McpError> {
        set_struct_refs_impl(args)
    }

    /// Puts a Tagged PDF element in a namespace (/NS).
    #[tool(
        name = "set_struct_namespace",
        description = "Puts a Tagged PDF structure element in a namespace (/NS, PDF 2.0), \
                       adding the namespace to the structure tree root if absent. An empty \
                       namespace returns it to the default standard namespace."
    )]
    pub async fn set_struct_namespace(
        &self,
        Parameters(args): Parameters<SetStructNamespaceArgs>,
    ) -> Result<String, McpError> {
        set_struct_namespace_impl(args)
    }

    /// Maps a structure type of a namespace to another (RoleMapNS).
    #[tool(
        name = "map_struct_type",
        description = "Maps a structure type of a namespace to a type of the default standard \
                       namespace, or of another namespace (RoleMapNS, PDF 2.0)."
    )]
    pub async fn map_struct_type(
        &self,
        Parameters(args): Parameters<MapStructTypeArgs>,
    ) -> Result<String, McpError> {
        map_struct_type_impl(args)
    }

    /// Marks a tagged marked-content sequence as an artifact.
    #[tool(
        name = "mark_artifact",
        description = "Marks the tagged marked-content sequence with an MCID on a page as an \
                       artifact (a running header, a footer, a watermark), and removes the \
                       MCID from the structure tree."
    )]
    pub async fn mark_artifact(
        &self,
        Parameters(args): Parameters<MarkArtifactArgs>,
    ) -> Result<String, McpError> {
        mark_artifact_impl(args)
    }

    /// Wraps a run of an element's kids in a new structure element.
    #[tool(
        name = "wrap_struct_elem",
        description = "Wraps `count` kids of a structure element (or the structure tree \
                       root), from `first`, in a new element of the given type — the Caption \
                       of a Figure, the Lbl and LBody of an LI, the RB, RT and RP of a Ruby — \
                       which takes their place; the parent tree follows."
    )]
    pub async fn wrap_struct_elem(
        &self,
        Parameters(args): Parameters<WrapStructElemArgs>,
    ) -> Result<String, McpError> {
        wrap_struct_elem_impl(args)
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
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
        update_layers_impl(args)
    }

    /// Embeds an Associated File (/AF) with relationship metadata compliant with PDF 2.0 / PDF/A-3.
    #[tool(
        name = "attach_associated_file",
        description = "Embeds an Associated File (/AF) with relationship metadata compliant with PDF 2.0 / PDF/A-3, on the document or, given element_handle, on a structure element."
    )]
    pub async fn attach_associated_file(
        &self,
        Parameters(args): Parameters<AttachAssociatedFileArgs>,
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
        apply_bates_numbering_impl(args)
    }

    /// Adds interactive annotations (highlights, underlines, notes, stamps, links) to a page.
    #[tool(
        name = "add_annotation",
        description = "Adds an annotation to a page, with the appearance it is drawn by: a note, a highlight, underline, strike-out or squiggly line, a text box, typewriter text or a callout (set in a face that draws every character, so Japanese works), ink strokes, a rectangle, ellipse or line, a stamp from a JPEG file, or a link. A kind it does not know is refused."
    )]
    pub async fn add_annotation(
        &self,
        Parameters(args): Parameters<AddAnnotationArgs>,
    ) -> Result<String, McpError> {
        add_annotation_impl(args)
    }

    /// Replaces the text of one run on a page, named by its position among the runs.
    ///
    /// The description says the unit because a caller expecting find-and-replace would be
    /// surprised: a word is several runs in a real file, and this changes one of them and
    /// nothing else. Nothing groups runs, because which of them are one phrase is a
    /// question the content stream does not answer.
    #[tool(
        name = "edit_run",
        description = "Replaces the text of one run — one show-text operator — on a page, named by its position among the page's runs. The text is encoded in that run's own font, and a character the font cannot draw is refused by name. Runs are not grouped: a word is usually several of them."
    )]
    pub async fn edit_run(
        &self,
        Parameters(args): Parameters<EditRunArgs>,
    ) -> Result<String, McpError> {
        edit_run_impl(args)
    }

    /// Lists a page's runs, so that a caller has a number to name.
    ///
    /// **The three edits take a run number and nothing offered one.** `edit_run` shipped
    /// reachable with no listing beside it, which leaves a caller guessing at an index
    /// into a stream it cannot see. This is the other half of naming a run.
    #[tool(
        name = "list_runs",
        description = "Lists the runs of a page — one per show-text operator — with what each reads and the font it is set in. The `run` number here is the one `edit_run`, `split_run` and `delete_run` take."
    )]
    pub async fn list_runs(
        &self,
        Parameters(args): Parameters<ListRunsArgs>,
    ) -> Result<String, McpError> {
        list_runs_impl(args)
    }

    /// Compares two documents page by page.
    #[tool(
        name = "compare_documents",
        description = "Compares two PDF documents page by page, pairing pages by position. For each page that differs: the lines of text only the first has (`removed`) and only the second has (`added`), and the regions where the page looks different, in the first page's points (left, bottom, right, top), found by rendering both at `dpi` (72 unless given). `only_in_one` marks a page one document has and the other does not."
    )]
    pub async fn compare_documents(
        &self,
        Parameters(args): Parameters<crate::tools::compare::CompareArgs>,
    ) -> Result<String, McpError> {
        crate::tools::compare::compare_documents_impl(args)
    }

    /// Lays the text an OCR engine read over a page, invisibly.
    #[tool(
        name = "add_text_layer",
        description = "Lays text an OCR engine read over a page, invisibly (text rendering mode 3) in an embedded face with /ToUnicode, so it is found, selected and copied and the page looks exactly as it did. Each item is one line of text and the box it was read from: left, bottom, right, top in page points, or — with the `pixel_to_page` page_for_ocr answered — left, top, right, bottom in its pixels. Each is set to its box's height and stretched to its width."
    )]
    pub async fn add_text_layer(
        &self,
        Parameters(args): Parameters<crate::tools::text_layer::AddTextLayerArgs>,
    ) -> Result<String, McpError> {
        crate::tools::text_layer::add_text_layer_impl(args)
    }

    /// Lists the objects a page draws: its images and form XObjects, and where.
    #[tool(
        name = "list_objects",
        description = "Lists the objects a page draws with Do — images and form XObjects — with the resource name each is drawn by and its four corners on the page. The `object` number here is the one `edit_object` takes."
    )]
    pub async fn list_objects(
        &self,
        Parameters(args): Parameters<ListObjectsArgs>,
    ) -> Result<String, McpError> {
        list_objects_impl(args)
    }

    /// Moves, scales, turns or replaces one object a page draws.
    #[tool(
        name = "edit_object",
        description = "Moves, scales, turns or replaces one object a page draws, named by the number `list_objects` gives it. Give exactly one of move_to (its lower left corner, in points), scale (about its centre), rotate_degrees (about its centre, anticlockwise) or replace_with (a JPEG file; images only). The edit is written around this one drawing of it, so the same image drawn elsewhere is not changed."
    )]
    pub async fn edit_object(
        &self,
        Parameters(args): Parameters<EditObjectArgs>,
    ) -> Result<String, McpError> {
        edit_object_impl(args)
    }

    /// Cuts one run in two, so that either half can be named afterwards.
    #[tool(
        name = "split_run",
        description = "Cuts one run in two after a given number of its characters. The page draws exactly what it drew before — consecutive show-text operators draw from the current point — and afterwards a caller can name either half."
    )]
    pub async fn split_run(
        &self,
        Parameters(args): Parameters<SplitRunArgs>,
    ) -> Result<String, McpError> {
        split_run_impl(args)
    }

    /// Takes one run off the page.
    #[tool(
        name = "delete_run",
        description = "Takes one run off a page. This is not the same as editing it to the empty string: an emptied run keeps its number and can be typed into again, while a deleted one is gone from the listing and the runs after it move up by one."
    )]
    pub async fn delete_run(
        &self,
        Parameters(args): Parameters<DeleteRunArgs>,
    ) -> Result<String, McpError> {
        delete_run_impl(args)
    }

    /// Joins one run with the run after it.
    #[tool(
        name = "merge_runs",
        description = "Joins one run with the run after it, so a phrase drawn as two runs becomes one name. Nothing may stand between them: an operator in the way — a `Td`, a `Tf`, a `T*` — is named and the join refused, because joining across one would draw the second half somewhere it was not."
    )]
    pub async fn merge_runs(
        &self,
        Parameters(args): Parameters<MergeRunsArgs>,
    ) -> Result<String, McpError> {
        merge_runs_impl(args)
    }

    /// Puts one run somewhere else on the page.
    #[tool(
        name = "move_run",
        description = "Puts one run somewhere else on the page, in points from the bottom-left corner, and leaves every other run where it is. The run keeps its number and the face, size and angle it was set in; only where it draws from changes."
    )]
    pub async fn move_run(
        &self,
        Parameters(args): Parameters<MoveRunArgs>,
    ) -> Result<String, McpError> {
        move_run_impl(args)
    }

    /// Cuts pages down to a rectangle.
    #[tool(
        name = "crop_pages",
        description = "Cuts pages down to a rectangle given in points from the bottom-left corner. By default `/CropBox` hides what falls outside and it stays in the file, which is a view any reader can undo. With remove_outside, the content outside is taken out of the file — half a drawing left behind is still searchable on a page showing the other half."
    )]
    pub async fn crop_pages(
        &self,
        Parameters(args): Parameters<CropPagesArgs>,
    ) -> Result<String, McpError> {
        crop_pages_impl(args)
    }

    /// Puts pages on a different sheet, or scales what they draw.
    #[tool(
        name = "resize_pages",
        description = "Puts pages on a different sheet, and says what happens to what is drawn on them: kept at its size, fit so all of it is on the sheet, filling the sheet, or scaled by a factor. Leave the sheet out to scale the drawing inside the sheet it is already on. Sizes and offsets are in points; the offset is from the sheet's middle, right then up."
    )]
    pub async fn resize_pages(
        &self,
        Parameters(args): Parameters<crate::tools::operations::page::ResizePagesArgs>,
    ) -> Result<String, McpError> {
        crate::tools::operations::page::resize_pages_impl(args)
    }

    /// Takes off one page every glyph outside a rectangle.
    #[tool(
        name = "remove_outside",
        description = "Takes off one page, counting from zero, every glyph that falls outside a rectangle given in points from the bottom-left corner. What stays does not move. This is what a crop that cuts does to the text; crop_pages with remove_outside does it to the text, the images and the drawing together."
    )]
    pub async fn remove_outside(
        &self,
        Parameters(args): Parameters<crate::tools::operations::page::RemoveOutsideArgs>,
    ) -> Result<String, McpError> {
        crate::tools::operations::page::remove_outside_impl(args)
    }

    /// Applies the document's own redaction annotations.
    #[tool(
        name = "apply_redact_annotations",
        description = "Applies the redaction annotations a document already carries (12.5.6.23): what each marks, by its quadrilaterals or else its rectangle, is removed from the file — text, image pixels, drawing, form content, annotations and the replacement text that read it — the annotation goes, and its place is drawn as it says: its overlay form, else its interior colour and overlay text, else nothing. Pages count from 1: \"all\", \"1\", \"1-3\"."
    )]
    pub async fn apply_redact_annotations(
        &self,
        Parameters(args): Parameters<crate::tools::redact::ApplyRedactAnnotationsArgs>,
    ) -> Result<String, McpError> {
        crate::tools::redact::apply_redact_annotations_impl(args)
    }

    /// Cuts one page into several.
    #[tool(
        name = "split_page",
        description = "Cuts one page into several, as an even grid of columns and rows or as regions named outright. A grid comes out in reading order: across a row first, then down. What belongs to the other sheets is taken out of the file rather than hidden — half a drawing left behind is still searchable on a page showing the other half."
    )]
    pub async fn split_page(
        &self,
        Parameters(args): Parameters<SplitPageArgs>,
    ) -> Result<String, McpError> {
        split_page_impl(args)
    }

    /// Puts several pages onto one sheet.
    #[tool(
        name = "combine_pages",
        description = "Puts several pages onto one sheet in a grid, filling it in reading order: across a row first, then down. Each page is scaled to fit its cell whole and centred, so it keeps its shape. What a page carries beside what it draws — its annotations above all — belongs to the page it was on and does not come across."
    )]
    pub async fn combine_pages(
        &self,
        Parameters(args): Parameters<CombinePagesArgs>,
    ) -> Result<String, McpError> {
        combine_pages_impl(args)
    }

    /// Creates a form field with its widget on a page.
    #[tool(
        name = "add_form_field",
        description = "Creates a form field of one of nine kinds — text, text_area, password, check_box, radio_button, push_button, combo_box, list_box, signature — with its widget on a page, and writes the /AcroForm if the document has none. A tooltip is required: a field without one is an accessibility failure this engine reports."
    )]
    pub async fn add_form_field(
        &self,
        Parameters(args): Parameters<AddFormFieldArgs>,
    ) -> Result<String, McpError> {
        add_form_field_impl(args)
    }

    /// Sets the order a reader's Tab key moves through a page's annotations.
    #[tool(
        name = "set_tab_order",
        description = "Sets /Tabs on pages: the order a reader's Tab key moves through their annotations and form fields. One of row, column, structure (the structure tree's order, which PDF/UA asks of a page with annotations), annotations or widgets (PDF 2.0). Without it the order is left to the reader."
    )]
    pub async fn set_tab_order(
        &self,
        Parameters(args): Parameters<SetTabOrderArgs>,
    ) -> Result<String, McpError> {
        set_tab_order_impl(args)
    }

    /// Sets the order the form recalculates its calculated fields in.
    #[tool(
        name = "set_calculation_order",
        description = "Sets /CO, the order a form recalculates its calculated fields in when any value changes. Name every field that has a calculation action, by fully qualified name, each once; an order that leaves one out, names one twice, or names a field that calculates nothing is refused by name."
    )]
    pub async fn set_calculation_order(
        &self,
        Parameters(args): Parameters<SetCalculationOrderArgs>,
    ) -> Result<String, McpError> {
        set_calculation_order_impl(args)
    }

    /// Configures drawing measurement scale dictionary (/Measure) for CAD and technical drawings.
    #[tool(
        name = "set_measurement_scale",
        description = "Declares the scale a page is measured in (ISO 32000-2 12.9): a viewport over the whole page whose measure says one unit of the page's user space — a point, unless the page sets /UserUnit — is scale_ratio units, with distances in the unit and areas in its square. Replaces any rectilinear scale the page had; a geospatial one is kept."
    )]
    pub async fn set_measurement_scale(
        &self,
        Parameters(args): Parameters<SetMeasurementScaleArgs>,
    ) -> Result<String, McpError> {
        set_measurement_scale_impl(args)
    }

    /// Fills or updates values in AcroForm interactive form fields.
    ///
    /// The description says the cascade because a caller would otherwise be surprised by
    /// it: setting one field can change several, and those are the document's scripts
    /// deciding, not this server.
    #[tool(
        name = "set_form_field_value",
        description = "Fills or updates values in AcroForm interactive form fields. \
                       If the form declares a calculation order (/CO), the document's own \
                       ECMAScript then runs and may change other fields' values; this is \
                       the only tool that runs it."
    )]
    pub async fn set_form_field_value(
        &self,
        Parameters(args): Parameters<SetFormFieldValueArgs>,
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
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
    ) -> Result<String, McpError> {
        update_article_threads_impl(args)
    }

    /// Sets the action a reader runs when it opens the document (12.6.2).
    #[tool(
        name = "set_open_action",
        description = "Sets the document's /OpenAction, the action a reader runs when it \
                       opens the file: go to another file (gotor), to an embedded file \
                       (gotoe), or a named action (named). Nothing is run here, and any \
                       /OpenAction the document had is replaced."
    )]
    pub async fn set_open_action(
        &self,
        Parameters(args): Parameters<SetOpenActionArgs>,
    ) -> Result<String, McpError> {
        set_open_action_impl(args)
    }

    /// Sets geospatial GIS metadata anchor (/Geo) with coordinates (latitude, longitude) and CRS.
    #[tool(
        name = "set_geospatial_anchor",
        description = "Sets geospatial GIS metadata anchor (/Geo) with coordinates (latitude, longitude) and CRS."
    )]
    pub async fn set_geospatial_anchor(
        &self,
        Parameters(args): Parameters<SetGeospatialAnchorArgs>,
    ) -> Result<String, McpError> {
        set_geospatial_anchor_impl(args)
    }

    /// Makes a document the unencrypted wrapper of an encrypted payload (7.6.7).
    #[tool(
        name = "set_unencrypted_wrapper",
        description = "Makes the document the unencrypted wrapper of an encrypted payload \
                       (ISO 32000-2 7.6.7): a PDF file encrypted by a security handler the \
                       standard does not define. The payload is embedded with AFRelationship \
                       EncryptedPayload and an encrypted payload dictionary naming \
                       crypto_filter (and filter_version), listed in /AF and as the only \
                       /EmbeddedFiles entry, and a hidden /Collection opens it first, so a \
                       reader holding the filter shows the payload and one without it shows \
                       this document. Refused when the document already embeds a file or is \
                       a collection, or the payload has no %PDF- header."
    )]
    pub async fn set_unencrypted_wrapper(
        &self,
        Parameters(args): Parameters<SetUnencryptedWrapperArgs>,
    ) -> Result<String, McpError> {
        set_unencrypted_wrapper_impl(args)
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
    let tools = FepdfServer::tool_router();
    // Counted, not quoted. Each of these three read "24 Operation tools and
    // Resource/Prompt support": the router carried 36, and nothing served a resource or a
    // prompt at all.
    let tool_count = tools.list_all().len();
    let prompt_count = crate::prompts::catalogue().len();
    let template_count = crate::resources::templates().len();
    let router = Router::new(server).with_tools(tools);

    let transport = rmcp::transport::stdio();

    println!(
        "fepdf MCP Server starting on stdio with {tool_count} tools, {prompt_count} prompts \
         and {template_count} resource templates..."
    );
    router.serve(transport).await.map_err(|e| format!("Server error: {e}"))?;

    Ok(())
}
