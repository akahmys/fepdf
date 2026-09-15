//! What happens to content pushed off the sheet by an offset.

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "/tmp".into());
    let bytes = std::fs::read("samples/print_sample.pdf").expect("the sample is there");
    let before = fepdf::PdfDocument::open(bytes.clone().into()).expect("it opens");
    let text = before.extract_text(2).expect("it reads");
    println!("before: {} characters of text", text.chars().count());

    let mut doc = fepdf::PdfDocument::open(bytes.into()).expect("it opens");
    doc.apply(fepdf::Operation::ResizePages(
        fepdf::PageSelection::All,
        fepdf::PageResize {
            sheet: None,
            scale: fepdf::ContentScale::Keep,
            // Most of the page pushed off the right edge.
            offset: (300.0, 0.0),
        },
    ))
    .expect("it resizes");

    let after = doc.extract_text(2).expect("it reads");
    println!("after:  {} characters of text", after.chars().count());
    println!("same text: {}", after == text);
    doc.render_page_to_file(2, std::path::Path::new(&format!("{out}/overflow.png")))
        .expect("it renders");
}
