//! The gateway's request body: `application/x-www-form-urlencoded` pairs.
//!
//! Request types write themselves through [`ToForm`], usually derived with
//! `#[derive(Model)]` and `#[achq(form)]`. Each value is formatted by its
//! [`FormValue`] impl, or by a function in [`with`].

use std::borrow::Cow;
use std::fmt::Write as _;
use std::net::{IpAddr, Ipv4Addr};

use time::Date;
use time::macros::format_description;

/// The form pairs of a request, in the order they were written.
///
/// Each pair remembers whether its value is sensitive (a gate key, an account
/// number, a token), so that [`redacted`](Self::redacted) can hide it in logs.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct FormWriter {
    pairs: Vec<Pair>,
}

#[derive(Clone, PartialEq, Eq)]
struct Pair {
    name: String,
    value: String,
    sensitive: bool,
}

impl FormWriter {
    /// An empty body.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `name=value`.
    pub fn push(&mut self, name: &str, value: &str) {
        self.push_pair(name, value, false);
    }

    /// Adds `name=value`, hiding the value in [`redacted`](Self::redacted).
    pub fn push_sensitive(&mut self, name: &str, value: &str) {
        self.push_pair(name, value, true);
    }

    fn push_pair(&mut self, name: &str, value: &str, sensitive: bool) {
        self.pairs.push(Pair {
            name: name.into(),
            value: value.into(),
            sensitive,
        });
    }

    /// Every pair as `(name, value, sensitive)`, in order.
    pub fn pairs(&self) -> impl Iterator<Item = (&str, &str, bool)> {
        self.pairs
            .iter()
            .map(|pair| (pair.name.as_str(), pair.value.as_str(), pair.sensitive))
    }

    /// The value of the first pair named `name`.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|pair| pair.name == name)
            .map(|pair| pair.value.as_str())
    }

    /// Whether no pair has been written.
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// The body, encoded as `application/x-www-form-urlencoded`.
    pub fn encode(&self) -> String {
        self.render(false)
    }

    /// The body as [`encode`](Self::encode) writes it, but with each sensitive
    /// value replaced by `[REDACTED]`; safe to log.
    pub fn redacted(&self) -> String {
        self.render(true)
    }

    fn render(&self, redact: bool) -> String {
        let mut out = String::new();
        for (i, pair) in self.pairs.iter().enumerate() {
            if i > 0 {
                out.push('&');
            }
            encode_component(&pair.name, &mut out);
            out.push('=');
            if redact && pair.sensitive {
                out.push_str("[REDACTED]");
            } else {
                encode_component(&pair.value, &mut out);
            }
        }
        out
    }
}

/// Shows the redacted body.
impl std::fmt::Debug for FormWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("FormWriter").field(&self.redacted()).finish()
    }
}

/// Appends `value` as the WHATWG `application/x-www-form-urlencoded` serializer
/// does: ASCII letters, digits and `*-._` as they are, a space as `+`, and every
/// other byte of the UTF-8 text as `%XX`.
fn encode_component(value: &str, out: &mut String) {
    for byte in value.bytes() {
        match byte {
            b' ' => out.push('+'),
            b if b.is_ascii_alphanumeric() || matches!(b, b'*' | b'-' | b'.' | b'_') => {
                out.push(char::from(b));
            }
            b => write!(out, "%{b:02X}").expect("writing to a String"),
        }
    }
}

/// A type written to the form body with [`ToForm`].
pub trait ToForm {
    /// Writes each field as a pair, with `prefix` before each name.
    #[doc(hidden)]
    fn write_form(&self, form: &mut FormWriter, prefix: &str);

    /// Describes each field this type can write, with `prefix` before each name.
    fn form_fields(prefix: &str, out: &mut Vec<FieldMeta>)
    where
        Self: Sized;

    /// The form body of `self` alone.
    fn to_form(&self) -> FormWriter {
        let mut form = FormWriter::new();
        self.write_form(&mut form, "");
        form
    }
}

/// A form field a [`ToForm`] type can write: its name and the constraints the
/// derive knows about. Checked against the API's documentation in the tests.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FieldMeta {
    /// The name as sent, prefix included: `Billing_City`.
    pub wire: String,
    /// Whether the API needs the field.
    pub required: bool,
    /// The longest value allowed, in characters.
    pub max_length: Option<usize>,
    /// Whether the value is hidden in logs.
    pub sensitive: bool,
}

impl FieldMeta {
    #[doc(hidden)]
    pub fn new(wire: String, required: bool, max_length: Option<usize>, sensitive: bool) -> Self {
        Self {
            wire,
            required,
            max_length,
            sensitive,
        }
    }
}

/// `prefix` then `name`.
#[doc(hidden)]
pub fn join_prefix(prefix: &str, name: &str) -> String {
    let mut out = String::with_capacity(prefix.len() + name.len());
    out.push_str(prefix);
    out.push_str(name);
    out
}

