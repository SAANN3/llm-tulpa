//! Derive macro for `tools::base::ToolParams`.
mod helper;

use helper::field_properties;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

/// Implements `ToolParams` for a tool's args struct (e.g. `TemperatureArgs`), generating
/// `tool_properties()` from each named field's type and `#[tool(description = "...")]`
/// attribute. Lets `Tool::required_properties` delegate to `<Args>::tool_properties()`
/// instead of hand-writing the `Vec<PropertyInfo>` literal per tool.
///
/// Expects `PropertyInfo`, `PropertyType`, and `ToolParams` to already be in scope at
/// the call site — the generated code references them unqualified rather than by a
/// fixed path, since this macro's own crate has no dependency on (and can't depend on)
/// the crate defining those types.
#[proc_macro_derive(ToolParams, attributes(tool))]
pub fn derive_tool_params(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(
                    &input.ident,
                    "ToolParams only supports structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input.ident, "ToolParams can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    let properties = match field_properties(fields) {
        Ok(properties) => properties,
        Err(err) => return err.to_compile_error().into(),
    };

    let ident = &input.ident;
    quote! {
        impl ToolParams for #ident {
            fn tool_properties() -> Vec<PropertyInfo> {
                vec![#(#properties),*]
            }
        }
    }
    .into()
}
