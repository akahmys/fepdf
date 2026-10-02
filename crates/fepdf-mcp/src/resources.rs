use crate::McpError;
use bytes::Bytes;
use fepdf::PdfDocument;
use std::fs;

/// Reads the PDF/UA-2 logical structure tree of a local PDF document as JSON.
pub fn read_struct_tree_resource(path: &str) -> Result<String, McpError> {
    let data = fs::read(path).map_err(|e| format!("Failed to read file '{path}': {e}"))?;
    let doc = PdfDocument::open(Bytes::from(data))
        .map_err(|e| McpError::pdf(format!("Failed to open PDF '{path}'"), e))?;

    let tree = doc.extract_struct_tree();

    serde_json::to_string_pretty(&tree).map_err(McpError::from)
}

/// Reads document metadata (XMP and Info) as JSON.
pub fn read_metadata_resource(path: &str) -> Result<String, McpError> {
    let data = fs::read(path).map_err(|e| format!("Failed to read file '{path}': {e}"))?;
    let doc = PdfDocument::open(Bytes::from(data))
        .map_err(|e| McpError::pdf(format!("Failed to open PDF '{path}'"), e))?;

    let summary = doc.get_summary().map_err(|e| McpError::pdf("Failed to inspect document", e))?;

    serde_json::to_string_pretty(&summary.metadata).map_err(McpError::from)
}

/// Reads the compliance audit report of a local PDF document as JSON.
pub fn read_audit_resource(path: &str) -> Result<String, McpError> {
    let data = fs::read(path).map_err(|e| format!("Failed to read file '{path}': {e}"))?;
    let doc = PdfDocument::open(Bytes::from(data))
        .map_err(|e| McpError::pdf(format!("Failed to open PDF '{path}'"), e))?;

    let summary = doc.get_summary().map_err(|e| McpError::pdf("Failed to inspect document", e))?;

    serde_json::to_string_pretty(&summary.compliance).map_err(McpError::from)
}

/// Every resource this server serves names a local file under this prefix.
const LOCAL_PREFIX: &str = "pdf://local/";

/// The three views of a local document, as the URI templates a client lists.
///
/// **These are what makes the module reachable.** The functions above answered correctly
/// from the day they were written and no MCP client could call one: nothing registered a
/// resource, so `resources/list` returned an empty array and `resources/read` returned
/// method-not-found. `prompt_audit_accessibility` had been directing models to
/// `pdf://local/{path}/struct_tree` throughout.
pub(crate) fn templates() -> Vec<rmcp::model::ResourceTemplate> {
    use rmcp::model::AnnotateAble as _;
    [
        ("struct_tree", "The PDF/UA-2 logical structure tree, as JSON."),
        ("metadata", "The document's XMP and /Info metadata, as JSON."),
        ("audit", "The structural and accessibility audit of the document, as JSON."),
    ]
    .into_iter()
    .map(|(kind, description)| {
        rmcp::model::RawResourceTemplate::new(
            format!("{LOCAL_PREFIX}{{path}}/{kind}"),
            format!("Local PDF {kind}"),
        )
        .with_description(description)
        .with_mime_type("application/json")
        .no_annotation()
    })
    .collect()
}

/// Reads the resource `uri` names.
///
/// # Errors
/// Fails when the URI is not one this server serves, or when reading the document does.
pub(crate) fn read(uri: &str) -> Result<String, rmcp::ErrorData> {
    let rest = uri.strip_prefix(LOCAL_PREFIX).ok_or_else(|| {
        rmcp::ErrorData::resource_not_found(
            format!("`{uri}` does not begin with `{LOCAL_PREFIX}`"),
            None,
        )
    })?;
    let (path, kind) = rest.rsplit_once('/').ok_or_else(|| {
        rmcp::ErrorData::resource_not_found(
            format!("`{uri}` names no view: expected `{LOCAL_PREFIX}<path>/<view>`"),
            None,
        )
    })?;
    let body = match kind {
        "struct_tree" => read_struct_tree_resource(path),
        "metadata" => read_metadata_resource(path),
        "audit" => read_audit_resource(path),
        other => {
            return Err(rmcp::ErrorData::resource_not_found(
                format!("`{other}` is not a view this server serves"),
                None,
            ));
        }
    };
    body.map_err(|why| rmcp::ErrorData::internal_error(why.to_string(), None))
}

#[cfg(test)]
mod reachable {
    use super::{LOCAL_PREFIX, read, templates};

    /// **The prompt directs a model to a URI this must serve.**
    ///
    /// `prompt_audit_accessibility` names `pdf://local/{path}/struct_tree`. It named it
    /// while nothing served any resource at all, so the instruction was one no model
    /// could follow. This asserts the two agree on the shape.
    #[test]
    fn the_uri_the_audit_prompt_names_is_one_this_module_serves() {
        let named = crate::prompts::prompt_audit_accessibility("some.pdf");
        let uri = format!("{LOCAL_PREFIX}some.pdf/struct_tree");
        assert!(named.contains(&uri), "the prompt no longer names {uri}:\n{named}");
        assert!(
            templates()
                .iter()
                .any(|t| t.uri_template == format!("{LOCAL_PREFIX}{{path}}/struct_tree")),
            "and nothing lists a template matching it"
        );
    }

    #[test]
    fn every_view_this_module_reads_is_listed_as_a_template() {
        let listed: Vec<String> = templates().iter().map(|t| t.uri_template.clone()).collect();
        for view in ["struct_tree", "metadata", "audit"] {
            let expected = format!("{LOCAL_PREFIX}{{path}}/{view}");
            assert!(listed.contains(&expected), "{view} is read but not listed: {listed:?}");
        }
    }

    #[test]
    fn a_uri_this_module_does_not_serve_is_refused_rather_than_guessed() {
        assert!(read("file:///etc/passwd").is_err(), "a foreign scheme");
        assert!(read(&format!("{LOCAL_PREFIX}some.pdf/no_such_view")).is_err(), "an unknown view");
        assert!(read(&format!("{LOCAL_PREFIX}no-slash-here")).is_err(), "no view named");
    }
}
