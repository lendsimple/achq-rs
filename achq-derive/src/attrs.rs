//! Parsing of `#[achq(...)]` attributes.

use proc_macro2::Span;
use syn::meta::ParseNestedMeta;
use syn::{Attribute, LitInt, LitStr, Result};

/// Attributes on a struct field.
#[derive(Default)]
pub struct FieldAttrs {
    pub sensitive: bool,
    pub required: Option<Span>,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub min: Option<LitStr>,
    pub max: Option<LitStr>,
    pub rule: Option<LitStr>,
    pub form: Option<LitStr>,
    pub flatten: Option<Span>,
    pub prefix: Option<LitStr>,
    pub with: Option<LitStr>,
    pub skip_form: Option<Span>,
}

impl FieldAttrs {
    pub fn parse(attrs: &[Attribute]) -> Result<Self> {
        let mut out = Self::default();
        for attr in achq_attrs(attrs) {
            attr.parse_nested_meta(|meta| {
                let int = || -> Result<usize> { meta.value()?.parse::<LitInt>()?.base10_parse() };
                let span = || meta.path.segments[0].ident.span();
                if meta.path.is_ident("sensitive") {
                    out.sensitive = true;
                } else if meta.path.is_ident("required") {
                    out.required = Some(span());
                } else if meta.path.is_ident("min_length") {
                    out.min_length = Some(int()?);
                } else if meta.path.is_ident("max_length") {
                    out.max_length = Some(int()?);
                } else if meta.path.is_ident("min") {
                    out.min = Some(number(&meta)?);
                } else if meta.path.is_ident("max") {
                    out.max = Some(number(&meta)?);
                } else if meta.path.is_ident("rule") {
                    out.rule = Some(function(&meta, "a rule in `achq::validate::rules`")?);
                } else if meta.path.is_ident("form") {
                    out.form = Some(wire_name(&meta)?);
                } else if meta.path.is_ident("flatten") {
                    out.flatten = Some(span());
                } else if meta.path.is_ident("prefix") {
                    out.prefix = Some(wire_name(&meta)?);
                } else if meta.path.is_ident("with") {
                    out.with = Some(function(&meta, "a function in `achq::form::with`")?);
                } else if meta.path.is_ident("skip_form") {
                    out.skip_form = Some(span());
                } else {
                    return Err(unknown(&meta));
                }
                Ok(())
            })?;
        }
        Ok(out)
    }
}

/// Attributes on the struct.
#[derive(Default)]
pub struct ContainerAttrs {
    /// Each `one_of = "a b | c"`: groups of alternative fields.
    pub one_of: Vec<LitStr>,
    /// Each `check = "method"`: a cross-field check.
    pub check: Vec<LitStr>,
    /// `form`: implement `ToForm` too.
    pub form: bool,
}

impl ContainerAttrs {
    pub fn parse(attrs: &[Attribute]) -> Result<Self> {
        let mut out = Self::default();
        for attr in achq_attrs(attrs) {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("one_of") {
                    out.one_of.push(meta.value()?.parse()?);
                } else if meta.path.is_ident("check") {
                    out.check.push(function(&meta, "a method of this type")?);
                } else if meta.path.is_ident("form") {
                    out.form = true;
                } else {
                    let name = meta
                        .path
                        .get_ident()
                        .map_or_else(String::new, ToString::to_string);
                    return Err(meta.error(format!(
                        "`{name}` goes on fields; on the struct, only `form`, `one_of` and `check` are supported"
                    )));
                }
                Ok(())
            })?;
        }
        Ok(out)
    }
}

/// A decimal number written as a string, such as `"0"` or `"-1.5"`.
fn number(meta: &ParseNestedMeta) -> Result<LitStr> {
    let lit: LitStr = meta.value()?.parse()?;
    let text = lit.value();
    let digits = text.strip_prefix('-').unwrap_or(&text);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "0"));
    let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if all_digits(whole) && all_digits(fraction) {
        Ok(lit)
    } else {
        Err(syn::Error::new(
            lit.span(),
            "expected a decimal number, such as \"0\" or \"1.5\"",
        ))
    }
}

/// The name of a function; `what` says where it lives.
fn function(meta: &ParseNestedMeta, what: &str) -> Result<LitStr> {
    let lit: LitStr = meta.value()?.parse()?;
    if syn::parse_str::<syn::Ident>(&lit.value()).is_err() {
        return Err(syn::Error::new(
            lit.span(),
            format!("expected the name of {what}"),
        ));
    }
    Ok(lit)
}

/// A name as sent in the form body: not empty or padded. Inner spaces are allowed;
/// the API documents at least one such name (`Phone Number`).
fn wire_name(meta: &ParseNestedMeta) -> Result<LitStr> {
    let lit: LitStr = meta.value()?.parse()?;
    let text = lit.value();
    if text.is_empty() || text.trim() != text {
        return Err(syn::Error::new(
            lit.span(),
            "expected a form field name, such as \"Billing_City\"",
        ));
    }
    Ok(lit)
}

fn achq_attrs(attrs: &[Attribute]) -> impl Iterator<Item = &Attribute> {
    attrs.iter().filter(|a| a.path().is_ident("achq"))
}

fn unknown(meta: &ParseNestedMeta) -> syn::Error {
    let name = meta
        .path
        .get_ident()
        .map(ToString::to_string)
        .unwrap_or_else(|| "?".into());
    meta.error(format!("unknown achq attribute `{name}`"))
}
