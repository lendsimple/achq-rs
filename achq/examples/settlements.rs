//! The last 30 days of settlements, and the payments in the first one, a page at a
//! time.
//!
//! ```sh
//! cargo run -p achq --example settlements
//! ```
//!
//! Uses ACHQ's public sandbox, unless `ACHQ_PROVIDER_ID`, `ACHQ_PROVIDER_GATE_ID`
//! and `ACHQ_PROVIDER_GATE_KEY` are set.

use achq::settlements::{SettlementDetailsQuery, SettlementQuery};
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
    let today = time::OffsetDateTime::now_utc().date();
    let query = SettlementQuery::between(today - time::Duration::days(30), today);
    let settlements = client.settlements().list(&query).await?;
    for settlement in &settlements {
        println!(
            "{:?} {:?} {:?} {:?} ({:?} payments)",
            settlement.effective_date.map(|d| d.as_primitive().date()),
            settlement.direction,
            settlement.amount,
            settlement.settlement_type,
            settlement.item_count,
        );
    }

    let Some(id) = settlements.iter().find_map(|s| s.settlement_id.clone()) else {
        println!("no settlements");
        return Ok(());
    };
    let mut page_query = SettlementDetailsQuery::new(id);
    page_query.page_size = Some(10);
    // The first three pages, to keep the example short.
    for _ in 0..3 {
        let page = client.settlements().details(&page_query).await?;
        println!("page {:?}:", page.page_number);
        for item in &page.items {
            println!(
                "  {:?} {:?} {:?}",
                item.payment_id, item.amount, item.current_status
            );
        }
        match page.next_page(&page_query) {
            Some(next) => page_query = next,
            None => break,
        }
    }
    Ok(())
}
