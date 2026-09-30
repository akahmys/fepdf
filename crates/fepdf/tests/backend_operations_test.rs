//! Integration and Unit Test Suite for ISO 32000-2 Extended Backend Operations.

use fepdf::{DecorationPosition, Operation, PageSelection};
use fepdf_model::{
    AFRelationship, AnnotationKind, AnnotationSpec, ArticleBead, ArticleThread, AssociatedFile,
    CollectionViewMode, FormFieldSpec, FormValue, GeoSpatialAnchor, LayerGroup, MeasurementScale,
    OptionalContentProperties, OutlineNode, OutlineTree, OutputIntent, PageLabelSpec,
    PageLabelStyle, PdfAction, PortfolioCollection, PortfolioItem, UnencryptedWrapperSpec,
    UserProperty, UserPropertyValue, VisibilityState,
};

use fepdf_fixtures::assemble;

#[test]
fn test_portfolio_domain_model() {
    let portfolio = PortfolioCollection {
        view_mode: CollectionViewMode::Details,
        initial_document: Some("cover.pdf".to_string()),
        items: vec![PortfolioItem {
            filename: "data.csv".to_string(),
            mime_type: Some("text/csv".to_string()),
            description: Some("Raw dataset".to_string()),
            size_bytes: 100,
            data: b"a,b,c\n1,2,3".to_vec(),
        }],
    };

    let op = Operation::CreatePortfolio(portfolio);
    if let Operation::CreatePortfolio(p) = op {
        assert_eq!(p.view_mode, CollectionViewMode::Details);
        assert_eq!(p.items.len(), 1);
        assert_eq!(p.items[0].filename, "data.csv");
    } else {
        panic!("Operation variant mismatch");
    }
}

#[test]
fn test_outline_tree_domain_model() {
    let outlines = OutlineTree {
        items: vec![OutlineNode {
            title: "Chapter 1".to_string(),
            destination_page: 0,
            children: vec![OutlineNode {
                title: "Section 1.1".to_string(),
                destination_page: 1,
                children: vec![],
            }],
        }],
    };

    let op = Operation::UpdateOutlines(outlines);
    if let Operation::UpdateOutlines(tree) = op {
        assert_eq!(tree.items.len(), 1);
        assert_eq!(tree.items[0].title, "Chapter 1");
        assert_eq!(tree.items[0].children[0].title, "Section 1.1");
    } else {
        panic!("Operation variant mismatch");
    }
}

#[test]
fn test_optional_content_properties() {
    let layers = OptionalContentProperties {
        layers: vec![LayerGroup {
            name: "Electrical Wiring".to_string(),
            default_state: VisibilityState::On,
            printable: true,
        }],
    };

    let op = Operation::UpdateLayers(layers);
    if let Operation::UpdateLayers(props) = op {
        assert_eq!(props.layers.len(), 1);
        assert_eq!(props.layers[0].name, "Electrical Wiring");
    } else {
        panic!("Operation variant mismatch");
    }
}

#[test]
fn test_associated_file_domain_model() {
    let af = AssociatedFile {
        filename: "factur-x.xml".to_string(),
        relationship: AFRelationship::Data,
        mime_type: "text/xml".to_string(),
        data: b"<r></r>".to_vec(),
    };

    let op = Operation::AttachAssociatedFile(af);
    if let Operation::AttachAssociatedFile(file) = op {
        assert_eq!(file.filename, "factur-x.xml");
        assert_eq!(file.relationship, AFRelationship::Data);
    } else {
        panic!("Operation variant mismatch");
    }
}

#[test]
fn test_output_intent_domain_model() {
    let intent = OutputIntent {
        subtype: "GTS_PDFX".to_string(),
        identifier: "CGATS TR 001".to_string(),
        info: Some("SWOP 2006".to_string()),
        icc_profile_bytes: None,
    };

    let op = Operation::SetOutputIntent(intent);
    if let Operation::SetOutputIntent(intent_obj) = op {
        assert_eq!(intent_obj.identifier, "CGATS TR 001");
    } else {
        panic!("Operation variant mismatch");
    }
}

/// A file with no signature reports no signature, rather than reporting a verdict.
///
/// This replaces a test of `PkiValidator`, which returned `Valid` and a `signer_name` of
/// the literal string "Valid Signer" for any bytes that parsed as one DER element. The
/// only branch of it that told the truth was the empty-input one, and that was the
/// branch the test pinned.
#[test]
fn a_file_with_no_signature_reports_none() {
    let bytes =
        assemble(&["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [] /Count 0 >>"]);
    let report = fepdf::SignatureReport::survey(&bytes).expect("a report");
    assert!(report.signatures.is_empty(), "found a signature in an unsigned file");
    assert_eq!(report.unsigned_fields, 0);
}

