//! `cargo xtask record`: runs every command against ACHQ's public sandbox and
//! keeps the raw responses as fixtures in `achq/tests/data/recorded/`.
//!
//! Only the public sandbox (ProviderID 99, merchant 2001) is ever used, with
//! made-up data: its records are visible to anyone using it. The onboarding API
//! is only sent an invalid application (to see its error response) and one fee
//! profile; no merchant application is submitted.

use std::fmt::Write as _;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use achq::bank::BankAccount;
use achq::blocking::{Transport, UreqTransport};
use achq::events::EventQuery;
use achq::insights::{AccountStatusRequest, OwnerAuthRequest};
use achq::operations::{events, insights, payments, settlements, tokens};
use achq::payments::{
    Customer, PaymentDirection, PaymentRequest, PaymentSource, PaymentUpdate, RefundRequest,
    SecCode, TokenSource,
};
use achq::protocol::{self, Context};
use achq::settlements::{SettlementDetailsQuery, SettlementQuery};
use achq::tokens::TokenRequest;
use achq::types::{AchqToken, PaymentId};
use achq::{API_URL, Decimal, MerchantCredentials, Operation, ProviderCredentials};

use crate::Result;

/// The sandbox's test accounts (see ACHQ's *Application testing* guide).
const PASSING_ACCOUNT: &str = "100200300";
const EXPRESS_VERIFY_REJECT: &str = "987654321";
const OWNER_FAILS: &str = "999999999";
const ROUTING: &str = "123123123";
const FEE_PROFILE_FILE: &str = "fee_profile_created.json";
const PLAID_SANDBOX_TOKEN: &str = "processor-sandbox-74fb6a84-c434-41bb-8c3f-dd71fd1c18c9";

/// Refuses any credentials but the public sandbox's.
pub fn check_sandbox(provider: &ProviderCredentials) -> Result<()> {
    if provider.provider_id() == "99" {
        Ok(())
    } else {
        Err(format!(
            "refusing to record with ProviderID {}: only the public sandbox (99) is used",
            provider.provider_id()
        )
        .into())
    }
}

struct Recorder {
    transport: UreqTransport,
    dir: std::path::PathBuf,
    run: String,
    count: u32,
    summary: Vec<(String, String)>,
}

impl Recorder {
    /// A `Provider_TransactionID` unique to this run.
    fn id(&mut self) -> String {
        self.count += 1;
        format!("achq-rs-{}-{}", self.run, self.count)
    }

    fn save(&mut self, name: &str, body: &[u8]) -> String {
        let body = &scrub(name, body);
        let first = body.iter().find(|b| !b.is_ascii_whitespace()).copied();
        let ext = match first {
            Some(b'{') | Some(b'[') => "json",
            Some(b'<') => "html",
            // A failed event query answers in plain text.
            _ if body.starts_with(b"Command Error") => "txt",
            _ => "csv",
        };
        let file = format!("{name}.{ext}");
        std::fs::write(self.dir.join(&file), body).expect("writing a recorded response");
        file
    }

    /// Sends `op`, keeps the raw response, and decodes it.
    fn call<O: Operation>(&mut self, name: &str, op: &O, context: &Context) -> Option<O::Response>
    where
        O::Response: std::fmt::Debug,
    {
        let outcome = protocol::encode(op, context)
            .map_err(|e| format!("not sent: {e}"))
            .and_then(|request| {
                self.transport
                    .send(request)
                    .map_err(|e| format!("transport: {e}"))
            });
        let response = match outcome {
            Ok(response) => response,
            Err(err) => {
                println!("{name}: {err}");
                self.summary.push((name.to_owned(), err));
                return None;
            }
        };
        let file = self.save(name, &response.body);
        let (result, line) = match protocol::decode::<O>(&response) {
            Ok(value) => {
                let line = format!("HTTP {}, decoded: {value:?}", response.status);
                (Some(value), line)
            }
            Err(err) => (None, format!("HTTP {}, error: {err}", response.status)),
        };
        println!("{file}: {line}");
        self.summary.push((file, line));
        result
    }

