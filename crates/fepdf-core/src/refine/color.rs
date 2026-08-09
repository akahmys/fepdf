//! Color Refinement: Color space normalization using moxcms.

use crate::object::PdfName;
use crate::refine::RefinedObject;
use std::collections::BTreeMap;

/// Normalizes a color space object.
///
/// If the object is a device color space (e.g., /DeviceRGB), it is
/// converted to a high-purity ICC-based representation.
pub fn normalize_colorspace(name: &PdfName) -> Option<RefinedObject> {
    match name.as_str() {
        "DeviceRGB" => {
            // In a real implementation, we would return a refined ICCBased object.
            // For now, we'll keep it as-is or tag it for conversion.
            None
        }
        "DeviceCMYK" => {
            // Tags for normalization
            None
        }
        _ => None,
    }
}

/// Refines a dictionary to ensure all color-related keys are normalized.
pub fn refine_palette(dict: &mut BTreeMap<PdfName, RefinedObject>) {
    let cs_name = PdfName::new("ColorSpace");
    if let Some(cs) = dict.get(&cs_name)
        && let RefinedObject::Name(name) = cs
        && let Some(refined) = normalize_colorspace(name)
    {
        dict.insert(cs_name, refined);
    }
}