#[test]
fn test_bates_numbering_operation() {
    let op = Operation::ApplyBatesNumbering {
        pages: PageSelection::All,
        prefix: "DOC-".to_string(),
        start_number: 1,
        digits: 6,
        position: DecorationPosition::BottomRight,
    };

    if let Operation::ApplyBatesNumbering { prefix, digits, .. } = op {
        assert_eq!(prefix, "DOC-");
        assert_eq!(digits, 6);
    } else {
        panic!("Operation variant mismatch");
    }
}

#[test]
fn test_annotation_spec() {
    let annot = AnnotationSpec {
        page: 0,
        rect: [10.0, 10.0, 100.0, 100.0],
        kind: AnnotationKind::Link { destination_page: 2, url: None },
    };

    let op = Operation::AddAnnotation(annot);
    if let Operation::AddAnnotation(a) = op {
        assert_eq!(a.page, 0);
        if let AnnotationKind::Link { destination_page, .. } = a.kind {
            assert_eq!(destination_page, 2);
        } else {
            panic!("Annotation kind mismatch");
        }
    } else {
        panic!("Operation variant mismatch");
    }
}

/// **A link to a page the document does not have is refused.** It was written with no
/// `/Dest` and no `/A` and answered `Ok`: a link that goes nowhere, which a caller one page
/// off was told had worked. Every operation that takes a page refuses one that is not there.
#[test]
fn a_link_to_a_page_that_is_not_there_is_refused() {
    let link = |to: usize| {
        Operation::AddAnnotation(AnnotationSpec {
            page: 0,
            rect: [10.0, 10.0, 100.0, 100.0],
            kind: AnnotationKind::Link { destination_page: to, url: None },
        })
    };
    let mut doc = fepdf::PdfDocument::create_empty().expect("a document");
    let refused = doc.apply(link(5)).expect_err("a link to page 5 of one is refused");
    assert!(refused.to_string().contains('5'), "the refusal does not name the page: {refused}");
    doc.apply(link(0)).expect("a link to the page that is there is written");
}

/// **A thread that cannot be written as 12.4.3 has one is refused**: a bead on a page the
/// document does not have, which was written with no `/P` (Table 162 requires one), and a
/// thread with no beads, which has no `/F` to start from (Table 160).
#[test]
fn a_thread_with_a_bead_nowhere_or_no_beads_is_refused() {
    let mut doc = fepdf::PdfDocument::create_empty().expect("a document");
    let thread = |beads: Vec<ArticleBead>| {
        Operation::UpdateArticleThreads(vec![ArticleThread { title: "t".to_string(), beads }])
    };
    let refused = doc
        .apply(thread(vec![ArticleBead { page: 5, rect: [0.0, 0.0, 10.0, 10.0] }]))
        .expect_err("a bead on page 5 of one is refused");
    assert!(refused.to_string().contains('5'), "the refusal does not name the page: {refused}");
    doc.apply(thread(Vec::new())).expect_err("a thread with no beads is refused");
    doc.apply(thread(vec![ArticleBead { page: 0, rect: [0.0, 0.0, 10.0, 10.0] }]))
        .expect("a thread on the page that is there is written");
}

/// The keys of a name tree's leaves, in the order the tree holds them.
fn name_tree_keys(
    arena: &fepdf_model::PdfArena,
    node: &fepdf_model::Object,
    depth: usize,
) -> Vec<String> {
    let Some(dict) = node.resolve(arena).as_dict_handle() else { return Vec::new() };
    let array = |key: &str| match arena.dict_entry(dict, arena.name(key)).map(|v| v.resolve(arena))
    {
        Some(fepdf_model::Object::Array(a)) => arena.get_array(a).unwrap_or_default(),
        _ => Vec::new(),
    };
    if depth < 8 && !array("Kids").is_empty() {
        return array("Kids")
            .iter()
            .flat_map(|kid| name_tree_keys(arena, kid, depth + 1))
            .collect();
    }
    array("Names")
        .chunks(2)
        .filter_map(|pair| match pair.first()?.resolve(arena) {
            fepdf_model::Object::String(b) | fepdf_model::Object::Hex(b) => {
                Some(String::from_utf8_lossy(&b).into_owned())
            }
            fepdf_model::Object::Text(t) => Some(t),
            _ => None,
        })
        .collect()
}

