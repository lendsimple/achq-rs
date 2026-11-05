//! Derive macros for the [`achq`](https://docs.rs/achq) crate.
//!
//! They are used by the crate's request and response types and are not intended
//! as a general framework. JSON stays with serde; these derives add what serde
//! does not: validation of the API's constraints, a `Debug` that hides secrets,
//! and the gateway's form encoding.
//!
//! # `#[derive(Model)]`
//!
//! On a struct with named fields, implements `achq::validate::Validate` and
//! `Debug`. Do not derive `Debug` separately.
//!
//! The field's type decides how a constraint applies: to the value of a `T`, to the
//! value of an `Option<T>` when it is `Some`, and to each item of a `Vec<T>` or
//! `Option<Vec<T>>`. Every field is also validated recursively through its own
//! `Validate` impl.
//!
//! Field attributes, inside `#[achq(...)]`:
//! - `sensitive`: shown as `[REDACTED]` in `Debug` output, and marked sensitive in
//!   the form body so that logging hides it.
//! - `required`: on an `Option<T>`, the API needs a value although the wire format
//!   allows leaving it out.
//! - `min_length = N`, `max_length = N`: length in characters, for string values.
//! - `min = "0"`, `max = "1.5"`: inclusive bounds, for numeric values.
//! - `rule = "name"`: a check from `achq::validate::rules`, for string values.
//!
//! Struct attributes:
//! - `one_of = "account_number | account_token"`, which may repeat: alternatives.
//!   Exactly one group's first field must be set, and then the group's other
//!   fields. Every field named must be an `Option<T>`.
//! - `check = "method"`, which may repeat: a cross-field check, called after the
//!   field checks as `Self::method(&self, path, out)`.
//! - `form`: also implements `achq::form::ToForm`, writing the fields in
//!   declaration order as a form body.
//!
//! Field attributes of a `form` struct, of which every field has exactly one of
//! `form`, `flatten` or `skip_form`:
//! - `form = "Wire_Name"`: the field's name in the form body. The value is written
//!   with `achq::form::FormValue`, or with `with = "name"`, a function in
//!   `achq::form::with` (such as `yes_no` for a `bool`).
//! - `flatten`: the field's type is itself `ToForm`, and its fields are written in
//!   place, with `prefix = "Billing_"` before their names if given.
//! - `skip_form`: the field is not sent.
//!
//! A `T` field is always written, an `Option<T>` field only when it is `Some`.

#![warn(missing_docs)]

mod attrs;
mod cardinality;
mod form;
mod model;

use proc_macro::TokenStream;
use syn::{Data, DeriveInput, parse_macro_input};

/// Derives `Validate` and a redacting `Debug`, and with `#[achq(form)]` `ToForm`;
/// see the crate documentation.
#[proc_macro_derive(Model, attributes(achq))]
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let result = if !input.generics.params.is_empty() {
        Err(syn::Error::new_spanned(
            &input.generics,
            "Model does not support generic types",
        ))
    } else {
        match &input.data {
            Data::Struct(data) => model::expand(&input, data),
            Data::Enum(_) | Data::Union(_) => Err(syn::Error::new_spanned(
                &input.ident,
                "Model can only be derived for structs with named fields",
            )),
        }
    };
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}
