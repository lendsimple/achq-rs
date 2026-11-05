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

    /// The onboarding API refused the request, with an `ErrorResponse`.
    #[error(transparent)]
    Onboarding(Box<OnboardingError>),

    /// The request breaks the API's constraints, so it was not sent.
    #[error("invalid request: {0}")]
    Validation(#[from] ValidationError),

    /// An onboarding API body could not be written as JSON.
    #[error("could not write the request body: {0}")]
    Encode(#[source] serde_json::Error),

    /// A successful response could not be read as the expected type.
    #[error("could not read the response (HTTP {status}): {source}")]
    Decode {
        /// Why the response could not be read.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
        /// The HTTP status.
        status: u16,
        /// The start of the response body.
        body: String,
    },

    /// The response was not in the format the command answers with, such as an
    /// HTML error page.
    #[error("expected a {expected} response, got {content_type} (HTTP {status})")]
    UnexpectedContentType {
        /// The HTTP status.
        status: u16,
        /// What the command answers with: JSON, or CSV for event tracking.
        expected: &'static str,
        /// The response's `Content-Type`, or a note that it had none.
        content_type: String,
        /// The start of the response body.
        body: String,
    },

    /// The server answered with an error status.
    #[error("HTTP status {status}")]
    Status {
        /// The HTTP status.
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
            Error::Onboarding(err) => Some(err.status),
            Error::Decode { status, .. }
            | Error::UnexpectedContentType { status, .. }
            | Error::Status { status, .. } => Some(*status),
            Error::Validation(_) | Error::Encode(_) | Error::Transport(_) => None,
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

/// An error the onboarding API reported: an error status with an
/// `ErrorResponse` body.
///
/// Displays as `HTTP 400 [code] message: additional information`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct OnboardingError {
    #[serde(skip)]
    status: u16,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    additional_information: Option<String>,
    #[serde(default)]
    api_request_id: Option<String>,
    #[serde(default)]
    api_request_time_stamp: Option<String>,
}

impl OnboardingError {
    pub(crate) fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    /// Whether the body said anything, as opposed to being some other JSON.
    pub(crate) fn has_content(&self) -> bool {
        self.error_code.is_some() || self.message.is_some()
    }

    /// The HTTP status, usually 400.
    pub fn status(&self) -> u16 {
        self.status
    }

    /// The API's error code.
    pub fn error_code(&self) -> Option<&str> {
        self.error_code.as_deref()
    }

    /// The API's message; empty if there is none.
    pub fn message(&self) -> &str {
        self.message.as_deref().unwrap_or("")
    }

    /// More detail, such as the fields at fault.
    pub fn additional_information(&self) -> Option<&str> {
        self.additional_information.as_deref()
    }

    /// The API's ID of the request, to quote to ACHQ support.
    pub fn api_request_id(&self) -> Option<&str> {
        self.api_request_id.as_deref()
    }

    /// When the API handled the request, as it wrote it.
    pub fn api_request_timestamp(&self) -> Option<&str> {
        self.api_request_time_stamp.as_deref()
    }
}

impl fmt::Display for OnboardingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HTTP {}", self.status)?;
        if let Some(code) = &self.error_code {
            write!(f, " [{code}]")?;
        }
        if !self.message().is_empty() {
            write!(f, " {}", self.message())?;
        }
        if let Some(info) = self
            .additional_information
            .as_deref()
            .filter(|i| !i.is_empty())
        {
            write!(f, ": {info}")?;
        }
        Ok(())
    }
}

impl std::error::Error for OnboardingError {}

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

    /// The payment's bank account failed verification (*Express Verify*): see
    /// [`express_verify`](Self::express_verify) for why.
    pub fn is_account_rejected(&self) -> bool {
        self.code_in(&[
            ResponseCode::FAILED_EXPRESS_VERIFY,
            ResponseCode::REJECTED_BY_GATEWAY,
        ])
    }

    /// The payment was just created and is still queued, so it cannot be changed
    /// yet (103, "Payment still in a queued state. Please try again later."). Any
    /// other 103 means the payment can no longer change; see
    /// [`field`](Self::field) for its status.
    ///
    /// In the sandbox, a payment sent without
    /// [`run_express_verify`](crate::payments::PaymentRequest::run_express_verify)
    /// stayed queued for more than 30 seconds, while one sent with it could be
    /// changed at once.
    pub fn is_queued(&self) -> bool {
        self.code() == Some(ResponseCode::TRANSACTION_NOT_MODIFIABLE)
            && self
                .field()
                .is_some_and(|info| info.to_ascii_lowercase().contains("queued"))
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
    fn tells_a_queued_payment_from_one_that_cannot_change() {
        let queued = api_error(json!({"CommandStatus": "Error", "ResponseCode": "103",
            "ErrorInformation": "Payment still in a queued state. Please try again later."}));
        assert!(queued.is_queued());
        let settled = api_error(json!({"CommandStatus": "Error", "ResponseCode": "103",
            "ErrorInformation": "Scheduled"}));
        assert!(!settled.is_queued());
    }

    #[test]
    fn express_verify_is_none_when_empty() {
        let empty = api_error(json!({"ExpressVerify": {"Status": null, "Code": null}}));
        assert_eq!(empty.express_verify(), None);
        let rejected = api_error(json!({"ExpressVerify": {"Status": "NEG", "Code": "GS02"}}));
        assert!(rejected.express_verify().is_some());
    }

    #[test]
    fn displays_onboarding_errors() {
        let err: OnboardingError = serde_json::from_value(json!({
            "errorCode": "ValidationError", "message": "Invalid request",
            "additionalInformation": "businessEntity.taxId is required",
            "apiRequestId": "abc", "apiRequestTimeStamp": "2026-11-05T12:00:00Z"
        }))
        .unwrap();
        let err = err.with_status(400);
        assert_eq!(
            err.to_string(),
            "HTTP 400 [ValidationError] Invalid request: businessEntity.taxId is required"
        );
        assert_eq!(err.api_request_id(), Some("abc"));
        assert_eq!(Error::Onboarding(Box::new(err)).status(), Some(400));
        let empty: OnboardingError = serde_json::from_value(json!({"title": "x"})).unwrap();
        assert!(!empty.has_content());
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
