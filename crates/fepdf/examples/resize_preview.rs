//! Renders a page before and after a resize, so the geometry can be looked at.

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "samples/print_sample.pdf".into());
    let out = args.next().unwrap_or_else(|| "/tmp".into());
    let bytes = std::fs::read(&path).expect("the sample is there");
    let before = fepdf::PdfDocument::open(bytes.clone().into()).expect("it opens");
    before
        .render_page_to_file(2, std::path::Path::new(&format!("{out}/resize-before.png")))
        .expect("it renders");

    let sheet = (842.0, 1191.0);
    let page = before.get_page_size(2).unwrap_or((595.0, 842.0));
    // The named corners come out as offsets, which is what the form's buttons fill in.
    let corner = |across, up, scale| fepdf::PageResize::offset_to((across, up), page, sheet, scale);
    let fits: [(&str, fepdf::ContentScale, (f64, f64)); 5] = [
        (
            "origin",
            fepdf::ContentScale::Keep,
            corner(fepdf::Align::Start, fepdf::Align::Start, 1.0),
        ),
        (
            "top-left",
            fepdf::ContentScale::Keep,
            corner(fepdf::Align::Start, fepdf::Align::End, 1.0),
        ),
        ("centre", fepdf::ContentScale::Keep, (0.0, 0.0)),
        ("fit", fepdf::ContentScale::Fit, (0.0, 0.0)),
        ("scale-half", fepdf::ContentScale::By(0.5), (0.0, 0.0)),
    ];
    for (name, scale, offset) in fits {
        let mut doc = fepdf::PdfDocument::open(bytes.clone().into()).expect("it opens");
        doc.apply(fepdf::Operation::ResizePages(
            fepdf::PageSelection::All,
            fepdf::PageResize { sheet: Some(sheet), scale, offset },
        ))
        .expect("it resizes");
        doc.render_page_to_file(2, std::path::Path::new(&format!("{out}/resize-{name}.png")))
            .expect("it renders");
        println!("{name}: {:?} offset {offset:?}", doc.get_page_size(2).expect("a size"));
    }
}
