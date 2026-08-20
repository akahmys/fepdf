use super::render_decisions_text;

pub fn render_structure_text(s: &fepdf::FileStructure, input: &std::path::Path) {
    println!("fepdf structure: {}", input.display());

    println!("\n--- [ FILE ] ---");
    println!("PDF version:      {}", s.version);
    println!("Size:             {} bytes", s.size);
    if s.header_offset == 0 {
        println!("Header:           at offset 0");
    } else {
        println!("Header:           at offset {} — bytes precede %PDF- (7.5.2)", s.header_offset);
    }
    println!("Encrypted:        {}", if s.encrypted { "yes (7.6)" } else { "no" });
    println!("Declares /Root:   {}", if s.declares_root { "yes" } else { "no" });

    render_structure_revisions(s);
    render_structure_objects(s);
    render_structure_filters(s);
    render_structure_decisions(s);
}

/// Which filters the file's streams name (7.4), and whether this engine decodes them.
///
/// Reported here rather than left to a text search, because a text search cannot find
/// them: the name usually sits inside a compressed object stream. `on images` is the
/// column that settles what a missing codec costs — a filter that only ever appears
/// there holds no text, so building it would add pixels and nothing else.
pub fn render_structure_filters(s: &fepdf::FileStructure) {
    if s.filters.is_empty() {
        return;
    }
    println!("\n--- [ FILTERS (7.4) ] ---");
    println!("  {:<20} {:>8} {:>10}  decoded", "filter", "streams", "on images");
    for f in &s.filters {
        println!(
            "  /{:<19} {:>8} {:>10}  {}",
            f.name,
            f.streams,
            f.on_images,
            if f.decoded { "yes" } else { "NO" }
        );
    }
}

pub fn render_structure_revisions(s: &fepdf::FileStructure) {
    println!("\n--- [ REVISIONS (7.5.6) ] ---");
    if s.revisions.is_empty() {
        println!("  none readable — the cross-reference was reconstructed by scanning");
        return;
    }
    println!("  {:<4} {:>12} {:>9} {:>8}  form", "#", "offset", "entries", "trailer");
    for r in &s.revisions {
        println!(
            "  {:<4} {:>12} {:>9} {:>8}  {}",
            r.index,
            r.offset,
            r.entries,
            if r.has_trailer { "yes" } else { "-" },
            r.form
        );
    }
    if s.revisions.len() > 1 {
        println!("  {} object numbers are defined by more than one revision", s.superseded);
    }
}

pub fn render_structure_objects(s: &fepdf::FileStructure) {
    println!("\n--- [ OBJECTS ] ---");
    if s.objects.from_scan {
        println!("Source:           recovered by scanning; the cross-reference gave nothing");
    }
    println!("Highest number:   {}", s.objects.highest_number);
    println!("Written in file:  {}", s.objects.in_file);
    println!("In object stream: {} (7.5.7)", s.objects.in_object_stream);
    println!("Free:             {}", s.objects.free);

    if s.object_streams.is_empty() {
        return;
    }
    println!("\n--- [ OBJECT STREAMS ] ---");
    println!("{} containers, largest first:", s.object_streams.len());
    for c in s.object_streams.iter().take(10) {
        println!("  object {:>7} carries {:>7}", c.container, c.carries);
    }
    if s.object_streams.len() > 10 {
        println!("  … and {} more", s.object_streams.len() - 10);
    }
}

pub fn render_structure_decisions(s: &fepdf::FileStructure) {
    render_decisions_text(&s.decisions);
}

pub fn render_structure_markdown(s: &fepdf::FileStructure, input: &std::path::Path) {
    println!("# File structure: {}", input.display());
    println!("\n| Property | Value |");
    println!("| :--- | :--- |");
    println!("| PDF version | {} |", s.version);
    println!("| Size | {} bytes |", s.size);
    println!("| Header offset | {} |", s.header_offset);
    println!("| Encrypted | {} |", if s.encrypted { "yes" } else { "no" });
    println!("| Declares `/Root` | {} |", if s.declares_root { "yes" } else { "no" });
    println!("| Revisions | {} |", s.revisions.len());
    println!("| Objects in file | {} |", s.objects.in_file);
    println!("| Objects in object streams | {} |", s.objects.in_object_stream);
    println!("| Free slots | {} |", s.objects.free);

    println!("\n## Revisions\n");
    println!("| # | Offset | Entries | Trailer | Form |");
    println!("| ---: | ---: | ---: | :---: | :--- |");
    for r in &s.revisions {
        println!(
            "| {} | {} | {} | {} | {} |",
            r.index,
            r.offset,
            r.entries,
            if r.has_trailer { "yes" } else { "—" },
            r.form
        );
    }

    render_filters_markdown(s);

    println!("\n## Decisions\n");
    if s.is_conforming() {
        println!("None — the file was read without departing from the standard.");
    } else {
        println!("| Severity | Clause | Found | Action |");
        println!("| :--- | :--- | :--- | :--- |");
        for d in &s.decisions {
            println!("| {:?} | {} | {} | {} |", d.severity, d.clause, d.found, d.action);
        }
    }
}

fn render_filters_markdown(s: &fepdf::FileStructure) {
    if s.filters.is_empty() {
        return;
    }
    println!("\n## Filters (7.4)\n");
    println!("| Filter | Streams | On images | Decoded |");
    println!("| :--- | ---: | ---: | :---: |");
    for f in &s.filters {
        println!(
            "| `/{}` | {} | {} | {} |",
            f.name,
            f.streams,
            f.on_images,
            if f.decoded { "yes" } else { "**no**" }
        );
    }
}
