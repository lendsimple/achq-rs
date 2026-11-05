//! A payment and its cancellation, with the blocking client.
//!
//! ```sh
//! cargo run -p achq --example charge_blocking --features ureq
//! ```
//!
//! Uses ACHQ's public sandbox, unless `ACHQ_PROVIDER_ID`, `ACHQ_PROVIDER_GATE_ID`,
//! `ACHQ_PROVIDER_GATE_KEY`, `ACHQ_MERCHANT_ID`, `ACHQ_MERCHANT_GATE_ID` and
//! `ACHQ_MERCHANT_GATE_KEY` are set. The sandbox is shared: send only made-up data.

use std::net::Ipv4Addr;

use achq::bank::BankAccount;
use achq::blocking::Client;
use achq::payments::{Customer, PaymentDirection, PaymentRequest, PaymentSource, SecCode};
use achq::{MerchantCredentials, ProviderCredentials, dec};

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (provider, merchant) = credentials();
    let merchant = Client::new(provider).merchant(merchant);

    let payment = PaymentRequest::builder()
        .amount(dec!(10))
        .direction(PaymentDirection::ToCustomer)
        .description("achq-rs example payout")
        .sec_code(SecCode::Ppd)
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
        .source(PaymentSource::bank(BankAccount::savings(
            "123123123",
            "100200300",
        )))
        .build();
    let created = merchant.payments().create(&payment)?;
    println!("payout {} created", created.payment_id);
    let receipt = merchant.payments().cancel(created.payment_id)?;
    println!("cancelled (command {})", receipt.command_id);
    Ok(())
}
