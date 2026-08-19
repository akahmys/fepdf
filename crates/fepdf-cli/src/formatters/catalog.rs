use super::{render_decisions_markdown, render_decisions_text};

pub fn support_label(s: fepdf::Support) -> &'static str {
    match s {
        fepdf::Support::Modelled => "modelled",
        fepdf::Support::Declared => "declared",
        fepdf::Support::TypeOnly => "type only",
        fepdf::Support::Untyped => "untyped",
    }
}

pub fn render_catalog_text(r: &fepdf::CatalogReport, input: &std::path::Path, all: bool) {
    println!("fepdf catalog: {}", input.display());

    let (modelled, declared, type_only, preserved) = r.support_counts();
    println!("\n--- [ ENTRIES ({}) ] ---", r.entries.len());
    println!("  {:<18} {:<11} {:<8} value", "key", "support", "in 7.7.2");
    for e in &r.entries {
        println!(
            "  {:<18} {:<11} {:<8} {}",
            e.key,
            support_label(e.support),
            if e.standard { "yes" } else { "no" },
            e.value
        );
    }

    println!("\n--- [ WHAT THE ENGINE CAN DO WITH THEM ] ---");
    println!("  modelled:  {modelled} — a field whose type says what the entry holds");
    println!("  declared:  {declared} — a field typed Object; reachable, contents opaque");
    println!("  type only: {type_only} — a type for its contents exists; no read path");
    println!("  untyped:   {preserved} — round-trips; any handling is ad hoc");
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
            println!("  {key}");
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
