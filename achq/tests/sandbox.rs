//! The clients against ACHQ's public sandbox (ProviderID 99, merchant 2001).
//!
//! Ignored by default; run with
//!
//! ```sh
//! ACHQ_SANDBOX=1 cargo test -p achq --all-features --test sandbox -- --ignored
//! ```
//!
//! The sandbox is shared and its records are visible to anyone using it: these
//! tests send only made-up names, addresses and accounts. Its payments are never
//! processed.
#![cfg(all(feature = "reqwest", feature = "ureq"))]

use std::net::Ipv4Addr;
use std::time::{SystemTime, UNIX_EPOCH};

use achq::bank::BankAccount;
use achq::events::{EventQuery, PaymentStatus};
use achq::insights::{AccountStatusRequest, OwnerAuthRequest};
use achq::payments::{
    Customer, PaymentDirection, PaymentRequest, PaymentSource, PaymentUpdate, SecCode,
};
use achq::settlements::{SettlementDetailsQuery, SettlementQuery};
use achq::tokens::TokenRequest;
use achq::transport::ReqwestTransport;
use achq::{Client, Merchant, MerchantCredentials, ProviderCredentials, dec};

const ROUTING: &str = "123123123";
const PASSING_ACCOUNT: &str = "100200300";

/// Skips unless `ACHQ_SANDBOX` is set, so that `--ignored` alone does not reach
/// the network by accident.
fn enabled() -> bool {
    let enabled = std::env::var_os("ACHQ_SANDBOX").is_some();
    if !enabled {
        eprintln!("skipping: set ACHQ_SANDBOX=1 to run against the public sandbox");
    }
    enabled
}

fn client() -> Client {
    Client::new(ProviderCredentials::sandbox())
}

fn merchant() -> Merchant<ReqwestTransport> {
    client().merchant(MerchantCredentials::sandbox())
}

/// A `Provider_TransactionID` no other run uses.
fn unique(label: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("achq-rs-test-{label}-{nanos:x}")
}

