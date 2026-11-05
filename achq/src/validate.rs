//! Checks of the API's constraints: lengths, numeric ranges, formats, required
//! values, and the precision of amounts.
//!
//! Every request type implements [`Validate`]. Validating a request before it is
//! sent reports every problem with the path of the offending field, instead of the
//! gateway's first error code.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use rust_decimal::Decimal;

/// A type whose values can be checked against the API's constraints.
pub trait Validate {
    /// Checks `self` and everything inside it, collecting every violation.
    fn validate(&self) -> Result<(), ValidationError> {
        let mut path = Path::default();
        let mut violations = Vec::new();
        self.validate_into(&mut path, &mut violations);
        if violations.is_empty() {
            Ok(())
        } else {
            Err(ValidationError { violations })
        }
    }

    #[doc(hidden)]
    fn validate_into(&self, path: &mut Path, out: &mut Vec<Violation>);
}

/// One value that breaks a constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// Where the value is, by Rust field names: `customer.zip`.
    pub path: String,
    /// What is wrong with it.
    pub message: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// A value does not satisfy the API's constraints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    violations: Vec<Violation>,
}

impl ValidationError {
    /// Every violation found, in field order. Never empty.
    pub fn violations(&self) -> &[Violation] {
        &self.violations
    }

    /// The error for `violations`, or `None` if there are none.
    pub(crate) fn from_violations(violations: Vec<Violation>) -> Option<Self> {
        (!violations.is_empty()).then_some(Self { violations })
    }

    pub(crate) fn into_violations(self) -> Vec<Violation> {
        self.violations
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (first, rest) = self.violations.split_first().expect("never empty");
        write!(f, "{first}")?;
        match rest.len() {
            0 => Ok(()),
            1 => write!(f, " (and 1 more violation)"),
            n => write!(f, " (and {n} more violations)"),
        }
    }
}

impl std::error::Error for ValidationError {}

/// The location being validated, maintained by derived code.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct Path {
    segments: Vec<Segment>,
}

#[derive(Debug)]
enum Segment {
    Field(&'static str),
    Index(usize),
}

impl Path {
    pub fn push_field(&mut self, name: &'static str) {
        self.segments.push(Segment::Field(name));
    }

    pub fn push_index(&mut self, index: usize) {
        self.segments.push(Segment::Index(index));
    }

    pub fn pop(&mut self) {
        self.segments.pop();
    }

