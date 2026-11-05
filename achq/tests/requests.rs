//! Golden requests: the exact method, URL, headers and form fields each command
//! encodes to. Review changes with `cargo insta review`.

use std::net::Ipv4Addr;

use achq::bank::BankAccount;
use achq::events::EventQuery;
use achq::insights::{AccountStatusRequest, OwnerAuthRequest};
use achq::operations::{events, insights, payments, settlements, tokens};
use achq::payments::{
    Customer, PaymentDirection, PaymentRequest, PaymentSource, PaymentUpdate, RefundRequest,
    SecCode, TokenSource,
};
use achq::protocol::{self, Context};
use achq::settlements::{SettlementDetailsQuery, SettlementQuery};
use achq::tokens::TokenRequest;
use achq::transport::HttpRequest;
use achq::types::AchqToken;
use achq::{Error, MerchantCredentials, Operation, ProviderCredentials, dec};
use time::macros::date;

fn platform() -> Context {
    Context::new(ProviderCredentials::new(
        "99",
        "provider-gate",
        "provider-s3cret",
    ))
    .user_agent("achq-rs/test")
}

fn merchant() -> Context {
    platform().merchant(MerchantCredentials::new(
        "2001",
        "merchant-gate",
        "merchant-s3cret",
    ))
}

/// The request as text: the request line, sorted headers, and each form field on
/// a line of its own (decoded), with the gate keys replaced.
fn render(request: &HttpRequest) -> String {
    let mut out = format!("{} {}\n", request.method, request.url);
    let mut headers = request.headers.clone();
    headers.sort();
    for (name, value) in headers {
        out.push_str(&format!("{name}: {value}\n"));
    }
    out.push('\n');
    let body = String::from_utf8(request.body.clone()).unwrap();
    for pair in body.split('&') {
        let (name, value) = pair.split_once('=').unwrap();
        let value = decode(value);
        let value = match name {
            "Provider_GateKey" => {
                assert_eq!(value, "provider-s3cret");
                "<key>".to_owned()
            }
            "Merchant_GateKey" => {
                assert_eq!(value, "merchant-s3cret");
                "<key>".to_owned()
            }
            _ => value,
        };
        out.push_str(&format!("{}={value}\n", decode(name)));
    }
    out
}

