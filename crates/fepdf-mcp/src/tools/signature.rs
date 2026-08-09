use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, JsonSchema)]
/// Arguments for the verify_signatures tool.
pub struct VerifySignaturesArgs {
    /// Path to the PDF file to verify.
    pub path: String,
    /// Whether to attempt network-based revocation checking (default: false).
    #[serde(default)]
    pub allow_network: bool,
}

#[derive(Serialize)]
/// Represents a report for a single digital signature verification.
pub struct SignatureReport {
    /// The PDF object ID of the signature dictionary.
    pub object_id: u32,
    /// The validation status (Valid, Invalid, etc.).
    pub status: String,
    /// Additional details about the validation status.
    pub details: Option<String>,
}

/// Implementation of the verify_signatures tool.
pub async fn verify_signatures_impl(args: VerifySignaturesArgs) -> Result<String, String> {
    use fepdf_sdk::PdfDocument;
    let path = std::path::Path::new(&args.path);
    let data = std::fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;
    let doc = PdfDocument::open(bytes::Bytes::from(data))
        .map_err(|e| format!("Failed to parse PDF document: {e:?}"))?;

    let arena = doc.inner().arena();
    let mut sig_count = 0;
    let mut details = Vec::new();

    for i in 0..arena.object_count() {
        let handle = fepdf_core::Handle::new(i);
        if let Some(fepdf_core::Object::Dictionary(dh)) = arena.get_object(handle)
            && let Some(dict) = arena.get_dict(dh)
        {
            let type_key = arena.name("Type");
            if let Some(val) = dict.get(&type_key)
                && let Some(name_h) = val.resolve(arena).as_name()
                && let Some(name) = arena.get_name(name_h)
                && name.as_str() == "Sig"
            {
                sig_count += 1;
                let mut detail_str = format!("Signature Object ID: {i}");
                let name_key = arena.name("Name");
                if let Some(n_val) = dict.get(&name_key)
                    && let Some(b) = n_val.resolve(arena).as_string()
                    && let Ok(s) = std::str::from_utf8(b)
                {
                    use std::fmt::Write;
                    let _ = write!(detail_str, " (Signed by: {s})");
                }
                details.push(detail_str);
            }
        }
    }

    if sig_count > 0 {
        Ok(format!(
            "Found {sig_count} digital signature(s) in document.\n\nDetails:\n{}",
            details.join("\n")
        ))
    } else {
        Ok("No digital signatures found in this document.".to_string())
    }
}
