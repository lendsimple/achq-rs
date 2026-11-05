# achq

[![CI](https://github.com/lendsimple/achq-rs/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/lendsimple/achq-rs/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/achq.svg)](https://crates.io/crates/achq)
[![docs.rs](https://docs.rs/achq/badge.svg)](https://docs.rs/achq)

A typed Rust client for the [ACHQ](https://www.achq.com) bank payments API (formerly
SpeedChex), with async and blocking clients, for native targets and WebAssembly.

- Every gateway command: tokens, payments (create, update, cancel, hold, refund),
  bank account status and ownership checks, settlement reports and event tracking;
  and the onboarding API, for enrolling merchants.
- Requests are checked before they are sent: lengths, routing number checksums,
  amounts with at most two decimal places, and the API's conditional rules (a
  company for business accounts, an IP address for WEB payments, one bank account
  or token per payment).
- Gate keys, account numbers, tokens, SSNs and tax IDs never appear in `Debug`
  output or logs.
- The clients are thin layers over a protocol layer without I/O, so any HTTP client
  can send the requests.

```toml
[dependencies]
achq = "0.1"
```

| Feature | Default | |
|---|---|---|
| `reqwest` | yes | `ReqwestTransport`: the async `Client` sends with reqwest, on tokio (or `fetch` in WebAssembly) |
| `ureq` | | `UreqTransport`: the `blocking::Client` sends with ureq, with no async runtime |
| `rustls` | yes | TLS with rustls |
| `native-tls` | | TLS with the platform's library |
| `onboarding` | yes | The onboarding API: merchant applications and fee profiles |

The clients themselves need no feature: you can give them any HTTP client (see
[Other HTTP clients](#other-http-clients)).

The minimum supported Rust version is 1.88.

## Charging a bank account

```rust,no_run
use achq::bank::BankAccount;
use achq::payments::{Customer, PaymentDirection, PaymentRequest, PaymentSource, SecCode};
use achq::{Client, MerchantCredentials, ProviderCredentials, dec};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Your platform's credentials, and one of its merchants'.
    let client = Client::new(ProviderCredentials::new("1234", "gate-id", "gate-key"));
    let merchant = client.merchant(MerchantCredentials::new("5678", "gate-id", "gate-key"));

    let payment = PaymentRequest::builder()
        .provider_transaction_id("order-1234")
        .amount(dec!(42.00))
        .direction(PaymentDirection::FromCustomer)
        .sec_code(SecCode::Web)
        .customer(
            Customer::builder()
                .name("Bob Loblaw")
                .address1("123 Street")
                .city("Town")
                .state("NY")
                .zip("12345")
                .phone("555-867-5309")
                .email("bob@example.com")
                .build(),
        )
        .customer_ip(std::net::Ipv4Addr::new(203, 0, 113, 9))
        .run_express_verify(true)
        // Keep a token for the account, to charge it again without its numbers.
        .source(PaymentSource::bank(BankAccount::checking("123123123", "12345678")).tokenize())
        .build();

    let created = merchant.payments().create(&payment).await?;
    println!("payment {}, token {:?}", created.payment_id, created.token);
    Ok(())
}
```

The `Client` acts for your platform; `client.merchant(...)` gives a `Merchant`,
cheap and shareable, that acts for one of your merchants. Commands are grouped as in
ACHQ's documentation:

- on a `Merchant`: `payments()`, `tokens()` and `insights()` (account checks);
- on the `Client`: `settlements()`, `events()` and `onboarding()`.

So a command on a merchant's payments cannot be sent without the merchant's
credentials. Each method takes the payment's ID, if the command has one, then the
request.

Requests have builders, and shortcuts for the usual cases:
`BankAccount::checking(..)` and `::savings(..)` (with `.business()`),
`PaymentSource::bank(..)`, `::token(..)` and `::achq_token(..)`,
`RefundRequest::full()` and `::partial(..)`, `EventQuery::on(date)`,
`SettlementQuery::between(from, to)`.

### The blocking client

With the `ureq` feature:

```rust,no_run
use achq::blocking::Client;
use achq::{MerchantCredentials, ProviderCredentials};

fn main() -> Result<(), achq::Error> {
    let client = Client::new(ProviderCredentials::new("1234", "gate-id", "gate-key"));
    let merchant = client.merchant(MerchantCredentials::new("5678", "gate-id", "gate-key"));
    let receipt = merchant.payments().cancel("14477175")?;
    println!("cancelled; ACHQ's reference for the cancellation is {}", receipt.command_id);
    Ok(())
}
```

Do not call the blocking client from async code: it blocks the thread.

## Payment IDs

ACHQ gives every command it accepts an ID of its own (`TransAct_ReferenceID`). The
one returned when a payment is created, a `PaymentId`, identifies the payment from
then on: keep it. Updates, cancellations and holds return a `CommandReceipt`, whose
`CommandId` identifies only that command; it does not replace the payment's ID. A
refund is a new payment, with a `PaymentId` of its own.

Set `provider_transaction_id` on every payment: ACHQ refuses a second payment with
the same one (`ApiError::is_duplicate`), which makes retrying after a timeout safe,
and reports and events carry it.

A payment can be changed until ACHQ sends it to the Federal Reserve (while it is
`Scheduled` or on hold), and refunded once it has `Cleared`.

## Tracking payment status

ACH payments settle over days, and their outcome (cleared, returned, charged back)
arrives as events. A query for a day returns the payments that had an event that
day:

```rust,no_run
use achq::events::EventQuery;

# async fn run(client: achq::Client) -> Result<(), achq::Error> {
let yesterday = time::OffsetDateTime::now_utc().date().previous_day().unwrap();
let batch = client.events().query(&EventQuery::on(yesterday)).await?;
for event in batch.events {
    if event.status.is_returned() {
        println!(
            "{} was returned: {:?} {:?}",
            event.payment_id, event.return_code, event.return_explanation
        );
    }
}
# Ok(())
# }
```

ACHQ recommends querying at 9 AM, 1 PM, 7 PM and midnight Eastern, and at least
once after midnight Central for the whole previous day. Returns can arrive up to 60
days after a payment. `PaymentStatus` tells what a payment allows next:
`can_modify()`, `can_refund()`.

## Verifying accounts

NACHA requires WEB debits to validate an account the first time it is used. ACHQ
checks an account in real time, and can confirm who owns it:

```rust,no_run
use achq::insights::{AccountStatusRequest, OwnerAuthRequest};

# async fn run(merchant: achq::Merchant<achq::transport::ReqwestTransport>) -> Result<(), achq::Error> {
let status = AccountStatusRequest::builder()
    .routing_number("123123123")
    .account_number("12345678")
    .build();
let account = merchant.insights().account_status(&status).await?;
println!("{:?}: {:?}", account.express_verify.status, account.express_verify.description);

let owner = OwnerAuthRequest::builder()
    .routing_number("123123123")
    .account_number("12345678")
    .customer_name("Bob Loblaw")
    .build();
let checked = merchant.insights().account_owner(&owner).await?;
println!("owner confirmed: {}", checked.is_owner_confirmed());
# Ok(())
# }
```

A failed check is a result, not an error.

## Settlements

```rust,no_run
use achq::settlements::{SettlementDetailsQuery, SettlementQuery};

# async fn run(client: achq::Client) -> Result<(), achq::Error> {
let today = time::OffsetDateTime::now_utc().date();
let query = SettlementQuery::between(today - time::Duration::days(30), today);
for settlement in client.settlements().list(&query).await? {
    let Some(id) = settlement.settlement_id else { continue };
    let mut page_query = SettlementDetailsQuery::new(id);
    loop {
        let page = client.settlements().details(&page_query).await?;
        for item in &page.items {
            println!("{:?} {:?}", item.payment_id, item.amount);
        }
        match page.next_page(&page_query) {
            Some(next) => page_query = next,
            None => break,
        }
    }
}
# Ok(())
# }
```

## Onboarding

With the `onboarding` feature, a platform enrolls its merchants: an application
describes the business, its owner, the payment services it needs, its bank accounts
and its fees.

```rust,no_run
use achq::onboarding::FeeSchedule;
use achq::dec;

# async fn run(client: achq::Client) -> Result<(), achq::Error> {
let fees = FeeSchedule::builder()
    .name("Standard")
    .transaction_fee(dec!(0.25))
    .return_fee(dec!(5))
    .build();
let profile = client.onboarding().create_fee_profile(&fees).await?;
println!("fee profile {}", profile.fee_profile_id);
# Ok(())
# }
```

A `MerchantApplication` is built from a `BusinessEntity`, a `PrincipalOwner`, a
`PaymentProfile` (charge and payout `Limits`, which enable each service, and the SEC
codes the merchant uses), `BankAccounts`, and the fees: a `FeeChoice::Profile` ID or
an inline `FeeSchedule`. Every fee is sent, as zero unless set, because the API
requires them all. The onboarding API answers errors with an `ErrorResponse`, read as
`Error::Onboarding`.

## Errors

- `Error::Api`: the gateway refused the command or failed (`CommandStatus` `Declined`
  or `Error`). ACHQ answers these with HTTP 200, so the client reads the envelope.
  `ApiError` gives the response code, the description, and the field at fault, and
  tells the cases apart:
  - `is_credentials()`: wrong gate keys, or a source IP ACHQ does not allow;
  - `is_duplicate()`: a payment with the same `Provider_TransactionID` already exists;
  - `is_validation()`: a field is missing or invalid;
  - `is_account_rejected()`: the bank account failed verification (*Express Verify*),
    with `express_verify()` saying why;
  - `is_queued()`: the payment was just created and cannot be changed yet. In the
    sandbox, payments created without `run_express_verify(true)` stayed queued for
    more than 30 seconds;
  - `is_retryable()`: an internal failure. Retry only with the same
    `Provider_TransactionID`.
- `Error::Onboarding`: the onboarding API's error response.
- `Error::Validation`: the request breaks a rule, so it was not sent. Every violation
  is listed, with the path of its field.
- `Error::Status`, `Error::UnexpectedContentType`, `Error::Decode`: the server answered
  with an error status or something that is not a response, such as an error page.
- `Error::Transport`: no response was received; `is_timeout()` tells a timeout.

The client never retries on its own: a payment command is not safe to repeat blindly.

## Testing with the sandbox

ACHQ's public sandbox accepts payments that are stored and reported but never
processed:

```rust
use achq::{MerchantCredentials, ProviderCredentials};

let provider = ProviderCredentials::sandbox(); // ProviderID 99
let merchant = MerchantCredentials::sandbox(); // MerchantID 2001
# let _ = (provider, merchant);
```

Use routing number `123123123` with these account numbers:

| Account | Outcome |
|---|---|
| `111111111` | Returned for insufficient funds the next business day |
| `222222222` | Returned for another reason the next business day |
| `444444444` | Cleared, then charged back the next business day |
| `987654321` | Fails Express Verify and is rejected at once |
| `999999999` | Fails the account owner check |
| any other | Passes every check |

The sandbox is shared: anyone using it can see what you send. Send only made-up
names, addresses and accounts.

To try production credentials without moving money, set
`Client::builder(...).test_mode(true)`: ACHQ answers as it would, but records
nothing.

## Other HTTP clients

The clients send requests through a `Transport` (async) or `blocking::Transport`.
Implement it to use another HTTP client, and pass it to `Client::with_transport` or
`ClientBuilder::new`:

```rust
use achq::transport::{HttpRequest, HttpResponse, Transport};
use achq::{Client, ProviderCredentials, TransportError};

struct MyTransport;

impl Transport for MyTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
        // Send `request.method` to `request.url`, with `request.headers` and
        // `request.body`, within `request.timeout`.
        # let _ = request;
        # Ok(HttpResponse::new(200, Vec::new(), Vec::new()))
    }
}

let client = Client::with_transport(ProviderCredentials::sandbox(), MyTransport);
# let _ = client;
```

A `ReqwestTransport` can also wrap your own `reqwest::Client` (`ReqwestTransport::from`),
to share its connection pool or proxy settings, and a `UreqTransport` your own
`ureq::Agent`.

Without any client, `achq::protocol::encode` turns a command into an HTTP request,
and `protocol::decode` reads the response.

## WebAssembly

The crate builds for `wasm32-unknown-unknown`, where `ReqwestTransport` uses the
JavaScript `fetch` API, and for WASI (`wasm32-wasip2`) with a transport of your own.
On WebAssembly the transport traits do not require `Send`.

The gate keys can move your merchants' money: keep them on a server. The WebAssembly
builds are for servers, edge workers and WASI hosts, not for browsers, where ACHQ's
IP allow-listing would refuse the requests anyway.

## Logging

The clients log with [`tracing`](https://docs.rs/tracing), in an `achq` span with the
`operation` (such as `payments.create`) and the gateway `command`. Each request and
response is logged at DEBUG, and their bodies at TRACE, with gate keys, account
numbers, tokens, SSNs, dates of birth and tax IDs replaced by `[REDACTED]`.

## Examples

`achq/examples/` has runnable examples. They use the public sandbox unless
`ACHQ_PROVIDER_ID`, `ACHQ_PROVIDER_GATE_ID`, `ACHQ_PROVIDER_GATE_KEY` (and
`ACHQ_MERCHANT_ID`, `ACHQ_MERCHANT_GATE_ID`, `ACHQ_MERCHANT_GATE_KEY`) are set:

```sh
cargo run -p achq --example tokenize_and_charge
cargo run -p achq --example payment_lifecycle
cargo run -p achq --example daily_events
cargo run -p achq --example settlements
cargo run -p achq --example charge_blocking --features ureq
cargo run -p achq --example create_fee_profile   # prints the request; --send sends it
```

## Development

ACHQ publishes no single API specification: each page of its API reference embeds an
OpenAPI fragment for one command. `schema/` keeps them, and `cargo xtask` maintains
them:

```sh
cargo xtask drift          # what changed in ACHQ's documentation
cargo xtask fetch          # download the changed pages into schema/
cargo xtask codegen        # regenerate the onboarding industry table
cargo xtask conformance    # check the request types against the fragments
cargo xtask record         # record responses from the public sandbox as fixtures
```

The fragments and ACHQ's guides disagree in places; `schema/overrides.toml` lists
every difference the request types keep, each with its reason. `implementation.md`
records the design, what was learned from the sandbox, and what is still open.

To run the tests against the public sandbox:

```sh
ACHQ_SANDBOX=1 cargo test -p achq --all-features --test sandbox -- --ignored
```

## License

MIT
