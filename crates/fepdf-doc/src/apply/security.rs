use crate::apply::metadata::{attach_to_catalog, create_embedded_filespec};
use crate::operation::{AFRelationship, UnencryptedWrapperSpec};
use bytes::Bytes;
use fepdf_model::{Document, Object, PdfError, PdfResult};
use std::collections::BTreeMap;

/// Makes this document the unencrypted wrapper of an encrypted payload (7.6.7).
///
/// **All that the clause's `shall`s ask, and nothing short of it.** The payload is embedded
/// with `/AFRelationship /EncryptedPayload` and an encrypted payload dictionary (Table 28)
/// naming its cryptographic filter; it is listed in the catalogue's `/AF` and in the
/// `/EmbeddedFiles` name tree, which then holds it alone; and a `/Collection` makes it the
/// initial document with the view hidden, so a reader holding the filter opens the
/// payload and one without it shows this document.
///
/// This embedded the payload as `EncryptedPayload.pdf` with `/AFRelationship
/// /Unspecified`, with no encrypted payload dictionary and no collection, and so made
/// none of that true (ROADMAP Y-1c).
///
/// # Errors
/// Refuses, naming what 7.6.7 or Table 28 asks, when the payload has no `%PDF-` header,
/// the filter is not a name or the version not integers with periods between them, or
/// the document already embeds a file or is already a collection.
pub fn apply_set_unencrypted_wrapper(
    doc: &Document,
    wrapper: UnencryptedWrapperSpec,
) -> PdfResult<()> {
    refuse_unwrappable(doc, &wrapper)?;
    let arena = doc.arena();
    let filespec_h = create_embedded_filespec(
        arena,
        wrapper.payload_name.clone(),
        Some("application/pdf".to_string()),
        Some(wrapper.notice_message),
        wrapper.encrypted_payload_bytes.len() as u64,
        wrapper.encrypted_payload_bytes,
        Some(AFRelationship::EncryptedPayload),
    );

    // Table 28: the filter by name, and its version where one is given.
    let mut payload = BTreeMap::new();
    payload.insert(arena.name("Type"), Object::Name(arena.name("EncryptedPayload")));
    payload.insert(arena.name("Subtype"), Object::Name(arena.name(&wrapper.crypto_filter)));
    if let Some(version) = &wrapper.filter_version {
        payload.insert(arena.name("Version"), Object::Name(arena.name(version)));
    }
    let payload_dh = arena.alloc_dict(payload);
    if let Some(Object::Dictionary(spec_dh)) = arena.get_object(filespec_h) {
        let mut spec = arena.get_dict(spec_dh).unwrap_or_default();
        spec.insert(arena.name("EP"), Object::Dictionary(payload_dh));
        arena.set_dict(spec_dh, spec);
    }
    attach_to_catalog(doc, wrapper.payload_name.clone(), filespec_h)?;

    // The collection: the payload first, and nothing of the collection shown.
    let mut collection = BTreeMap::new();
    collection.insert(arena.name("Type"), Object::Name(arena.name("Collection")));
    collection.insert(arena.name("View"), Object::Name(arena.name("H")));
    // A byte string, equal to the payload's key in the `/EmbeddedFiles` name tree.
    collection.insert(arena.name("D"), Object::String(Bytes::from(wrapper.payload_name)));
    let collection_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(collection)));
    let Some(catalog_h) = doc.catalog_handle() else {
        return Err(PdfError::violation(
            "7.7.2",
            "the document has no catalogue to name the collection in",
        ));
    };
    let catalog_dh = doc.resolve_to_dict(catalog_h)?;
    let mut catalog = arena.get_dict(catalog_dh).unwrap_or_default();
    catalog.insert(arena.name("Collection"), Object::Reference(collection_h));
    arena.set_dict(catalog_dh, catalog);
    Ok(())
}

/// What 7.6.7 and Table 28 rule out, said before anything is written.
fn refuse_unwrappable(doc: &Document, wrapper: &UnencryptedWrapperSpec) -> PdfResult<()> {
    let refuse = |why: String| Err(PdfError::refused("SetUnencryptedWrapper", why));
    let head = &wrapper.encrypted_payload_bytes[..wrapper.encrypted_payload_bytes.len().min(1024)];
    if !head.windows(5).any(|w| w == b"%PDF-") {
        return refuse(
            "7.6.7: the encrypted payload is a PDF file, and these bytes carry no %PDF- header"
                .into(),
        );
    }
    let is_name = |s: &str| {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_graphic() && !b"()<>[]{}/%#".contains(&b))
    };
    if !is_name(&wrapper.crypto_filter) {
        return refuse(format!(
            "Table 28: /Subtype names the cryptographic filter, and {:?} is not a name",
            wrapper.crypto_filter
        ));
    }
    if let Some(version) = &wrapper.filter_version {
        let numbered = version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
        if !numbered {
            return refuse(format!(
                "Table 28: /Version is integers with a period between them, and {version:?} is not"
            ));
        }
    }
    if wrapper.payload_name.is_empty() {
        return refuse(
            "7.6.7: the payload is named in the /EmbeddedFiles name tree, and no name was given"
                .into(),
        );
    }
    let catalog = doc.catalog()?;
    let embedded = catalog
        .names
        .as_ref()
        .and_then(|names| names.trees.iter().find(|tree| tree.key == "EmbeddedFiles"))
        .map_or(0, |tree| tree.names);
    if embedded > 0 {
        return refuse(format!(
            "7.6.7: a wrapper's /EmbeddedFiles name tree holds exactly one entry, the payload, and this document already embeds {embedded}"
        ));
    }
    if catalog.collection.is_some() {
        return refuse("7.6.7: a wrapper's /Collection names the payload, and this document is already a collection".into());
    }
    Ok(())
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
