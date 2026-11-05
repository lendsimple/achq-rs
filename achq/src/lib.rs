//! A typed client for the [ACHQ](https://www.achq.com) bank payments API
//! (formerly SpeedChex).
//!
//! **Work in progress.** This release has the protocol, without I/O: build a
//! command, [`protocol::encode`] it as an HTTP request, send that with any HTTP
//! client, and [`protocol::decode`] the answer. The clients that send requests
//! come in a later release.
//!
//! ```
//! use achq::bank::BankAccount;
//! use achq::operations::payments::CreatePayment;
//! use achq::payments::{Customer, PaymentDirection, PaymentRequest, PaymentSource, SecCode};
//! use achq::protocol::{self, Context};
//! use achq::{MerchantCredentials, ProviderCredentials, dec};
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
//!
//! let context = Context::new(ProviderCredentials::sandbox()).merchant(MerchantCredentials::sandbox());
//! let request = protocol::encode(&CreatePayment::new(&payment), &context)?;
//! // Send `request`, then `protocol::decode::<CreatePayment>(&response)?.payment_id`.
//! # let _ = request;
//! # Ok::<(), achq::Error>(())
//! ```
//!
//! - [`operations`]: a type for every gateway command, by resource.
//! - [`payments`], [`tokens`], [`insights`], [`settlements`] and [`events`]: the
//!   commands' requests and results; [`bank`]: bank accounts.
//! - [`protocol`]: [`encode`](protocol::encode) and [`decode`](protocol::decode);
//!   [`envelope`]: the gateway's response envelope and codes;
//!   [`transport`]: the HTTP request and response types.
//! - [`types`]: exact money [`Amount`](types::Amount)s, the gateway's
//!   [`LocalDateTime`](types::LocalDateTime)s, and typed IDs, such as
//!   [`PaymentId`](types::PaymentId).
//! - [`form`]: the gateway's form-encoded request bodies, with sensitive values
//!   hidden from logs.
//! - [`validate`]: checks of the API's constraints, collecting every violation
//!   with the path of the field.
//! - `onboarding` (feature `onboarding`, on by default): the industry table a
//!   merchant application classifies the business with.
//!
//! The gateway keys grant access to merchants' bank payments: keep them on a
//! server. The WebAssembly builds are for servers, edge workers and WASI hosts,
//! not for browsers.

// docs.rs builds with `--cfg docsrs` and labels items that need a feature.
#![cfg_attr(docsrs, feature(doc_cfg))]

// Lets derived code refer to `::achq::...` inside this crate too.
extern crate self as achq;

pub mod bank;
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

pub use credentials::{API_URL, Endpoints, GATEWAY_URL, MerchantCredentials, ProviderCredentials};
pub use error::{ApiError, Error, TransportError};
pub use operation::{Operation, Scope};
pub use rust_decimal::{Decimal, dec};
pub use secrecy::SecretString;

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