/// **Attaching a file keeps what the catalogue's `/Names` held**: a `/Names` written in
/// place was replaced by a new one holding the attachment alone, so the named destinations
/// beside it went; an `/EmbeddedFiles` tree with `/Kids` was read as having no entries, so
/// the files already attached went; and the new one was put last whatever its name, where
/// 7.9.6 has a tree's keys in order.
#[test]
fn attaching_a_file_keeps_what_the_name_trees_held() {
    let mut doc = fepdf::PdfDocument::open(
        assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /Names << /Dests << /Names [(two) [3 0 R /Fit]] >> \
             /EmbeddedFiles << /Kids [4 0 R] >> >> >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            "<< /Names [(c.txt) 5 0 R] /Limits [(c.txt) (c.txt)] >>",
            "<< /Type /Filespec /F (c.txt) /UF (c.txt) >>",
        ])
        .into(),
    )
    .expect("the fixture opens");
    doc.apply(Operation::AttachAssociatedFile(AssociatedFile {
        filename: "b.xml".to_string(),
        relationship: AFRelationship::Data,
        mime_type: "text/xml".to_string(),
        data: b"<r/>".to_vec(),
    }))
    .expect("the file is attached");

    let arena = doc.inner().arena();
    let catalog = doc.inner().catalog_handle().and_then(|c| doc.inner().resolve_to_dict(c).ok());
    let names = catalog
        .and_then(|c| arena.dict_entry(c, arena.name("Names")))
        .and_then(|n| n.resolve(arena).as_dict_handle())
        .expect("the catalogue names things");
    let entry = |key: &str| arena.dict_entry(names, arena.name(key)).expect("it is there");
    assert_eq!(name_tree_keys(arena, &entry("Dests"), 0), ["two"], "the destinations went");
    assert_eq!(name_tree_keys(arena, &entry("EmbeddedFiles"), 0), ["b.xml", "c.txt"]);
}

/// **A bookmark to a page the document does not have is refused**, as a link to one is:
/// it was written with no destination and answered `Ok`.
#[test]
fn a_bookmark_to_a_page_that_is_not_there_is_refused() {
    let mut doc = fepdf::PdfDocument::create_empty().expect("a document");
    let outline = |page: usize| {
        Operation::UpdateOutlines(fepdf_model::document::extensions::OutlineTree {
            items: vec![fepdf_model::document::extensions::OutlineNode {
                title: "Chapter".to_string(),
                destination_page: page,
                children: Vec::new(),
            }],
        })
    };
    let refused = doc.apply(outline(5)).expect_err("a bookmark to page 5 of one is refused");
    assert!(refused.to_string().contains('5'), "the refusal does not name the page: {refused}");
    doc.apply(outline(0)).expect("a bookmark to the page that is there is written");
}

/// The 128-byte header of an ICC profile whose data colour space is `space` (ICC.1 7.2).
fn icc_header(space: &[u8; 4]) -> Vec<u8> {
    let mut header = vec![0u8; 128];
    header[..4].copy_from_slice(&128u32.to_be_bytes());
    header[16..20].copy_from_slice(space);
    header[36..40].copy_from_slice(b"acsp");
    header
}

/// **An output intent's profile says how many components it has** (Table 401, 8.6.5.5):
/// `/N` was 3 whatever the profile, so the CMYK profile a PDF/X intent names was declared
/// three-component — and five bytes that were no profile at all were written as one.
#[test]
fn an_output_intents_profile_is_declared_as_what_it_is() {
    let mut doc = fepdf::PdfDocument::create_empty().expect("a document");
    let intent = |icc: Vec<u8>| {
        Operation::SetOutputIntent(OutputIntent {
            subtype: "GTS_PDFX".to_string(),
            identifier: "FOGRA39".to_string(),
            info: None,
            icc_profile_bytes: Some(icc),
        })
    };
    doc.apply(intent(vec![1, 2, 3, 4, 5])).expect_err("five bytes are not a profile");
    doc.apply(intent(icc_header(b"CMYK"))).expect("a CMYK profile is named");
    let arena = doc.inner().arena();
    let catalog = doc.inner().catalog_handle().and_then(|c| doc.inner().resolve_to_dict(c).ok());
    let entry = |dict, key: &str| arena.dict_entry(dict, arena.name(key)).map(|v| v.resolve(arena));
    let intents = match catalog.and_then(|c| entry(c, "OutputIntents")) {
        Some(fepdf_model::Object::Array(a)) => arena.get_array(a).unwrap_or_default(),
        _ => Vec::new(),
    };
    let intent = intents.first().and_then(|i| i.resolve(arena).as_dict_handle()).expect("one");
    let profile = entry(intent, "DestOutputProfile").and_then(|p| p.as_dict_handle());
    let n = profile.and_then(|p| entry(p, "N")).and_then(|n| n.as_integer());
    assert_eq!(n, Some(4), "a CMYK profile is declared with {n:?} components");
}

