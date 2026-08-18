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
            let property_type = property_type_for(schema_ty)?;
            let description = description_for(field)?;

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
    let Type::Path(type_path) = ty else { return None };
    let segment = type_path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else { return None };
    match args.args.first()? {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

/// Reads `#[tool(description = "...")]` off a field. This is the one attribute key we
/// support today — anything else inside `tool(...)` is a compile error rather than
/// silently ignored, so typos surface immediately instead of producing an empty schema.
fn description_for(field: &Field) -> Result<String, Error> {
    let attr = field
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("tool"))
        .ok_or_else(|| {
            Error::new_spanned(field, "missing #[tool(description = \"...\")] attribute")
        })?;

    let mut description = None;
    attr.meta.require_list()?.parse_nested_meta(|meta| {
        if meta.path.is_ident("description") {
            let value: LitStr = meta.value()?.parse()?;
            description = Some(value.value());
            Ok(())
        } else {
            Err(meta.error("unsupported key in #[tool(...)], expected `description`"))
        }
    })?;

    description.ok_or_else(|| Error::new_spanned(attr, "expected description = \"...\""))
}

/// Maps a Rust field type to the `PropertyType` the model's tool schema expects.
/// Deliberately narrow (primitives + `Vec`) — anything else falls back to `Object`
/// rather than failing, since a wrong-but-present schema is easier to debug than a
/// macro that refuses to compile on an unrecognized type.
fn property_type_for(ty: &Type) -> Result<TokenStream, Error> {
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
        "Vec" => quote! { PropertyType::Array },
        _ => quote! { PropertyType::Object },
    };

    Ok(variant)
}
