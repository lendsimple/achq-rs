//! What the clients log. A separate test binary, because these tests install a
//! global `tracing` subscriber.
#![cfg(any(feature = "reqwest", feature = "ureq"))]

mod common;

use std::sync::OnceLock;

use common::{Answer, LogCapture, MockApi, merchant_credentials, provider};

/// What the requests and responses carry that must never be logged: the gate
/// keys, the account number, the tax ID, and the token the gateway returns.
const SECRETS: [&str; 5] = [
    common::PROVIDER_KEY,
    common::MERCHANT_KEY,
    common::ACCOUNT_NUMBER,
    common::TAX_ID,
    "achq-sandbox-3ff6d-b64fd-5a8a5",
];

fn logs() -> &'static LogCapture {
    static LOGS: OnceLock<LogCapture> = OnceLock::new();
    LOGS.get_or_init(|| {
        let logs = LogCapture::default();
        tracing::subscriber::set_global_default(logs.subscriber()).unwrap();
        logs
    })
}

fn assert_redacted(logs: &str) {
    for expected in [
        r#"operation="payments.create""#,
        r#"command="ECheck.ProcessPayment""#,
        r#"operation="insights.account_owner""#,
        r#"operation="tokens.create""#,
        "Provider_GateKey=[REDACTED]",
        "Merchant_GateKey=[REDACTED]",
        "AccountNumber=[REDACTED]",
        "TaxID=[REDACTED]",
        r#""ACHQToken":"[REDACTED]""#,
        "RoutingNumber=123123123",
    ] {
        assert!(
            logs.contains(expected),
            "{expected} missing from logs:\n{logs}"
        );
    }
    for secret in SECRETS {
        assert!(!logs.contains(secret), "{secret} leaked into logs:\n{logs}");
    }
}

#[cfg(feature = "ureq")]
#[test]
fn blocking_client_logs_redact_secrets() {
    let logs = logs();
    let merchant = |api: &MockApi| {
        achq::blocking::Client::builder(provider())
            .endpoints(api.endpoints())
            .build()
            .merchant(merchant_credentials())
    };
    let api = MockApi::new(Answer::fixture("payment_bank.json"));
    merchant(&api)
        .payments()
        .create(&common::bank_payment())
        .unwrap();
    let api = MockApi::new(Answer::json(common::VERIFICATION));
    merchant(&api)
        .insights()
        .account_owner(&common::owner())
        .unwrap();
    let api = MockApi::new(Answer::fixture("token_created.json"));
    merchant(&api)
        .tokens()
        .create(&common::token_request())
        .unwrap();
    assert_redacted(&logs.contents());
}

#[cfg(feature = "reqwest")]
#[tokio::test]
async fn async_client_logs_redact_secrets() {
    let logs = logs();
    let merchant = |api: &MockApi| {
        achq::Client::builder(provider())
            .endpoints(api.endpoints())
            .build()
            .merchant(merchant_credentials())
    };
    let api = MockApi::new_async(Answer::fixture("payment_bank.json")).await;
    merchant(&api)
        .payments()
        .create(&common::bank_payment())
        .await
        .unwrap();
    let api = MockApi::new_async(Answer::json(common::VERIFICATION)).await;
    merchant(&api)
        .insights()
        .account_owner(&common::owner())
        .await
        .unwrap();
    let api = MockApi::new_async(Answer::fixture("token_created.json")).await;
    merchant(&api)
        .tokens()
        .create(&common::token_request())
        .await
        .unwrap();
    assert_redacted(&logs.contents());
}
