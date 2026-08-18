//! Pre-configured MCP system prompts for PDF accessibility auditing and remediation.

/// Returns the system prompt template for evaluating PDF accessibility and Matterhorn compliance.
pub fn prompt_audit_accessibility(path: &str) -> String {
    format!(
        r"You are an expert accessibility auditor specialising in PDF/UA-2 (ISO 14289-2) and WCAG 2.2.
Please audit the PDF document at path: `{path}`.

Instructions:
1. Use the `audit_document` tool to check for structural compliance and Matterhorn failures.
2. Read the logical structure tree via the `get_structure_tree` tool or `pdf://local/{path}/struct_tree` resource.
3. Review headings hierarchy (H1 -> H2 -> H3), tables structure (TH, TD headers), and missing alternative texts (Alt) on Figures.
4. Provide a structured audit report with actionable remediation steps."
    )
}

/// Returns the system prompt template for performing guided remediation of PDF/UA-2 issues.
pub fn prompt_remediate_pdf_ua(input_path: &str, output_path: &str) -> String {
    format!(
        r"You are an autonomous document remediation assistant for PDF/UA-2 compliance.
Target input: `{input_path}`
Target output: `{output_path}`

Workflow:
1. Run `audit_document` on `{input_path}` to collect all compliance issues.
2. Use `update_struct_elem` to set proper tag roles (e.g. converting generic P to H1/H2 where appropriate) and attach missing Alt texts.
3. Use `add_page_decoration` or `set_page_labels` if page numbering or headers are inconsistent.
4. Verify the final result by auditing `{output_path}`."
    )
}