/// **Updating the layers keeps the groups content is in** (8.11.2): a layer named as one
/// the document has is that group, not a new one of the same name, and one not named
/// stays among `/OCGs`, which lists every group in the document (8.11.4.2). Each layer was
/// made anew, so what the pages had marked `/OC` belonged to groups the document no longer
/// listed, and turning "Draft" off turned off nothing on the page.
#[test]
fn updating_the_layers_keeps_the_groups_content_is_in() {
    let mut doc = fepdf::PdfDocument::open(
        assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [5 0 R 6 0 R] \
             /D << /ON [5 0 R] /OFF [6 0 R] >> >> >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
             /Resources << /Properties << /oc1 5 0 R >> >> >>",
            "<< /Length 32 >>\nstream\n/OC /oc1 BDC 0 0 9 9 re f EMC\n\nendstream",
            "<< /Type /OCG /Name (Draft) >>",
            "<< /Type /OCG /Name (Other) >>",
        ])
        .into(),
    )
    .expect("the fixture opens");
    doc.apply(Operation::UpdateLayers(OptionalContentProperties {
        layers: vec![
            LayerGroup {
                name: "Draft".into(),
                default_state: VisibilityState::Off,
                printable: false,
            },
            LayerGroup { name: "New".into(), default_state: VisibilityState::On, printable: true },
        ],
    }))
    .expect("the layers are updated");

    let arena = doc.inner().arena();
    let catalog = doc.inner().catalog_handle().and_then(|c| doc.inner().resolve_to_dict(c).ok());
    let entry = |dict, key: &str| arena.dict_entry(dict, arena.name(key)).map(|v| v.resolve(arena));
    let refs = |object: Option<fepdf_model::Object>| match object {
        Some(fepdf_model::Object::Array(a)) => arena
            .get_array(a)
            .unwrap_or_default()
            .iter()
            .filter_map(|o| o.as_reference().map(|h| h.index()))
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let properties =
        catalog.and_then(|c| entry(c, "OCProperties")).and_then(|p| p.as_dict_handle());
    let properties = properties.expect("the document has layers");
    let groups = refs(entry(properties, "OCGs"));
    assert!(groups.contains(&5), "the group the page is marked with left /OCGs: {groups:?}");
    assert!(groups.contains(&6), "a group not named left /OCGs: {groups:?}");
    assert_eq!(groups.len(), 3, "Draft was made again beside the one it is: {groups:?}");
    let config = entry(properties, "D").and_then(|d| d.as_dict_handle()).expect("a configuration");
    let off = refs(entry(config, "OFF"));
    assert!(off.contains(&5) && off.contains(&6), "Draft off and Other as it was: {off:?}");
}

/// **Page labels are written as 12.4.2 has a number tree**: from page 0, in page order,
/// each range starting on a page the document has, and numbering from 1 or more. They were
/// written in the order given and none of that was asked, so a document could be handed
/// labels no reader can apply. Ranges that begin after page 0 are given one there.
#[test]
fn page_labels_are_written_as_a_number_tree_from_page_zero() {
    let mut doc = fepdf::PdfDocument::create_empty().expect("a document");
    for _ in 0..2 {
        doc.apply(Operation::DuplicatePages(PageSelection::Single(0))).expect("a page more");
    }
    let label = |start_page: usize, start_number: u32| PageLabelSpec {
        start_page,
        style: PageLabelStyle::Decimal,
        prefix: None,
        start_number,
    };
    for (refused, why) in [
        (vec![label(0, 1), label(5, 1)], "page 5 of 3"),
        (vec![label(0, 1), label(0, 3)], "two ranges start at page 0"),
        (vec![label(0, 0)], "numbering from 0"),
    ] {
        doc.apply(Operation::SetPageLabels(refused)).expect_err(why);
    }
    doc.apply(Operation::SetPageLabels(vec![label(2, 1), label(0, 1)])).expect("it is written");
    assert_eq!(label_tree_keys(&doc), [0, 2], "the tree's keys are not in page order");
    doc.apply(Operation::SetPageLabels(vec![label(1, 5)])).expect("it is written");
    assert_eq!(label_tree_keys(&doc), [0, 1], "the tree does not start at page 0");
}