    fn render(&self) -> String {
        let mut out = String::new();
        for segment in &self.segments {
            match segment {
                Segment::Field(name) => {
                    if !out.is_empty() {
                        out.push('.');
                    }
                    out.push_str(name);
                }
                Segment::Index(i) => out.push_str(&format!("[{i}]")),
            }
        }
        out
    }
}

/// Records that the value at `path` breaks a constraint. For cross-field checks
/// (`#[achq(check = "...")]`), which push the field they report on first.
#[doc(hidden)]
pub fn violation(path: &Path, out: &mut Vec<Violation>, message: String) {
    out.push(Violation {
        path: path.render(),
        message,
    });
}

impl<T: Validate> Validate for Option<T> {
    fn validate_into(&self, path: &mut Path, out: &mut Vec<Violation>) {
        if let Some(value) = self {
            value.validate_into(path, out);
        }
    }
}

impl<T: Validate> Validate for Vec<T> {
    fn validate_into(&self, path: &mut Path, out: &mut Vec<Violation>) {
        for (index, value) in self.iter().enumerate() {
            path.push_index(index);
            value.validate_into(path, out);
            path.pop();
        }
    }
}

impl<T: Validate> Validate for Box<T> {
    fn validate_into(&self, path: &mut Path, out: &mut Vec<Violation>) {
        (**self).validate_into(path, out);
    }
}

/// Types with no constraints of their own.
macro_rules! no_constraints {
    ($($ty:ty),* $(,)?) => {$(
        impl $crate::validate::Validate for $ty {
            fn validate_into(
                &self,
                _path: &mut $crate::validate::Path,
                _out: &mut ::std::vec::Vec<$crate::validate::Violation>,
            ) {
            }
        }
    )*};
}

pub(crate) use no_constraints;

no_constraints!(
    String,
    bool,
    u16,
    u32,
    i32,
    i64,
    f64,
    Decimal,
    BTreeMap<String, String>,
    time::Date,
    std::net::Ipv4Addr,
    std::net::IpAddr,
);

/// A value that `min` and `max` bounds compare against.
#[doc(hidden)]
pub trait Numeric {
    fn to_decimal(&self) -> Decimal;
}

impl Numeric for u16 {
    fn to_decimal(&self) -> Decimal {
        Decimal::from(*self)
    }
}

impl Numeric for u32 {
    fn to_decimal(&self) -> Decimal {
        Decimal::from(*self)
    }
}

impl Numeric for i32 {
    fn to_decimal(&self) -> Decimal {
        Decimal::from(*self)
    }
}

impl Numeric for i64 {
    fn to_decimal(&self) -> Decimal {
        Decimal::from(*self)
    }
}

impl Numeric for Decimal {
    fn to_decimal(&self) -> Decimal {
        *self
    }
}

// Checks called by derived code. Lengths count characters.

#[doc(hidden)]
pub fn check_required(present: bool, path: &Path, out: &mut Vec<Violation>) {
    if !present {
        violation(path, out, "is required".into());
    }
}

#[doc(hidden)]
pub fn check_length(
    value: &str,
    min: Option<usize>,
    max: Option<usize>,
    path: &Path,
    out: &mut Vec<Violation>,
) {
    let len = value.chars().count();
    if let Some(min) = min
        && len < min
    {
        violation(
            path,
            out,
            format!("must be at least {}, not {len}", characters(min)),
        );
    }
    if let Some(max) = max
        && len > max
    {
        violation(
            path,
            out,
            format!("must be at most {}, not {len}", characters(max)),
        );
    }
}

fn characters(n: usize) -> String {
    if n == 1 {
        "1 character".into()
    } else {
        format!("{n} characters")
    }
}

/// `min` and `max` are decimal numbers, checked when the derive expands.
#[doc(hidden)]
pub fn check_range(
    value: Decimal,
    min: Option<&str>,
    max: Option<&str>,
    path: &Path,
    out: &mut Vec<Violation>,
) {
    let bound = |text: &str| {
        Decimal::from_str(text)
            .unwrap_or_else(|_| panic!("bound {text:?} must be a decimal number"))
    };
    if let Some(min) = min
        && value < bound(min)
    {
        violation(path, out, format!("must be at least {min}, not {value}"));
    }
    if let Some(max) = max
        && value > bound(max)
    {
        violation(path, out, format!("must be at most {max}, not {value}"));
    }
}

#[doc(hidden)]
pub fn check_rule(
    value: &str,
    rule: fn(&str) -> Result<(), &'static str>,
    path: &Path,
    out: &mut Vec<Violation>,
) {
    if let Err(message) = rule(value) {
        violation(path, out, message.into());
    }
}

/// Checks alternatives, such as a bank account number (with its routing number)
/// *or* a token: exactly one group's first field (its key) must be set, and then
/// the group's other fields too. Each group is its field names and whether each
/// is set.
#[doc(hidden)]
pub fn check_one_of(
    groups: &[(&[&'static str], &[bool])],
    path: &mut Path,
    out: &mut Vec<Violation>,
) {
    let keys: Vec<&str> = groups.iter().map(|(names, _)| names[0]).collect();
    let chosen: Vec<&(&[&'static str], &[bool])> =
        groups.iter().filter(|(_, set)| set[0]).collect();
    match chosen.as_slice() {
        [] => violation(
            path,
            out,
            format!("set exactly one of {}", join(&keys, "or")),
        ),
        [(names, set)] => {
            for (name, set) in names.iter().zip(set.iter()).skip(1) {
                if !set {
                    path.push_field(name);
                    violation(path, out, format!("is required with {}", names[0]));
                    path.pop();
                }
            }
        }
        several => {
            let set: Vec<&str> = several.iter().map(|(names, _)| names[0]).collect();
            violation(
                path,
                out,
                format!(
                    "set exactly one of {}, not {}",
                    join(&keys, "or"),
                    join(&set, "and")
                ),
            );
        }
    }
}

/// `a, b or c`.
fn join(items: &[&str], last: &str) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [init @ .., tail] => format!("{} {last} {tail}", init.join(", ")),
    }
}

/// Format checks for string fields, named in `#[achq(rule = "...")]`.
///
/// A rule returns what is wrong with the value.
pub mod rules {
    /// Only ASCII digits, and at least one. Combine with `min_length` and
    /// `max_length` for an exact count, as for a 10-digit phone number.
    pub fn digits(value: &str) -> Result<(), &'static str> {
        if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
            Ok(())
        } else {
            Err("must be digits only")
        }
    }

    /// An ABA routing number: 9 digits whose checksum is valid, so that a mistyped
    /// digit is caught before the gateway rejects the payment.
    ///
    /// The checksum weights the digits 3, 7, 1, 3, 7, 1, 3, 7, 1; the sum must be
    /// a multiple of 10. The sandbox's routing number, `123123123`, is valid.
    pub fn routing_number(value: &str) -> Result<(), &'static str> {
        if value.len() != 9 || digits(value).is_err() {
            return Err("must be 9 digits");
        }
        let sum: u32 = value
            .bytes()
            .zip([3, 7, 1].iter().cycle())
            .map(|(b, weight)| u32::from(b - b'0') * weight)
            .sum();
        if sum.is_multiple_of(10) {
            Ok(())
        } else {
            Err("is not a valid routing number (checksum mismatch)")
        }
    }

    /// A US ZIP code: `#####` or `#####-####`.
    pub fn zip(value: &str) -> Result<(), &'static str> {
        let five = |s: &str| s.len() == 5 && digits(s).is_ok();
        let valid = match value.split_once('-') {
            None => five(value),
            Some((head, tail)) => five(head) && tail.len() == 4 && digits(tail).is_ok(),
        };
        if valid {
            Ok(())
        } else {
            Err("must be a ZIP code: ##### or #####-####")
        }
    }

