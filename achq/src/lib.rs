//! A typed client for the [ACHQ](https://www.achq.com) bank payments API
//! (formerly SpeedChex), for native targets and WebAssembly.
//!
//! ```no_run
//! # #[cfg(feature = "reqwest")]
//! # async fn run() -> Result<(), achq::Error> {
//! use achq::bank::BankAccount;
//! use achq::payments::{Customer, PaymentDirection, PaymentRequest, PaymentSource, SecCode};
//! use achq::{Client, MerchantCredentials, ProviderCredentials, dec};
//!
//! let client = Client::new(ProviderCredentials::sandbox());
//! let merchant = client.merchant(MerchantCredentials::sandbox());
//!
//! let payment = PaymentRequest::builder()
//!     .provider_transaction_id("order-1234")
//!     .amount(dec!(42.00))
//!     .direction(PaymentDirection::FromCustomer)
//!     .sec_code(SecCode::Ppd)
//!     .customer(
//!         Customer::builder()
//!             .name("Bob Loblaw")
//!             .address1("123 Street")
//!             .city("Town")
//!             .state("NY")
//!             .zip("12345")
//!             .phone("555-867-5309")
//!             .email("bob@example.com")
//!             .build(),
//!     )
//!     .source(PaymentSource::bank(BankAccount::checking("123123123", "12345678")).tokenize())
//!     .build();
//! let created = merchant.payments().create(&payment).await?;
//! println!("payment {}, token {:?}", created.payment_id, created.token);
//! # Ok(())
//! # }
//! ```
//!
//! - [`Client`]: the async client, sending requests with reqwest (feature
//!   `reqwest`, on by default) or any [`Transport`](transport::Transport). It runs
//!   the platform's reports; [`Client::merchant`] gives a [`Merchant`] for the
//!   commands on a merchant's payments.
//! - [`blocking::Client`]: the blocking client, with ureq (feature `ureq`) or any
//!   [`blocking::Transport`].
//! - [`operations`]: a type for every gateway command, by resource.
//! - [`payments`], [`tokens`], [`insights`], [`settlements`] and [`events`]: the
//!   commands' requests and results; [`bank`]: bank accounts.
//! - [`protocol`]: [`encode`](protocol::encode) a command as an HTTP request,
//!   send it with any HTTP client, and [`decode`](protocol::decode) the answer;
//!   [`envelope`]: the gateway's response envelope and codes;
//!   [`transport`]: the HTTP request and response types.
//! - [`types`]: exact money [`Amount`](types::Amount)s, the gateway's
//!   [`LocalDateTime`](types::LocalDateTime)s, and typed IDs, such as
//!   [`PaymentId`](types::PaymentId).
//! - [`form`]: the gateway's form-encoded request bodies, with sensitive values
//!   hidden from logs.
//! - [`validate`]: checks of the API's constraints, collecting every violation
//!   with the path of the field.
//! - `onboarding` (feature `onboarding`, on by default): merchant applications
//!   and fee profiles for the onboarding API, and its industry table.
//!
//! # Features
//!
//! - `reqwest` (default): `transport::ReqwestTransport`, the async client's
//!   default transport, on tokio or (in WebAssembly) `fetch`.
//! - `ureq`: `blocking::UreqTransport`, the blocking client's default transport.
//! - `rustls` (default) or `native-tls`: the TLS library of those transports.
//! - `onboarding` (default): the onboarding API.
//!
//! Without a transport feature, the clients take any
//! [`Transport`](transport::Transport) or [`blocking::Transport`].
//!
//! # Security
//!
//! - The gate keys can move your merchants' money: keep them on a server. The
//!   WebAssembly builds are for servers, edge workers and WASI hosts, not for
//!   browsers, where ACHQ's IP allow-listing would refuse the requests anyway.
//! - Gate keys, account numbers, tokens, SSNs, dates of birth and tax IDs are kept
//!   out of `Debug` output and logs.
//! - The public sandbox ([`ProviderCredentials::sandbox`]) is shared: anyone using
//!   it can see what you send. Send it only made-up data.

// docs.rs builds with `--cfg docsrs` and labels items that need a feature.
#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs)]

// Lets derived code refer to `::achq::...` inside this crate too.
extern crate self as achq;

pub mod bank;
pub mod blocking;
pub mod client;
mod config;
mod credentials;
pub mod envelope;
mod error;
pub mod events;
pub mod form;
pub mod insights;
mod macros;
#[cfg(feature = "onboarding")]
#[cfg_attr(docsrs, doc(cfg(feature = "onboarding")))]
pub mod onboarding;
mod operation;
pub mod operations;
pub mod payments;
pub mod protocol;
pub mod settlements;
pub mod tokens;
pub mod transport;
pub mod types;
pub mod validate;

pub use client::{Client, ClientBuilder, Merchant};
pub use config::DEFAULT_TIMEOUT;
pub use credentials::{API_URL, Endpoints, GATEWAY_URL, MerchantCredentials, ProviderCredentials};
pub use error::{ApiError, Error, OnboardingError, TransportError};
pub use operation::{Operation, Scope};
pub use rust_decimal::{Decimal, dec};
pub use secrecy::SecretString;

/// Compiles the README's examples as doc tests.
#[cfg(all(doctest, feature = "reqwest", feature = "ureq", feature = "onboarding"))]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

/// Implements `Validate`, a redacting `Debug`, and (with `#[achq(form)]`)
/// `ToForm`.
#[doc(hidden)]
pub use achq_derive::Model;

/// Items used by derived code. Not part of the public API.
#[doc(hidden)]
pub mod __private {
    use std::fmt;

    /// Stands in for a sensitive value in `Debug` output.
    pub struct Redacted(bool);

    impl Redacted {
        /// `present` is whether there is a value to hide.
        pub fn new(present: bool) -> Self {
            Self(present)
        }
    }

    impl fmt::Debug for Redacted {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(if self.0 { "[REDACTED]" } else { "None" })
        }
    }
}
