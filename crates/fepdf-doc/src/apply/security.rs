use crate::apply::metadata::{attach_to_catalog, create_embedded_filespec};
use crate::operation::{AFRelationship, UnencryptedWrapperSpec};
use fepdf_model::{Document, Object, PdfResult};

/// Sets unencrypted wrapper payload (Clause 7.6.7).
pub fn apply_set_unencrypted_wrapper(
    doc: &Document,
    wrapper: UnencryptedWrapperSpec,
) -> PdfResult<()> {
    let arena = doc.arena();
    let filespec_h = create_embedded_filespec(
        arena,
        "EncryptedPayload.pdf".to_string(),
        Some("application/pdf".to_string()),
        Some(wrapper.notice_message),
        wrapper.encrypted_payload_bytes.len() as u64,
        wrapper.encrypted_payload_bytes,
        Some(AFRelationship::Unspecified),
    );

    attach_to_catalog(doc, "EncryptedPayload.pdf".to_string(), filespec_h)
}

/// Writes a Document Security Store (`/DSS`, 12.8.4.3) into the catalogue.
///
/// Moved out of the facade unchanged. It had no caller and no test there, and it still
/// has no test: what this writes has never been read back by anything, and `/DSS` occurs
/// in none of the 524 corpus files, so nothing has ever checked the shape against a real
/// one. ROADMAP's P3 is where that gets settled; this is the part of it that exists.
pub fn apply_add_ltv_info(doc: &mut Document, certificates: Vec<Vec<u8>>) -> PdfResult<()> {
    let arena = doc.arena();
    let mut dss_dict = std::collections::BTreeMap::new();

    let mut cert_refs = Vec::new();
    for cert_data in certificates {
        let mut stream_dict = std::collections::BTreeMap::new();
        #[allow(clippy::cast_possible_wrap)]
        stream_dict.insert(arena.name("Length"), Object::Integer(cert_data.len() as i64));
        let stream_h = arena.alloc_dict(stream_dict);
        let stream_ref = arena.alloc_object(Object::Stream(
            stream_h,
            std::sync::Arc::new(fepdf_model::object::SublimatedData::Raw(bytes::Bytes::from(
                cert_data,
            ))),
        ));
        cert_refs.push(Object::Reference(stream_ref));
    }

    if !cert_refs.is_empty() {
        dss_dict.insert(arena.name("Certs"), Object::Array(arena.alloc_array(cert_refs)));
    }

    if let Some(catalog_handle) = doc.catalog_handle() {
        let dh = doc.resolve_to_dict(catalog_handle)?;
        let mut catalog = arena.get_dict(dh).unwrap_or_default();
        catalog.insert(arena.name("DSS"), Object::Dictionary(arena.alloc_dict(dss_dict)));
        arena.set_dict(dh, catalog);
    }
    Ok(())
}
