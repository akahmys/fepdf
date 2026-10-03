//! A procedure set a document names is dropped at load, and that is said (Y-F25).

use fepdf::PdfDocument;
use fepdf_model::Object;
use fepdf_model::access::entry;

/// **14.2 deprecates procedure sets, and a translation to 2.0 drops them.** Nothing reads
/// one but a PostScript printer, and a save kept 1,086 in `fy05.pdf`'s output.
#[test]
fn a_procedure_set_is_dropped_and_the_drop_is_said() {
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /ProcSet [/PDF /Text] >> >>",
    ]);
    let doc = PdfDocument::open(bytes.into()).expect("the fixture opens");
    let arena = doc.inner().arena();
    let page = Object::Reference(doc.inner().get_page_handle(0).expect("a page"));
    let resources = entry(arena, &page, "Resources").expect("the page's resources");
    assert!(entry(arena, &resources, "ProcSet").is_none(), "the procedure set is still there");
    assert!(
        doc.decisions().iter().any(|d| d.clause == "14.2"),
        "the procedure set was dropped in silence: {:?}",
        doc.decisions()
    );
}
