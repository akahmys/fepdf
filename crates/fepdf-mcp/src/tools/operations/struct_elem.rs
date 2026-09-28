//! Structural elements and Tagged PDF accessibility editing tools.

use super::page::execute_single_op;
use fepdf::{
    AttributeValue, Operation, Placement, StructAttribute, StructElemMove, StructElemUpdate,
    StructElemWrap, UserProperty, UserPropertyValue,
};
use schemars::JsonSchema;
use serde::Deserialize;

/// Arguments for updating a structural element's tag, and the text strings it states.
#[derive(Deserialize, JsonSchema)]
pub struct UpdateStructElemArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Handle index of the structural element object.
    pub handle_index: u32,
    /// New structure tag name (e.g. "H1", "P", "Figure", "Table").
    pub new_tag: Option<String>,
    /// Alternate text description for accessibility (Alt).
    pub alt_text: Option<String>,
    /// The language of the element's content (Lang), e.g. "en-GB"; "" states it unknown.
    pub lang: Option<String>,
    /// The text the element's content stands for (ActualText).
    pub actual_text: Option<String>,
    /// The expansion of an abbreviation or acronym (E).
    pub expansion: Option<String>,
}

/// Arguments for setting one attribute of a structure element.
#[derive(Deserialize, JsonSchema)]
pub struct SetStructAttributeArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Handle index of the structural element object.
    pub handle_index: u32,
    /// The attribute owner (/O): "Table", "List", "Layout", "ARIA-1.1", ….
    pub owner: String,
    /// The key: "Scope", "Headers", "ListNumbering", "Placement", ….
    pub key: String,
    /// A name value, such as "Column" or "Decimal".
    pub value_name: Option<String>,
    /// A number value.
    pub value_number: Option<f64>,
    /// A text string value.
    pub value_text: Option<String>,
    /// A boolean value.
    pub value_bool: Option<bool>,
    /// An array of names.
    pub value_names: Option<Vec<String>>,
    /// An array of numbers, such as a BBox.
    pub value_numbers: Option<Vec<f64>>,
    /// An array of byte strings, such as a cell's Headers (element IDs).
    pub value_strings: Option<Vec<String>>,
}

/// Arguments for setting the elements a structure element refers to.
#[derive(Deserialize, JsonSchema)]
pub struct SetStructRefsArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Handle index of the element that refers (a TOCI, a citation, a continued list).
    pub handle_index: u32,
    /// Handle indices of the elements it refers to; empty removes /Ref.
    pub targets: Vec<u32>,
}

/// Arguments for putting a structure element in a namespace.
#[derive(Deserialize, JsonSchema)]
pub struct SetStructNamespaceArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Handle index of the structural element.
    pub handle_index: u32,
    /// The namespace URI, e.g. "http://iso.org/pdf2/ssn"; "" returns it to the default.
    pub namespace: String,
}

/// Arguments for mapping a structure type of a namespace (RoleMapNS).
#[derive(Deserialize, JsonSchema)]
pub struct MapStructTypeArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// The namespace URI whose type is mapped.
    pub namespace: String,
    /// The type being mapped.
    pub from: String,
    /// The type it maps to.
    pub to: String,
    /// The namespace URI of the target type; the default standard namespace if absent.
    pub to_namespace: Option<String>,
}

/// Arguments for marking a tagged sequence as an artifact.
#[derive(Deserialize, JsonSchema)]
pub struct MarkArtifactArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Zero-based page index.
    pub page: usize,
    /// The MCID of the marked-content sequence on that page.
    pub mcid: i64,
    /// Artifact /Type: "Pagination", "Layout", "Page" or "Background".
    pub kind: Option<String>,
    /// Artifact /Subtype: "Header", "Footer" or "Watermark".
    pub subtype: Option<String>,
}