/// The keys of the document's `/PageLabels` tree, in the order it holds them.
fn label_tree_keys(doc: &fepdf::PdfDocument) -> Vec<i64> {
    let arena = doc.inner().arena();
    let catalog = doc.inner().catalog_handle().and_then(|c| doc.inner().resolve_to_dict(c).ok());
    let entry = |dict, key: &str| arena.dict_entry(dict, arena.name(key)).map(|v| v.resolve(arena));
    let tree = catalog.and_then(|c| entry(c, "PageLabels")).and_then(|t| t.as_dict_handle());
    match tree.and_then(|t| entry(t, "Nums")) {
        Some(fepdf_model::Object::Array(a)) => arena
            .get_array(a)
            .unwrap_or_default()
            .iter()
            .step_by(2)
            .filter_map(fepdf_model::Object::as_integer)
            .collect(),
        _ => Vec::new(),
    }
}

#[test]
fn test_measurement_scale_spec() {
    let scale = MeasurementScale { page: 0, scale_ratio: 0.01, unit_label: "m".to_string() };

    let op = Operation::SetMeasurementScale(scale);
    if let Operation::SetMeasurementScale(s) = op {
        assert_eq!(s.unit_label, "m");
    } else {
        panic!("Operation variant mismatch");
    }
}

#[test]
fn test_form_field_spec() {
    let field = FormFieldSpec {
        name: "CustomerName".to_string(),
        value: FormValue::Text("Alice".to_string()),
    };

    let op = Operation::SetFormFieldValue(field);
    if let Operation::SetFormFieldValue(f) = op {
        assert_eq!(f.name, "CustomerName");
        assert_eq!(f.value, FormValue::Text("Alice".to_string()));
    } else {
        panic!("Operation variant mismatch");
    }
}

// --- Phase 5-7 Extended Tests ---

#[test]
fn test_page_label_operation() {
    let labels = vec![PageLabelSpec {
        start_page: 0,
        style: PageLabelStyle::LowerRoman,
        prefix: Some("i-".to_string()),
        start_number: 1,
    }];

    let op = Operation::SetPageLabels(labels);
    if let Operation::SetPageLabels(list) = op {
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].style, PageLabelStyle::LowerRoman);
    } else {
        panic!("Operation variant mismatch");
    }
}

#[test]
fn test_article_thread_operation() {
    let threads = vec![ArticleThread {
        title: "Main Story".to_string(),
        beads: vec![ArticleBead { page: 0, rect: [0.0, 0.0, 200.0, 400.0] }],
    }];

    let op = Operation::UpdateArticleThreads(threads);
    if let Operation::UpdateArticleThreads(list) = op {
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Main Story");
    } else {
        panic!("Operation variant mismatch");
    }
}

#[test]
fn test_user_property_operation() {
    let props = vec![UserProperty {
        name: "Department".to_string(),
        value: UserPropertyValue::Text("Engineering".to_string()),
        formatted: Some("Engineering Dept".to_string()),
    }];

    let op = Operation::AddUserProperties { target_handle: 42, properties: props };

    if let Operation::AddUserProperties { target_handle, properties } = op {
        assert_eq!(target_handle, 42);
        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].name, "Department");
    } else {
        panic!("Operation variant mismatch");
    }
}

#[test]
fn test_action_execute_operation() {
    let action = PdfAction::GoToRemote { file_path: "appendix.pdf".to_string(), page: 5 };

    let op = Operation::SetOpenAction(action);
    if let Operation::SetOpenAction(PdfAction::GoToRemote { file_path, page }) = op {
        assert_eq!(file_path, "appendix.pdf");
        assert_eq!(page, 5);
    } else {
        panic!("Operation variant mismatch");
    }
}

#[test]
fn test_geospatial_anchor_operation() {
    let anchor = GeoSpatialAnchor {
        page: 0,
        latitude: 35.6895,
        longitude: 139.6917,
        altitude_meters: Some(40.0),
        crs_wkt: "GEOGCS[\"WGS 84\"]".to_string(),
    };

    let op = Operation::SetGeospatialAnchor(anchor);
    if let Operation::SetGeospatialAnchor(a) = op {
        assert!((a.latitude - 35.6895).abs() < f64::EPSILON);
        assert!((a.longitude - 139.6917).abs() < f64::EPSILON);
    } else {
        panic!("Operation variant mismatch");
    }
}

