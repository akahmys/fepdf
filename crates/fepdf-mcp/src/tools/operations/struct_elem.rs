//! Structural elements and Tagged PDF accessibility editing tools.

use super::page::execute_single_op;
use fepdf::{Operation, StructElemUpdate, UserProperty, UserPropertyValue};
use schemars::JsonSchema;
use serde::Deserialize;

/// Arguments for updating a structural element tag or Alt text.
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
