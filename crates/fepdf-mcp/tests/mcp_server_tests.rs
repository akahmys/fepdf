//! Integration tests for Model Context Protocol (MCP) Server logic, tools, resources, and prompts.

#![allow(clippy::redundant_clone)]

use fepdf_mcp::McpError;
use fepdf_mcp::prompts::{prompt_audit_accessibility, prompt_remediate_pdf_ua};
use fepdf_mcp::resources::{
    read_audit_resource, read_metadata_resource, read_struct_tree_resource,
};
use fepdf_mcp::tools::{
    AddAnnotationArgs, AddPageDecorationArgs, ApplyBatesNumberingArgs, AttachAssociatedFileArgs,
    AuditArgs, ExtractTextArgs, LayerArg, OutlineNodeArg, RedactDocumentArgs, RedactionTarget,
    RemovePagesArgs, ReorderPagesArgs, RotatePagesArgs, SetFormFieldValueArgs,
    SetMeasurementScaleArgs, SetOutputIntentArgs, UpdateLayersArgs, UpdateOutlinesArgs,
    UpdateStructElemArgs, VerifySignaturesArgs, add_annotation_impl, add_page_decoration_impl,
    apply_bates_numbering_impl, apply_redaction_impl, attach_associated_file_impl,
    audit_document_impl, extract_text_impl, remove_pages_impl, reorder_pages_impl,
    rotate_pages_impl, set_form_field_value_impl, set_measurement_scale_impl,
    set_output_intent_impl, update_layers_impl, update_outlines_impl, update_struct_elem_impl,
    verify_signatures_impl,
};
use std::fs;
use std::path::PathBuf;

fn get_sample_path() -> String {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates
    p.pop(); // root
    p.push("samples");
    p.push("sample.pdf");
    p.to_string_lossy().to_string()
}

fn temp_out(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!("mcp_test_{name}.pdf"));
    p.to_string_lossy().to_string()
}

#[test]
fn test_mcp_error_conversion() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "PDF file missing");
    let mcp_err: McpError = io_err.into();
    assert!(format!("{mcp_err}").contains("PDF file missing"));
}

#[test]
fn test_extract_text_tool() {
    let sample = get_sample_path();
    if !std::path::Path::new(&sample).exists() {
        return;
    }

    let args = ExtractTextArgs { path: sample, page_range: Some("0".into()) };
    let res = extract_text_impl(args);
    assert!(res.is_ok());
    let json_str = res.unwrap();
    assert!(json_str.contains("total_pages"));
}

#[test]
fn test_audit_and_signature_tools() {
    let sample = get_sample_path();
    if !std::path::Path::new(&sample).exists() {
        return;
    }

    let audit_res = audit_document_impl(AuditArgs { path: sample.clone() });
    assert!(audit_res.is_ok());

    let sig_res =
        verify_signatures_impl(VerifySignaturesArgs { path: sample, allow_network: false });
    assert!(sig_res.is_ok());
}

#[test]
fn test_page_operations() {
    let sample = get_sample_path();
    if !std::path::Path::new(&sample).exists() {
        return;
    }

    // 1. Rotate
    let out_rotate = temp_out("rotate");
    let rot_res = rotate_pages_impl(RotatePagesArgs {
        input_path: sample,
        output_path: out_rotate.clone(),
        selection: Some("all".into()),
        angle: 90,
        relative: Some(true),
    });
    assert!(rot_res.is_ok());

    // 2. Reorder
    let out_reorder = temp_out("reorder");
    let reorder_res = reorder_pages_impl(ReorderPagesArgs {
        input_path: out_rotate,
        output_path: out_reorder.clone(),
        from: 0,
        to: 0,
    });
    assert!(reorder_res.is_ok());

    // 3. Remove pages
    let out_remove = temp_out("remove");
    let remove_res = remove_pages_impl(RemovePagesArgs {
        input_path: out_reorder,
        output_path: out_remove,
        pages: "1".into(),
    });
    assert!(remove_res.is_ok());
}

