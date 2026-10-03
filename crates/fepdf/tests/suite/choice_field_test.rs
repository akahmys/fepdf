//! Choice fields (/FT /Ch) reading, value setting, and appearance generation (ISO 32000-2 12.7.4.4).

use fepdf::{ChoiceOption, FormFieldSpec, FormValue, InteractiveReport, PdfDocument};
use fepdf_doc::operation::Operation;
use kurbo::Affine;

use fepdf_fixtures::assemble;
use fepdf_fixtures::recorder::Recorder;

fn choice_form(field_extra: &str) -> Vec<u8> {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] \
         /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 6 0 R >> >> >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 100] /Annots [5 0 R] \
          /Contents 4 0 R >>"
            .to_string(),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
        format!(
            "<< /Type /Annot /Subtype /Widget /FT /Ch /T (Colors) \
             /Rect [20 40 280 70] /P 3 0 R {field_extra} >>"
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Name /Helv >>".to_string(),
    ];
    assemble(&bodies)
}

#[test]
fn test_read_combo_box_simple_options() {
    let pdf = choice_form("/Ff 131072 /Opt [(Red) (Green) (Blue)] /V (Green) /I [1]");
    let report = InteractiveReport::survey(&pdf).expect("reads");
    assert_eq!(report.form.fields, 1);
    let field = &report.form.terminal[0];
    assert_eq!(field.name.as_deref(), Some("Colors"));
    assert_eq!(field.field_type.as_deref(), Some("Ch"));
    assert!(field.is_combo());
    assert!(!field.is_multiselect());
    // Bit 19 (Edit) is clear: a plain combo box offers its `/Opt` list and nothing else.
    assert!(!field.is_editable_combo());
    assert_eq!(
        field.options,
        vec![
            ChoiceOption::simple("Red"),
            ChoiceOption::simple("Green"),
            ChoiceOption::simple("Blue"),
        ]
    );
    assert_eq!(field.value.as_deref(), Some("Green"));
    assert_eq!(field.selected_indices, vec![1]);
}

/// A combo box with bit 19 set lets the reader type a value that is not in `/Opt`.
///
/// `is_editable_combo` reads that bit, and until 2026-09-06 nothing called it — its two
/// siblings `is_combo` and `is_multiselect` were both asserted here and it was not. An
/// accessor over a `/Ff` bit that no test reaches is a bit the engine cannot be said to
/// read (RR-15 Rule 20 keeps the accessor; this makes it true).
#[test]
fn an_editable_combo_is_distinguished_from_a_plain_one() {
    // 131072 = bit 18 (Combo), 262144 = bit 19 (Edit).
    let pdf = choice_form("/Ff 393216 /Opt [(Red) (Green)] /V (Puce)");
    let report = InteractiveReport::survey(&pdf).expect("reads");
    let field = &report.form.terminal[0];
    assert!(field.is_combo());
    assert!(field.is_editable_combo());
    assert_eq!(field.value.as_deref(), Some("Puce"), "a typed value need not be in /Opt");
}

/// Edit without Combo is not an editable combo: Table 232 makes bit 19 meaningful only
/// with bit 18, and a list box with it set is still a list box.
#[test]
fn edit_without_combo_is_not_an_editable_combo() {
    let pdf = choice_form("/Ff 262144 /Opt [(Red) (Green)]");
    let report = InteractiveReport::survey(&pdf).expect("reads");
    let field = &report.form.terminal[0];
    assert!(!field.is_combo());
    assert!(!field.is_editable_combo());
}

#[test]
fn test_read_combo_box_export_display_pairs() {
    let pdf = choice_form(
        "/Ff 131072 /Opt [[(CA) (California)] [(NY) (New York)] [(TX) (Texas)]] /V (NY)",
    );
    let report = InteractiveReport::survey(&pdf).expect("reads");
    let field = &report.form.terminal[0];
    assert_eq!(
        field.options,
        vec![
            ChoiceOption::pair("CA", "California"),
            ChoiceOption::pair("NY", "New York"),
            ChoiceOption::pair("TX", "Texas"),
        ]
    );
    assert_eq!(field.value.as_deref(), Some("NY"));
    assert_eq!(field.selected_indices, vec![1]);
}

