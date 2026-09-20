//! Dispatcher and domain modules for applying operations to documents.

/// Annotation, form field, action, and decoration operation handlers.
pub mod annotations;
/// Building a field's appearance from its value (12.7.4.3).
pub mod appearance;
/// Putting a font program into a document.
pub mod font;
/// Portfolio, outline, layer, associated file, and metadata operation handlers.
pub mod metadata;
/// Page rotation, reordering, removal, and page label operation handlers.
pub mod page;
/// Security, unencrypted wrapper, and public-key recipient operation handlers.
pub mod security;
/// Structure element and article thread operation handlers.
pub mod structure;
/// Changing the text a page already draws.
pub mod text;

use crate::operation::Operation;
use fepdf_model::{Document, PdfResult};

/// Applies a canonical mutation operation to the document model.
pub fn apply_operation(doc: &mut Document, op: Operation) -> PdfResult<()> {
    // RR-15 Limit: Dispatcher - the vocabulary's one routing table, exhaustive by Rule 5
    //
    // Thirty-two arms, each a name and where it goes. It passed fifty when `ResizePages`
    // was added, and the fix is not to split it: half a table is half an answer to "what
    // can this engine do", and the halves would need a wildcard arm between them — which
    // Rule 5 forbids over a domain enum, and rightly, since that arm is where a variant
    // goes to be silently ignored.
    //
    // An edit can rewrite the object a resolved colour space was parsed from, and the
    // cache is keyed by that object's arena handle.
    doc.forget_color_spaces();
    match op {
        Operation::Rotate { pages, mode } => page::apply_rotate(doc, &pages, &mode),
        Operation::Reorder { from, to } => page::apply_reorder(doc, from, to),
        Operation::ReorderBatch { sources, target } => {
            page::apply_reorder_batch(doc, &sources, target).map(|_| ())
        }
        Operation::DuplicatePages(pages) => page::apply_duplicate_pages(doc, &pages),
        Operation::ResizePages(pages, to) => page::apply_resize_pages(doc, &pages, &to),
        Operation::InsertFrom { source, at } => {
            page::apply_insert_from(doc, &source, at).map(|_| ())
        }
        Operation::AddLtvInfo { certificates } => security::apply_add_ltv_info(doc, certificates),
        Operation::Retag => crate::remediation::retag(doc),
        Operation::Upgrade { standard } => page::apply_upgrade(doc, standard),
        Operation::RemovePages(pages) => page::apply_remove_pages(doc, &pages),
        Operation::SetPageLabels(labels) => page::apply_set_page_labels(doc, labels),
        Operation::UpdateStructElem(u) => structure::apply_update_struct(doc, u),
        Operation::DeleteStructElem { handle_index } => {
            structure::apply_delete_struct(doc, handle_index)
        }
        Operation::MoveStructElem(m) => structure::apply_move_struct(doc, m),
        Operation::UpdateArticleThreads(t) => structure::apply_update_article_threads(doc, t),
        Operation::AddUserProperties { target_handle, properties } => {
            structure::apply_add_user_properties(doc, target_handle, properties)
        }
        Operation::CreatePortfolio(p) => metadata::apply_create_portfolio(doc, p),
        Operation::UpdateOutlines(o) => metadata::apply_update_outlines(doc, o),
        Operation::UpdateLayers(l) => metadata::apply_update_layers(doc, l),
        Operation::AttachAssociatedFile(f) => metadata::apply_attach_associated_file(doc, f),
        Operation::SetOutputIntent(i) => metadata::apply_set_output_intent(doc, i),
        Operation::SetPronunciationLexicon { lexicon_xml_bytes } => {
            metadata::apply_set_pronunciation_lexicon(doc, lexicon_xml_bytes)
        }
        Operation::AddPageDecoration { pages, text, position, layer } => {
            annotations::apply_add_page_decoration(doc, &pages, &text, &position, layer.as_deref())
        }
        Operation::ApplyBatesNumbering { pages, prefix, start_number, digits, position } => {
            annotations::apply_bates(doc, &pages, &prefix, start_number, digits, &position)
        }
        Operation::AddAnnotation(a) => annotations::apply_add_annotation(doc, a),
        Operation::EditRun { page, run, text: to } => text::apply_edit_run(doc, page, run, &to),
        Operation::SplitRun { page, run, after } => text::apply_split_run(doc, page, run, after),
        Operation::DeleteRun { page, run } => text::apply_delete_run(doc, page, run),
        Operation::MergeRuns { page, run } => text::apply_merge_runs(doc, page, run),
        Operation::MoveRun { page, run, to } => text::apply_move_run(doc, page, run, to),
        Operation::RemoveOutside { page, keep } => text::apply_remove_outside(doc, page, keep),
        Operation::SetMeasurementScale(s) => annotations::apply_set_measurement_scale(doc, s),
        Operation::SetFormFieldValue(f) => annotations::apply_set_form_field_value(doc, f),
        Operation::ExecuteAction(a) => annotations::apply_execute_action(doc, a),
        Operation::SetGeospatialAnchor(a) => annotations::apply_set_geospatial_anchor(doc, a),
        Operation::AddMeshShading(s) => annotations::apply_add_mesh_shading(doc, s),
        Operation::SetUnencryptedWrapper(w) => security::apply_set_unencrypted_wrapper(doc, w),
        Operation::AddPublicKeyRecipient(r) => security::apply_add_public_key_recipient(doc, r),
    }
}
