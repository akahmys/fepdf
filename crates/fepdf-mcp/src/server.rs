//! MCP Server implementation and tool routing for fepdf.

#![allow(missing_docs)]

use crate::tools::operations::vocabulary::{
    AddLtvInfoArgs, DuplicatePagesArgs, InsertFromArgs, ReorderBatchArgs, RetagArgs, UpgradeArgs,
    add_ltv_info_impl, duplicate_pages_impl, insert_from_impl, reorder_batch_impl, retag_impl,
    upgrade_impl,
};
use crate::tools::{
    AddAnnotationArgs, AddFormFieldArgs, AddMeshShadingArgs, AddPageDecorationArgs,
    AddPublicKeyRecipientArgs, AddUserPropertiesArgs, ApplyBatesNumberingArgs, ApplyOperationArgs,
    AttachAssociatedFileArgs, AuditArgs, CombinePagesArgs, CreatePortfolioArgs, CropPagesArgs,
    DeleteRunArgs, DeleteStructElemArgs, EditRunArgs, ExecuteActionArgs, ExtractTextArgs,
    ListRunsArgs, MergeRunsArgs, MoveRunArgs, MoveStructElemArgs, RedactDocumentArgs,
    RemovePagesArgs, ReorderPagesArgs, RotatePagesArgs, SetFormFieldValueArgs,
    SetGeospatialAnchorArgs, SetMeasurementScaleArgs, SetOutputIntentArgs, SetPageLabelsArgs,
    SetPronunciationLexiconArgs, SetUnencryptedWrapperArgs, SplitPageArgs, SplitRunArgs,
    UpdateArticleThreadsArgs, UpdateLayersArgs, UpdateOutlinesArgs, UpdateStructElemArgs,
    VerifySignaturesArgs, add_annotation_impl, add_form_field_impl, add_mesh_shading_impl,
    add_page_decoration_impl, add_public_key_recipient_impl, add_user_properties_impl,
    apply_bates_numbering_impl, apply_operation_impl, apply_redaction_impl,
    attach_associated_file_impl, audit_document_impl, combine_pages_impl, create_portfolio_impl,
    crop_pages_impl, delete_run_impl, delete_struct_elem_impl, edit_run_impl, execute_action_impl,
    extract_text_impl, list_runs_impl, merge_runs_impl, move_run_impl, move_struct_elem_impl,
    remove_pages_impl, reorder_pages_impl, rotate_pages_impl, set_form_field_value_impl,
    set_geospatial_anchor_impl, set_measurement_scale_impl, set_output_intent_impl,
    set_page_labels_impl, set_pronunciation_lexicon_impl, set_unencrypted_wrapper_impl,
    split_page_impl, split_run_impl, update_article_threads_impl, update_layers_impl,
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
        #[cfg(feature = "render")]
        router.merge(Self::render_tool_router());
        router
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
    ) -> Result<String, String> {
        crate::tools::render::render_page_impl(args)
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
        description = "Checks every digital signature a PDF carries: whether it verifies, who the certificate says signed it, and how much of the file the signature covers. No certificate chain is built to a root and no revocation list is checked."
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
        description = "Applies any canonical fepdf Operation in JSON format to mutate a PDF \
                       document. A SetFormFieldValue operation also runs the form's \
                       calculation order, as set_form_field_value does; no other operation \
                       runs the document's scripts."
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
    ) -> Result<String, String> {
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
    ) -> Result<String, String> {
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
    ) -> Result<String, String> {
        list_runs_impl(args)
    }

    /// Cuts one run in two, so that either half can be named afterwards.
    #[tool(
        name = "split_run",
        description = "Cuts one run in two after a given number of its characters. The page draws exactly what it drew before — consecutive show-text operators draw from the current point — and afterwards a caller can name either half."
    )]
    pub async fn split_run(
        &self,
        Parameters(args): Parameters<SplitRunArgs>,
    ) -> Result<String, String> {
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
    ) -> Result<String, String> {
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
    ) -> Result<String, String> {
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
    ) -> Result<String, String> {
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
    ) -> Result<String, String> {
        crop_pages_impl(args)
    }

    /// Cuts one page into several.
    #[tool(
        name = "split_page",
        description = "Cuts one page into several, as an even grid of columns and rows or as regions named outright. A grid comes out in reading order: across a row first, then down. What belongs to the other sheets is taken out of the file rather than hidden — half a drawing left behind is still searchable on a page showing the other half."
    )]
    pub async fn split_page(
        &self,
        Parameters(args): Parameters<SplitPageArgs>,
    ) -> Result<String, String> {
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
    ) -> Result<String, String> {
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
    ) -> Result<String, String> {
        add_form_field_impl(args)
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
