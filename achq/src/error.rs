//! Errors returned by the client.

use std::fmt;

use crate::envelope::{CommandStatus, Envelope, ExpressVerify, ResponseCode};
use crate::types::CommandId;
use crate::validate::ValidationError;

/// How much of a response body an error keeps, for diagnosis.
const MAX_BODY: usize = 4096;

/// Why a request failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The gateway refused the command or failed to carry it out: its
    /// `CommandStatus` was `Declined` or `Error`.
    #[error(transparent)]
    Api(Box<ApiError>),

    /// The request breaks the API's constraints, so it was not sent.
    #[error("invalid request: {0}")]
    Validation(#[from] ValidationError),

    /// A successful response could not be read as the expected type.
    #[error("could not read the response (HTTP {status}): {source}")]
    Decode {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
        status: u16,
        /// The start of the response body.
        body: String,
    },

    /// The response was not in the format the command answers with, such as an
    /// HTML error page.
    #[error("expected a {expected} response, got {content_type} (HTTP {status})")]
    UnexpectedContentType {
        status: u16,
        /// What the command answers with: JSON, or CSV for event tracking.
        expected: &'static str,
        content_type: String,
        /// The start of the response body.
        body: String,
    },

    /// The server answered with an error status.
    #[error("HTTP status {status}")]
    Status {
        status: u16,
        /// The start of the response body, which may help diagnose the failure.
        body: String,
    },

    /// The request could not be sent or its response not received: a connection,
    /// TLS or timeout failure.
    #[error("HTTP request failed: {0}")]
    Transport(#[from] TransportError),
}

impl Error {
    /// The gateway's error, if it reported one.
    pub fn as_api(&self) -> Option<&ApiError> {
        match self {
            Error::Api(err) => Some(err),
            _ => None,
        }
    }

    /// The HTTP status of the response, if one was received.
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Api(err) => Some(err.status),
            Error::Decode { status, .. }
            | Error::UnexpectedContentType { status, .. }
            | Error::Status { status, .. } => Some(*status),
            Error::Validation(_) | Error::Transport(_) => None,
        }
    }

    pub(crate) fn decode(
        source: impl Into<Box<dyn std::error::Error + Send + Sync>>,
        status: u16,
        body: &[u8],
    ) -> Self {
        Error::Decode {
            source: source.into(),
            status,
            body: body_excerpt(body),
        }
    }
}

impl From<ApiError> for Error {
    fn from(err: ApiError) -> Self {
        Error::Api(Box::new(err))
    }
}

/// The start of a body, as text, for an error.
pub(crate) fn body_excerpt(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(&body[..body.len().min(MAX_BODY)]).into_owned();
    if body.len() > MAX_BODY {
        format!("{text}…")
    } else {
        text
    }
}

/// A failure of the HTTP transport underneath a client.
#[derive(Debug, thiserror::Error)]
#[error("{source}")]
pub struct TransportError {
    source: Box<dyn std::error::Error + Send + Sync>,
    timeout: bool,
}

impl TransportError {
    /// A failure to send a request or receive its response.
    pub fn new(source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self {
            source: source.into(),
            timeout: false,
        }
    }

    /// A request that timed out.
    pub fn timeout(source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self {
            source: source.into(),
            timeout: true,
        }
    }

    /// Whether the request timed out.
    pub fn is_timeout(&self) -> bool {
        self.timeout
    }
}

/// A command the gateway refused or failed: its envelope, whose `CommandStatus`
/// was not `Approved`.
///
/// Displays as `Declined [100] Invalid Gateway Credentials (Provider_Credentials)`:
/// the status, the response code, the description, and the error information,
/// which often names the offending field.
///
/// ```no_run
/// # fn handle(err: achq::Error) {
/// if let Some(api) = err.as_api() {
///     if api.is_duplicate() {
///         // Already processed: look the payment up rather than resending it.
///     }
///     eprintln!("{api}; quote {:?} to ACHQ support", api.reference_id());
/// }
/// # }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    status: u16,
    envelope: Envelope<serde_json::Value>,
}

impl ApiError {
    pub(crate) fn new(status: u16, envelope: Envelope<serde_json::Value>) -> Self {
        Self { status, envelope }
    }

    /// The HTTP status, usually 200: the gateway reports failures in the body.
    pub fn status(&self) -> u16 {
        self.status
    }

    /// `Declined` or `Error`, or `None` if the gateway sent no status.
    pub fn command_status(&self) -> Option<&CommandStatus> {
        self.envelope.command_status.as_ref()
    }

    /// The response code, such as [`ResponseCode::REQUIRED_FIELD_MISSING`].
    pub fn code(&self) -> Option<ResponseCode> {
        self.envelope.response_code
    }

    /// The gateway's description of the code; empty if there is none.
    pub fn description(&self) -> &str {
        self.envelope.description.as_deref().unwrap_or("")
    }

    /// What caused the failure, often the name of a field (`Provider_Credentials`).
    pub fn field(&self) -> Option<&str> {
        self.envelope
            .error_information
            .as_deref()
            .filter(|info| !info.trim().is_empty())
    }

    /// The bank account verification's result, when a payment was rejected for
    /// its account.
    pub fn express_verify(&self) -> Option<&ExpressVerify> {
        self.envelope
            .express_verify
            .as_ref()
            .filter(|verify| !verify.is_empty())
    }