/// A wrapper for `payload`, filtered by `AcmeCustomCrypto` version 1.0.
fn wrapper(payload: &[u8]) -> UnencryptedWrapperSpec {
    UnencryptedWrapperSpec {
        notice_message: "Install the Acme handler to open this document.".to_string(),
        encrypted_payload_bytes: payload.to_vec(),
        payload_name: "Protected.pdf".to_string(),
        crypto_filter: "AcmeCustomCrypto".to_string(),
        filter_version: Some("1.0".to_string()),
    }
}

/// **Every `shall` of 7.6.7, read back from a saved and reopened file.** The operation
/// embedded the payload with `/AFRelationship /Unspecified` and nothing else the clause
/// asks for; each assertion here names one of the things it asks for.
#[test]
fn an_unencrypted_wrapper_meets_what_7_6_7_asks() {
    use fepdf::Object;
    let mut doc = fepdf::PdfDocument::create_empty().expect("a document");
    doc.apply(Operation::SetUnencryptedWrapper(wrapper(b"%PDF-2.0\nencrypted")))
        .expect("the wrapper is written");
    let path = std::env::temp_dir().join(format!("fepdf_wrapper_{}.pdf", std::process::id()));
    doc.save_with_options(&path, "2.0", &fepdf::SaveOptions::default()).expect("it saves");
    let reopened =
        fepdf::PdfDocument::open(std::fs::read(&path).expect("read").into()).expect("it reopens");
    let _ = std::fs::remove_file(&path);

    let inner = reopened.inner();
    let arena = inner.arena();
    let dict = |o: &Object| {
        arena.get_dict(o.resolve(arena).as_dict_handle().expect("a dictionary")).expect("held")
    };
    let catalog = dict(&Object::Reference(*inner.root_handle()));
    let get = |d: &std::collections::BTreeMap<_, Object>, k: &str| {
        d.get(&arena.name(k)).cloned().expect(k)
    };
    let name = |o: Object| {
        arena
            .get_name(o.resolve(arena).as_name().expect("a name"))
            .expect("held")
            .as_str()
            .to_string()
    };

    // A Collection making the payload the initial document, the view hidden.
    let collection = dict(&get(&catalog, "Collection"));
    assert_eq!(name(get(&collection, "View")), "H", "the collection view is hidden");
    assert!(
        matches!(get(&collection, "D").resolve(arena), Object::String(ref b) if &b[..] == b"Protected.pdf"),
        "the payload is the initial document"
    );

    // In /EmbeddedFiles, alone, and in /AF.
    let names = dict(&get(&catalog, "Names"));
    let embedded = dict(&get(&names, "EmbeddedFiles"));
    let pairs = arena
        .get_array(get(&embedded, "Names").resolve(arena).as_array().expect("an array"))
        .expect("held");
    assert_eq!(pairs.len(), 2, "the name tree holds exactly one entry");
    let af = arena
        .get_array(get(&catalog, "AF").resolve(arena).as_array().expect("an array"))
        .expect("held");
    assert_eq!(af.len(), 1, "the payload is in /AF");

    // AFRelationship EncryptedPayload, and Table 28's dictionary.
    let spec = dict(&af[0]);
    assert_eq!(name(get(&spec, "AFRelationship")), "EncryptedPayload");
    let ep = dict(&get(&spec, "EP"));
    assert_eq!(name(get(&ep, "Type")), "EncryptedPayload");
    assert_eq!(name(get(&ep, "Subtype")), "AcmeCustomCrypto");
    assert_eq!(name(get(&ep, "Version")), "1.0");
}

/// What 7.6.7 and Table 28 rule out is refused, before anything is written.
#[test]
fn an_unencrypted_wrapper_is_refused_what_7_6_7_rules_out() {
    let mut doc = fepdf::PdfDocument::create_empty().expect("a document");
    assert!(
        doc.apply(Operation::SetUnencryptedWrapper(wrapper(b"not a pdf"))).is_err(),
        "a payload that is no PDF"
    );
    let mut bad_filter = wrapper(b"%PDF-2.0");
    bad_filter.crypto_filter = "Acme Crypto".to_string();
    assert!(
        doc.apply(Operation::SetUnencryptedWrapper(bad_filter)).is_err(),
        "a filter that is no name"
    );
    let mut bad_version = wrapper(b"%PDF-2.0");
    bad_version.filter_version = Some("1.x".to_string());
    assert!(
        doc.apply(Operation::SetUnencryptedWrapper(bad_version)).is_err(),
        "a version that is not integers"
    );

    doc.apply(Operation::SetUnencryptedWrapper(wrapper(b"%PDF-2.0"))).expect("the first wrapper");
    assert!(
        doc.apply(Operation::SetUnencryptedWrapper(wrapper(b"%PDF-2.0"))).is_err(),
        "a second payload, which would make two /EmbeddedFiles entries"
    );
}

