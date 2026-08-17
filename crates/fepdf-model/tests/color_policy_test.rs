//! Tests for ColorPolicy validation and refinement (ISO 32000-2 Clause 8.6).

use fepdf_model::ingest::ColorPolicy;
use fepdf_model::object::PdfName;
use fepdf_model::refine::RefinedObject;
use fepdf_model::refine::color::{refine_palette, validate_and_refine_colorspace};
use std::collections::BTreeMap;

#[test]
fn test_strict_color_policy_rejects_malformed_and_unknown() {
    let mut issues = Vec::new();

    // Unknown color space name
    let unknown_cs = RefinedObject::Name(PdfName::new("CustomUnknownSpace"));
    let res = validate_and_refine_colorspace(&unknown_cs, ColorPolicy::Strict, &mut issues);
    assert!(res.is_none());
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].clause, "8.6");

    // Malformed array
    let malformed_array = RefinedObject::Array(vec![RefinedObject::Name(PdfName::new("CalRGB"))]);
    let res_arr =
        validate_and_refine_colorspace(&malformed_array, ColorPolicy::Strict, &mut issues);
    assert!(res_arr.is_none());
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[1].clause, "8.6.5");
}

#[test]
fn test_relaxed_color_policy_repairs_malformed_to_fallback() {
    let mut issues = Vec::new();

    // Unknown color space name repaired to DeviceRGB
    let unknown_cs = RefinedObject::Name(PdfName::new("CustomUnknownSpace"));
    let res = validate_and_refine_colorspace(&unknown_cs, ColorPolicy::Relaxed, &mut issues);
    assert!(res.is_some());
    if let Some(RefinedObject::Name(n)) = res {
        assert_eq!(n.as_str(), "DeviceRGB");
    } else {
        panic!("Expected RefinedObject::Name(DeviceRGB)");
    }
    assert_eq!(issues.len(), 1);

    // Empty array repaired to DeviceRGB
    let empty_arr = RefinedObject::Array(vec![]);
    let res_empty = validate_and_refine_colorspace(&empty_arr, ColorPolicy::Relaxed, &mut issues);
    assert!(res_empty.is_some());
    if let Some(RefinedObject::Name(n)) = res_empty {
        assert_eq!(n.as_str(), "DeviceRGB");
    } else {
        panic!("Expected RefinedObject::Name(DeviceRGB)");
    }

    // Refine palette dictionary
    let mut dict = BTreeMap::new();
    dict.insert(PdfName::new("ColorSpace"), RefinedObject::Name(PdfName::new("MalformedSpace")));
    refine_palette(&mut dict, ColorPolicy::Relaxed, &mut issues);
    let refined_cs = dict.get(&PdfName::new("ColorSpace")).unwrap();
    if let RefinedObject::Name(n) = refined_cs {
        assert_eq!(n.as_str(), "DeviceRGB");
    } else {
        panic!("Expected refined ColorSpace to be DeviceRGB");
    }
}