/// A value written to the form body as text.
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no single form encoding",
    label = "no `FormValue` for `{Self}`",
    note = "choose a format with `with = \"...\"`: `yes_no` or `on_off` for a `bool`, `mdy_slash`, `mdy` or `iso_date` for a `Date`"
)]
pub trait FormValue {
    /// The value as sent.
    fn form_value(&self) -> Cow<'_, str>;
}

impl FormValue for str {
    fn form_value(&self) -> Cow<'_, str> {
        Cow::Borrowed(self)
    }
}

impl FormValue for String {
    fn form_value(&self) -> Cow<'_, str> {
        Cow::Borrowed(self)
    }
}

macro_rules! display_form_value {
    ($($ty:ty),*) => {$(
        impl FormValue for $ty {
            fn form_value(&self) -> Cow<'_, str> {
                Cow::Owned(self.to_string())
            }
        }
    )*};
}

display_form_value!(u16, u32, Ipv4Addr, IpAddr);

/// Formats for values that have more than one way to be written, named in
/// `#[achq(with = "...")]`.
pub mod with {
    use super::*;

    /// `Yes` or `No`.
    pub fn yes_no(value: &bool) -> Cow<'static, str> {
        Cow::Borrowed(if *value { "Yes" } else { "No" })
    }

    /// `On` or `Off`.
    pub fn on_off(value: &bool) -> Cow<'static, str> {
        Cow::Borrowed(if *value { "On" } else { "Off" })
    }

    /// `MM/DD/YYYY`, as for a payment's scheduled date.
    pub fn mdy_slash(value: &Date) -> Cow<'static, str> {
        format(value, format_description!("[month]/[day]/[year]"))
    }

    /// `MMDDYYYY`, as for an event tracking date.
    pub fn mdy(value: &Date) -> Cow<'static, str> {
        format(value, format_description!("[month][day][year]"))
    }

    /// `YYYY-MM-DD`, as for a settlement date range.
    pub fn iso_date(value: &Date) -> Cow<'static, str> {
        format(value, format_description!("[year]-[month]-[day]"))
    }

    fn format(
        value: &Date,
        description: &[time::format_description::BorrowedFormatItem<'_>],
    ) -> Cow<'static, str> {
        Cow::Owned(
            value
                .format(description)
                .expect("a date formats with a fixed description"),
        )
    }
}

#[cfg(test)]
mod tests {
    use time::macros::date;

    use super::*;

    #[test]
    fn encodes_as_a_form_body() {
        let mut form = FormWriter::new();
        form.push("Billing_CustomerName", "Bob Loblaw");
        form.push("Description", "a&b=c+d/é");
        form.push("Empty", "");
        form.push("Safe", "AZaz09*-._");
        assert_eq!(
            form.encode(),
            "Billing_CustomerName=Bob+Loblaw&Description=a%26b%3Dc%2Bd%2F%C3%A9&Empty=&Safe=AZaz09*-._"
        );
    }

    #[test]
    fn names_are_encoded_too() {
        let mut form = FormWriter::new();
        form.push("Phone Number", "5558675309");
        assert_eq!(form.encode(), "Phone+Number=5558675309");
    }

    #[test]
    fn keeps_the_order_written() {
        let mut form = FormWriter::new();
        form.push("b", "1");
        form.push("a", "2");
        form.push("b", "3");
        let names: Vec<&str> = form.pairs().map(|(name, _, _)| name).collect();
        assert_eq!(names, ["b", "a", "b"]);
        assert_eq!(form.get("b"), Some("1"));
        assert_eq!(form.get("c"), None);
    }

    #[test]
    fn redacts_sensitive_values() {
        let mut form = FormWriter::new();
        form.push("RoutingNumber", "123123123");
        form.push_sensitive("AccountNumber", "12345678");
        assert_eq!(
            form.redacted(),
            "RoutingNumber=123123123&AccountNumber=[REDACTED]"
        );
        assert_eq!(
            format!("{form:?}"),
            "FormWriter(\"RoutingNumber=123123123&AccountNumber=[REDACTED]\")"
        );
        assert!(form.encode().contains("12345678"));
    }

    #[test]
    fn formats_values() {
        assert_eq!(with::yes_no(&true), "Yes");
        assert_eq!(with::yes_no(&false), "No");
        assert_eq!(with::on_off(&true), "On");
        assert_eq!(with::on_off(&false), "Off");
        assert_eq!(with::mdy_slash(&date!(2026 - 01 - 05)), "01/05/2026");
        assert_eq!(with::mdy(&date!(2026 - 01 - 05)), "01052026");
        assert_eq!(with::iso_date(&date!(2026 - 01 - 05)), "2026-01-05");
        assert_eq!(Ipv4Addr::new(203, 0, 113, 9).form_value(), "203.0.113.9");
        assert_eq!(500u16.form_value(), "500");
    }
}
