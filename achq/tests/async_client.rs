//! The async client against a mock gateway.
#![cfg(feature = "reqwest")]

#[macro_use]
mod common;

use std::time::Duration;

use achq::{Client, Error, Merchant};
use common::{Answer, MockApi, merchant_credentials, provider};

fn client(api: &MockApi) -> Client {
    Client::builder(provider())
        .endpoints(api.endpoints())
        .build()
}

fn platform(api: &MockApi) -> Client {
    client(api)
}

fn merchant(api: &MockApi) -> Merchant<achq::transport::ReqwestTransport> {
    client(api).merchant(merchant_credentials())
}

macro_rules! async_cases {
    ($(
        $name:ident: $owner:ident $resource:ident.$method:ident($($arg:expr),* $(,)?)
            => $command:literal, $answer:expr;
    )*) => {$(
        #[tokio::test]
        async fn $name() {
            let api = MockApi::new_async($answer).await;
            $owner(&api).$resource().$method($($arg),*).await.unwrap();
            let received = api.received();
            common::assert_command(&received, $command, stringify!($owner) == "merchant");
        }
    )*};
}

client_cases!(async_cases);

#[tokio::test]
async fn sends_the_command_fields_after_the_credentials() {
    let api = MockApi::new_async(Answer::fixture("cancel.json")).await;
    let receipt = merchant(&api).payments().cancel("14477175").await.unwrap();
    assert_eq!(receipt.command_id.as_str(), "14477176");
    let fields = common::form_fields(&api.received().body);
    let names: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "ProviderID",
            "Provider_GateID",
            "Provider_GateKey",
            "MerchantID",
            "Merchant_GateID",
            "Merchant_GateKey",
            "Command",
            "CommandVersion",
            "Transact_ReferenceID",
        ]
    );
}

#[tokio::test]
async fn test_mode_is_sent() {
    let api = MockApi::new_async(Answer::fixture("cancel.json")).await;
    let client = Client::builder(provider())
        .endpoints(api.endpoints())
        .test_mode(true)
        .build();
    client
        .merchant(merchant_credentials())
        .payments()
        .cancel("1")
        .await
        .unwrap();
    assert_eq!(api.received().field("TestMode").as_deref(), Some("On"));
}

#[tokio::test]
async fn a_declined_command_is_an_api_error() {
    let api = MockApi::new_async(Answer::fixture("declined_credentials.json")).await;
    let err = merchant(&api).payments().hold("1").await.unwrap_err();
    let api_error = err.as_api().expect("an API error");
    assert_eq!(api_error.field(), Some("Provider_Credentials"));
    assert!(api_error.is_credentials());
}

#[tokio::test]
async fn an_error_status_is_returned_with_its_body() {
    let api =
        MockApi::new_async(Answer::with("text/plain", "Service Unavailable").status(503)).await;
    let err = platform(&api)
        .events()
        .query(&common::event_query())
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Status { status: 503, body } if body == "Service Unavailable"),
        "{err:?}"
    );
}

#[tokio::test]
async fn an_html_page_is_not_a_response() {
    let page = common::fixture("responses/iis_error.html");
    let api = MockApi::new_async(Answer::with("text/html", page)).await;
    let err = merchant(&api).payments().cancel("1").await.unwrap_err();
    assert!(
        matches!(err, Error::UnexpectedContentType { status: 200, .. }),
        "{err:?}"
    );
}

#[tokio::test]
async fn times_out() {
    let api =
        MockApi::new_async(Answer::fixture("cancel.json").delayed(Duration::from_secs(2))).await;
    let client = Client::builder(provider())
        .endpoints(api.endpoints())
        .timeout(Duration::from_millis(200))
        .build();
    let err = client
        .merchant(merchant_credentials())
        .payments()
        .cancel("1")
        .await
        .unwrap_err();
    match err {
        Error::Transport(err) => assert!(err.is_timeout(), "{err}"),
        other => panic!("expected a timeout, got {other:?}"),
    }
}

#[tokio::test]
async fn a_refused_connection_is_a_transport_error() {
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
        .await
        .unwrap_err();
    match err {
        Error::Transport(err) => assert!(!err.is_timeout(), "{err}"),
        other => panic!("expected a transport error, got {other:?}"),
    }
}

#[tokio::test]
async fn an_invalid_command_is_not_sent() {
    let api = MockApi::new_async(Answer::fixture("cancel.json")).await;
    let err = merchant(&api).payments().cancel(" ").await.unwrap_err();
    assert!(matches!(err, Error::Validation(_)), "{err:?}");
    api.assert_not_called();
}

#[test]
fn futures_are_send() {
    fn assert_send<T: Send>(_: &T) {}
    let client = Client::new(provider());
    let merchant = client.merchant(merchant_credentials());
    let payment = common::bank_payment();
    assert_send(&merchant.payments().create(&payment));
    assert_send(&merchant.payments().cancel("1"));
    let query = common::event_query();
    assert_send(&client.events().query(&query));
    let merchant_clone = merchant.clone();
    assert_send(&merchant_clone);
}
