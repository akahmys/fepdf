//! Compile-time derives for the fepdf object model.

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, LitFloat, LitStr, parse_macro_input};

#[proc_macro_derive(FromPdfObject, attributes(pdf_key, pdf_dict))]
/// Derives `FromPdfObject`, mapping a PDF dictionary onto a struct's fields.
///
/// Each named field is read from the dictionary key matching its name; the
/// generated impl reports a typed error rather than panicking on a mismatch.
pub fn derive_from_pdf_object(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match derive_from_pdf_object_impl(input) {
        Ok(expanded) => TokenStream::from(expanded),
        Err(err) => TokenStream::from(err.into_compile_error()),
    }
}

fn derive_from_pdf_object_impl(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;

    // Parse #[pdf_dict(clause = "...")]
    let mut iso_clause = "Unknown".to_string();
    for attr in &input.attrs {
        if attr.path().is_ident("pdf_dict") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("clause") {
                    let value = meta.value()?;
                    let s: LitStr = value.parse()?;
                    iso_clause = s.value();
                }
                Ok(())
            })?;
        }
    }

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    &input.ident,
                    "FromPdfObject only supports structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "FromPdfObject only supports structs",
            ));
        }
    };

    let mut field_parsers = Vec::new();
    // Collected so `PdfSchema::pdf_keys` can report them; see object.rs.
    let mut declared_keys: Vec<String> = Vec::new();
    for f in fields {
        let field_name = &f.ident;
        let field_type = &f.ty;

        let mut pdf_key = field_name.as_ref().map(|id| id.to_string()).unwrap_or_default();
        let mut since_version: Option<f32> = None;
        let mut default_expr: Option<String> = None;

        for attr in &f.attrs {
            if attr.path().is_ident("pdf_key") {
                if let Ok(lit) = attr.parse_args::<LitStr>() {
                    pdf_key = lit.value();
                } else {
                    attr.parse_nested_meta(|meta| {
                        if meta.path.is_ident("name") {
                            let value = meta.value()?;
                            let s: LitStr = value.parse()?;
                            pdf_key = s.value();
                        } else if meta.path.is_ident("since") {
                            let value = meta.value()?;
                            let f: LitFloat = value.parse()?;
                            since_version = Some(f.base10_parse::<f32>()?);
                        } else if meta.path.is_ident("default") {
                            let value = meta.value()?;
                            let s: LitStr = value.parse()?;
                            default_expr = Some(s.value());
                        }
                        Ok(())
                    })?;
                }
            }
        }

        let version_check = if let Some(v) = since_version {
            quote! {
                if arena.version() < #v {
                    fepdf_model::Object::Null
                } else {
                    dict.get(&key).cloned().unwrap_or(fepdf_model::Object::Null)
                }
            }
        } else {
            quote! {
                dict.get(&key).cloned().unwrap_or(fepdf_model::Object::Null)
            }
        };

        let parser = if let Some(def) = default_expr {
            match syn::parse_str::<syn::Expr>(&def) {
                Ok(def_token) => quote! {
                    if matches!(val, fepdf_model::Object::Null) {
                        #def_token
                    } else {
                        <#field_type as fepdf_model::object::FromPdfObject>::from_pdf_object(val, arena)?
                    }
                },
                Err(_) => {
                    return Err(syn::Error::new_spanned(
                        f,
                        format!("Invalid default expression: {def}"),
                    ));
                }
            }
        } else {
            quote! {
                <#field_type as fepdf_model::object::FromPdfObject>::from_pdf_object(val, arena)?
            }
        };

        declared_keys.push(pdf_key.clone());
        field_parsers.push(quote! {
            let #field_name = {
                let key = arena.name(#pdf_key);
                let val = #version_check;
                #parser
            };
        });
    }

    let field_names = fields.iter().map(|f| &f.ident);
    let iso_clause_str = iso_clause;

    let expanded = quote! {
        impl fepdf_model::object::FromPdfObject for #name {
            fn from_pdf_object(obj: fepdf_model::Object, arena: &fepdf_model::PdfArena) -> fepdf_model::PdfResult<Self> {
                let dict_handle = obj.resolve(arena).as_dict_handle()
                    .ok_or_else(|| fepdf_model::PdfError::Parse {
                        pos: 0,
                        message: format!("Expected dictionary for {}, got {:?}", stringify!(#name), obj).into()
                    })?;

                let dict = arena.get_dict(dict_handle)
                    .ok_or_else(|| fepdf_model::PdfError::Arena("Missing dictionary in arena".into()))?;

                #(#field_parsers)*

                Ok(Self {
                    #(#field_names),*
                })
            }
        }

        impl fepdf_model::object::PdfSchema for #name {
            fn iso_clause() -> &'static str {
                #iso_clause_str
            }

            fn pdf_keys() -> &'static [&'static str] {
                &[#(#declared_keys),*]
            }
        }
    };

    Ok(expanded)
}
