//! Example to inspect open operations.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Step 1: Reading file...");
    let data = std::fs::read("samples/bokutokitan.pdf")?;
    println!("Step 2: lopdf::Document::load_mem...");
    let mut lopdf_doc = lopdf::Document::load_mem(&data)?;
    println!("Step 3: Ingestor::ingest with active_refinement=false...");
    let options =
        fepdf_model::ingest::IngestionOptions { active_refinement: false, ..Default::default() };
    let ingested = fepdf_model::ingest::Ingestor::ingest(&mut lopdf_doc, &options)?;
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
