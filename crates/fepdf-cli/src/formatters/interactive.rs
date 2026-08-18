use super::{render_decisions_markdown, render_decisions_text};

pub fn render_interactive_text(r: &fepdf::InteractiveReport, input: &std::path::Path) {
    println!("fepdf interactive: {}", input.display());
    if r.is_empty() {
        println!(
            "\n  nothing interactive: no annotations, fields, outline, actions or destinations"
        );
        render_decisions_text(&r.decisions);
        return;
    }

    render_interactive_annotations(r);
    render_interactive_form(r);
    render_interactive_outline(r);
    render_interactive_destinations(r);

    println!("\n--- [ ACTIONS (12.6) ] ---");
    if r.actions.is_empty() {
        println!("  none");
    }
    for (kind, n) in &r.actions {
        println!("  {kind:<18} {n:>7}");
    }

    render_decisions_text(&r.decisions);
}

pub fn render_interactive_annotations(r: &fepdf::InteractiveReport) {
    println!("\n--- [ ANNOTATIONS (12.5) ] ---");
    if r.annotations.total == 0 {
        println!("  none");
        return;
    }
    println!("  {} across {} of {} pages", r.annotations.total, r.annotations.pages_with, r.pages);
    for (subtype, n) in &r.annotations.by_subtype {
        println!("    {subtype:<18} {n:>7}");
    }
    if r.annotations.without_subtype > 0 {
        println!(
            "    {:<18} {:>7}  /Subtype is required (12.5.2)",
            "(missing)", r.annotations.without_subtype
        );
    }
}

pub fn render_interactive_form(r: &fepdf::InteractiveReport) {
    println!("\n--- [ FORM (12.7) ] ---");
    if !r.form.declared {
        println!("  no /AcroForm");
        return;
    }
    println!("  /AcroForm present, {} terminal fields", r.form.fields);
    for (kind, n) in &r.form.by_type {
        println!("    /FT {kind:<14} {n:>7}");
    }
    if let Some(needs) = r.form.needs_appearances {
        println!("  /NeedAppearances {needs}");
    }
    if r.form.fields == 0 {
        println!("  the form declares no fields, so there is nothing to fill");
    }
}

pub fn render_interactive_outline(r: &fepdf::InteractiveReport) {
    println!("\n--- [ OUTLINE (12.3.3) ] ---");
    if !r.outline.present {
        println!("  no /Outlines");
        return;
    }
    println!("  items in the tree      {}", r.outline.total);
    println!("  visible                {}", r.outline.visible);
    println!("  /Count declares        {}", r.outline.declared_visible);
    if r.outline.count_disagrees() {
        println!("  /Count does not match the visible items it is defined as (12.3.3)");
    }
}

/// Destinations declared and referenced, and the references that reach nothing.
///
/// Declared and referenced are printed as separate lines because they measure different
/// things and the corpus separates them: `volvo_xc90.pdf` declares 651 and references
/// 698, `intel_sdm.pdf` declares 279,501 and references 25,946. A single "destinations"
/// figure would have said neither.
pub fn render_interactive_destinations(r: &fepdf::InteractiveReport) {
    let d = &r.destinations;
    println!("\n--- [ DESTINATIONS (12.3.2) ] ---");
    let declared = d.declared();
    if declared == 0 {
        println!("  none declared");
    } else {
        println!("  declared               {declared}");
        if d.declared_by_name > 0 {
            println!(
                "    by name              {:>7}  catalogue /Dests (PDF 1.1)",
                d.declared_by_name
            );
        }
        if d.declared_by_string > 0 {
            println!(
                "    by string            {:>7}  /Names -> /Dests name tree (PDF 1.2)",
                d.declared_by_string
            );
        }
    }
    println!("  referenced             {}", d.referenced());
    println!("    written in place     {:>7}", d.inline);
    println!("    resolved by name     {:>7}", d.resolved);
    if d.dangling_references > 0 {
        println!("    resolved to nothing  {:>7}", d.dangling_references);
    }
    if d.unreadable > 0 {
        println!("  {:<21} {:>7}  not a destination Table 151 defines", "unreadable", d.unreadable);
    }
    if d.dangling.is_empty() {
        if d.resolved > 0 {
            println!("  every named reference resolves");
        }
    } else {
        println!(
            "\n  {} name{} nothing declares, referenced {} time{} — links that go nowhere:",
            d.dangling.len(),
            if d.dangling.len() == 1 { "" } else { "s" },
            d.dangling_references,
            if d.dangling_references == 1 { "" } else { "s" }
        );
        for name in d.dangling.iter().take(10) {
            println!("    {name}");
        }
        if d.dangling.len() > 10 {
            println!("    ... and {} more", d.dangling.len() - 10);
        }
    }
}

pub fn render_interactive_markdown(r: &fepdf::InteractiveReport, input: &std::path::Path) {
    println!("# Interactive features: {}", input.display());
    println!("\n| Feature | Value |");
    println!("| :--- | :--- |");
    println!("| Pages | {} |", r.pages);
    println!("| Annotations | {} on {} pages |", r.annotations.total, r.annotations.pages_with);
    println!("| Form fields | {} |", r.form.fields);
    println!("| Outline items / visible | {} / {} |", r.outline.total, r.outline.visible);
    println!("| Actions | {} |", r.actions.iter().map(|(_, n)| n).sum::<usize>());
    println!(
        "| Destinations declared / referenced | {} / {} |",
        r.destinations.declared(),
        r.destinations.referenced()
    );
    if !r.destinations.dangling.is_empty() {
        println!(
            "| **Names nothing declares** | {} ({} references) |",
            r.destinations.dangling.len(),
            r.destinations.dangling_references
        );
    }

    if !r.annotations.by_subtype.is_empty() {
        println!("\n## Annotation subtypes\n");
        println!("| Subtype | Count |");
        println!("| :--- | ---: |");
        for (subtype, n) in &r.annotations.by_subtype {
            println!("| `{subtype}` | {n} |");
        }
    }
    if !r.actions.is_empty() {
        println!("\n## Actions\n");
        println!("| Kind | Count |");
        println!("| :--- | ---: |");
        for (kind, n) in &r.actions {
            println!("| `{kind}` | {n} |");
        }
    }
    render_decisions_markdown(&r.decisions);
}
