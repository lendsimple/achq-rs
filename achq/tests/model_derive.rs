//! `#[derive(Model)]`: validation paths and messages, cross-field checks, and
//! redacted `Debug` output.

use achq::Model;
use achq::dec;
use achq::types::Amount;
use achq::validate::{Path, Validate, Violation, violation};

#[derive(Model)]
struct Contact {
    #[achq(min_length = 1, max_length = 10)]
    name: String,
    #[achq(rule = "email")]
    email: Option<String>,
}

#[derive(Model)]
struct Payment {
    amount: Amount,
    #[achq(sensitive)]
    account_number: String,
    #[achq(rule = "routing_number")]
    routing_number: String,
    #[achq(sensitive)]
    tax_id: Option<String>,
    #[achq(required, rule = "zip")]
    zip: Option<String>,
    #[achq(min = "1", max = "500")]
    page_size: Option<u16>,
    #[achq(max_length = 3)]
    tags: Vec<String>,
    #[achq(rule = "us_state")]
    states: Option<Vec<String>>,
    contacts: Vec<Contact>,
    #[achq(rule = "digits")]
    r#type: Option<String>,
}

fn valid_payment() -> Payment {
    Payment {
        amount: Amount::from(dec!(42.00)),
        account_number: "12345678".into(),
        routing_number: "123123123".into(),
        tax_id: None,
        zip: Some("12345".into()),
        page_size: Some(500),
        tags: vec!["web".into()],
        states: Some(vec!["NY".into()]),
        contacts: vec![Contact {
            name: "Bob".into(),
            email: Some("bob@example.com".into()),
        }],
        r#type: Some("1".into()),
    }
}

fn messages(value: &impl Validate) -> Vec<String> {
    match value.validate() {
        Ok(()) => Vec::new(),
        Err(error) => error.violations().iter().map(ToString::to_string).collect(),
    }
}

#[test]
fn a_valid_value_passes() {
    assert!(valid_payment().validate().is_ok());
}

#[test]
fn every_violation_is_reported_with_its_path() {
    let mut payment = valid_payment();
    payment.amount = Amount::from(dec!(1.005));
    payment.routing_number = "123456789".into();
    payment.zip = None;
    payment.page_size = Some(501);
    payment.tags = vec!["ok".into(), "toolong".into()];
    payment.states = Some(vec!["NY".into(), "XX".into()]);
    payment.contacts.push(Contact {
        name: String::new(),
        email: Some("nope".into()),
    });
    payment.r#type = Some("1a".into());

    assert_eq!(
        messages(&payment),
        [
            "amount: must have at most 2 decimal places, not 1.005",
            "routing_number: is not a valid routing number (checksum mismatch)",
            "zip: is required",
            "page_size: must be at most 500, not 501",
            "tags[1]: must be at most 3 characters, not 7",
            "states[1]: must be a US state code or name, such as NY or New York",
            "contacts[1].name: must be at least 1 character, not 0",
            "contacts[1].email: must be an email address",
            "type: must be digits only",
        ]
    );
}

#[test]
fn missing_optional_values_are_not_checked() {
    let mut payment = valid_payment();
    payment.page_size = None;
    payment.states = None;
    payment.r#type = None;
    assert!(payment.validate().is_ok());
}

#[test]
fn debug_redacts_sensitive_fields() {
    let mut payment = valid_payment();
    payment.tax_id = Some("123456789".into());
    let debug = format!("{payment:?}");
    assert!(!debug.contains("12345678\""), "{debug}");
    assert!(debug.contains("account_number: [REDACTED]"), "{debug}");
    assert!(debug.contains("tax_id: [REDACTED]"), "{debug}");
    assert!(debug.contains("routing_number: \"123123123\""), "{debug}");
    assert!(debug.starts_with("Payment {"), "{debug}");

    payment.tax_id = None;
    assert!(format!("{payment:?}").contains("tax_id: None"));
}

#[derive(Model)]
#[achq(one_of = "account_number routing_number | account_token")]
struct Source {
    #[achq(sensitive)]
    account_number: Option<String>,
    routing_number: Option<String>,
    #[achq(max_length = 10)]
    account_token: Option<String>,
}

#[test]
fn one_of_needs_exactly_one_complete_group() {
    let bank = Source {
        account_number: Some("12345678".into()),
        routing_number: Some("123123123".into()),
        account_token: None,
    };
    assert!(messages(&bank).is_empty());

    let neither = Source {
        account_number: None,
        routing_number: None,
        account_token: None,
    };
    assert_eq!(
        messages(&neither),
        ["set exactly one of account_number or account_token"]
    );

    let half_bank_and_long_token = Source {
        account_number: Some("12345678".into()),
        routing_number: None,
        account_token: Some("a-very-long-token".into()),
    };
    assert_eq!(
        messages(&half_bank_and_long_token),
        [
            "account_token: must be at most 10 characters, not 17",
            "set exactly one of account_number or account_token, not account_number and account_token",
        ]
    );
}

#[derive(Model)]
#[achq(check = "company_for_business", check = "not_both_empty")]
struct Customer {
    business: bool,
    #[achq(max_length = 5)]
    company: Option<String>,
    note: Option<String>,
}

impl Customer {
    fn company_for_business(&self, path: &mut Path, out: &mut Vec<Violation>) {
        if self.business && self.company.is_none() {
            path.push_field("company");
            violation(path, out, "is required for a business account".into());
            path.pop();
        }
    }

    fn not_both_empty(&self, path: &mut Path, out: &mut Vec<Violation>) {
        if self.company.is_none() && self.note.is_none() {
            violation(path, out, "set company or note".into());
        }
    }
}

#[test]
fn checks_run_after_the_field_checks_in_order() {
    let customer = Customer {
        business: true,
        company: None,
        note: None,
    };
    assert_eq!(
        messages(&customer),
        [
            "company: is required for a business account",
            "set company or note"
        ]
    );

    let customer = Customer {
        business: false,
        company: Some("too long".into()),
        note: None,
    };
    assert_eq!(
        messages(&customer),
        ["company: must be at most 5 characters, not 8"]
    );
}

#[test]
fn nested_checks_see_the_nested_path() {
    #[derive(Model)]
    struct Outer {
        customer: Customer,
    }
    let outer = Outer {
        customer: Customer {
            business: true,
            company: None,
            note: Some("hi".into()),
        },
    };
    assert_eq!(
        messages(&outer),
        ["customer.company: is required for a business account"]
    );
}
