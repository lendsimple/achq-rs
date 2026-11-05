//! What the async and blocking clients share: settings, logging, and the calls
//! into the protocol.

use std::time::Duration;

use serde_json::Value;

use crate::credentials::ProviderCredentials;
use crate::error::Error;
use crate::operation::Operation;
use crate::protocol::{self, Context};
use crate::transport::{HttpRequest, HttpResponse};

/// How long a request may take, including reading the response, unless set
/// otherwise.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

const REDACTED: &str = "[REDACTED]";

/// Response fields whose values are never logged.
const SENSITIVE_RESPONSE_FIELDS: &[&str] = &["ACHQToken", "AccountToken", "AccountNumber", "TaxID"];

/// A client's settings.
#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub context: Context,
    pub timeout: Duration,
    pub validate_requests: bool,
}

impl Config {
    pub fn new(provider: ProviderCredentials) -> Self {
        Self {
            context: Context::new(provider),
            timeout: DEFAULT_TIMEOUT,
            validate_requests: true,
        }
    }

    /// The HTTP request for `op`, validated unless validation is turned off.
    pub fn encode<O: Operation>(&self, op: &O) -> Result<HttpRequest, Error> {
        let mut request = if self.validate_requests {
            protocol::encode(op, &self.context)
                .inspect_err(|err| tracing::debug!(error = %err, "request not sent"))?
        } else {
            protocol::encode_unvalidated(op, &self.context)
        };
        request.timeout = Some(self.timeout);
        tracing::debug!(method = %request.method, url = %request.url, "sending request");
        if tracing::enabled!(tracing::Level::TRACE) {
            // The gate keys and account details are in the body: log it from the
            // form, which knows which fields are sensitive.
            let body = protocol::form(op, &self.context).redacted();
            tracing::trace!(%body, "request body");
        }
        Ok(request)
    }

    pub fn decode<O: Operation>(&self, response: &HttpResponse) -> Result<O::Response, Error> {
        tracing::debug!(status = response.status, "received response");
        if tracing::enabled!(tracing::Level::TRACE) {
            let body = redact_response(response.content_type().as_deref(), &response.body);
            tracing::trace!(%body, "response body");
        }
        let result = protocol::decode::<O>(response);
        if let Err(err) = &result {
            tracing::debug!(error = %err, "request failed");
        }
        result
    }
}

/// The span every request runs in.
pub(crate) fn request_span<O: Operation>() -> tracing::Span {
    tracing::debug_span!("achq", operation = O::NAME, command = O::COMMAND)
}

/// A response body for logging: JSON with the values of sensitive fields replaced
/// by `[REDACTED]`, or a summary of anything else (such as an event report).
pub(crate) fn redact_response(content_type: Option<&str>, body: &[u8]) -> String {
    if body.is_empty() {
        return String::new();
    }
    match serde_json::from_slice::<Value>(body) {
        Ok(mut json) => {
            redact_json(&mut json);
            json.to_string()
        }
        Err(_) => format!(
            "<{} bytes of {}>",
            body.len(),
            content_type.unwrap_or("unknown type")
        ),
    }
}

fn redact_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                let sensitive = SENSITIVE_RESPONSE_FIELDS
                    .iter()
                    .any(|field| field.eq_ignore_ascii_case(key));
                if sensitive && !value.is_null() {
                    *value = Value::String(REDACTED.to_owned());
                } else {
                    redact_json(value);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact_json),
        _ => {}
    }
}

/// The builder settings both clients have, as methods on a builder whose
/// `config` field is a [`Config`].
macro_rules! common_builder_methods {
    () => {
        /// Where requests go. The default is ACHQ's gateway and API.
        pub fn endpoints(mut self, endpoints: $crate::Endpoints) -> Self {
            self.config.context.endpoints = endpoints;
            self
        }

        /// Sends `TestMode=On` with every command: the gateway answers as it would,
        /// but records nothing. For trying production credentials without moving
        /// money.
        pub fn test_mode(mut self, test_mode: bool) -> Self {
            self.config.context.test_mode = test_mode;
            self
        }

        /// How long a request may take in total. The default is 60 seconds.
        pub fn timeout(mut self, timeout: ::std::time::Duration) -> Self {
            self.config.timeout = timeout;
            self
        }

        /// The `User-Agent` header. The default is `achq-rs/<version>`.
        pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
            self.config.context.user_agent = user_agent.into();
            self
        }

        /// Whether to check requests against the API's constraints before sending
        /// them (the default), failing with
        /// [`Error::Validation`](crate::Error::Validation).
        pub fn validate_requests(mut self, validate: bool) -> Self {
            self.config.validate_requests = validate;
            self
        }
    };
}

pub(crate) use common_builder_methods;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_sensitive_response_fields_at_any_depth() {
        let body = br#"{"CommandStatus": "Approved", "ACHQToken": "achq-sandbox-1",
                        "ResponseData": [{"accountnumber": "123", "Name": "x"}],
                        "Other": {"TaxID": "1234", "AccountToken": null}}"#;
        assert_eq!(
            redact_response(Some("application/json"), body),
            r#"{"ACHQToken":"[REDACTED]","CommandStatus":"Approved","Other":{"AccountToken":null,"TaxID":"[REDACTED]"},"ResponseData":[{"Name":"x","accountnumber":"[REDACTED]"}]}"#
        );
    }

    #[test]
    fn summarizes_other_bodies() {
        assert_eq!(
            redact_response(Some("text/plain"), b"14447204,,2001,Created\n"),
            "<23 bytes of text/plain>"
        );
        assert_eq!(
            redact_response(None, b"<html>"),
            "<6 bytes of unknown type>"
        );
        assert_eq!(redact_response(None, b""), "");
    }
}
