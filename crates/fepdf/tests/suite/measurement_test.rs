//! The scale a drawing declares, written where a reader finds it and read back the way
//! the standard says to show it (ISO 32000-2 12.9, ROADMAP W-16).

use fepdf::{IngestionOptions, MeasurementScale, Operation, PdfDocument};

/// A 400 by 300 page with `page` in its dictionary and `more` after it.
fn page(page: &str, more: &[&str]) -> PdfDocument {
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 300] {page} >>"),
    ];
    bodies.extend(more.iter().map(|b| (*b).to_string()));
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

fn saved_and_reopened(doc: &PdfDocument) -> PdfDocument {
    let path = std::env::temp_dir().join(format!("fepdf_measure_{}.pdf", std::process::id()));
    doc.save_as_version(&path, "2.0").expect("it saves");
    let bytes = std::fs::read(&path).expect("it reads");
    let _ = std::fs::remove_file(&path);
    PdfDocument::open(bytes.into()).expect("it reopens")
}

/// **A scale set is a scale read**, through a save: an inch is a metre, so a line an
/// inch long measures a metre and an inch square a square metre.
#[test]
fn a_scale_set_is_read_back() {
    let mut doc = page("", &[]);
    let per_point = 1.0 / 72.0;
    #[allow(clippy::cast_possible_truncation)] // the operation carries an `f32`
    let scale = MeasurementScale { page: 0, scale_ratio: per_point as f32, unit_label: "m".into() };
    doc.apply(Operation::SetMeasurementScale(scale)).expect("the scale is set");
    let doc = saved_and_reopened(&doc);
    let scale = doc.scale_at(0, (200.0, 150.0)).expect("a scale holds on the page");
    assert_eq!(scale.distance((10.0, 10.0), (82.0, 10.0)), "1 m");
    let square = [(0.0, 0.0), (72.0, 0.0), (72.0, 72.0), (0.0, 72.0)];
    assert_eq!(scale.area(&square), "1 m²");
    assert_eq!(scale.path_length(&[(0.0, 0.0), (72.0, 0.0), (72.0, 144.0)]), "3 m");
    assert_eq!(scale.ratio, "1 in = 1 m");
}

/// **It is a viewport, not a page entry** — the page dictionary has no `/Measure`.
#[test]
fn the_scale_is_a_viewport() {
    let mut doc = page("", &[]);
    let scale = MeasurementScale { page: 0, scale_ratio: 0.5, unit_label: "mm".into() };
    doc.apply(Operation::SetMeasurementScale(scale)).expect("the scale is set");
    let arena = doc.inner().arena();
    let page_dh = doc
        .inner()
        .resolve_to_dict(doc.inner().get_page_handle(0).expect("a page"))
        .expect("a dictionary");
    let dict = arena.get_dict(page_dh).expect("entries");
    assert!(!dict.contains_key(&arena.name("Measure")), "a /Measure Table 31 does not have");
    assert!(dict.contains_key(&arena.name("VP")), "no viewport");
}

/// **The last viewport holding the point decides** (12.9.1), and a point in none has
/// no scale. The left viewport is the standard's own example.
#[test]
fn the_last_viewport_holding_the_point_decides() {
    let example = "<< /Type /Measure /Subtype /RL /R (1in = 0.1 mi) \
        /X [<< /U (mi) /C 0.00139 /D 100000 >>] \
        /D [<< /U (mi) /C 1 >> << /U (ft) /C 5280 >> << /U (in) /C 12 /F /F /D 8 >>] \
        /A [<< /U (acres) /C 640 >>] >>";
    let doubled = "<< /Type /Measure /R (1 pt = 2 m) /X [<< /U (m) /C 2 >>] \
        /D [<< /U (m) /C 1 >>] /A [<< /U (m2) /C 1 >>] >>";
    let doc = page(
        "/VP [<< /Type /Viewport /BBox [0 0 200 300] /Measure 4 0 R >> \
              << /Type /Viewport /BBox [100 0 300 300] /Measure 5 0 R >>]",
        &[example, doubled],
    );
    let left = doc.scale_at(0, (50.0, 50.0)).expect("the left viewport");
    assert_eq!(left.ratio, "1in = 0.1 mi");
    // 1.4505 miles, in user space units at 0.00139 miles each.
    assert_eq!(left.distance((0.0, 0.0), (1.4505 / 0.00139, 0.0)), "1 mi 2,378 ft 7 5/8 in");
    let overlap = doc.scale_at(0, (150.0, 50.0)).expect("both hold it");
    assert_eq!(overlap.ratio, "1 pt = 2 m", "the later viewport is the one that decides");
    assert!(doc.scale_at(0, (350.0, 50.0)).is_none(), "a point in no viewport has a scale");
}

