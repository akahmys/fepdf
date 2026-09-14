//! Renders a page before and after a resize, so the geometry can be looked at.

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "samples/print_sample.pdf".into());
    let out = args.next().unwrap_or_else(|| "/tmp".into());
    let bytes = std::fs::read(&path).expect("the sample is there");
    let fits = [
        ("anchor", fepdf::ContentFit::Anchor),
        ("centre", fepdf::ContentFit::Centre),
        ("fit", fepdf::ContentFit::Fit),
        ("scale-half", fepdf::ContentFit::Scale(0.5)),
    ];
    let before = fepdf::PdfDocument::open(bytes.clone().into()).expect("it opens");
    before
        .render_page_to_file(2, std::path::Path::new(&format!("{out}/resize-before.png")))
        .expect("it renders");
    for (name, content) in fits {
        let mut doc = fepdf::PdfDocument::open(bytes.clone().into()).expect("it opens");
        doc.apply(fepdf::Operation::ResizePages(
            fepdf::PageSelection::All,
            fepdf::PageResize { size: (842.0, 1191.0), content },
        ))
        .expect("it resizes");
        doc.render_page_to_file(2, std::path::Path::new(&format!("{out}/resize-{name}.png")))
            .expect("it renders");
        println!("{name}: {:?}", doc.get_page_size(2).expect("a size"));
    }
}