/// Arguments for wrapping a run of an element's kids in a new element.
#[derive(Deserialize, JsonSchema)]
pub struct WrapStructElemArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Handle index of the element, or the structure tree root, whose kids are wrapped.
    pub handle_index: u32,
    /// The first kid wrapped, counted from 0 in its /K.
    pub first: usize,
    /// How many kids are wrapped, from `first`.
    pub count: usize,
    /// The new element's structure type (e.g. "Caption", "Lbl", "LBody", "RB", "RT", "RP").
    pub tag: String,
}

/// Arguments for deleting a structural element from the tree.
#[derive(Deserialize, JsonSchema)]
pub struct DeleteStructElemArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Handle index of the structural element to delete.
    pub handle_index: u32,
}

/// Arguments for moving a structural element within the tree.
#[derive(Deserialize, JsonSchema)]
pub struct MoveStructElemArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Handle index of the structural element to move.
    pub handle_index: u32,
    /// Handle index of the element it moves relative to.
    pub target_index: u32,
    /// Where it lands: "before", "after" or "inside" the target.
    pub placement: String,
}

/// Arguments for a single user property.
#[derive(Deserialize, JsonSchema)]
pub struct UserPropertyArg {
    /// Name of the property.
    pub name: String,
    /// String value of the property.
    pub value_text: Option<String>,
    /// Numeric value of the property.
    pub value_number: Option<f64>,
    /// Boolean value of the property.
    pub value_bool: Option<bool>,
}

/// Arguments for adding user properties to a Tagged PDF element.
#[derive(Deserialize, JsonSchema)]
pub struct AddUserPropertiesArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Target structural element handle index.
    pub target_handle: u32,
    /// List of user properties to attach.
    pub properties: Vec<UserPropertyArg>,
}

/// Implementation of the update_struct_elem tool.
pub fn update_struct_elem_impl(args: UpdateStructElemArgs) -> Result<String, String> {
    let update = StructElemUpdate {
        handle_index: args.handle_index,
        new_tag: args.new_tag,
        new_alt: args.alt_text,
        new_lang: args.lang,
        new_actual_text: args.actual_text,
        new_expansion: args.expansion,
    };
    let op = Operation::UpdateStructElem(update);
    execute_single_op(
        &args.input_path,
        &args.output_path,
        op,
        &format!("Structural element #{} updated", args.handle_index),
    )
}

/// Implementation of the delete_struct_elem tool.
pub fn delete_struct_elem_impl(args: DeleteStructElemArgs) -> Result<String, String> {
    let op = Operation::DeleteStructElem { handle_index: args.handle_index };
    execute_single_op(
        &args.input_path,
        &args.output_path,
        op,
        &format!("Structural element #{} deleted", args.handle_index),
    )
}

/// Implementation of the move_struct_elem tool.
///
/// The placement is named rather than numbered: a caller writing `2` for "inside" has to
/// be told which number means what, and would get a different element either way.
pub fn move_struct_elem_impl(args: MoveStructElemArgs) -> Result<String, String> {
    let placement = match args.placement.to_ascii_lowercase().as_str() {
        "before" => Placement::Before,
        "after" => Placement::After,
        "inside" => Placement::Inside,
        other => {
            return Err(format!(
                "placement must be \"before\", \"after\" or \"inside\", not {other:?}"
            ));
        }
    };
    let op = Operation::MoveStructElem(StructElemMove {
        handle_index: args.handle_index,
        target_index: args.target_index,
        placement,
    });
    execute_single_op(
        &args.input_path,
        &args.output_path,
        op,
        &format!(
            "Structural element #{} moved {} #{}",
            args.handle_index, args.placement, args.target_index
        ),
    )
}

