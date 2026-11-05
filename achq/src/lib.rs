//! A typed client for the [ACHQ](https://www.achq.com) bank payments API
//! (formerly SpeedChex).
//!
//! **Work in progress.** This release has the foundation the client is built on:
//!
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

pub mod form;
mod macros;
#[cfg(feature = "onboarding")]
#[cfg_attr(docsrs, doc(cfg(feature = "onboarding")))]
pub mod onboarding;
pub mod types;
pub mod validate;

pub use rust_decimal::{Decimal, dec};

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
