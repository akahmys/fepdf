//! Integration and Unit Test Suite for ISO 32000-2 Extended Backend Operations.

use fepdf_model::{
    AFRelationship, AnnotationKind, AnnotationSpec, ArticleBead, ArticleThread, AssociatedFile,
    CollectionViewMode, FormFieldSpec, FormValue, GeoSpatialAnchor, LayerGroup, MeasurementScale,
    MeshShadingSpec, MeshShadingType, OptionalContentProperties, OutlineNode, OutlineTree,
    OutputIntent, PageLabelSpec, PageLabelStyle, PdfAction, PortfolioCollection, PortfolioItem,
    PublicKeyRecipientSpec, UnencryptedWrapperSpec, UserProperty, UserPropertyValue,
    VisibilityState,
};
use fepdf_sdk::{DecorationPosition, Operation, PageSelection, PkiValidator, SignatureStatus};

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

#[test]
fn test_pki_validator_empty_stream() {
    let report = PkiValidator::validate_signature_bytes("Sig1", &[]).unwrap();
    assert_eq!(report.status, SignatureStatus::NotASignatureField);
    assert_eq!(report.field_name, "Sig1");
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
