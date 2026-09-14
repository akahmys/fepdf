//! Renders a page before and after a resize, so the geometry can be looked at.

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "samples/print_sample.pdf".into());
    let out = args.next().unwrap_or_else(|| "/tmp".into());
    let bytes = std::fs::read(&path).expect("the sample is there");
    let middle = (fepdf::Align::Middle, fepdf::Align::Middle);
    let fits: [(&str, fepdf::ContentScale, (fepdf::Align, fepdf::Align)); 5] = [
        ("origin", fepdf::ContentScale::Keep, (fepdf::Align::Start, fepdf::Align::Start)),
        ("top-left", fepdf::ContentScale::Keep, (fepdf::Align::Start, fepdf::Align::End)),
        ("centre", fepdf::ContentScale::Keep, middle),
        ("fit", fepdf::ContentScale::Fit, middle),
        ("scale-half", fepdf::ContentScale::By(0.5), middle),
    ];
    let before = fepdf::PdfDocument::open(bytes.clone().into()).expect("it opens");
    before
        .render_page_to_file(2, std::path::Path::new(&format!("{out}/resize-before.png")))
        .expect("it renders");
    for (name, scale, place) in fits {
        let mut doc = fepdf::PdfDocument::open(bytes.clone().into()).expect("it opens");
        doc.apply(fepdf::Operation::ResizePages(
            fepdf::PageSelection::All,
            fepdf::PageResize { sheet: Some((842.0, 1191.0)), scale, place, offset: (0.0, 0.0) },
        ))
        .expect("it resizes");
        doc.render_page_to_file(2, std::path::Path::new(&format!("{out}/resize-{name}.png")))
            .expect("it renders");
        println!("{name}: {:?}", doc.get_page_size(2).expect("a size"));
    }
}
