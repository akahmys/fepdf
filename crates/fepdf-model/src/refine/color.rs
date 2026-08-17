//! Color Refinement: Color space validation and normalization (ISO 32000-2 Clause 8.6).

use crate::ingest::ColorPolicy;
use crate::interpretation::Decision;
use crate::object::PdfName;
use crate::refine::RefinedObject;
use std::collections::BTreeMap;

/// Validates a direct color space name.
fn validate_colorspace_name(
    name: &PdfName,
    policy: ColorPolicy,
    issues: &mut Vec<Decision>,
) -> Option<RefinedObject> {
    match name.as_str() {
        "DeviceGray" | "DeviceRGB" | "DeviceCMYK" | "Pattern" => None,
        unknown => match policy {
            ColorPolicy::Strict => {
                issues.push(Decision::violation(
                    "8.6",
                    "Unknown or non-standard device colour space name",
                    format!("/{} is not a recognized standard colour space", unknown),
                ));
                None
            }
            ColorPolicy::Relaxed => {
                issues.push(Decision::repaired(
                    "8.6",
                    "Unknown colour space name",
                    format!("Substituted /DeviceRGB for unrecognized /{}", unknown),
                ));
                Some(RefinedObject::Name(PdfName::new("DeviceRGB")))
            }
        },
    }
}

///// Validates CIE-based and ICCBased color space families.
fn validate_cie_or_icc(
    family: &str,
    len: usize,
    policy: ColorPolicy,
    issues: &mut Vec<Decision>,
) -> Option<RefinedObject> {
    match family {
        "CalGray" | "CalRGB" | "Lab" if len < 2 => match policy {
            ColorPolicy::Strict => {
                issues.push(Decision::violation(
                    "8.6.5",
                    "CIE-based colour space missing parameter dictionary",
                    format!("/{} requires a parameter dictionary containing /WhitePoint", family),
                ));
                None
            }
            ColorPolicy::Relaxed => {
                issues.push(Decision::repaired(
                    "8.6.5",
                    "CIE-based colour space missing parameter dictionary",
                    format!("Substituted /DeviceRGB for malformed /{}", family),
                ));
                Some(RefinedObject::Name(PdfName::new("DeviceRGB")))
            }
        },
        "ICCBased" if len < 2 => match policy {
            ColorPolicy::Strict => {
                issues.push(Decision::violation(
                    "8.6.5.5",
                    "/ICCBased colour space missing stream",
                    "/ICCBased requires an ICC profile stream",
                ));
                None
            }
            ColorPolicy::Relaxed => {
                issues.push(Decision::repaired(
                    "8.6.5.5",
                    "/ICCBased colour space missing stream",
                    "Substituted /DeviceRGB for malformed /ICCBased array",
                ));
                Some(RefinedObject::Name(PdfName::new("DeviceRGB")))
            }
        },
        _ => None,
    }
}

/// Validates Indexed and Special color space families.
fn validate_indexed_or_special(
    family: &str,
    len: usize,
    policy: ColorPolicy,
    issues: &mut Vec<Decision>,
) -> Option<RefinedObject> {
    match family {
        "Indexed" if len < 4 => match policy {
            ColorPolicy::Strict => {
                issues.push(Decision::violation(
                    "8.6.6.3",
                    "/Indexed colour space missing required entries",
                    "/Indexed requires base, hival, and lookup table",
                ));
                None
            }
            ColorPolicy::Relaxed => {
                issues.push(Decision::repaired(
                    "8.6.6.3",
                    "/Indexed colour space missing required entries",
                    "Substituted /DeviceRGB for malformed /Indexed array",
                ));
                Some(RefinedObject::Name(PdfName::new("DeviceRGB")))
            }
        },
        "Separation" | "DeviceN" if len < 4 => match policy {
            ColorPolicy::Strict => {
                issues.push(Decision::violation(
                    "8.6.6.4",
                    "Special colour space missing transform or alternate space",
                    format!("/{} requires names, alternateSpace, and tintTransform", family),
                ));
                None
            }
            ColorPolicy::Relaxed => {
                issues.push(Decision::repaired(
                    "8.6.6.4",
                    "Special colour space missing transform",
                    format!("Substituted /DeviceRGB for malformed /{}", family),
                ));
                Some(RefinedObject::Name(PdfName::new("DeviceRGB")))
            }
        },
        _ => None,
    }
}

/// Validates a color space array specification.
fn validate_colorspace_array(
    items: &[RefinedObject],
    policy: ColorPolicy,
    issues: &mut Vec<Decision>,
) -> Option<RefinedObject> {
    if items.is_empty() {
        match policy {
            ColorPolicy::Strict => {
                issues.push(Decision::violation(
                    "8.6",
                    "Empty colour space array",
                    "Colour space array must contain at least a family name",
                ));
                return None;
            }
            ColorPolicy::Relaxed => {
                issues.push(Decision::repaired(
                    "8.6",
                    "Empty colour space array",
                    "Substituted /DeviceRGB for empty colour space array",
                ));
                return Some(RefinedObject::Name(PdfName::new("DeviceRGB")));
            }
        }
    }

    let family = items[0].as_name().map(|n| n.as_str()).unwrap_or("");
    if let Some(repaired) = validate_cie_or_icc(family, items.len(), policy, issues) {
        return Some(repaired);
    }
    validate_indexed_or_special(family, items.len(), policy, issues)
}

/// Validates and normalizes a color space object according to the ingestion policy.
pub fn validate_and_refine_colorspace(
    obj: &RefinedObject,
    policy: ColorPolicy,
    issues: &mut Vec<Decision>,
) -> Option<RefinedObject> {
    match obj {
        RefinedObject::Name(name) => validate_colorspace_name(name, policy, issues),
        RefinedObject::Array(items) => validate_colorspace_array(items, policy, issues),
        _ => None,
    }
}

/// Refines a dictionary to ensure all color-related keys are validated and normalized.
pub fn refine_palette(
    dict: &mut BTreeMap<PdfName, RefinedObject>,
    policy: ColorPolicy,
    issues: &mut Vec<Decision>,
) {
    let cs_key = PdfName::new("ColorSpace");
    if let Some(cs_obj) = dict.get(&cs_key)
        && let Some(refined) = validate_and_refine_colorspace(cs_obj, policy, issues)
    {
        dict.insert(cs_key, refined);
    }
}