    /// Posts raw JSON to the onboarding API.
    fn post_api(&mut self, name: &str, path: &str, provider: &ProviderCredentials, body: &str) {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        let result = agent
            .post(format!("{API_URL}{path}"))
            .header("accept", "application/json")
            .header("content-type", "application/json")
            .header("user-agent", protocol::DEFAULT_USER_AGENT)
            .header("ProviderID", provider.provider_id())
            .header("Provider_GateID", provider.gate_id())
            .header("Provider_GateKey", "test")
            .send(body);
        let line = match result {
            Ok(mut response) => {
                let status = response.status().as_u16();
                let content_type = response
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("none")
                    .to_owned();
                let text = response.body_mut().read_to_string().unwrap_or_default();
                let file = self.save(name, text.as_bytes());
                format!("{file}: HTTP {status}, {content_type}")
            }
            Err(err) => format!("{name}: transport: {err}"),
        };
        println!("{line}");
        self.summary.push((name.to_owned(), line));
    }
}

/// How many event rows a recording keeps from each end of a day's report.
const EVENT_ROWS: usize = 20;

/// The public sandbox's reports list other users' payments: keep a sample of the
/// event rows, and replace the names in settlement items, before committing them.
fn scrub(name: &str, body: &[u8]) -> Vec<u8> {
    if name.starts_with("events_") {
        let text = String::from_utf8_lossy(body);
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() <= 2 * EVENT_ROWS {
            return body.to_vec();
        }
        let kept: Vec<&str> = lines[..EVENT_ROWS]
            .iter()
            .chain(&lines[lines.len() - EVENT_ROWS..])
            .copied()
            .collect();
        return (kept.join("\n") + "\n").into_bytes();
    }
    if name == "settlement_details"
        && let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(body)
    {
        let items = json["ResponseData"]["SettlementItems"].as_array_mut();
        for (i, item) in items.into_iter().flatten().enumerate() {
            item["CustomerName"] = format!("Customer {}", i + 1).into();
            if !item["Company"].is_null() {
                item["Company"] = format!("Company {}", i + 1).into();
            }
        }
        return serde_json::to_vec(&json).expect("JSON writes");
    }
    body.to_vec()
}

fn customer() -> Customer {
    Customer::builder()
        .name("Test Customer")
        .address1("123 Test Street")
        .city("Testville")
        .state("NY")
        .zip("12345")
        .phone("555-555-0100")
        .email("test@example.com")
        .build()
}

fn payment(id: String, source: PaymentSource) -> PaymentRequest {
    PaymentRequest::builder()
        .provider_transaction_id(id)
        .amount(Decimal::new(123, 2))
        .direction(PaymentDirection::FromCustomer)
        .description("achq-rs sandbox test")
        .sec_code(SecCode::Web)
        .customer(customer())
        .customer_ip(std::net::Ipv4Addr::new(203, 0, 113, 9))
        .run_express_verify(true)
        .source(source)
        .build()
}

