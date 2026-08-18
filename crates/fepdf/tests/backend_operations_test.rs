//! Integration and Unit Test Suite for ISO 32000-2 Extended Backend Operations.

use fepdf::{DecorationPosition, Operation, PageSelection};
use fepdf_model::{
    AFRelationship, AnnotationKind, AnnotationSpec, ArticleBead, ArticleThread, AssociatedFile,
    CollectionViewMode, FormFieldSpec, FormValue, GeoSpatialAnchor, LayerGroup, MeasurementScale,
    MeshShadingSpec, MeshShadingType, OptionalContentProperties, OutlineNode, OutlineTree,
    OutputIntent, PageLabelSpec, PageLabelStyle, PdfAction, PortfolioCollection, PortfolioItem,
    PublicKeyRecipientSpec, UnencryptedWrapperSpec, UserProperty, UserPropertyValue,
    VisibilityState,
};

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
            id: "layer_electrical".to_string(),
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
fn assemble_pdf(objs: &[&str], root: usize) -> Vec<u8> {
    use std::fmt::Write as _;
    let mut out = String::from("%PDF-2.0\n");
    let mut offsets = vec![0_usize];
    for (i, body) in objs.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{body}\nendobj\n", i + 1);
    }
    let xref_at = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1);
    for off in offsets.iter().skip(1) {
        let _ = writeln!(out, "{off:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root {root} 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
        objs.len() + 1
    );
    out.into_bytes()
}

#[test]
fn a_file_with_no_signature_reports_none() {
    let bytes = assemble_pdf(
        &["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [] /Count 0 >>"],
        1,
    );
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

    let op = Operation::ExecuteAction(action);
    if let Operation::ExecuteAction(PdfAction::GoToRemote { file_path, page }) = op {
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

#[test]
fn test_mesh_shading_operation() {
    let shading = MeshShadingSpec {
        shading_type: MeshShadingType::CoonsPatchMesh,
        color_space: "DeviceRGB".to_string(),
        data_bytes: vec![0, 1, 2, 3],
    };

    let op = Operation::AddMeshShading(shading);
    if let Operation::AddMeshShading(s) = op {
        assert_eq!(s.shading_type, MeshShadingType::CoonsPatchMesh);
        assert_eq!(s.data_bytes.len(), 4);
    } else {
        panic!("Operation variant mismatch");
    }
}

#[test]
fn test_unencrypted_wrapper_operation() {
    let wrapper = UnencryptedWrapperSpec {
        notice_message: "This PDF is encrypted.".to_string(),
        encrypted_payload_bytes: vec![10, 20, 30],
    };

    let op = Operation::SetUnencryptedWrapper(wrapper);
    if let Operation::SetUnencryptedWrapper(w) = op {
        assert_eq!(w.notice_message, "This PDF is encrypted.");
    } else {
        panic!("Operation variant mismatch");
    }
}

#[test]
fn test_public_key_recipient_operation() {
    let recipient = PublicKeyRecipientSpec {
        certificate_der_bytes: vec![1, 2, 3],
        encrypted_key_bytes: vec![4, 5, 6],
    };

    let op = Operation::AddPublicKeyRecipient(recipient);
    if let Operation::AddPublicKeyRecipient(r) = op {
        assert_eq!(r.certificate_der_bytes.len(), 3);
    } else {
        panic!("Operation variant mismatch");
    }
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
        icc_profile_bytes: Some(vec![1, 2, 3, 4, 5]),
    };
    doc.apply(Operation::SetOutputIntent(intent)).expect("SetOutputIntent failed");

    // 6. UpdateLayers
    let layers = OptionalContentProperties {
        layers: vec![LayerGroup {
            id: "ocg_1".to_string(),
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

    // 2. ExecuteAction
    doc.apply(Operation::ExecuteAction(PdfAction::Named("FirstPage".to_string())))
        .expect("ExecuteAction failed");

    // 3. AddMeshShading
    let shading = MeshShadingSpec {
        shading_type: MeshShadingType::CoonsPatchMesh,
        color_space: "DeviceRGB".to_string(),
        data_bytes: vec![0u8; 32],
    };
    doc.apply(Operation::AddMeshShading(shading)).expect("AddMeshShading failed");

    // 4. SetUnencryptedWrapper
    let wrapper = UnencryptedWrapperSpec {
        notice_message: "Please use PDF 2.0 compliant viewer".to_string(),
        encrypted_payload_bytes: b"%PDF-2.0 mock encrypted".to_vec(),
    };
    doc.apply(Operation::SetUnencryptedWrapper(wrapper)).expect("SetUnencryptedWrapper failed");

    // 5. AddPublicKeyRecipient
    let recipient = PublicKeyRecipientSpec {
        certificate_der_bytes: vec![0x30, 0x82, 0x01, 0x0a],
        encrypted_key_bytes: vec![0xaa, 0xbb, 0xcc],
    };
    doc.apply(Operation::AddPublicKeyRecipient(recipient)).expect("AddPublicKeyRecipient failed");

    // 6. SetPronunciationLexicon
    doc.apply(Operation::SetPronunciationLexicon {
        lexicon_xml_bytes: b"<?xml version=\"1.0\"?><lexicon/>".to_vec(),
    })
    .expect("SetPronunciationLexicon failed");

    // 7. SetMeasurementScale
    let scale = MeasurementScale { page: 0, scale_ratio: 0.0254, unit_label: "in".to_string() };
    doc.apply(Operation::SetMeasurementScale(scale)).expect("SetMeasurementScale failed");

    // Verify catalog has the expected entries
    let arena = doc.inner().arena();
    let cadh = doc.inner().resolve_to_dict(doc.inner().catalog_handle().unwrap()).unwrap();
    let cdict = arena.get_dict(cadh).unwrap();

    assert!(cdict.contains_key(&arena.name("Threads")), "Threads missing");
    assert!(cdict.contains_key(&arena.name("OpenAction")), "OpenAction missing");
    assert!(cdict.contains_key(&arena.name("Resources")), "Resources missing");
    assert!(cdict.contains_key(&arena.name("AF")), "AF missing");
    assert!(cdict.contains_key(&arena.name("Encrypt")), "Encrypt missing");
    assert!(cdict.contains_key(&arena.name("PL")), "PL missing");

    // Verify page dictionary entries
    let page_h = doc.inner().get_page_handle(0).expect("Page 0 missing");
    let page_dh = doc.inner().resolve_to_dict(page_h).expect("Page dict missing");
    let page_dict = arena.get_dict(page_dh).expect("Dict lookup failed");
    assert!(page_dict.contains_key(&arena.name("Measure")), "Measure missing");
}
