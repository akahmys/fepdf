//! Shows what each cross-reference section says about one object number.
use fepdf_model::arena::PdfArena;
use fepdf_model::interpretation::DecisionLog;
use fepdf_model::reader;
use fepdf_syntax::xref;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: section_probe <pdf> <object number>")?;
    let target: u32 = args.next().ok_or("need an object number")?.parse()?;
    let data = std::fs::read(&path)?;
    let arena = PdfArena::new();
    let mut log = DecisionLog::default();

    let start = xref::find_startxref(&data).ok_or("no startxref")?;
    let chain = xref::section_chain(&data, start);
    println!("startxref = {start}; chain oldest-first = {chain:?}");
    for at in chain {
        let offset = usize::try_from(at)?;
        let section = reader::read_xref_section(&data, offset, &arena, &mut log)?;
        println!(
            "  section @{offset}: {} entries, trailer={}, object {target} -> {:?}",
            section.entries.len(),
            section.trailer.is_some(),
            section.entries.get(&target)
        );
        if let Some(t) = section.trailer
            && let Some(d) = arena.get_dict(t)
        {
            let root = d.get(&arena.name("Root")).cloned();
            println!("      /Root = {root:?}");
        }
    }
    Ok(())
}