pub fn run(root: &Path) -> Result<()> {
    let provider = ProviderCredentials::sandbox();
    check_sandbox(&provider)?;
    let platform = Context::new(provider.clone());
    let merchant = platform.clone().merchant(MerchantCredentials::sandbox());

    let dir = root.join("achq/tests/data/recorded");
    std::fs::create_dir_all(&dir)?;
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        let keep = path.extension().is_some_and(|e| e == "md")
            || path.file_name().is_some_and(|n| n == FEE_PROFILE_FILE);
        if !keep {
            std::fs::remove_file(path)?;
        }
    }
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let mut r = Recorder {
        transport: UreqTransport::default(),
        dir,
        run: format!("{nanos:x}"),
        count: 0,
        summary: Vec::new(),
    };

    // Tokens and payments.
    let token_request = TokenRequest::builder()
        .provider_transaction_id(r.id())
        .account(BankAccount::checking(ROUTING, PASSING_ACCOUNT))
        .build();
    let token = r
        .call(
            "token_created",
            &tokens::CreateToken::new(&token_request),
            &merchant,
        )
        .map(|created| created.token);

    let bank = payment(
        r.id(),
        PaymentSource::bank(BankAccount::checking(ROUTING, PASSING_ACCOUNT)).tokenize(),
    );
    let payment_id: Option<PaymentId> = r
        .call(
            "payment_bank",
            &payments::CreatePayment::new(&bank),
            &merchant,
        )
        .map(|created| created.payment_id);

    let achq_token = token.unwrap_or_else(|| AchqToken::new("achq-sandbox-3ff6d-b64fd-5a8a5"));
    let by_token = payment(r.id(), PaymentSource::achq_token(&achq_token));
    let token_payment = r
        .call(
            "payment_achq_token",
            &payments::CreatePayment::new(&by_token),
            &merchant,
        )
        .map(|created| created.payment_id);
    let plaid = payment(
        r.id(),
        PaymentSource::token(PLAID_SANDBOX_TOKEN, TokenSource::Plaid),
    );
    r.call(
        "payment_plaid_token",
        &payments::CreatePayment::new(&plaid),
        &merchant,
    );

    let rejected = payment(
        r.id(),
        PaymentSource::bank(BankAccount::checking(ROUTING, EXPRESS_VERIFY_REJECT)),
    );
    r.call(
        "payment_express_verify_reject",
        &payments::CreatePayment::new(&rejected),
        &merchant,
    );

    if let Some(id) = &payment_id {
        let update = PaymentUpdate::builder()
            .amount(Decimal::new(234, 2))
            .build();
        r.call(
            "update",
            &payments::UpdatePayment::new(id.clone(), &update),
            &merchant,
        );
        r.call("hold", &payments::HoldPayment::new(id.clone()), &merchant);
        r.call(
            "cancel",
            &payments::CancelPayment::new(id.clone()),
            &merchant,
        );
        // Cancelling twice: the error for a payment that can no longer change.
        r.call(
            "cancel_again",
            &payments::CancelPayment::new(id.clone()),
            &merchant,
        );
    }
    if let Some(id) = &token_payment {
        // The payment has not cleared, so the refund should be refused.
        let refund = RefundRequest::builder()
            .provider_transaction_id(r.id())
            .build();
        r.call(
            "refund_not_cleared",
            &payments::RefundPayment::new(id.clone(), &refund),
            &merchant,
        );
    }
    r.call(
        "cancel_unknown",
        &payments::CancelPayment::new("1"),
        &merchant,
    );

    // Account insights.
    let status = AccountStatusRequest::builder()
        .provider_transaction_id(r.id())
        .routing_number(ROUTING)
        .account_number(PASSING_ACCOUNT)
        .build();
    r.call(
        "account_status",
        &insights::VerifyAccount::new(&status),
        &merchant,
    );
    for (name, account) in [
        ("account_owner", PASSING_ACCOUNT),
        ("account_owner_fails", OWNER_FAILS),
    ] {
        let owner = OwnerAuthRequest::builder()
            .provider_transaction_id(r.id())
            .routing_number(ROUTING)
            .account_number(account)
            .customer_name("Test Customer")
            .zip("12345")
            .phone("5555550100")
            .build();
        r.call(name, &insights::VerifyOwner::new(&owner), &merchant);
    }

    // Reports.
    let today = time::OffsetDateTime::now_utc().date();
    let month_ago = today - time::Duration::days(30);
    let query = SettlementQuery::between(month_ago, today);
    let settlements = r
        .call(
            "settlements",
            &settlements::ListSettlements::new(&query),
            &platform,
        )
        .unwrap_or_default();
    if let Some(id) = settlements.iter().find_map(|s| s.settlement_id.clone()) {
        let mut details = SettlementDetailsQuery::new(id);
        details.page_size = Some(5);
        r.call(
            "settlement_details",
            &settlements::SettlementDetails::new(&details),
            &platform,
        );
    }
    for (name, date) in [
        ("events_today", today),
        ("events_yesterday", today - time::Duration::days(1)),
    ] {
        r.call(
            name,
            &events::TrackEvents::new(&EventQuery::on(date)),
            &platform,
        );
    }

    // Errors, with a wrong gate key.
    let wrong = Context::new(ProviderCredentials::new("99", "test", "wrong-key"));
    r.call(
        "events_bad_credentials",
        &events::TrackEvents::new(&EventQuery::on(today)),
        &wrong,
    );
    let wrong_merchant = wrong.merchant(MerchantCredentials::new("2001", "test", "wrong-key"));
    r.call(
        "cancel_bad_credentials",
        &payments::CancelPayment::new("1"),
        &wrong_merchant,
    );

    // The onboarding API: an invalid application, and one fee profile.
    r.post_api("onboarding_invalid", "/onboarding", &provider, "{}");
    // One fee profile only: an earlier run's is kept rather than adding another.
    if r.dir.join(FEE_PROFILE_FILE).exists() {
        println!("{FEE_PROFILE_FILE}: kept from an earlier run");
        r.summary.push((
            FEE_PROFILE_FILE.to_owned(),
            "kept from an earlier run".to_owned(),
        ));
    } else {
        r.post_api(
            "fee_profile_created",
            "/onboarding/fees",
            &provider,
            // The API requires every fee, though its schema marks them optional.
            r#"{"feeName": "achq-rs sandbox test", "monthlyFee": 0, "transactionFee": 0.01,
            "returnFee": 0, "disputeFee": 0, "percentFee": 0, "percentCap": 0,
            "bankStatusFee": 0, "bankIdFee": 0, "setupFee": 0}"#,
        );
    }

    let mut readme = String::from(
        "# Recorded sandbox responses\n\n\
         Written by `cargo xtask record` from ACHQ's public sandbox (ProviderID 99,\n\
         merchant 2001), with made-up data. Rerun it to refresh them.\n\n",
    );
    writeln!(readme, "Recorded on {}.\n", crate::drift::today())?;
    readme.push_str("| File | Outcome |\n|---|---|\n");
    for (file, line) in &r.summary {
        let mut line = line.replace('|', "\\|");
        if line.chars().count() > 160 {
            line = line.chars().take(160).collect::<String>() + "…";
        }
        writeln!(readme, "| `{file}` | {line} |")?;
    }
    std::fs::write(r.dir.join("README.md"), readme)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubs_other_users_data() {
        let details = br#"{"ResponseData": {"SettlementItems": [
            {"CustomerName": "Real Person", "Company": null},
            {"CustomerName": "Other Person", "Company": "Real Co"}]}}"#;
        let scrubbed = String::from_utf8(scrub("settlement_details", details)).unwrap();
        assert!(
            !scrubbed.contains("Real") && !scrubbed.contains("Other"),
            "{scrubbed}"
        );
        assert!(scrubbed.contains("Customer 2") && scrubbed.contains("Company 2"));

        let events: String = (0..50).map(|i| format!("{i},,2001,Created\n")).collect();
        let kept = String::from_utf8(scrub("events_today", events.as_bytes())).unwrap();
        assert_eq!(kept.lines().count(), 2 * EVENT_ROWS);
        assert!(kept.starts_with("0,") && kept.ends_with("49,,2001,Created\n"));
        assert_eq!(scrub("cancel", b"{}"), b"{}");
    }

    #[test]
    fn records_only_from_the_public_sandbox() {
        assert!(check_sandbox(&ProviderCredentials::sandbox()).is_ok());
        let real = ProviderCredentials::new("1234", "gate", "key");
        assert_eq!(
            check_sandbox(&real).unwrap_err().to_string(),
            "refusing to record with ProviderID 1234: only the public sandbox (99) is used"
        );
    }
}
