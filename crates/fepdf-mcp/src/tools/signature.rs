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

    let signatures = doc.list_signatures();
    let sig_count = signatures.len();
    let mut details = Vec::new();

    for sig in signatures {
        let mut detail_str = format!("Signature Object ID: {}", sig.object_id);
        if let Some(ref s) = sig.signer_name {
            use std::fmt::Write;
            let _ = write!(detail_str, " (Signed by: {s})");
        }
        details.push(detail_str);
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
