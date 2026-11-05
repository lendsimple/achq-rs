//! `#[derive(Model)]` with `#[achq(form)]`: the form body a struct writes, and the
//! description of its fields.

use std::net::Ipv4Addr;

use achq::form::{FieldMeta, ToForm};
use achq::types::{Amount, ProviderTransactionId};
use achq::{Model, dec};
use pretty_assertions::assert_eq;
use time::Date;
use time::macros::date;

#[derive(Model)]
#[achq(form)]
struct Billing {
    #[achq(form = "CustomerName", max_length = 80)]
    name: String,
    #[achq(form = "Address1", max_length = 70)]
    address1: String,
    #[achq(form = "Address2", max_length = 40)]
    address2: Option<String>,
    #[achq(form = "Zip", rule = "zip")]
    zip: String,
}

#[derive(Model)]
#[achq(form)]
struct BankAccount {
    #[achq(form = "RoutingNumber", rule = "routing_number")]
    routing_number: String,
    #[achq(form = "AccountNumber", sensitive, max_length = 30)]
    account_number: String,
}

#[derive(Model)]
#[achq(form)]
struct Payment {
    #[achq(form = "Provider_TransactionID", max_length = 50)]
    provider_transaction_id: Option<ProviderTransactionId>,
    #[achq(form = "Amount")]
    amount: Amount,
    #[achq(form = "DateScheduled", with = "mdy_slash")]
    date_scheduled: Option<Date>,
    #[achq(flatten, prefix = "Billing_")]
    billing: Billing,
    #[achq(form = "Customer_IPAddress", required)]
    customer_ip: Option<Ipv4Addr>,
    #[achq(flatten)]
    account: Option<BankAccount>,
    #[achq(form = "SendEmailToCustomer", with = "yes_no")]
    send_email: bool,
    #[achq(skip_form)]
    note: Option<String>,
}

fn payment() -> Payment {
    Payment {
        provider_transaction_id: Some("order 1234".into()),
        amount: Amount::from(dec!(42)),
        date_scheduled: Some(date!(2026 - 01 - 05)),
        billing: Billing {
            name: "Bob Loblaw".into(),
            address1: "123 Street".into(),
            address2: None,
            zip: "12345".into(),
        },
        customer_ip: Some(Ipv4Addr::new(203, 0, 113, 9)),
        account: Some(BankAccount {
            routing_number: "123123123".into(),
            account_number: "12345678".into(),
        }),
        send_email: false,
        note: Some("not sent".into()),
    }
}

#[test]
fn writes_fields_in_declaration_order() {
    assert_eq!(
        payment().to_form().encode(),
        "Provider_TransactionID=order+1234\
         &Amount=42.00\
         &DateScheduled=01%2F05%2F2026\
         &Billing_CustomerName=Bob+Loblaw\
         &Billing_Address1=123+Street\
         &Billing_Zip=12345\
         &Customer_IPAddress=203.0.113.9\
         &RoutingNumber=123123123\
         &AccountNumber=12345678\
         &SendEmailToCustomer=No"
    );
}

#[test]
fn leaves_out_unset_options() {
    let mut payment = payment();
    payment.provider_transaction_id = None;
    payment.date_scheduled = None;
    payment.customer_ip = None;
    payment.account = None;
    payment.send_email = true;
    assert_eq!(
        payment.to_form().encode(),
        "Amount=42.00\
         &Billing_CustomerName=Bob+Loblaw\
         &Billing_Address1=123+Street\
         &Billing_Zip=12345\
         &SendEmailToCustomer=Yes"
    );
}

#[test]
fn redacts_sensitive_fields() {
    let form = payment().to_form();
    let redacted = form.redacted();
    assert!(
        redacted.contains("&AccountNumber=[REDACTED]&"),
        "{redacted}"
    );
    assert!(!redacted.contains("12345678"), "{redacted}");
    assert!(redacted.contains("RoutingNumber=123123123"), "{redacted}");
    let sensitive: Vec<&str> = form
        .pairs()
        .filter(|(_, _, sensitive)| *sensitive)
        .map(|(name, _, _)| name)
        .collect();
    assert_eq!(sensitive, ["AccountNumber"]);
}

#[test]
fn describes_its_fields() {
    let mut fields = Vec::new();
    Payment::form_fields("", &mut fields);
    let field = |wire: &str, required: bool, max_length: Option<usize>, sensitive: bool| {
        FieldMeta::new(wire.into(), required, max_length, sensitive)
    };
    assert_eq!(
        fields,
        [
            field("Provider_TransactionID", false, Some(50), false),
            field("Amount", true, None, false),
            field("DateScheduled", false, None, false),
            field("Billing_CustomerName", true, Some(80), false),
            field("Billing_Address1", true, Some(70), false),
            field("Billing_Address2", false, Some(40), false),
            field("Billing_Zip", true, None, false),
            field("Customer_IPAddress", true, None, false),
            // In an `Option<BankAccount>`, so optional.
            field("RoutingNumber", false, None, false),
            field("AccountNumber", false, Some(30), true),
            field("SendEmailToCustomer", true, None, false),
        ]
    );
}

#[derive(Model)]
#[achq(form)]
struct Outer {
    #[achq(flatten, prefix = "Outer_")]
    inner: Inner,
}

#[derive(Model)]
#[achq(form)]
struct Inner {
    #[achq(flatten, prefix = "Inner_")]
    billing: Billing,
}

#[test]
fn nested_prefixes_compose() {
    let outer = Outer {
        inner: Inner {
            billing: payment().billing,
        },
    };
    let names: Vec<String> = outer
        .to_form()
        .pairs()
        .map(|(name, _, _)| name.to_owned())
        .collect();
    assert_eq!(
        names,
        [
            "Outer_Inner_CustomerName",
            "Outer_Inner_Address1",
            "Outer_Inner_Zip"
        ]
    );
    let mut fields = Vec::new();
    Outer::form_fields("", &mut fields);
    assert_eq!(fields[3].wire, "Outer_Inner_Zip");
}

#[test]
fn validation_still_applies() {
    let mut payment = payment();
    payment.billing.zip = "1234".into();
    payment.customer_ip = None;
    let error = achq::validate::Validate::validate(&payment).unwrap_err();
    let messages: Vec<String> = error.violations().iter().map(ToString::to_string).collect();
    assert_eq!(
        messages,
        [
            "billing.zip: must be a ZIP code: ##### or #####-####",
            "customer_ip: is required"
        ]
    );
}
