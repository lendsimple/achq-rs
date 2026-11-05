//! Create a token for a bank account, then charge the token.
//!
//! ```sh
//! cargo run -p achq --example tokenize_and_charge
//! ```
//!
//! Uses ACHQ's public sandbox, unless `ACHQ_PROVIDER_ID`, `ACHQ_PROVIDER_GATE_ID`,
//! `ACHQ_PROVIDER_GATE_KEY`, `ACHQ_MERCHANT_ID`, `ACHQ_MERCHANT_GATE_ID` and
//! `ACHQ_MERCHANT_GATE_KEY` are set. The sandbox is shared: send only made-up data.

use std::net::Ipv4Addr;

use achq::bank::BankAccount;
use achq::payments::{Customer, PaymentDirection, PaymentRequest, PaymentSource, SecCode};
use achq::tokens::TokenRequest;
use achq::{Client, MerchantCredentials, ProviderCredentials, dec};

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (provider, merchant) = credentials();
    let merchant = Client::new(provider).merchant(merchant);

    // The token stands for the account from now on: store it, not the numbers.
    let account = BankAccount::checking("123123123", "100200300");
    let created = merchant
        .tokens()
        .create(&TokenRequest::from(account))
        .await?;
    println!("token created (command {})", created.command_id);

    let payment = PaymentRequest::builder()
        .provider_transaction_id(format!("example-{}", created.command_id))
        .amount(dec!(19.99))
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
        .run_express_verify(true)
        .source(PaymentSource::achq_token(&created.token))
        .build();
    let charged = merchant.payments().create(&payment).await?;
    println!("payment {} created", charged.payment_id);
    if let Some(verify) = charged.express_verify {
        println!(
            "express verify: {:?} {:?}",
            verify.status, verify.description
        );
    }
    Ok(())
}