    /// A plausible email address: one `@` with text on both sides, a dot in the
    /// domain, and no whitespace. Deliverability is the gateway's concern.
    pub fn email(value: &str) -> Result<(), &'static str> {
        const INVALID: &str = "must be an email address";
        if value.chars().any(char::is_whitespace) {
            return Err(INVALID);
        }
        match value.split_once('@') {
            Some((local, domain))
                if !local.is_empty()
                    && !domain.contains('@')
                    && domain
                        .split_once('.')
                        .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty()) =>
            {
                Ok(())
            }
            _ => Err(INVALID),
        }
    }

    /// A US state, the District of Columbia, or a territory the onboarding API
    /// accepts, by its two-letter code or its full name, ignoring case.
    pub fn us_state(value: &str) -> Result<(), &'static str> {
        let known = US_STATES.iter().any(|(code, name)| {
            value.eq_ignore_ascii_case(code) || value.eq_ignore_ascii_case(name)
        });
        if known {
            Ok(())
        } else {
            Err("must be a US state code or name, such as NY or New York")
        }
    }

    /// The states and territories the onboarding API lists.
    const US_STATES: &[(&str, &str)] = &[
        ("AL", "Alabama"),
        ("AK", "Alaska"),
        ("AS", "American Samoa"),
        ("AZ", "Arizona"),
        ("AR", "Arkansas"),
        ("CA", "California"),
        ("CO", "Colorado"),
        ("CT", "Connecticut"),
        ("DE", "Delaware"),
        ("DC", "District of Columbia"),
        ("FL", "Florida"),
        ("GA", "Georgia"),
        ("GU", "Guam"),
        ("HI", "Hawaii"),
        ("ID", "Idaho"),
        ("IL", "Illinois"),
        ("IN", "Indiana"),
        ("IA", "Iowa"),
        ("KS", "Kansas"),
        ("KY", "Kentucky"),
        ("LA", "Louisiana"),
        ("ME", "Maine"),
        ("MD", "Maryland"),
        ("MA", "Massachusetts"),
        ("MI", "Michigan"),
        ("MN", "Minnesota"),
        ("MS", "Mississippi"),
        ("MO", "Missouri"),
        ("MT", "Montana"),
        ("NE", "Nebraska"),
        ("NV", "Nevada"),
        ("NH", "New Hampshire"),
        ("NJ", "New Jersey"),
        ("NM", "New Mexico"),
        ("NY", "New York"),
        ("NC", "North Carolina"),
        ("ND", "North Dakota"),
        ("OH", "Ohio"),
        ("OK", "Oklahoma"),
        ("OR", "Oregon"),
        ("PA", "Pennsylvania"),
        ("PR", "Puerto Rico"),
        ("RI", "Rhode Island"),
        ("SC", "South Carolina"),
        ("SD", "South Dakota"),
        ("TN", "Tennessee"),
        ("TX", "Texas"),
        ("UT", "Utah"),
        ("VT", "Vermont"),
        ("VI", "Virgin Islands"),
        ("VA", "Virginia"),
        ("WA", "Washington"),
        ("WV", "West Virginia"),
        ("WI", "Wisconsin"),
        ("WY", "Wyoming"),
    ];
}

