//! `ToForm` for structs marked `#[achq(form)]`: the fields written as form pairs, in
//! declaration order, and a description of those fields.

use proc_macro2::{Literal, Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Field, Ident, Result};

use crate::attrs::FieldAttrs;
use crate::cardinality::Cardinality;

/// The code one field contributes to `write_form` and to `form_fields`.
pub struct FormField {
    write: TokenStream,
    describe: TokenStream,
}

/// Fails if a field of a struct without `#[achq(form)]` uses a form attribute.
pub fn reject_attrs(attrs: &FieldAttrs) -> Result<()> {
    let used = [
        attrs.form.as_ref().map(|lit| ("form", lit.span())),
        attrs.flatten.map(|span| ("flatten", span)),
        attrs.prefix.as_ref().map(|lit| ("prefix", lit.span())),
        attrs.with.as_ref().map(|lit| ("with", lit.span())),
        attrs.skip_form.map(|span| ("skip_form", span)),
    ];
    match used.into_iter().flatten().next() {
        Some((name, span)) => Err(syn::Error::new(
            span,
            format!("`{name}` needs `#[achq(form)]` on the struct"),
        )),
        None => Ok(()),
    }
}

pub fn field(field: &Field, card: &Cardinality<'_>, attrs: &FieldAttrs) -> Result<FormField> {
    let ident = field.ident.as_ref().expect("named field");
    if let Some(prefix) = &attrs.prefix
        && attrs.flatten.is_none()
    {
        return Err(syn::Error::new(
            prefix.span(),
            "`prefix` is only valid with `flatten`",
        ));
    }
    if let Some(with) = &attrs.with
        && attrs.form.is_none()
    {
        return Err(syn::Error::new(
            with.span(),
            "`with` is only valid with `form = \"...\"`",
        ));
    }
    let kinds: Vec<Span> = [
        attrs.form.as_ref().map(|lit| lit.span()),
        attrs.flatten,
        attrs.skip_form,
    ]
    .into_iter()
    .flatten()
    .collect();
    match kinds.as_slice() {
        [] => {
            return Err(syn::Error::new_spanned(
                ident,
                "a field of a `#[achq(form)]` struct needs one of `form = \"...\"`, `flatten` or `skip_form`",
            ));
        }
        [_] => {}
        [_, second, ..] => {
            return Err(syn::Error::new(
                *second,
                "use only one of `form`, `flatten` and `skip_form`",
            ));
        }
    }
    if attrs.skip_form.is_some() {
        return Ok(FormField {
            write: quote!(),
            describe: quote!(),
        });
    }

    let (ty, optional) = match card {
        Cardinality::Required(ty) => (*ty, false),
        Cardinality::Optional(ty) => (*ty, true),
        Cardinality::List(_) | Cardinality::OptionalList(_) => {
            return Err(syn::Error::new_spanned(
                &field.ty,
                "form bodies have no lists; a form field cannot be a `Vec`",
            ));
        }
    };
    let f = quote!(::achq::form);
    let span = ty.span();

    let (write_value, describe) = if let Some(wire) = &attrs.form {
        let value = match &attrs.with {
            Some(with) => {
                let func = Ident::new(&with.value(), with.span());
                quote_spanned!(with.span()=> #f::with::#func(value))
            }
            None => quote_spanned!(span=> #f::FormValue::form_value(value)),
        };
        let push = if attrs.sensitive {
            quote!(push_sensitive)
        } else {
            quote!(push)
        };
        let required = !optional || attrs.required.is_some();
        let max_length = match attrs.max_length {
            Some(n) => {
                let n = Literal::usize_unsuffixed(n);
                quote!(::core::option::Option::Some(#n))
            }
            None => quote!(::core::option::Option::None),
        };
        let sensitive = attrs.sensitive;
        (
            quote! {
                form.#push(&#f::join_prefix(prefix, #wire), &#value);
            },
            quote! {
                out.push(#f::FieldMeta::new(
                    #f::join_prefix(prefix, #wire),
                    #required,
                    #max_length,
                    #sensitive,
                ));
            },
        )
    } else {
        let prefix = match &attrs.prefix {
            Some(lit) => quote!(#lit),
            None => quote!(""),
        };
        let to_form = quote_spanned!(span=> <#ty as #f::ToForm>);
        (
            quote! {
                #to_form::write_form(value, form, &#f::join_prefix(prefix, #prefix));
            },
            quote! {
                #to_form::form_fields(&#f::join_prefix(prefix, #prefix), out);
            },
        )
    };

    let write = if optional {
        quote! {
            if let ::core::option::Option::Some(value) = &self.#ident {
                #write_value
            }
        }
    } else {
        quote! {
            let value = &self.#ident;
            #write_value
        }
    };
    Ok(FormField { write, describe })
}

pub fn impl_to_form(ident: &Ident, fields: &[FormField]) -> TokenStream {
    let f = quote!(::achq::form);
    let writes = fields.iter().map(|field| &field.write);
    let describes = fields.iter().map(|field| &field.describe);
    quote! {
        #[automatically_derived]
        impl #f::ToForm for #ident {
            fn write_form(&self, form: &mut #f::FormWriter, prefix: &str) {
                #({ #writes })*
            }

            fn form_fields(prefix: &str, out: &mut ::std::vec::Vec<#f::FieldMeta>) {
                #(#describes)*
            }
        }
    }
}
