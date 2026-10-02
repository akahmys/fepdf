//! Digital signature verification tool.

use crate::McpError;
use schemars::JsonSchema;
use serde::Deserialize;
use std::fmt::Write as _;

#[derive(Deserialize, JsonSchema)]
/// Arguments for the verify_signatures tool.
pub struct VerifySignaturesArgs {
    /// Path to the PDF file to verify.
    pub path: String,
}

/// Implementation of the verify_signatures tool.
///
/// **This reached `list_signatures`, which enumerates signatures and checks none of
/// them**, while the tool told its clients it performed "integrity checks (MD5/SHA) and
/// signer certificate validation". `fepdf-cli` had reached `SignatureReport::survey` for
/// the same question all along: two frontends implementing one operation separately,
/// which is the drift [`CODING.md`'s Rule D](../../../../CODING.md) exists to stop and
/// ADR-0005 records happening once before.
///
/// It also took an `allow_network` argument, offering revocation checking in the tool's
/// schema. Nothing read it, and nothing could have — `fepdf-syntax`'s `cms` module states
/// that it builds no chain to a root and checks no revocation list. A client could ask
/// for it and be told nothing, so the argument is gone and the answer says so instead.
pub fn verify_signatures_impl(args: VerifySignaturesArgs) -> Result<String, McpError> {
    let data = std::fs::read(&args.path).map_err(|e| format!("Failed to read file: {e}"))?;
    let report = fepdf::SignatureReport::survey(&data)
        .map_err(|e| McpError::pdf("Failed to parse PDF document", e))?;

    let mut out = if report.signatures.is_empty() {
        String::from("No digital signatures found in this document.")
    } else {
        format!("Found {} digital signature(s).", report.signatures.len())
    };
    let _ = write!(out, " {} signature field(s) carry none.", report.unsigned_fields);

    for (n, check) in report.signatures.iter().enumerate() {
        let field = check.field.clone().unwrap_or_else(|| format!("(unnamed field {})", n + 1));
        match &check.refused {
            None => {
                let _ = write!(out, "\n\n{field}: verifies");
            }
            Some(why) => {
                let _ = write!(out, "\n\n{field}: REFUSED - {why}");
            }
        }
        if let Some(signer) = &check.signer {
            let _ = write!(out, "\n  signer: {signer}");
        }
        if let Some(sub_filter) = &check.sub_filter {
            let _ = write!(out, "\n  /SubFilter: {sub_filter}");
        }
        if let Some(at) = &check.signed_at {
            let _ = write!(out, "\n  /M: {at} (the document's word, not a fact)");
        }
        let (covered, total) = check.covered;
        if check.covers_whole_file {
            let _ = write!(out, "\n  covers: the whole file, {covered} of {total} bytes");
        } else {
            let _ = write!(out, "\n  covers: {covered} of {total} bytes - NOT the whole file");
        }
    }

    if !report.signatures.is_empty() {
        out.push_str(
            "\n\nNo certificate chain was built to a root, and no revocation list was checked.",
        );
    }
    Ok(out)
}
