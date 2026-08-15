//! Walks `Document::open` a step at a time, so a hang or a panic can be placed.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Step 1: Reading file...");
    let data = std::fs::read("samples/bokutokitan.pdf")?;
    println!("Step 2: reader::load_document...");
    let raw = fepdf_model::reader::load_document(&data)?;
    println!(
        "        {} object slots, {} decisions",
        raw.arena.object_count(),
        raw.decisions.entries().len()
    );
    println!("Step 3: Ingestor::ingest with active_refinement=false...");
    let options =
        fepdf_model::ingest::IngestionOptions { active_refinement: false, ..Default::default() };
    let ingested = fepdf_model::ingest::Ingestor::ingest(raw, &options)?;
    println!("Step 4: Creating Document...");
    let mut doc = fepdf_model::Document::with_issues(
        ingested.arena,
        ingested.root,
        ingested.info,
        ingested.issues,
    );
    println!("Step 5: load_system_fonts...");
    doc.load_system_fonts();
    println!("Step 6: normalize_resources...");
    doc.normalize_resources();
    println!("Step 7: normalize_page_tree...");
    doc.normalize_page_tree();
    println!("Done!");
    Ok(())
}
