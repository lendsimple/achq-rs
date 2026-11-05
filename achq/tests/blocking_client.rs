//! The blocking client against a mock gateway.
#![cfg(feature = "ureq")]

#[macro_use]
mod common;

use std::time::Duration;

use achq::Error;
use achq::blocking::{Client, Merchant, UreqTransport};
use common::{Answer, MockApi, merchant_credentials, provider};

fn client(api: &MockApi) -> Client {
    Client::builder(provider())
        .endpoints(api.endpoints())
        .build()
}

fn platform(api: &MockApi) -> Client {
    client(api)
}

fn merchant(api: &MockApi) -> Merchant<UreqTransport> {
    client(api).merchant(merchant_credentials())
}

macro_rules! blocking_cases {
    ($(
        $name:ident: $owner:ident $resource:ident.$method:ident($($arg:expr),* $(,)?)
            => $command:literal, $answer:expr;
    )*) => {$(
        #[test]
        fn $name() {
            let api = MockApi::new($answer);
            $owner(&api).$resource().$method($($arg),*).unwrap();
            let received = api.received();
            common::assert_command(&received, $command, stringify!($owner) == "merchant");
        }
    )*};
}

client_cases!(blocking_cases);

#[test]
fn reads_the_results() {
    let api = MockApi::new(Answer::fixture("payment_bank.json"));
    let created = merchant(&api)
        .payments()
        .create(&common::bank_payment())
        .unwrap();
    assert_eq!(created.payment_id.as_str(), "14476929");
    assert!(created.token.is_some());
    let received = api.received();
    assert_eq!(received.field("Amount").as_deref(), Some("42.00"));
    assert_eq!(
        received.field("AccountNumber").as_deref(),
        Some(common::ACCOUNT_NUMBER)
    );

    let api = MockApi::new(Answer::csv(common::fixture("events/docs_sample.csv")));
    let batch = platform(&api)
        .events()
        .query(&common::event_query())
        .unwrap();
    assert_eq!(batch.events.len(), 89);
    assert_eq!(
        api.received().field("TrackingDate").as_deref(),
        Some("06172022")
    );
}

#[test]
fn test_mode_is_sent() {
    let api = MockApi::new(Answer::fixture("cancel.json"));
    let client = Client::builder(provider())
        .endpoints(api.endpoints())
        .test_mode(true)
        .build();
    client
        .merchant(merchant_credentials())
        .payments()
        .cancel("1")
        .unwrap();
    assert_eq!(api.received().field("TestMode").as_deref(), Some("On"));
}

#[test]
fn a_declined_query_is_an_api_error() {
    let api = MockApi::new(Answer::csv(common::fixture("events/declined.csv")));
    let err = platform(&api)
        .events()
        .query(&common::event_query())
        .unwrap_err();
    assert!(err.as_api().unwrap().is_credentials(), "{err}");
}

#[test]
fn an_error_status_is_returned_with_its_body() {
    let api = MockApi::new(Answer::with("text/plain", "Bad Gateway").status(502));
    let err = merchant(&api).payments().hold("1").unwrap_err();
    assert!(
        matches!(&err, Error::Status { status: 502, body } if body == "Bad Gateway"),
        "{err:?}"
    );
}

#[test]
fn times_out() {
    let api = MockApi::new(Answer::fixture("cancel.json").delayed(Duration::from_secs(2)));
    let client = Client::builder(provider())
        .endpoints(api.endpoints())
        .timeout(Duration::from_millis(200))
        .build();
    let err = client
        .merchant(merchant_credentials())
        .payments()
        .cancel("1")
        .unwrap_err();
    match err {
        Error::Transport(err) => assert!(err.is_timeout(), "{err}"),
        other => panic!("expected a timeout, got {other:?}"),
    }
}

#[test]
fn a_refused_connection_is_a_transport_error() {
    let client = Client::builder(provider())
        .endpoints(achq::Endpoints::custom(
            "http://127.0.0.1:9/datalinks/transact.aspx",
            "http://127.0.0.1:9/api",
        ))
        .build();
    let err = client
        .merchant(merchant_credentials())
        .payments()
        .cancel("1")
        .unwrap_err();
    assert!(matches!(err, Error::Transport(_)), "{err:?}");
}

#[cfg(feature = "onboarding")]
#[test]
fn creates_a_fee_profile() {
    let api = MockApi::api(
        "/onboarding/fees",
        Answer::recorded("fee_profile_created.json"),
    );
    let created = platform(&api)
        .onboarding()
        .create_fee_profile(&common::fees())
        .unwrap();
    assert_eq!(created.fee_profile_id.as_str(), "36438685");
    let body: serde_json::Value = serde_json::from_slice(&api.received().body).unwrap();
    assert_eq!(body["feeName"], "achq-rs test");
}