#[test]
fn test_decoration_and_annotation_tools() {
    let sample = get_sample_path();
    if !std::path::Path::new(&sample).exists() {
        return;
    }

    // Decoration
    let out_dec = temp_out("dec");
    let dec_res = add_page_decoration_impl(AddPageDecorationArgs {
        input_path: sample,
        output_path: out_dec.clone(),
        pages: Some("all".into()),
        text: "CONFIDENTIAL".into(),
        position: "top_center".into(),
    });
    assert!(dec_res.is_ok());

    // Bates
    let out_bates = temp_out("bates");
    let bates_res = apply_bates_numbering_impl(ApplyBatesNumberingArgs {
        input_path: out_dec,
        output_path: out_bates.clone(),
        pages: None,
        prefix: Some("TEST-".into()),
        start_number: Some(100),
        digits: Some(6),
        position: Some("bottom_right".into()),
    });
    assert!(bates_res.is_ok());

    // Annotation
    let out_annot = temp_out("annot");
    let annot_res = add_annotation_impl(AddAnnotationArgs {
        input_path: out_bates,
        output_path: out_annot.clone(),
        page: 0,
        rect: [100.0, 100.0, 200.0, 150.0],
        contents: "Test Comment".into(),
        kind: Some("text".into()),
    });
    assert!(annot_res.is_ok());

    // Measurement scale
    let out_measure = temp_out("measure");
    let measure_res = set_measurement_scale_impl(SetMeasurementScaleArgs {
        input_path: out_annot,
        output_path: out_measure.clone(),
        page: 0,
        scale_ratio: 0.5,
        unit_label: "mm".into(),
    });
    assert!(measure_res.is_ok());

    // Form field
    let out_form = temp_out("form");
    let form_res = set_form_field_value_impl(SetFormFieldValueArgs {
        input_path: out_measure,
        output_path: out_form,
        field_name: "SignatureField".into(),
        value_text: Some("Approved".into()),
        value_bool: None,
    });
    assert!(form_res.is_ok());
}

#[test]
fn test_metadata_and_structure_tools() {
    let sample = get_sample_path();
    if !std::path::Path::new(&sample).exists() {
        return;
    }

    // Outlines
    let out_outline = temp_out("outline");
    let outline_res = update_outlines_impl(UpdateOutlinesArgs {
        input_path: sample,
        output_path: out_outline.clone(),
        roots: vec![OutlineNodeArg {
            title: "Chapter 1".into(),
            dest_page: Some(0),
            children: None,
        }],
    });
    assert!(outline_res.is_ok());

    // Layers
    let out_layers = temp_out("layers");
    let layers_res = update_layers_impl(UpdateLayersArgs {
        input_path: out_outline,
        output_path: out_layers.clone(),
        layers: vec![LayerArg {
            id: Some("L1".into()),
            name: "Background".into(),
            default_state: Some("on".into()),
        }],
    });
    assert!(layers_res.is_ok());

    // Associated File
    let dummy_txt = temp_out("dummy.txt");
    let _ = fs::write(&dummy_txt, b"Sample AF data");
    let out_af = temp_out("af");
    let af_res = attach_associated_file_impl(AttachAssociatedFileArgs {
        input_path: out_layers,
        output_path: out_af.clone(),
        file_path: dummy_txt,
        filename: Some("data.txt".into()),
        relationship: Some("supplement".into()),
        mime_type: Some("text/plain".into()),
    });
    assert!(af_res.is_ok());

    // OutputIntent
    let out_oi = temp_out("oi");
    let oi_res = set_output_intent_impl(SetOutputIntentArgs {
        input_path: out_af,
        output_path: out_oi.clone(),
        subtype: Some("GTS_PDFX".into()),
        identifier: "FOGRA39".into(),
        info: Some("Offset printing".into()),
    });
    assert!(oi_res.is_ok());

    // StructElem
    let out_se = temp_out("se");
    let se_res = update_struct_elem_impl(UpdateStructElemArgs {
        input_path: out_oi,
        output_path: out_se,
        handle_index: 0,
        new_tag: Some("H1".into()),
        alt_text: Some("Heading Alt".into()),
    });
    assert!(se_res.is_ok());
}

#[test]
fn test_redaction_tool() {
    let sample = get_sample_path();
    if !std::path::Path::new(&sample).exists() {
        return;
    }

    let out_redact = temp_out("redact");
    let redact_res = apply_redaction_impl(RedactDocumentArgs {
        input_path: sample,
        output_path: out_redact,
        targets: vec![RedactionTarget { page: 0, rect: [50.0, 50.0, 150.0, 100.0] }],
    });
    assert!(redact_res.is_ok());
}

#[test]
fn test_resources_and_prompts() {
    let sample = get_sample_path();
    if !std::path::Path::new(&sample).exists() {
        return;
    }

    // Resources
    let st_res = read_struct_tree_resource(&sample);
    assert!(st_res.is_ok());

    let meta_res = read_metadata_resource(&sample);
    assert!(meta_res.is_ok());

    let audit_res = read_audit_resource(&sample);
    assert!(audit_res.is_ok());

    // Prompts
    let prompt1 = prompt_audit_accessibility(&sample);
    assert!(prompt1.contains("PDF/UA-2"));

    let prompt2 = prompt_remediate_pdf_ua(&sample, "output.pdf");
    assert!(prompt2.contains("remediation"));
}
