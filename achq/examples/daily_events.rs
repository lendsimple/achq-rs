//! Yesterday's payment events, by the status they left each payment in.
//!
//! ```sh
//! cargo run -p achq --example daily_events
//! ```
//!
//! Uses ACHQ's public sandbox, unless `ACHQ_PROVIDER_ID`, `ACHQ_PROVIDER_GATE_ID`
//! and `ACHQ_PROVIDER_GATE_KEY` are set.

use std::collections::BTreeMap;

use achq::events::EventQuery;
use achq::{Client, ProviderCredentials};

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
    let client = Client::new(provider());
    let yesterday = time::OffsetDateTime::now_utc()
        .date()
        .previous_day()
        .expect("not the first day");
    let batch = client.events().query(&EventQuery::on(yesterday)).await?;

    let mut by_status: BTreeMap<String, usize> = BTreeMap::new();
    for event in &batch.events {
        *by_status.entry(event.status.to_string()).or_default() += 1;
        if event.status.is_returned() {
            println!(
                "{} returned: {:?} {:?}",
                event.payment_id, event.return_code, event.return_explanation
            );
        }
    }
    println!("{} events on {yesterday}:", batch.events.len());
    for (status, count) in by_status {
        println!("  {status}: {count}");
    }
    Ok(())
}
