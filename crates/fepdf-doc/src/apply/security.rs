use crate::apply::metadata::{add_embedded_files_to_catalog, create_embedded_filespec};
use crate::operation::{AFRelationship, PublicKeyRecipientSpec, UnencryptedWrapperSpec};
use bytes::Bytes;
use fepdf_model::{Document, Object, PdfResult};
use std::collections::BTreeMap;

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

    if let Some(cah) = doc.catalog_handle() {
        let cadh = doc.resolve_to_dict(cah)?;
        let mut cdict = arena.get_dict(cadh).unwrap_or_default();
        let af_key = arena.name("AF");
        let mut af_items = if let Some(existing_af) = cdict.get(&af_key) {
            match existing_af {
                Object::Array(ah) => arena.get_array(*ah).unwrap_or_default(),
                Object::Reference(h) => {
                    if let Some(Object::Array(ah)) = arena.get_object(*h) {
                        arena.get_array(ah).unwrap_or_default()
                    } else {
                        Vec::new()
                    }
                }
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        af_items.push(Object::Reference(filespec_h));
        let new_af_ah = arena.alloc_array(af_items);
        cdict.insert(af_key, Object::Array(new_af_ah));
        arena.set_dict(cadh, cdict);

        add_embedded_files_to_catalog(doc, vec![("EncryptedPayload.pdf".to_string(), filespec_h)])?;
    }
    Ok(())
}

/// Adds a public key recipient to Adobe.PubSec (Clause 7.6.5).
pub fn apply_add_public_key_recipient(
    doc: &Document,
    recipient: PublicKeyRecipientSpec,
) -> PdfResult<()> {
    let arena = doc.arena();
    if let Some(cah) = doc.catalog_handle() {
        let cadh = doc.resolve_to_dict(cah)?;
        let mut cdict = arena.get_dict(cadh).unwrap_or_default();
        let enc_key = arena.name("Encrypt");

        let enc_dh = if let Some(enc_obj) = cdict.get(&enc_key)
            && let Some(dh) = enc_obj.as_dict_handle()
        {
            dh
        } else {
            let mut ed = BTreeMap::new();
            ed.insert(arena.name("Filter"), Object::Name(arena.name("Adobe.PubSec")));
            ed.insert(arena.name("V"), Object::Integer(4));
            ed.insert(arena.name("R"), Object::Integer(4));
            let dh = arena.alloc_dict(ed);
            let eh = arena.alloc_object(Object::Dictionary(dh));
            cdict.insert(enc_key, Object::Reference(eh));
            dh
        };

        let mut enc_dict = arena.get_dict(enc_dh).unwrap_or_default();
        let rec_key = arena.name("Recipients");
        let mut rec_items = if let Some(existing_rec) = enc_dict.get(&rec_key) {
            match existing_rec {
                Object::Array(ah) => arena.get_array(*ah).unwrap_or_default(),
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        rec_items.push(Object::String(Bytes::from(recipient.certificate_der_bytes)));
        let rec_ah = arena.alloc_array(rec_items);
        enc_dict.insert(rec_key, Object::Array(rec_ah));
        arena.set_dict(enc_dh, enc_dict);
        arena.set_dict(cadh, cdict);
    }
    Ok(())
}