    /// The gateway's ID of the failed command, to quote to ACHQ support.
    pub fn reference_id(&self) -> Option<CommandId> {
        self.envelope.reference().map(CommandId::from)
    }

    /// The whole envelope.
    pub fn envelope(&self) -> &Envelope<serde_json::Value> {
        &self.envelope
    }

    fn code_in(&self, codes: &[ResponseCode]) -> bool {
        self.code().is_some_and(|code| codes.contains(&code))
    }

    /// The gateway rejected the credentials (100), or the request's source IP
    /// address (109) is not on the platform's allow list.
    pub fn is_credentials(&self) -> bool {
        self.code_in(&[
            ResponseCode::INVALID_CREDENTIALS,
            ResponseCode::INVALID_SOURCE_IP,
        ])
    }

    /// The command duplicates one already processed (102), or reuses an ID that
    /// must be unique (107). Look the earlier command up rather than resending.
    pub fn is_duplicate(&self) -> bool {
        self.code_in(&[ResponseCode::DUPLICATE_COMMAND, ResponseCode::NON_UNIQUE_ID])
    }

    /// The command or one of its fields is invalid: fix the request rather than
    /// retrying it.
    pub fn is_validation(&self) -> bool {
        self.code_in(&[
            ResponseCode::INVALID_COMMAND,
            ResponseCode::INVALID_VALUE,
            ResponseCode::REQUIRED_FIELD_MISSING,
            ResponseCode::INVALID_FIELD_VALUE,
            ResponseCode::FIELD_TOO_LONG,
            ResponseCode::INVALID_ROUTING_NUMBER,
            ResponseCode::INVALID_ACCOUNT_NUMBER,
            ResponseCode::INVALID_ACHQ_TOKEN,
            ResponseCode::INVALID_ACCOUNT_TYPE,
        ])
    }

    /// An internal or communication failure (600, 601, 603) that a later retry may
    /// not repeat.
    ///
    /// The command may still have been processed: retry only with the same
    /// `Provider_TransactionID`, so that the gateway can refuse a duplicate.
    pub fn is_retryable(&self) -> bool {
        self.code_in(&[
            ResponseCode::INTERNAL_GATEWAY_ERROR,
            ResponseCode::INTERNAL_PROCESSOR_ERROR,
            ResponseCode::PROCESSOR_COMMUNICATION_ERROR,
        ])
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.command_status() {
            Some(status) => write!(f, "{status}")?,
            None => f.write_str("Failed")?,
        }
        if let Some(code) = self.code() {
            write!(f, " [{code}]")?;
        }
        let description = self.description().trim();
        if !description.is_empty() {
            write!(f, " {description}")?;
        } else if let Some(known) = self.code().and_then(ResponseCode::description) {
            write!(f, " {known}")?;
        }
        if let Some(field) = self.field() {
            write!(f, " ({field})")?;
        }
        Ok(())
    }
}

impl std::error::Error for ApiError {}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn api_error(value: serde_json::Value) -> ApiError {
        ApiError::new(200, serde_json::from_value(value).unwrap())
    }

    #[test]
    fn displays_status_code_description_and_field() {
        let err = api_error(json!({
            "CommandStatus": "Declined", "Description": "Invalid Gateway Credentials",
            "ErrorInformation": "Provider_Credentials", "ResponseCode": "100",
            "TransAct_ReferenceID": "12345"
        }));
        assert_eq!(
            err.to_string(),
            "Declined [100] Invalid Gateway Credentials (Provider_Credentials)"
        );
        assert_eq!(err.field(), Some("Provider_Credentials"));
        assert_eq!(err.reference_id(), Some(CommandId::new("12345")));
        assert!(err.is_credentials());
        assert!(!err.is_retryable());
    }

    #[test]
    fn falls_back_to_the_documented_description() {
        let err = api_error(
            json!({"CommandStatus": "Error", "ResponseCode": "150", "ErrorInformation": ""}),
        );
        assert_eq!(err.to_string(), "Error [150] Required field missing");
        assert_eq!(err.field(), None);
        assert!(err.is_validation());
        assert_eq!(api_error(json!({})).to_string(), "Failed");
    }

    #[test]
    fn classifies_codes() {
        let code = |c: &str| api_error(json!({"CommandStatus": "Declined", "ResponseCode": c}));
        assert!(code("102").is_duplicate());
        assert!(code("107").is_duplicate());
        assert!(code("109").is_credentials());
        assert!(code("603").is_retryable());
        assert!(!code("604").is_retryable());
        assert!(!code("300").is_validation());
    }

    #[test]
    fn express_verify_is_none_when_empty() {
        let empty = api_error(json!({"ExpressVerify": {"Status": null, "Code": null}}));
        assert_eq!(empty.express_verify(), None);
        let rejected = api_error(json!({"ExpressVerify": {"Status": "NEG", "Code": "GS02"}}));
        assert!(rejected.express_verify().is_some());
    }

    #[test]
    fn truncates_long_bodies() {
        assert_eq!(body_excerpt(b"short"), "short");
        let long = vec![b'a'; MAX_BODY + 10];
        assert_eq!(body_excerpt(&long).chars().count(), MAX_BODY + 1);
    }

    #[test]
    fn status_of_each_error() {
        let err = Error::Status {
            status: 502,
            body: String::new(),
        };
        assert_eq!(err.status(), Some(502));
        assert_eq!(Error::from(api_error(json!({}))).status(), Some(200));
    }
}