#[test]
fn test_tier1_operations_execution() {
    use fepdf::PdfDocument;

    let mut doc = PdfDocument::create_empty().expect("Failed to create document");

    // 1. SetPageLabels
    let labels = vec![PageLabelSpec {
        start_page: 0,
        style: PageLabelStyle::UpperRoman,
        prefix: Some("Sec-".to_string()),
        start_number: 1,
    }];
    doc.apply(Operation::SetPageLabels(labels)).expect("SetPageLabels failed");

    // 2. CreatePortfolio
    let portfolio = PortfolioCollection {
        view_mode: CollectionViewMode::Details,
        initial_document: Some("main.pdf".to_string()),
        items: vec![PortfolioItem {
            filename: "embedded.txt".to_string(),
            mime_type: Some("text/plain".to_string()),
            description: Some("Text file".to_string()),
            size_bytes: 11,
            data: b"hello world".to_vec(),
        }],
    };
    doc.apply(Operation::CreatePortfolio(portfolio)).expect("CreatePortfolio failed");

    // 3. AttachAssociatedFile
    let af = AssociatedFile {
        filename: "data.xml".to_string(),
        relationship: AFRelationship::Source,
        mime_type: "text/xml".to_string(),
        data: b"<root/>".to_vec(),
    };
    doc.apply(Operation::AttachAssociatedFile(af)).expect("AttachAssociatedFile failed");

    // 4. UpdateOutlines
    let outlines = OutlineTree {
        items: vec![OutlineNode {
            title: "Chapter 1".to_string(),
            destination_page: 0,
            children: vec![OutlineNode {
                title: "Section 1.1".to_string(),
                destination_page: 0,
                children: vec![],
            }],
        }],
    };
    doc.apply(Operation::UpdateOutlines(outlines)).expect("UpdateOutlines failed");

    // 5. SetOutputIntent
    let intent = OutputIntent {
        subtype: "GTS_PDFA1".to_string(),
        identifier: "sRGB".to_string(),
        info: Some("Standard sRGB profile".to_string()),
        icc_profile_bytes: Some(icc_header(b"RGB ")),
    };
    doc.apply(Operation::SetOutputIntent(intent)).expect("SetOutputIntent failed");

    // 6. UpdateLayers
    let layers = OptionalContentProperties {
        layers: vec![LayerGroup {
            name: "Layer 1".to_string(),
            default_state: VisibilityState::On,
            printable: true,
        }],
    };
    doc.apply(Operation::UpdateLayers(layers)).expect("UpdateLayers failed");

    // Verify catalog has the expected entries
    let catalog = doc.inner().catalog().expect("Failed to get catalog");
    assert!(catalog.outlines.is_some(), "Outlines missing");
    let arena = doc.inner().arena();
    let cadh = doc.inner().resolve_to_dict(doc.inner().catalog_handle().unwrap()).unwrap();
    let cdict = arena.get_dict(cadh).unwrap();
    assert!(cdict.contains_key(&arena.name("PageLabels")), "PageLabels missing");
    assert!(cdict.contains_key(&arena.name("Collection")), "Collection missing");
    assert!(cdict.contains_key(&arena.name("AF")), "AF missing");
    assert!(cdict.contains_key(&arena.name("OutputIntents")), "OutputIntents missing");
    assert!(cdict.contains_key(&arena.name("OCProperties")), "OCProperties missing");
}

