//! Dispatcher and domain modules for applying operations to documents.

/// Annotation, form field, action, and decoration operation handlers.
pub mod annotations;
/// Portfolio, outline, layer, associated file, and metadata operation handlers.
pub mod metadata;
/// Page rotation, reordering, removal, and page label operation handlers.
pub mod page;
/// Security, unencrypted wrapper, and public-key recipient operation handlers.
pub mod security;
/// Structure element and article thread operation handlers.
pub mod structure;

use crate::operation::Operation;
use fepdf_model::{Document, PdfResult};

/// Applies a canonical mutation operation to the document model.
pub fn apply_operation(doc: &mut Document, op: Operation) -> PdfResult<()> {
    match op {
        Operation::Rotate { pages, mode } => page::apply_rotate(doc, &pages, &mode),
        Operation::Reorder { from, to } => page::apply_reorder(doc, from, to),
        Operation::RemovePages(pages) => page::apply_remove_pages(doc, &pages),
        Operation::SetPageLabels(labels) => page::apply_set_page_labels(doc, labels),
        Operation::UpdateStructElem(u) => structure::apply_update_struct(doc, u),
        Operation::DeleteStructElem { handle_index } => {
            structure::apply_delete_struct(doc, handle_index)
        }
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
        Operation::SetMeasurementScale(s) => annotations::apply_set_measurement_scale(doc, s),
        Operation::SetFormFieldValue(f) => annotations::apply_set_form_field_value(doc, f),
        Operation::ExecuteAction(a) => annotations::apply_execute_action(doc, a),
        Operation::SetGeospatialAnchor(a) => annotations::apply_set_geospatial_anchor(doc, a),
        Operation::AddMeshShading(s) => annotations::apply_add_mesh_shading(doc, s),
        Operation::SetUnencryptedWrapper(w) => security::apply_set_unencrypted_wrapper(doc, w),
        Operation::AddPublicKeyRecipient(r) => security::apply_add_public_key_recipient(doc, r),
    }
}
