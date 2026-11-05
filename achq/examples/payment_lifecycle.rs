//! A payment's life before it is sent: create it, change its amount, hold it, and
//! cancel it. Each change returns an ID of its own; the payment keeps its ID.
//!
//! ```sh
//! cargo run -p achq --example payment_lifecycle
//! ```
//!
//! Uses ACHQ's public sandbox, unless `ACHQ_PROVIDER_ID`, `ACHQ_PROVIDER_GATE_ID`,
//! `ACHQ_PROVIDER_GATE_KEY`, `ACHQ_MERCHANT_ID`, `ACHQ_MERCHANT_GATE_ID` and
//! `ACHQ_MERCHANT_GATE_KEY` are set. The sandbox is shared: send only made-up data.

use std::net::Ipv4Addr;
use std::time::Duration;

use achq::bank::BankAccount;
use achq::payments::{
    CommandReceipt, Customer, PaymentDirection, PaymentRequest, PaymentSource, PaymentUpdate,
    SecCode,
};
use achq::{Client, Error, MerchantCredentials, ProviderCredentials, dec};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))
}

/// The credentials from the environment, or the public sandbox's.
fn credentials() -> (ProviderCredentials, MerchantCredentials) {
    if std::env::var_os("ACHQ_PROVIDER_ID").is_none() {
        return (
            ProviderCredentials::sandbox(),
            MerchantCredentials::sandbox(),
        );
    }
    (
        ProviderCredentials::new(
            env("ACHQ_PROVIDER_ID"),
            env("ACHQ_PROVIDER_GATE_ID"),
            env("ACHQ_PROVIDER_GATE_KEY"),
        ),
        MerchantCredentials::new(
            env("ACHQ_MERCHANT_ID"),
            env("ACHQ_MERCHANT_GATE_ID"),
            env("ACHQ_MERCHANT_GATE_KEY"),
        ),
    )
}

/// A new payment may still be queued for a moment: try again while it is.
async fn retry_while_queued<F, Fut>(mut command: F) -> Result<CommandReceipt, Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<CommandReceipt, Error>>,
{
    for _ in 0..10 {
        match command().await {
            Err(err) if err.as_api().is_some_and(achq::ApiError::is_queued) => {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            result => return result,
        }
    }
    command().await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (provider, merchant) = credentials();
    let merchant = Client::new(provider).merchant(merchant);

    let payment = PaymentRequest::builder()
        .amount(dec!(25))
        .direction(PaymentDirection::FromCustomer)
        .description("achq-rs example")
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
        // Without Express Verify, the sandbox keeps a new payment queued for a while.
        .run_express_verify(true)
        .source(PaymentSource::bank(BankAccount::checking(
            "123123123",
            "100200300",
        )))
        .build();
    let payment_id = merchant.payments().create(&payment).await?.payment_id;
    println!("payment {payment_id} created");

    let payments = merchant.payments();
    let update = PaymentUpdate::builder().amount(dec!(30)).build();
    let updated = retry_while_queued(|| payments.update(payment_id.clone(), &update)).await?;
    println!("amount changed (command {})", updated.command_id);
    let held = payments.hold(payment_id.clone()).await?;
    println!("held (command {})", held.command_id);
    let cancelled = payments.cancel(payment_id.clone()).await?;
    println!(
        "cancelled (command {}); the payment is still {payment_id}",
        cancelled.command_id
    );
    Ok(())
}