/// Setting a scale keeps a viewport of another kind, and replaces its own.
#[test]
fn setting_a_scale_replaces_only_a_scale() {
    let geo = "<< /Type /Measure /Subtype /GEO /GCS << /Type /GEOGCS /WKT (GEOGCS[]) >> >>";
    let mut doc = page("/VP [<< /Type /Viewport /BBox [0 0 400 300] /Measure 4 0 R >>]", &[geo]);
    for ratio in [0.5, 0.25] {
        let scale = MeasurementScale { page: 0, scale_ratio: ratio, unit_label: "m".into() };
        doc.apply(Operation::SetMeasurementScale(scale)).expect("the scale is set");
    }
    let arena = doc.inner().arena();
    let page_dh = doc
        .inner()
        .resolve_to_dict(doc.inner().get_page_handle(0).expect("a page"))
        .expect("a dictionary");
    let viewports = arena.dict_entry(page_dh, arena.name("VP")).expect("viewports");
    let fepdf::Object::Array(viewports) = viewports.resolve(arena) else { panic!("not an array") };
    assert_eq!(arena.get_array(viewports).expect("items").len(), 2, "one GEO, one RL");
    let scale = doc.scale_at(0, (10.0, 10.0)).expect("a scale");
    assert_eq!(
        scale.distance((0.0, 0.0), (4.0, 0.0)),
        "1 m",
        "the second scale did not replace the first"
    );
}

/// A scale that measures nothing is refused.
#[test]
fn a_scale_of_nothing_is_refused() {
    let mut doc = page("", &[]);
    let scale = MeasurementScale { page: 0, scale_ratio: 0.0, unit_label: "m".into() };
    assert!(doc.apply(Operation::SetMeasurementScale(scale)).is_err());
}

/// **A geospatial anchor and a scale live side by side**, and the anchor's viewport has
/// the `/BBox` Table 265 requires — it had none, and replaced every viewport the page had.
#[test]
fn a_geospatial_anchor_keeps_the_scale() {
    let mut doc = page("", &[]);
    let scale = MeasurementScale { page: 0, scale_ratio: 0.25, unit_label: "m".into() };
    doc.apply(Operation::SetMeasurementScale(scale)).expect("the scale is set");
    doc.apply(Operation::SetGeospatialAnchor(fepdf::GeoSpatialAnchor {
        page: 0,
        latitude: 35.0,
        longitude: 139.0,
        altitude_meters: None,
        crs_wkt: "GEOGCS[\"WGS 84\"]".into(),
    }))
    .expect("the anchor is set");
    assert!(doc.scale_at(0, (10.0, 10.0)).is_some(), "the anchor took the scale's viewport");
    let arena = doc.inner().arena();
    let page_dh = doc
        .inner()
        .resolve_to_dict(doc.inner().get_page_handle(0).expect("a page"))
        .expect("a dictionary");
    let viewports = arena.dict_entry(page_dh, arena.name("VP")).expect("viewports");
    let fepdf::Object::Array(viewports) = viewports.resolve(arena) else { panic!("not an array") };
    for viewport in arena.get_array(viewports).expect("items") {
        let dict = arena.get_dict(viewport.resolve(arena).as_dict_handle().expect("a dictionary"));
        assert!(
            dict.expect("entries").contains_key(&arena.name("BBox")),
            "a viewport with no /BBox"
        );
    }
}

/// **The ratio a title block states is per inch of the sheet.** On a page of `/UserUnit 10`
/// a unit is ten points, so a scale of one metre per inch is a tenth of a metre per unit,
/// and `/R` still says an inch is a metre — it said ten.
#[test]
fn the_stated_ratio_is_per_inch_of_the_sheet_on_a_page_with_a_user_unit() {
    let mut doc = page("/UserUnit 10", &[]);
    #[allow(clippy::cast_possible_truncation)] // the operation carries an `f32`
    let per_unit = (10.0 / 72.0) as f32;
    let scale = MeasurementScale { page: 0, scale_ratio: per_unit, unit_label: "m".into() };
    doc.apply(Operation::SetMeasurementScale(scale)).expect("the scale is set");
    let scale = doc.scale_at(0, (200.0, 150.0)).expect("a scale holds on the page");
    assert_eq!(scale.ratio, "1 in = 1 m");
    assert_eq!(scale.distance((0.0, 0.0), (7.2, 0.0)), "1 m", "an inch is 7.2 units here");
}