#[test]
fn test_read_list_box_multiselect() {
    let pdf = choice_form("/Ff 2097152 /Opt [(Item0) (Item1) (Item2) (Item3)] /I [0 2]");
    let report = InteractiveReport::survey(&pdf).expect("reads");
    let field = &report.form.terminal[0];
    assert!(!field.is_combo());
    assert!(field.is_multiselect());
    assert_eq!(field.options.len(), 4);
    assert_eq!(field.selected_indices, vec![0, 2]);
}

#[test]
fn test_choice_field_inheritance() {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] \
         /DA (/Helv 10 Tf 0 g) >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 100] /Annots [6 0 R] \
          /Contents 4 0 R >>"
            .to_string(),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
        // Parent non-terminal node carrying /FT and /Opt
        "<< /T (Parent) /FT /Ch /Ff 131072 /Opt [(First) (Second) (Third)] /Kids [6 0 R] >>"
            .to_string(),
        // Child terminal widget node
        "<< /Type /Annot /Subtype /Widget /T (Child) /V (Second) \
         /Rect [20 40 280 70] /P 3 0 R >>"
            .to_string(),
    ];
    let pdf = assemble(&bodies);
    let report = InteractiveReport::survey(&pdf).expect("reads");
    assert_eq!(report.form.fields, 1);
    let field = &report.form.terminal[0];
    assert_eq!(field.qualified_name.as_deref(), Some("Parent.Child"));
    assert_eq!(field.field_type.as_deref(), Some("Ch"));
    assert_eq!(field.options.len(), 3);
    assert_eq!(field.value.as_deref(), Some("Second"));
    assert_eq!(field.selected_indices, vec![1]);
}

#[test]
fn test_set_choice_field_value_and_appearance() {
    let pdf = choice_form("/Ff 131072 /Opt [(Small) (Medium) (Large)] /V (Small)");
    let mut doc = PdfDocument::open(pdf.into()).expect("opens");
    doc.apply(Operation::SetFormFieldValue(FormFieldSpec {
        name: "Colors".to_string(),
        value: FormValue::Choice("Large".to_string()),
    }))
    .expect("sets value");

    let val = fepdf::field_value(doc.inner(), "Colors");
    assert_eq!(val.as_deref(), Some("Large"));

    let out_dir = std::path::Path::new("target/tmp");
    let _ = std::fs::create_dir_all(out_dir);
    let out_path = out_dir.join("test_choice_set.pdf");
    doc.save_as_version(&out_path, "2.0").expect("saves");
    let saved = std::fs::read(&out_path).expect("reads saved file");
    let report = InteractiveReport::survey(&saved).expect("reads");
    let field = &report.form.terminal[0];
    assert_eq!(field.value.as_deref(), Some("Large"));
    assert_eq!(field.selected_indices, vec![2]);

    let mut drawn = Recorder::new();
    doc.render_page(0, &mut drawn, Affine::IDENTITY).expect("renders");
    assert_eq!(drawn.text(), "Large");
}

#[test]
fn test_set_choice_field_paired_display_appearance() {
    let pdf = choice_form(
        "/Ff 131072 /Opt [[(US) (United States)] [(JP) (Japan)] [(DE) (Germany)]] /V (US)",
    );
    let mut doc = PdfDocument::open(pdf.into()).expect("opens");
    doc.apply(Operation::SetFormFieldValue(FormFieldSpec {
        name: "Colors".to_string(),
        value: FormValue::Choice("JP".to_string()),
    }))
    .expect("sets value");

    let val = fepdf::field_value(doc.inner(), "Colors");
    assert_eq!(val.as_deref(), Some("JP"));

    let out_dir = std::path::Path::new("target/tmp");
    let _ = std::fs::create_dir_all(out_dir);
    let out_path = out_dir.join("test_choice_pair.pdf");
    doc.save_as_version(&out_path, "2.0").expect("saves");
    let saved = std::fs::read(&out_path).expect("reads saved file");
    let report = InteractiveReport::survey(&saved).expect("reads");
    let field = &report.form.terminal[0];
    assert_eq!(field.selected_indices, vec![1]);

    let mut drawn = Recorder::new();
    doc.render_page(0, &mut drawn, Affine::IDENTITY).expect("renders");
    assert_eq!(drawn.text(), "Japan");
}
