//! The facade's integration tests, as one binary (ROADMAP Y-9): each file is a module.

mod actions_test;
mod actual_text_test;
mod annotation_appearance_test;
mod annotation_drawing_test;
mod arena_growth_test;
mod arlington_test;
mod audit_scope_test;
mod backend_operations_test;
mod calculation_order_test;
mod choice_field_test;
mod cid_to_gid_map_test;
mod cjk_extraction_test;
mod close_path_test;
mod colour_space_test;
mod combine_pages_test;
mod compare_test;
mod compliance_clauses_test;
mod create_empty_test;
mod create_field_test;
mod crop_image_test;
mod crop_path_test;
mod crop_removes_test;
mod declaration_test;
mod direct_font_test;
mod edit_run_test;
mod edit_xobject_test;
mod encrypted_objstm_test;
mod extract_pages_test;
mod field_order_test;
mod find_across_runs_test;
mod font_census_test;
mod form_appearance_test;
mod form_filling_test;
mod form_xobject_test;
mod function_shading_test;
mod glyph_list_test;
mod graphics_state_parameters_test;
mod gs_font_test;
mod icc_rendering_test;
mod image_sample_count_test;
mod japanese_text_test;
mod layer_content_test;
mod layer_toggle_test;
mod linearized_hint_test;
mod mac_roman_test;
mod mark_artifact_test;
mod marked_content_boxes_test;
mod match_box_test;
mod measurement_test;
mod one_face_per_operation_test;
mod op_index_test;
mod open_form_test;
mod optional_content_test;
mod outline_tree_test;
mod page_decoration_test;
mod page_selection_test;
mod parser_twin_test;
mod pattern_color_test;
#[cfg(feature = "render")]
mod procset_test;
mod rasteriser_determinism_test;
mod reading_aloud_test;
mod redaction_test;
mod render_region_test;
mod rendering_mode_test;
mod resize_pages_test;
mod run_position_test;
mod sample_corpus_test;
mod save_metadata_test;
mod scanned_resize_test;
mod sdk_tests;
mod smask_in_data_test;
mod spacing_reaches_extraction_test;
mod split_page_test;
mod struct_attribute_test;
mod structure_boxes_test;
mod structure_language_test;
mod structure_move_test;
mod structure_tree_test;
mod text_layer_test;
mod text_string_encoding_test;
mod transparency_test;
mod undecodable_image_test;
mod vacuum_test;
mod wrap_struct_test;
mod written_annotation_draws_test;
mod written_text_reads_back_test;
mod xmp_claims_survive_test;

/// **Every file here is a module, and none changes what the whole process shares.**
///
/// A file this directory holds and `main.rs` does not name is never compiled, so its
/// tests pass by not running. And one binary is one process: a test that set an
/// environment variable or the working directory would reach every test after it, which
/// the 82 binaries this was could not do.
#[test]
fn every_file_is_a_module_and_none_changes_the_process() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/suite");
    let main = std::fs::read_to_string(dir.join("main.rs")).expect("main.rs reads");
    let mut unnamed = Vec::new();
    let mut shared = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("the suite directory reads").flatten() {
        let path = entry.path();
        let Some(name) = path.file_stem().and_then(|n| n.to_str()) else { continue };
        if path.extension().and_then(|e| e.to_str()) != Some("rs") || name == "main" {
            continue;
        }
        if !main.contains(&format!("mod {name};")) {
            unnamed.push(name.to_string());
        }
        let text = std::fs::read_to_string(&path).expect("the test file reads");
        if ["set_var(", "remove_var(", "set_current_dir("].iter().any(|c| text.contains(c)) {
            shared.push(name.to_string());
        }
    }
    assert!(unnamed.is_empty(), "files main.rs does not name, so nothing runs: {unnamed:?}");
    assert!(shared.is_empty(), "files changing process-wide state: {shared:?}");
}
