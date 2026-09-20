//! What the server puts in front of a client, checked rather than read.
//!
//! **`prompts.rs` named `get_structure_tree` for four phases and no such tool has ever
//! existed**, which is the kind of thing only a check catches: a prompt is prose until a
//! client tries to follow it. This walks the router the server actually serves.

use fepdf_mcp::FepdfServer;

/// Every tool this build registers, by name.
fn served() -> Vec<String> {
    FepdfServer::all_tools().list_all().into_iter().map(|t| t.name.to_string()).collect()
}

/// The `render_page` tool is present exactly when the feature that rasterises is.
///
/// It is the only tool that needs a GPU stack — 285 crates in the dependency tree with
/// it, 246 without, measured 2026-09-08 — so a server that answers over stdio and never
/// draws can be built without either.
#[test]
fn render_page_follows_its_feature() {
    let has = served().iter().any(|name| name == "render_page");
    assert_eq!(
        has,
        cfg!(feature = "render"),
        "the tool list and the feature disagree: {:?}",
        served()
    );
}

/// A tool a prompt names has to be one the server registers.
#[test]
fn every_tool_a_prompt_names_is_served() {
    let served = served();
    let prompts = [
        ("audit_accessibility", fepdf_mcp::prompts::prompt_audit_accessibility("f.pdf")),
        ("remediate_pdf_ua", fepdf_mcp::prompts::prompt_remediate_pdf_ua("in.pdf", "out.pdf")),
    ];
    let mut missing = Vec::new();
    for (name, text) in &prompts {
        for word in text.split('`') {
            if word.contains(' ') || word.is_empty() {
                continue;
            }
            if word.ends_with("_tool") || !word.contains('_') {
                continue;
            }
            if !served.contains(&word.to_string()) && looks_like_a_tool_name(word) {
                missing.push(format!("{name}: {word}"));
            }
        }
    }
    assert!(missing.is_empty(), "a prompt names a tool the server does not serve: {missing:?}");
}

/// A backticked word is taken for a tool name when it is `snake_case` and its first word
/// is a verb this server's tools begin with.
fn looks_like_a_tool_name(word: &str) -> bool {
    const VERBS: [&str; 8] = ["get", "set", "list", "read", "add", "remove", "render", "extract"];
    VERBS.iter().any(|v| word.starts_with(&format!("{v}_")))
}

/// Exactly the tools that run the document's ECMAScript say that they do.
///
/// **The cascade ran after every operation until 2026-09-09**, which no description
/// mentioned and which `tests/calculation_scope_test.rs` shows was not harmless: a page
/// rotation overwrote a date field with the deterministic clock's year. It is scoped to
/// `SetFormFieldValue` now, and this holds the descriptions to that scope from the other
/// side — a tool that gains the behaviour without gaining the sentence fails here.
#[test]
fn the_tools_that_run_scripts_are_the_ones_that_say_so() {
    const RUNS_SCRIPTS: [&str; 2] = ["set_form_field_value", "apply_operation"];

    let mut says = Vec::new();
    for tool in FepdfServer::all_tools().list_all() {
        let description = tool.description.as_deref().unwrap_or_default().to_lowercase();
        if description.contains("calculation order") || description.contains("ecmascript") {
            says.push(tool.name.to_string());
        }
    }
    says.sort();
    let mut expected: Vec<String> = RUNS_SCRIPTS.iter().map(|s| (*s).to_string()).collect();
    expected.sort();
    assert_eq!(says, expected, "the descriptions and the implementation disagree about scripts");
}

/// **An operation no frontend can call is one nobody looks at.**
///
/// `AddAnnotation` existed for phases with no caller, and its three defects — no `/AP`,
/// no `/QuadPoints` on a highlight, a stamp that discarded its image — were found the day
/// something finally called it. This server is the frontend `ARCHITECTURE.md` calls the
/// most complete, so a vocabulary entry it cannot reach is the same trap being set again.
///
/// **And a tool that takes a number nobody can obtain is the same trap.** `edit_run` was
/// served for a day with no listing beside it, so a caller had to guess which run it meant
/// — which is why `list_runs` is named here with the three that change a run.
#[test]
fn the_text_editing_operations_are_reachable() {
    let tools = served();
    for wanted in ["list_runs", "edit_run", "split_run", "delete_run"] {
        assert!(tools.iter().any(|name| name == wanted), "no tool reaches `{wanted}`: {tools:?}");
    }
}
