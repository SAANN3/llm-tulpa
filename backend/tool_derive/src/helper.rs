use proc_macro2::TokenStream;
use quote::quote;
use syn::{punctuated::Punctuated, Error, Field, LitStr, Token, Type};

/// Builds one `PropertyInfo { ... }` construction expression per named field, reading
/// the field's type (mapped to a `PropertyType` variant, unwrapping `Option<T>` first so
/// the schema describes `T`) and its `#[tool(description = "...")]` attribute. A field's
/// `required` comes from whether it's `Option<T>` — that's the one signal we have for
/// "the model doesn't have to supply this."
pub fn field_properties(fields: &Punctuated<Field, Token![,]>) -> Result<Vec<TokenStream>, Error> {
    fields
        .iter()
        .map(|field| {
            let ident = field
                .ident
                .as_ref()
                .ok_or_else(|| Error::new_spanned(field, "tuple struct fields aren't supported"))?;
            let name = ident.to_string();
            let (schema_ty, required) = match option_inner_type(&field.ty) {
                Some(inner) => (inner, false),
                None => (&field.ty, true),
            };
            let (description, nested) = metadata_for(field)?;
            let property_type = property_type_for(schema_ty, nested)?;

            Ok(quote! {
                PropertyInfo {
                    name: #name.to_string(),
                    property_type: #property_type,
                    description: #description.to_string(),
                    required: #required,
                }
            })
        })
        .collect()
}

/// If `ty` is `Option<T>`, returns `T`. Only recognizes the bare `Option<...>` path
/// (not a renamed/aliased import), same scope of "good enough, not exhaustive" as
/// `property_type_for` below.
fn option_inner_type(ty: &Type) -> Option<&Type> {
    generic_inner_type(ty, "Option")
}

/// If `ty` is `Vec<T>`, returns `T`: the element type an array parameter's `items` describes.
fn vec_inner_type(ty: &Type) -> Option<&Type> {
    generic_inner_type(ty, "Vec")
}

fn generic_inner_type<'a>(ty: &'a Type, wrapper: &str) -> Option<&'a Type> {
    let Type::Path(type_path) = ty else { return None };
    let segment = type_path.path.segments.last()?;
    if segment.ident != wrapper {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else { return None };
    match args.args.first()? {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

/// Unknown attribute keys are errors so a typo cannot silently produce an incomplete schema.
fn metadata_for(field: &Field) -> Result<(String, bool), Error> {
    let attr = field
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("tool"))
        .ok_or_else(|| {
            Error::new_spanned(field, "missing #[tool(description = \"...\")] attribute")
        })?;

    let mut description = None;
    let mut nested = false;
    attr.meta.require_list()?.parse_nested_meta(|meta| {
        if meta.path.is_ident("description") {
            let value: LitStr = meta.value()?.parse()?;
            description = Some(value.value());
            Ok(())
        } else if meta.path.is_ident("nested") {
            nested = true;
            Ok(())
        } else {
            Err(meta.error("unsupported key in #[tool(...)], expected `description` or `nested`"))
        }
    })?;

    description.map(|description| (description, nested)).ok_or_else(|| Error::new_spanned(attr, "expected description = \"...\""))
}

/// Maps a Rust field type to the `PropertyType` the model's tool schema expects.
/// Deliberately narrow (primitives + `Vec`) — anything else falls back to `Object`
/// rather than failing, since a wrong-but-present schema is easier to debug than a
/// macro that refuses to compile on an unrecognized type.
fn property_type_for(ty: &Type, nested: bool) -> Result<TokenStream, Error> {
    let Type::Path(type_path) = ty else {
        return Err(Error::new_spanned(ty, "unsupported type for a tool parameter"));
    };
    let ident = type_path
        .path
        .segments
        .last()
        .ok_or_else(|| Error::new_spanned(ty, "unsupported type for a tool parameter"))?
        .ident
        .to_string();

    let variant = match ident.as_str() {
        "String" | "str" => quote! { PropertyType::String },
        "f32" | "f64" => quote! { PropertyType::Number },
        "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "u64" | "usize" => {
            quote! { PropertyType::Integer }
        }
        "bool" => quote! { PropertyType::Boolean },
        "Vec" => {
            let element = vec_inner_type(ty).ok_or_else(|| Error::new_spanned(ty, "an array needs an element type"))?;
            let items = property_type_for(element, nested)?;
            quote! { PropertyType::Array { items: Box::new(#items) } }
        }
        _ if nested => quote! { PropertyType::Object { properties: <#ty as ToolParams>::tool_properties() } },
        _ => quote! { PropertyType::Object { properties: vec![] } },
    };

    Ok(variant)
}