#[cfg(test)]
mod tests {
    mod one_of {
        use super::super::*;

        const BANK: &[&str] = &["account_number", "routing_number"];
        const TOKEN: &[&str] = &["account_token"];

        fn check(bank: [bool; 2], token: bool) -> Vec<String> {
            let mut out = Vec::new();
            check_one_of(
                &[(BANK, &bank), (TOKEN, &[token])],
                &mut Path::default(),
                &mut out,
            );
            out.iter().map(ToString::to_string).collect()
        }

        #[test]
        fn one_complete_group_passes() {
            assert!(check([true, true], false).is_empty());
            assert!(check([false, false], true).is_empty());
            // A companion without its key does not choose a group.
            assert!(check([false, true], true).is_empty());
        }

        #[test]
        fn none_or_several_fail() {
            assert_eq!(
                check([false, false], false),
                ["set exactly one of account_number or account_token"]
            );
            assert_eq!(
                check([true, true], true),
                [
                    "set exactly one of account_number or account_token, not account_number and account_token"
                ]
            );
        }

        #[test]
        fn a_missing_companion_fails() {
            assert_eq!(
                check([true, false], false),
                ["routing_number: is required with account_number"]
            );
        }

        #[test]
        fn joins_lists() {
            assert_eq!(join(&["a", "b", "c"], "or"), "a, b or c");
            assert_eq!(join(&["a"], "or"), "a");
        }
    }

    use super::rules::*;
    use super::*;

    #[test]
    fn digits_accepts_only_digits() {
        assert!(digits("0123456789").is_ok());
        for bad in ["", "12 3", "12-3", "１２", "1a"] {
            assert!(digits(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn routing_number_checks_length_and_checksum() {
        // The sandbox's routing number, and a real one (JPMorgan Chase, NY).
        assert!(routing_number("123123123").is_ok());
        assert!(routing_number("021000021").is_ok());
        assert_eq!(
            routing_number("123456789"),
            Err("is not a valid routing number (checksum mismatch)")
        );
        for bad in ["12312312", "1231231234", "12312312a", ""] {
            assert_eq!(routing_number(bad), Err("must be 9 digits"), "{bad}");
        }
    }

    #[test]
    fn zip_accepts_five_or_nine_digits() {
        assert!(zip("12345").is_ok());
        assert!(zip("12345-6789").is_ok());
        for bad in [
            "1234",
            "123456",
            "12345-678",
            "12345-",
            "abcde",
            "12345 6789",
        ] {
            assert!(zip(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn email_is_a_light_check() {
        assert!(email("bob@example.com").is_ok());
        assert!(email("bob+ach@mail.example.co").is_ok());
        for bad in [
            "",
            "bob",
            "@example.com",
            "bob@",
            "bob@example",
            "a b@x.com",
            "a@b@c.com",
            "bob@.com",
        ] {
            assert!(email(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn us_state_accepts_codes_and_names() {
        for ok in [
            "NY",
            "ny",
            "New York",
            "new york",
            "American Samoa",
            "DC",
            "pr",
        ] {
            assert!(us_state(ok).is_ok(), "{ok}");
        }
        for bad in ["XX", "", "N.Y.", "Ontario"] {
            assert!(us_state(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn validation_error_reports_the_first_and_counts_the_rest() {
        let one = |path: &str| Violation {
            path: path.into(),
            message: "is required".into(),
        };
        let error = ValidationError {
            violations: vec![one("amount")],
        };
        assert_eq!(error.to_string(), "amount: is required");
        let error = ValidationError {
            violations: vec![one("amount"), one("zip"), one("street")],
        };
        assert_eq!(
            error.to_string(),
            "amount: is required (and 2 more violations)"
        );
    }

    #[test]
    fn range_bounds_are_inclusive() {
        let mut out = Vec::new();
        let path = Path::default();
        check_range(Decimal::from(0), Some("0"), Some("2"), &path, &mut out);
        check_range(Decimal::from(2), Some("0"), Some("2"), &path, &mut out);
        assert!(out.is_empty());
        check_range(Decimal::from(3), Some("0"), Some("2"), &path, &mut out);
        assert_eq!(out[0].message, "must be at most 2, not 3");
    }
}
