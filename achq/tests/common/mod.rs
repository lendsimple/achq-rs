//! Helpers shared by integration tests.

// Each test crate compiles this module and uses only part of it.
#![allow(dead_code, unused_macros)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use achq::bank::BankAccount;
use achq::events::EventQuery;
use achq::insights::{AccountStatusRequest, OwnerAuthRequest};
use achq::payments::{
    Customer, PaymentDirection, PaymentRequest, PaymentSource, PaymentUpdate, SecCode,
};
use achq::settlements::{SettlementDetailsQuery, SettlementQuery};
use achq::tokens::TokenRequest;
use achq::{Endpoints, MerchantCredentials, ProviderCredentials, dec};
use time::macros::date;

pub const PROVIDER_KEY: &str = "provider-s3cret";
pub const MERCHANT_KEY: &str = "merchant-s3cret";
pub const ACCOUNT_NUMBER: &str = "987650123";
pub const TAX_ID: &str = "123456789";

pub fn provider() -> ProviderCredentials {
    ProviderCredentials::new("99", "provider-gate", PROVIDER_KEY)
}

pub fn merchant_credentials() -> MerchantCredentials {
    MerchantCredentials::new("2001", "merchant-gate", MERCHANT_KEY)
}

pub fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

/// A file under `tests/data`, such as `responses/cancel.json`.
pub fn fixture(name: &str) -> Vec<u8> {
    let path = data_dir().join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Decodes a form component: `+` is a space, `%XX` a byte.
pub fn form_decode(component: &str) -> String {
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

/// The fields of a form body, decoded, in order.
pub fn form_fields(body: &[u8]) -> Vec<(String, String)> {
    let body = std::str::from_utf8(body).unwrap();
    body.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (form_decode(name), form_decode(value))
        })
        .collect()
}

/// What a mock gateway received.
#[derive(Debug, Clone, Default)]
pub struct Received {
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Received {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The value of the form field `name`, decoded.
    pub fn field(&self, name: &str) -> Option<String> {
        form_fields(&self.body)
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
    }
}

/// The gateway's path, as the mock serves it.
pub const GATEWAY_PATH: &str = "/datalinks/transact.aspx";

/// A mock gateway answering every command with one canned response, and recording
/// the request. It matches the method, path, and the client's form, `Accept` and
/// `User-Agent` headers; anything else gets mockito's 501.
pub struct MockApi {
    server: mockito::ServerGuard,
    mock: mockito::Mock,
    received: Arc<Mutex<Option<Received>>>,
}

/// One canned answer.
pub struct Answer {
    pub status: usize,
    pub content_type: &'static str,
    pub body: Vec<u8>,
    pub delay: std::time::Duration,
}

impl Answer {
    pub fn json(body: impl Into<Vec<u8>>) -> Self {
        Self::with("application/json; charset=utf-8", body)
    }

    /// A fixture under `tests/data/responses`, as JSON.
    pub fn fixture(name: &str) -> Self {
        Self::json(fixture(&format!("responses/{name}")))
    }

    /// Event tracking's answer: CSV, as plain text.
    pub fn csv(body: impl Into<Vec<u8>>) -> Self {
        Self::with("text/plain; charset=utf-8", body)
    }

    pub fn with(content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            content_type,
            body: body.into(),
            delay: std::time::Duration::ZERO,
        }
    }

    pub fn status(mut self, status: usize) -> Self {
        self.status = status;
        self
    }

    pub fn delayed(mut self, delay: std::time::Duration) -> Self {
        self.delay = delay;
        self
    }
}

impl MockApi {
    fn mock(
        server: &mut mockito::ServerGuard,
        answer: Answer,
    ) -> (mockito::Mock, Arc<Mutex<Option<Received>>>) {
        let received = Arc::new(Mutex::new(None));
        let captured = Arc::clone(&received);
        let Answer {
            status,
            content_type,
            body,
            delay,
        } = answer;
        let mock = server
            .mock("POST", GATEWAY_PATH)
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_header("accept", "application/json")
            .match_header("user-agent", mockito::Matcher::Regex("^achq-rs/".into()))
            .with_status(status)
            .with_header("content-type", content_type)
            .with_body_from_request(move |request| {
                *captured.lock().unwrap() = Some(Received {
                    headers: request
                        .headers()
                        .iter()
                        .map(|(n, v)| (n.to_string(), v.to_str().unwrap_or("").to_owned()))
                        .collect(),
                    body: request.body().unwrap().clone(),
                });
                std::thread::sleep(delay);
                body.clone()
            });
        (mock, received)
    }

    pub fn new(answer: Answer) -> Self {
        let mut server = mockito::Server::new();
        let (mock, received) = Self::mock(&mut server, answer);
        Self {
            mock: mock.create(),
            server,
            received,
        }
    }

    pub async fn new_async(answer: Answer) -> Self {
        let mut server = mockito::Server::new_async().await;
        let (mock, received) = Self::mock(&mut server, answer);
        Self {
            mock: mock.create_async().await,
            server,
            received,
        }
    }

    /// The mock's endpoints.
    pub fn endpoints(&self) -> Endpoints {
        let url = self.server.url();
        Endpoints::custom(format!("{url}{GATEWAY_PATH}"), format!("{url}/api"))
    }

    /// The request received; panics if there was none.
    pub fn received(&self) -> Received {
        self.mock.assert();
        self.received
            .lock()
            .unwrap()
            .clone()
            .expect("the client sent a request")
    }