#[test]
fn test_tier2_tier3_operations_execution() {
    use fepdf::PdfDocument;

    let mut doc = PdfDocument::create_empty().expect("Failed to create document");

    // 1. AddAnnotation
    let annot_spec = AnnotationSpec {
        page: 0,
        rect: [100.0, 100.0, 200.0, 150.0],
        kind: AnnotationKind::TextComment { contents: "Review note: Approved.".to_string() },
    };
    doc.apply(Operation::AddAnnotation(annot_spec)).expect("AddAnnotation failed");

    // 2. SetGeospatialAnchor
    let geo = GeoSpatialAnchor {
        page: 0,
        latitude: 35.6762,
        longitude: 139.6503,
        altitude_meters: Some(40.0),
        crs_wkt: "GEOGCS[\"WGS 84\",DATUM[\"WGS_1984\"]]".to_string(),
    };
    doc.apply(Operation::SetGeospatialAnchor(geo)).expect("SetGeospatialAnchor failed");

    // 3. AddPageDecoration
    doc.apply(Operation::AddPageDecoration {
        pages: PageSelection::All,
        text: "CONFIDENTIAL DRAFT".to_string(),
        position: DecorationPosition::TopCenter,
        layer: None,
    })
    .expect("AddPageDecoration failed");

    // 4. ApplyBatesNumbering
    doc.apply(Operation::ApplyBatesNumbering {
        pages: PageSelection::All,
        prefix: "CASE-".to_string(),
        start_number: 1,
        digits: 6,
        position: DecorationPosition::BottomRight,
    })
    .expect("ApplyBatesNumbering failed");

    // Verify page dictionary entries
    let arena = doc.inner().arena();
    let page_h = doc.inner().get_page_handle(0).expect("Page 0 missing");
    let page_dh = doc.inner().resolve_to_dict(page_h).expect("Page dict missing");
    let page_dict = arena.get_dict(page_dh).expect("Dict lookup failed");

    assert!(page_dict.contains_key(&arena.name("Annots")), "Annots missing");
    assert!(page_dict.contains_key(&arena.name("VP")), "VP (Viewport) missing");
    assert!(page_dict.contains_key(&arena.name("Contents")), "Contents missing");
}

#[test]
fn test_all_remaining_operations_execution() {
    use fepdf::PdfDocument;

    let mut doc = PdfDocument::create_empty().expect("Failed to create document");

    // 1. UpdateArticleThreads
    let thread = ArticleThread {
        title: "Feature Article".to_string(),
        beads: vec![ArticleBead { page: 0, rect: [50.0, 50.0, 300.0, 400.0] }],
    };
    doc.apply(Operation::UpdateArticleThreads(vec![thread])).expect("UpdateArticleThreads failed");

    // 2. SetOpenAction
    doc.apply(Operation::SetOpenAction(PdfAction::Named("FirstPage".to_string())))
        .expect("SetOpenAction failed");

    // 4. SetUnencryptedWrapper
    doc.apply(Operation::SetUnencryptedWrapper(wrapper(b"%PDF-2.0 mock encrypted")))
        .expect("SetUnencryptedWrapper failed");

    // 6. SetPronunciationLexicon — refused here: the lexicon is an entry of the structure
    // tree root (Table 354) and this document has no structure tree. Where it is written
    // when there is one is `reading_aloud_test.rs`'s.
    doc.apply(Operation::SetPronunciationLexicon {
        lexicon_xml_bytes: b"<?xml version=\"1.0\"?><lexicon/>".to_vec(),
    })
    .expect_err("a lexicon with no structure tree to name it");

    // 7. SetMeasurementScale
    let scale = MeasurementScale { page: 0, scale_ratio: 0.0254, unit_label: "in".to_string() };
    doc.apply(Operation::SetMeasurementScale(scale)).expect("SetMeasurementScale failed");

    // Verify catalog has the expected entries
    let arena = doc.inner().arena();
    let cadh = doc.inner().resolve_to_dict(doc.inner().catalog_handle().unwrap()).unwrap();
    let cdict = arena.get_dict(cadh).unwrap();

    assert!(cdict.contains_key(&arena.name("Threads")), "Threads missing");
    assert!(cdict.contains_key(&arena.name("OpenAction")), "OpenAction missing");
    assert!(cdict.contains_key(&arena.name("AF")), "AF missing");
    assert!(!cdict.contains_key(&arena.name("PL")), "a /PL ISO 32000-2 does not define");

    // Verify page dictionary entries
    let page_h = doc.inner().get_page_handle(0).expect("Page 0 missing");
    let page_dh = doc.inner().resolve_to_dict(page_h).expect("Page dict missing");
    let page_dict = arena.get_dict(page_dh).expect("Dict lookup failed");
    // A scale is a viewport's (Table 265); a page has no `/Measure` of its own (Table 31).
    assert!(!page_dict.contains_key(&arena.name("Measure")), "a /Measure Table 31 does not have");
    assert!(page_dict.contains_key(&arena.name("VP")), "the scale's viewport is missing");
}