/// Implementation of the add_user_properties tool.
pub fn add_user_properties_impl(args: AddUserPropertiesArgs) -> Result<String, String> {
    let properties = args
        .properties
        .into_iter()
        .map(|p| {
            let val = if let Some(t) = p.value_text {
                UserPropertyValue::Text(t)
            } else if let Some(n) = p.value_number {
                UserPropertyValue::Number(n)
            } else if let Some(b) = p.value_bool {
                UserPropertyValue::Boolean(b)
            } else {
                UserPropertyValue::Text(String::new())
            };
            UserProperty { name: p.name, value: val, formatted: None }
        })
        .collect();

    let op = Operation::AddUserProperties { target_handle: args.target_handle, properties };
    execute_single_op(
        &args.input_path,
        &args.output_path,
        op,
        &format!("User properties attached to element #{}", args.target_handle),
    )
}

/// Implementation of the set_struct_attribute tool.
pub fn set_struct_attribute_impl(args: SetStructAttributeArgs) -> Result<String, String> {
    let given: Vec<AttributeValue> = [
        args.value_name.map(AttributeValue::Name),
        args.value_number.map(AttributeValue::Number),
        args.value_text.map(AttributeValue::Text),
        args.value_bool.map(AttributeValue::Boolean),
        args.value_names.map(AttributeValue::Names),
        args.value_numbers.map(AttributeValue::Numbers),
        args.value_strings.map(AttributeValue::Strings),
    ]
    .into_iter()
    .flatten()
    .collect();
    let [value] = <[AttributeValue; 1]>::try_from(given)
        .map_err(|given| format!("give exactly one value; {} were given", given.len()))?;
    let (owner, key) = (args.owner.clone(), args.key.clone());
    let op = Operation::SetStructAttribute(StructAttribute {
        handle_index: args.handle_index,
        owner: args.owner,
        key: args.key,
        value,
    });
    execute_single_op(
        &args.input_path,
        &args.output_path,
        op,
        &format!("/{owner} /{key} set on structural element #{}", args.handle_index),
    )
}

/// Implementation of the set_struct_refs tool.
pub fn set_struct_refs_impl(args: SetStructRefsArgs) -> Result<String, String> {
    let count = args.targets.len();
    let op = Operation::SetStructRefs { handle_index: args.handle_index, targets: args.targets };
    execute_single_op(
        &args.input_path,
        &args.output_path,
        op,
        &format!("Structural element #{} refers to {count} elements", args.handle_index),
    )
}

/// Implementation of the set_struct_namespace tool.
pub fn set_struct_namespace_impl(args: SetStructNamespaceArgs) -> Result<String, String> {
    let message = format!("Structural element #{} put in {:?}", args.handle_index, args.namespace);
    let op = Operation::SetStructNamespace {
        handle_index: args.handle_index,
        namespace: args.namespace,
    };
    execute_single_op(&args.input_path, &args.output_path, op, &message)
}

/// Implementation of the map_struct_type tool.
pub fn map_struct_type_impl(args: MapStructTypeArgs) -> Result<String, String> {
    let message = format!("{} mapped to {} in {}", args.from, args.to, args.namespace);
    let op = Operation::MapStructType {
        namespace: args.namespace,
        from: args.from,
        to: args.to,
        to_namespace: args.to_namespace,
    };
    execute_single_op(&args.input_path, &args.output_path, op, &message)
}

/// Implementation of the mark_artifact tool.
pub fn mark_artifact_impl(args: MarkArtifactArgs) -> Result<String, String> {
    let message = format!("MCID {} on page {} marked as an artifact", args.mcid, args.page);
    let op = Operation::MarkArtifact {
        page: args.page,
        mcid: args.mcid,
        kind: args.kind,
        subtype: args.subtype,
    };
    execute_single_op(&args.input_path, &args.output_path, op, &message)
}

/// Implementation of the wrap_struct_elem tool.
pub fn wrap_struct_elem_impl(args: WrapStructElemArgs) -> Result<String, String> {
    let message = format!(
        "{} kids of element {} from {} wrapped in a new {}",
        args.count, args.handle_index, args.first, args.tag
    );
    let op = Operation::WrapStructElem(StructElemWrap {
        handle_index: args.handle_index,
        first: args.first,
        count: args.count,
        tag: args.tag,
    });
    execute_single_op(&args.input_path, &args.output_path, op, &message)
}
