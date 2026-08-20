use super::{render_decisions_markdown, render_decisions_text};

pub fn support_label(s: fepdf::Support) -> &'static str {
    match s {
        fepdf::Support::Modelled => "modelled",
        fepdf::Support::Declared => "declared",
        fepdf::Support::TypeOnly => "type only",
        fepdf::Support::Untyped => "untyped",
    }
}

/// Each entry the file carries: how far the engine goes with it, and — where the entry
/// is itself a table of keys — how much of *that* table its reader covers.
///
/// The second column is the question ADR-0017 asked about the catalogue, asked one level
/// down. A modelled entry reads that entry's scalars; `/AcroForm` leaves `/Fields`,
/// `/CO`, `/DR` and `/XFA` as objects, which is 4 of 8 rather than done.
fn render_catalog_entries(r: &fepdf::CatalogReport) {
    println!("\n--- [ ENTRIES ({}) ] ---", r.entries.len());
    println!("  {:<18} {:<11} {:<9} {:<8} value", "key", "support", "own table", "in 7.7.2");
    for e in &r.entries {
        let inner = e.inner.map_or_else(
            || "—".to_string(),
            |i| format!("{}/{}", i.modelled, i.modelled + i.declared),
        );
        println!(
            "  {:<18} {:<11} {:<9} {:<8} {}",
            e.key,
            support_label(e.support),
            inner,
            if e.standard { "yes" } else { "no" },
            e.value
        );
    }
}

pub fn render_catalog_text(r: &fepdf::CatalogReport, input: &std::path::Path, all: bool) {
    println!("fepdf catalog: {}", input.display());

    let (modelled, declared, type_only, preserved) = r.support_counts();
    render_catalog_entries(r);

    println!("\n--- [ WHAT THE ENGINE CAN DO WITH THEM ] ---");
    println!("  modelled:  {modelled} — a field whose type says what the entry holds");
    println!("  declared:  {declared} — a field typed Object; reachable, contents opaque");
    println!("  type only: {type_only} — a type for its contents exists; no read path");
    println!("  untyped:   {preserved} — round-trips; any handling is ad hoc");
    let (inner_read, inner_total) = r
        .entries
        .iter()
        .filter_map(|e| e.inner)
        .fold((0, 0), |(read, total), i| (read + i.modelled, total + i.modelled + i.declared));
    if inner_total > 0 {
        println!(
            "\n  of the entries above whose own table is a fixed set of keys: {inner_read} of \
             {inner_total} of those keys are read"
        );
    }
    let unmodelled = r.unmodelled();
    if !unmodelled.is_empty() {
        println!(
            "\n  {} of {} entries the engine cannot read the contents of: {}",
            unmodelled.len(),
            r.entries.len(),
            unmodelled.iter().map(|e| e.key.as_str()).collect::<Vec<_>>().join(" ")
        );
    }

    if all {
        println!("\n--- [ TABLE 29 KEYS THIS FILE DOES NOT CARRY ({}) ] ---", r.absent.len());
        for key in &r.absent {
            // The refusal, kept visible where the key is named. Twelve of Table 29's
            // keys occur in no file of either corpus, and each is declined a reader for
            // that reason rather than by oversight (`catalog.rs`).
            if fepdf::CATALOGUE_KEYS_NO_CORPUS_CARRIES.contains(&key.as_str()) {
                println!("  {key:<22} declined — no file of either corpus carries one");
            } else {
                println!("  {key}");
            }
        }
    } else if !r.absent.is_empty() {
        println!("\n  {} Table 29 keys are absent; --all lists them", r.absent.len());
    }

    render_decisions_text(&r.decisions);
}

pub fn render_catalog_markdown(r: &fepdf::CatalogReport, input: &std::path::Path) {
    println!("# Catalogue: {}", input.display());
    println!("\n| Key | Support | In 7.7.2 | Value |");
    println!("| :--- | :--- | :---: | :--- |");
    for e in &r.entries {
        println!(
            "| `{}` | {} | {} | {} |",
            e.key,
            support_label(e.support),
            if e.standard { "yes" } else { "no" },
            e.value
        );
    }
    let (modelled, declared, type_only, preserved) = r.support_counts();
    println!(
        "\nModelled {modelled}, declared {declared}, type only {type_only}, untyped {preserved}."
    );
    println!("\nAbsent Table 29 keys: {}.", r.absent.len());
    render_decisions_markdown(&r.decisions);
}