    /// Checks that no request reached the mock.
    pub fn assert_not_called(&self) {
        assert!(
            self.received.lock().unwrap().is_none(),
            "the request was sent"
        );
    }
}

/// Checks the fields every command sends: the command, its version, the
/// platform's credentials, and the merchant's for merchant commands only.
pub fn assert_command(received: &Received, command: &str, merchant: bool) {
    assert_eq!(received.field("Command").as_deref(), Some(command));
    assert_eq!(received.field("CommandVersion").as_deref(), Some("2.0"));
    assert_eq!(received.field("ProviderID").as_deref(), Some("99"));
    assert_eq!(
        received.field("Provider_GateKey").as_deref(),
        Some(PROVIDER_KEY)
    );
    if merchant {
        assert_eq!(received.field("MerchantID").as_deref(), Some("2001"));
        assert_eq!(
            received.field("Merchant_GateKey").as_deref(),
            Some(MERCHANT_KEY)
        );
    } else {
        assert_eq!(received.field("MerchantID"), None);
        assert_eq!(received.field("Merchant_GateKey"), None);
    }
}

/// Captures `tracing` output, for checking what the clients log.
#[derive(Clone, Default)]
pub struct LogCapture(Arc<Mutex<Vec<u8>>>);

impl LogCapture {
    /// A subscriber that writes every event, at all levels, here.
    pub fn subscriber(&self) -> impl tracing::Subscriber + Send + Sync + 'static {
        let writer = self.clone();
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish()
    }

    pub fn contents(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl std::io::Write for LogCapture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// Requests for the client cases.

pub fn account() -> BankAccount {
    BankAccount::checking("123123123", ACCOUNT_NUMBER)
}

pub fn token_request() -> TokenRequest {
    TokenRequest::from(account())
}

pub fn bank_payment() -> PaymentRequest {
    PaymentRequest::builder()
        .provider_transaction_id("order-1234")
        .amount(dec!(42))
        .direction(PaymentDirection::FromCustomer)
        .sec_code(SecCode::Ppd)
        .customer(
            Customer::builder()
                .name("Bob Loblaw")
                .address1("123 Street")
                .city("Town")
                .state("NY")
                .zip("12345")
                .phone("555-867-5309")
                .email("bob@example.com")
                .build(),
        )
        .source(PaymentSource::bank(account()).tokenize())
        .build()
}

pub fn update() -> PaymentUpdate {
    PaymentUpdate::builder().amount(dec!(142)).build()
}

pub fn account_status() -> AccountStatusRequest {
    AccountStatusRequest::builder()
        .routing_number("123123123")
        .account_number(ACCOUNT_NUMBER)
        .build()
}

pub fn owner() -> OwnerAuthRequest {
    OwnerAuthRequest::builder()
        .routing_number("123123123")
        .account_number(ACCOUNT_NUMBER)
        .customer_name("Bob Loblaw")
        .tax_id(TAX_ID)
        .build()
}

pub fn settlement_query() -> SettlementQuery {
    SettlementQuery::between(date!(2023 - 12 - 01), date!(2023 - 12 - 08))
}

pub fn details_query() -> SettlementDetailsQuery {
    SettlementDetailsQuery::new("64b7811c-f631-4bc6-9eaf-d2f45356cf9e")
}

pub fn event_query() -> EventQuery {
    EventQuery::on(date!(2022 - 06 - 17))
}

/// A verification answer, with an ownership check.
pub const VERIFICATION: &str = r#"{"CommandStatus": "Approved", "ResponseCode": "000",
    "TransAct_ReferenceID": "19000010",
    "ExpressVerify": {"Status": "POS", "Code": "1111", "Description": "Pass AV"},
    "OwnerAuth": {"Status": "POS", "Code": "CA11", "Description": "Pass"}}"#;

/// Invokes `$callback!` with one case per command:
/// `name: owner resource.method(args) => "Command", answer;`, where `owner` is
/// `merchant` or `platform`. Both client suites run every case.
macro_rules! client_cases {
    ($callback:ident) => {
        $callback! {
            create_token: merchant tokens.create(&common::token_request())
                => "ECheck.CreateACHQToken", Answer::fixture("token_created.json");
            create_payment: merchant payments.create(&common::bank_payment())
                => "ECheck.ProcessPayment", Answer::fixture("payment_bank.json");
            update_payment: merchant payments.update("14476929", &common::update())
                => "ECheck.Update", Answer::fixture("update.json");
            cancel_payment: merchant payments.cancel("14477175")
                => "ECheck.Void", Answer::fixture("cancel.json");
            hold_payment: merchant payments.hold("14477207")
                => "ECheck.Hold", Answer::fixture("hold.json");
            refund_payment: merchant payments.refund("14452405", &achq::payments::RefundRequest::full())
                => "ECheck.Refund", Answer::fixture("refund.json");
            verify_account: merchant insights.account_status(&common::account_status())
                => "ECheck.VerifyPlus", Answer::json(common::VERIFICATION);
            verify_owner: merchant insights.account_owner(&common::owner())
                => "ECheck.VerifyPlus", Answer::json(common::VERIFICATION);
            list_settlements: platform settlements.list(&common::settlement_query())
                => "ECheckReports.GetSettlements", Answer::fixture("settlements.json");
            settlement_details: platform settlements.details(&common::details_query())
                => "ECheckReports.GetSettlementItems", Answer::fixture("settlement_details.json");
            track_events: platform events.query(&common::event_query())
                => "ECheckReports.StatusTrackingQuery",
                Answer::csv(common::fixture("events/docs_sample.csv"));
        }
    };
}
