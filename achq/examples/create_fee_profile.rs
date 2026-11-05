//! A fee profile for the onboarding API. Prints the request; sends it only with
//! `--send`, which creates the profile.
//!
//! ```sh
//! cargo run -p achq --example create_fee_profile
//! cargo run -p achq --example create_fee_profile -- --send
//! ```
//!
//! Uses ACHQ's public sandbox, unless `ACHQ_PROVIDER_ID`, `ACHQ_PROVIDER_GATE_ID`
//! and `ACHQ_PROVIDER_GATE_KEY` are set.

use achq::onboarding::FeeSchedule;
use achq::{Client, ProviderCredentials, dec};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}"))
}

/// The platform's credentials from the environment, or the public sandbox's.
fn provider() -> ProviderCredentials {
    if std::env::var_os("ACHQ_PROVIDER_ID").is_none() {
        return ProviderCredentials::sandbox();
    }
    ProviderCredentials::new(
        env("ACHQ_PROVIDER_ID"),
        env("ACHQ_PROVIDER_GATE_ID"),
        env("ACHQ_PROVIDER_GATE_KEY"),
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Fees not set are sent as zero: the API requires every fee.
    let fees = FeeSchedule::builder()
        .name("Standard")
        .monthly_fee(dec!(25))
        .transaction_fee(dec!(0.25))
        .return_fee(dec!(5))
        .percent_fee(dec!(0.5))
        .percent_cap(dec!(10))
        .build();
    println!("{}", serde_json::to_string_pretty(&fees)?);

    if std::env::args().any(|arg| arg == "--send") {
        let created = Client::new(provider())
            .onboarding()
            .create_fee_profile(&fees)
            .await?;
        println!(
            "fee profile {} ({:?})",
            created.fee_profile_id, created.status
        );
    }
    Ok(())
}
