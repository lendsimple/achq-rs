//! What every gateway command has in common.

use std::fmt::Display;

use crate::error::Error;
use crate::form::{FieldMeta, FormWriter};
use crate::transport::HttpResponse;
use crate::validate::{Path, Validate, ValidationError, Violation};

/// One gateway command: its name, its fields, and the result it answers with.
///
/// Every command has a type in [`operations`](crate::operations), such as
/// [`payments::CreatePayment`](crate::operations::payments::CreatePayment).
/// [`protocol::encode`](crate::protocol::encode) turns one into an HTTP request,
/// and [`protocol::decode`](crate::protocol::decode) reads the answer.
pub trait Operation {
    /// What a successful command returns.
    type Response;

    /// The operation's name in the client, such as `payments.create`.
    const NAME: &'static str;

    /// The gateway command, such as `ECheck.ProcessPayment`.
    const COMMAND: &'static str;

    /// Whose credentials the command needs.
    const SCOPE: Scope;

    /// Fields every request of the command sends, such as `OwnerAuth=On`.
    #[doc(hidden)]
    const EXTRA: &'static [(&'static str, &'static str)];

    /// Checks the command's fields against the API's constraints.
    fn validate(&self) -> Result<(), ValidationError>;

    /// Writes the command's own fields.
    #[doc(hidden)]
    fn write_form(&self, form: &mut FormWriter);

    /// Describes the command's own fields, for checking them against the API's
    /// documentation.
    #[doc(hidden)]
    fn form_fields(out: &mut Vec<FieldMeta>);

    /// Reads a successful (2xx) response.
    #[doc(hidden)]
    fn decode(response: &HttpResponse) -> Result<Self::Response, Error>;
}

/// Whose credentials a command needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Scope {
    /// The platform's and a merchant's: commands on the merchant's payments.
    Merchant,
    /// The platform's only: reports across the platform's merchants.
    Platform,
}

/// A command's result, read from the gateway's response.
#[doc(hidden)]
pub trait Decode: Sized {
    fn decode(response: &HttpResponse) -> Result<Self, Error>;
}

/// Checks that an ID field is not blank.
#[doc(hidden)]
pub fn check_id(value: &dyn Display, name: &'static str, out: &mut Vec<Violation>) {
    if value.to_string().trim().is_empty() {
        out.push(Violation {
            path: name.to_owned(),
            message: "must not be empty".to_owned(),
        });
    }
}

/// Checks a body, collecting its violations.
#[doc(hidden)]
pub fn check_body<B: Validate + ?Sized>(body: &B, out: &mut Vec<Violation>) {
    body.validate_into(&mut Path::default(), out);
}

#[doc(hidden)]
pub fn finish(violations: Vec<Violation>) -> Result<(), ValidationError> {
    match ValidationError::from_violations(violations) {
        Some(err) => Err(err),
        None => Ok(()),
    }
}