/// Decodes a form component: `+` is a space, `%XX` a byte.
fn decode(component: &str) -> String {
    let bytes = component.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => {
                out.push(u8::from_str_radix(&component[i + 1..i + 3], 16).unwrap());
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8(out).unwrap()
}

fn encode<O: Operation>(op: &O, context: &Context) -> String {
    render(&protocol::encode(op, context).unwrap())
}

fn customer() -> Customer {
    Customer::builder()
        .name("Bob Loblaw")
        .address1("123 Street")
        .city("Town")
        .state("NY")
        .zip("12345")
        .country("US")
        .phone("111-867-5309")
        .email("lawblog@legal.com")
        .build()
}

fn bank_payment() -> PaymentRequest {
    PaymentRequest::builder()
        .provider_transaction_id("21321312")
        .amount(dec!(42))
        .direction(PaymentDirection::FromCustomer)
        .description("Rent Payment")
        .sec_code(SecCode::Web)
        .customer(customer())
        .customer_ip(Ipv4Addr::new(203, 0, 113, 9))
        .source(PaymentSource::bank(BankAccount::checking("123123123", "12345678")).tokenize())
        .build()
}

#[test]
fn create_token() {
    let request = TokenRequest::builder()
        .provider_transaction_id("21321312")
        .account(BankAccount::checking("123123123", "123456789"))
        .build();
    insta::assert_snapshot!(encode(&tokens::CreateToken::new(&request), &merchant()));
}

#[test]
fn create_payment_from_a_bank_account() {
    insta::assert_snapshot!(encode(
        &payments::CreatePayment::new(&bank_payment()),
        &merchant()
    ));
}

#[test]
fn create_payment_from_a_token() {
    let mut request = bank_payment();
    request.source = PaymentSource::token("processor-sandbox-74fb6a84", TokenSource::Plaid);
    request.date_scheduled = Some(date!(2026 - 01 - 05));
    request.merchant_reference_id = Some("44444".into());
    request.send_email = true;
    request.run_express_verify = Some(false);
    insta::assert_snapshot!(encode(&payments::CreatePayment::new(&request), &merchant()));
}

#[test]
fn create_payment_for_a_business_with_an_achq_token() {
    let mut request = bank_payment();
    request.source = PaymentSource::achq_token(&AchqToken::new("achq-sandbox-3ff6d-b64fd-5a8a5"));
    request.direction = PaymentDirection::ToCustomer;
    request.sec_code = SecCode::Ccd;
    request.customer.company = Some("Bluth Company".into());
    insta::assert_snapshot!(encode(&payments::CreatePayment::new(&request), &merchant()));
}

#[test]
fn update_payment() {
    let update = PaymentUpdate::builder()
        .amount(dec!(142))
        .date_scheduled(date!(2026 - 02 - 01))
        .build();
    insta::assert_snapshot!(encode(
        &payments::UpdatePayment::new("14476929", &update),
        &merchant()
    ));
}

#[test]
fn cancel_payment() {
    insta::assert_snapshot!(encode(
        &payments::CancelPayment::new("14477175"),
        &merchant()
    ));
}

#[test]
fn hold_payment() {
    insta::assert_snapshot!(encode(&payments::HoldPayment::new("14477207"), &merchant()));
}

#[test]
fn refund_part_of_a_payment() {
    let refund = RefundRequest::builder()
        .amount(dec!(23))
        .provider_transaction_id("1233")
        .merchant_reference_id("123456")
        .description("Partial refund")
        .build();
    insta::assert_snapshot!(encode(
        &payments::RefundPayment::new("14452405", &refund),
        &merchant()
    ));
}

#[test]
fn refund_a_whole_payment() {
    insta::assert_snapshot!(encode(
        &payments::RefundPayment::new("14452405", &RefundRequest::full()),
        &merchant()
    ));
}

#[test]
fn verify_an_account() {
    let request = AccountStatusRequest::builder()
        .routing_number("123123123")
        .account_number("123456789")
        .build();
    insta::assert_snapshot!(encode(&insights::VerifyAccount::new(&request), &merchant()));
}

#[test]
fn verify_an_owner() {
    let request = OwnerAuthRequest::builder()
        .routing_number("123123123")
        .account_number("123456789")
        .customer_name("Bob Loblaw")
        .zip("12345")
        .phone("5558675309")
        .tax_id("1234")
        .build();
    insta::assert_snapshot!(encode(&insights::VerifyOwner::new(&request), &merchant()));
}

#[test]
fn list_settlements() {
    let query =
        SettlementQuery::between(date!(2023 - 12 - 01), date!(2023 - 12 - 08)).merchant("2001");
    insta::assert_snapshot!(encode(
        &settlements::ListSettlements::new(&query),
        &platform()
    ));
}

#[test]
fn settlement_details() {
    let query = SettlementDetailsQuery::new("64b7811c-f631-4bc6-9eaf-d2f45356cf9e");
    insta::assert_snapshot!(encode(
        &settlements::SettlementDetails::new(&query),
        &platform()
    ));
}

#[test]
fn track_events() {
    let query = EventQuery::on(date!(2022 - 06 - 17));
    insta::assert_snapshot!(encode(&events::TrackEvents::new(&query), &platform()));
}

#[test]
fn test_mode() {
    insta::assert_snapshot!(encode(
        &payments::CancelPayment::new("14477175"),
        &merchant().test_mode(true)
    ));
}

#[test]
fn platform_commands_send_no_merchant_credentials() {
    let query = EventQuery::on(date!(2022 - 06 - 17));
    let rendered = encode(&events::TrackEvents::new(&query), &merchant());
    assert!(!rendered.contains("Merchant_"), "{rendered}");
}

#[test]
fn merchant_commands_need_a_merchant() {
    let err = protocol::encode(&payments::CancelPayment::new("1"), &platform()).unwrap_err();
    let Error::Validation(err) = err else {
        panic!("expected a validation error, got {err:?}");
    };
    assert_eq!(
        err.to_string(),
        "merchant credentials: are required for payments.cancel"
    );
}

#[test]
fn invalid_commands_are_not_encoded() {
    let mut payment = bank_payment();
    payment.customer.zip = "1234".into();
    let err = protocol::encode(&payments::CreatePayment::new(&payment), &merchant()).unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid request: customer.zip: must be a ZIP code: ##### or #####-####"
    );
    let err = protocol::encode(&payments::HoldPayment::new(" "), &merchant()).unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid request: payment_id: must not be empty"
    );
    // Without validation, the gateway decides.
    let request = protocol::encode_unvalidated(&payments::HoldPayment::new(" "), &platform());
    assert!(!request.body.is_empty());
}

#[test]
fn the_form_redacts_the_keys_and_account_numbers() {
    let form = protocol::form(&payments::CreatePayment::new(&bank_payment()), &merchant());
    let redacted = form.redacted();
    for secret in ["provider-s3cret", "merchant-s3cret", "12345678"] {
        assert!(!redacted.contains(secret), "{redacted}");
    }
    assert!(
        redacted.contains("Provider_GateKey=[REDACTED]"),
        "{redacted}"
    );
    assert!(redacted.contains("AccountNumber=[REDACTED]"), "{redacted}");
    assert!(redacted.contains("RoutingNumber=123123123"), "{redacted}");
}
