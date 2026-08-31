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
    if r.annotations.without_subtype > 0 {
        println!("  {} carry no /Subtype, which 12.5.2 requires", r.annotations.without_subtype);
    }

    // Per subtype, the entries the file writes and whether they were read. Counting
    // annotations by subtype was the whole report until Phase J, and it could not
    // distinguish a `/Redact` from a `/Watermark` by anything but the name.
    for sub in &r.annotations.subtypes {
        println!(
            "\n  /{:<16} {:>7}   {} of {} entries read",
            sub.subtype,
            sub.count,
            sub.read(),
            sub.entries.len()
        );
        let unread: Vec<&fepdf::AnnotationEntry> = sub.entries.iter().filter(|e| !e.read).collect();
        let read: Vec<&fepdf::AnnotationEntry> = sub.entries.iter().filter(|e| e.read).collect();
        if !read.is_empty() {
            println!("      read      {}", join_entries(&read));
        }
        if !unread.is_empty() {
            println!("      NOT read  {}", join_entries(&unread));
        }
    }
    let unread = r.annotations.unread_entries();
    if unread > 0 {
        println!("\n  {unread} distinct entries across all subtypes have no reader");
    }
    if r.annotations.unreadable > 0 {
        println!(
            "  {} annotations did not parse into the common entries of Table 166",
            r.annotations.unreadable
        );
        if let Some(why) = &r.annotations.first_failure {
            println!("      first: {why}");
        }
    }
}

/// `/Key×n` for each entry, so the count that makes an entry worth reading is visible
/// beside its name rather than in a separate table.
fn join_entries(entries: &[&fepdf::AnnotationEntry]) -> String {
    entries.iter().map(|e| format!("/{}×{}", e.key, e.annotations)).collect::<Vec<_>>().join(" ")
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
    println!(
        "  form /DA {}   form /DR {}",
        if r.form.has_default_appearance { "yes" } else { "no" },
        if r.form.has_default_resources { "yes" } else { "no" }
    );
    if r.form.too_deep > 0 {
        println!("  {} fields nest deeper than the walk descends", r.form.too_deep);
    }
    if r.form.fields == 0 {
        println!("  the form declares no fields, so there is nothing to fill");
        return;
    }

    println!("\n  {:<28} {:<5} {:<10} /V", "field (12.7.4.2)", "/FT", "/Ff");
    for f in r.form.terminal.iter().take(50) {
        render_terminal_field(f, r.form.has_default_appearance);
    }
    if r.form.terminal.len() > 50 {
        println!("  … and {} more", r.form.terminal.len() - 50);
    }
}

fn render_terminal_field(f: &fepdf::FormField, form_has_da: bool) {
    println!(
        "  {:<28} {:<5} {:<10} {}",
        f.qualified_name.as_deref().unwrap_or("(unnamed)"),
        f.field_type.as_deref().unwrap_or("—"),
        f.flags.map_or_else(|| "—".to_string(), |n| n.to_string()),
        f.value.as_deref().unwrap_or("(unset)")
    );
    // A variable-text field with no /DA anywhere has no way to draw its own value
    // (12.7.4.3); reported per field because the form's /DA can supply it.
    if !f.has_default_appearance && !form_has_da {
        println!("      no /DA on the field or the form (12.7.4.3)");
    }
    if f.field_type.as_deref() == Some("Ch") && !f.options.is_empty() {
        let opt_str = f
            .options
            .iter()
            .take(5)
            .map(|opt| {
                if opt.export_value == opt.display_value {
                    opt.export_value.clone()
                } else {
                    format!("{} ({})", opt.export_value, opt.display_value)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let more = if f.options.len() > 5 {
            format!(", +{} more", f.options.len() - 5)
        } else {
            String::new()
        };
        println!("      options ({}) [{opt_str}{more}]", f.options.len());
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

    if !r.annotations.subtypes.is_empty() {
        println!("\n## Annotation subtypes\n");
        println!("| Subtype | Count | Entries read | With no reader |");
        println!("| :--- | ---: | ---: | :--- |");
        for sub in &r.annotations.subtypes {
            let unread: Vec<String> =
                sub.entries.iter().filter(|e| !e.read).map(|e| format!("`/{}`", e.key)).collect();
            println!(
                "| `{}` | {} | {} of {} | {} |",
                sub.subtype,
                sub.count,
                sub.read(),
                sub.entries.len(),
                if unread.is_empty() { "—".to_string() } else { unread.join(" ") }
            );
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