/// Runs `command` until the payment it changes has left the queue a new payment
/// waits in (see `ApiError::is_queued`).
async fn when_dequeued<T, F, Fut>(mut command: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, achq::Error>>,
{
    for _ in 0..30 {
        match command().await {
            Err(err) if err.as_api().is_some_and(achq::ApiError::is_queued) => {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            result => return result.unwrap(),
        }
    }
    panic!("the payment stayed queued for 30 seconds");
}

fn payment(label: &str, source: PaymentSource) -> PaymentRequest {
    PaymentRequest::builder()
        .provider_transaction_id(unique(label))
        .amount(dec!(1.23))
        .direction(PaymentDirection::FromCustomer)
        .description("achq-rs sandbox test")
        .sec_code(SecCode::Web)
        .customer(
            Customer::builder()
                .name("Test Customer")
                .address1("123 Test Street")
                .city("Testville")
                .state("NY")
                .zip("12345")
                .phone("555-555-0100")
                .email("test@example.com")
                .build(),
        )
        .customer_ip(Ipv4Addr::new(203, 0, 113, 9))
        // Without Express Verify the sandbox keeps a new payment queued, so that
        // it cannot be changed for a while (see `ApiError::is_queued`).
        .run_express_verify(true)
        .source(source)
        .build()
}

#[tokio::test]
#[ignore = "reaches ACHQ's public sandbox; set ACHQ_SANDBOX=1"]
async fn a_payment_from_token_to_cancellation() {
    if !enabled() {
        return;
    }
    let merchant = merchant();
    let token = merchant
        .tokens()
        .create(&TokenRequest::from(BankAccount::checking(
            ROUTING,
            PASSING_ACCOUNT,
        )))
        .await
        .unwrap()
        .token;

    let created = merchant
        .payments()
        .create(&payment("flow", PaymentSource::achq_token(&token)))
        .await
        .unwrap();
    let id = created.payment_id;

    let update = PaymentUpdate::builder().amount(dec!(2.34)).build();
    let payments = merchant.payments();
    let receipts = [
        when_dequeued(|| payments.update(id.clone(), &update)).await,
        payments.hold(id.clone()).await.unwrap(),
        payments.cancel(id.clone()).await.unwrap(),
    ];
    for receipt in receipts {
        assert_ne!(receipt.command_id.as_str(), id.as_str());
    }
}

#[tokio::test]
#[ignore = "reaches ACHQ's public sandbox; set ACHQ_SANDBOX=1"]
async fn a_tokenized_bank_payment() {
    if !enabled() {
        return;
    }
    let source = PaymentSource::bank(BankAccount::checking(ROUTING, PASSING_ACCOUNT)).tokenize();
    let created = merchant()
        .payments()
        .create(&payment("bank", source))
        .await
        .unwrap();
    assert!(created.token.is_some());
}

#[tokio::test]
#[ignore = "reaches ACHQ's public sandbox; set ACHQ_SANDBOX=1"]
async fn a_rejected_account() {
    if !enabled() {
        return;
    }
    let mut request = payment(
        "reject",
        PaymentSource::bank(BankAccount::checking(ROUTING, "987654321")),
    );
    request.run_express_verify = Some(true);
    let err = merchant().payments().create(&request).await.unwrap_err();
    let api = err.as_api().expect("an API error");
    assert!(api.is_account_rejected(), "{api}");
    assert!(api.express_verify().is_some());
}

#[tokio::test]
#[ignore = "reaches ACHQ's public sandbox; set ACHQ_SANDBOX=1"]
async fn account_checks() {
    if !enabled() {
        return;
    }
    let merchant = merchant();
    let status = AccountStatusRequest::builder()
        .provider_transaction_id(unique("status"))
        .routing_number(ROUTING)
        .account_number(PASSING_ACCOUNT)
        .build();
    assert!(
        merchant
            .insights()
            .account_status(&status)
            .await
            .unwrap()
            .is_positive()
    );

    let owner = |account: &str| {
        OwnerAuthRequest::builder()
            .provider_transaction_id(unique("owner"))
            .routing_number(ROUTING)
            .account_number(account)
            .customer_name("Test Customer")
            .build()
    };
    let confirmed = merchant
        .insights()
        .account_owner(&owner(PASSING_ACCOUNT))
        .await
        .unwrap();
    assert!(confirmed.is_owner_confirmed());
    let fails = merchant
        .insights()
        .account_owner(&owner("999999999"))
        .await
        .unwrap();
    assert!(!fails.is_owner_confirmed());
}

#[tokio::test]
#[ignore = "reaches ACHQ's public sandbox; set ACHQ_SANDBOX=1"]
async fn reports() {
    if !enabled() {
        return;
    }
    let client = client();
    let today = time::OffsetDateTime::now_utc().date();
    let query = SettlementQuery::between(today - time::Duration::days(30), today);
    let settlements = client.settlements().list(&query).await.unwrap();
    if let Some(id) = settlements.iter().find_map(|s| s.settlement_id.clone()) {
        let mut details = SettlementDetailsQuery::new(id);
        details.page_size = Some(2);
        let page = client.settlements().details(&details).await.unwrap();
        if page.has_next_page() {
            let next = page.next_page(&details).unwrap();
            let second = client.settlements().details(&next).await.unwrap();
            assert_eq!(second.page_number, Some(2));
        }
    }
    let batch = client.events().query(&EventQuery::on(today)).await.unwrap();
    for event in &batch.events {
        assert!(
            !matches!(event.status, PaymentStatus::Other(_)),
            "{event:?}"
        );
    }
}

#[test]
#[ignore = "reaches ACHQ's public sandbox; set ACHQ_SANDBOX=1"]
fn the_blocking_client() {
    if !enabled() {
        return;
    }
    let merchant = achq::blocking::Client::new(ProviderCredentials::sandbox())
        .merchant(MerchantCredentials::sandbox());
    let created = merchant
        .payments()
        .create(&payment(
            "blocking",
            PaymentSource::bank(BankAccount::checking(ROUTING, PASSING_ACCOUNT)),
        ))
        .unwrap();
    for _ in 0..30 {
        match merchant.payments().cancel(created.payment_id.clone()) {
            Err(err) if err.as_api().is_some_and(achq::ApiError::is_queued) => {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            result => {
                result.unwrap();
                return;
            }
        }
    }
    panic!("the payment stayed queued for 30 seconds");
}
