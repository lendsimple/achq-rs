//! Encoding requests and decoding responses, without any I/O.
//!
//! The clients are thin layers over [`encode`] and [`decode`], which can also
//! drive any other HTTP client:
//!
//! ```
//! use achq::operations::payments::CancelPayment;
//! use achq::protocol::{self, Context};
//! use achq::transport::HttpResponse;
//! use achq::{MerchantCredentials, ProviderCredentials};
//!
//! let context = Context::new(ProviderCredentials::sandbox()).merchant(MerchantCredentials::sandbox());
//! let cancel = CancelPayment::new("14476929");
//!
//! let request = protocol::encode(&cancel, &context)?;
//! assert_eq!(request.url, achq::GATEWAY_URL);
//!
//! // ... send `request`, then read the answer:
//! let response = HttpResponse::new(
//!     200,
//!     vec![("Content-Type".into(), "application/json".into())],
//!     r#"{"CommandStatus": "Approved", "ResponseCode": "000", "TransAct_ReferenceID": "14477176"}"#,
//! );
//! let receipt = protocol::decode::<CancelPayment>(&response)?;
//! assert_eq!(receipt.command_id.as_str(), "14477176");
//! # Ok::<(), achq::Error>(())
//! ```

use serde::de::DeserializeOwned;

use crate::credentials::{Endpoints, MerchantCredentials, ProviderCredentials};
use crate::envelope::Envelope;
use crate::error::{ApiError, Error, OnboardingError, body_excerpt};
use crate::form::FormWriter;
use crate::operation::{Endpoint, Operation, Scope};
use crate::transport::{HttpRequest, HttpResponse, Method};
use crate::validate::{ValidationError, Violation};

/// The `User-Agent` sent unless set otherwise.
pub const DEFAULT_USER_AGENT: &str = concat!("achq-rs/", env!("CARGO_PKG_VERSION"));

/// The gateway API version every command is sent with (`CommandVersion`).
pub const COMMAND_VERSION: &str = "2.0";

/// What every request needs besides the command: who sends it, and where.
#[derive(Debug, Clone)]
pub struct Context {
    pub(crate) provider: ProviderCredentials,
    pub(crate) merchant: Option<MerchantCredentials>,
    pub(crate) endpoints: Endpoints,
    pub(crate) test_mode: bool,
    pub(crate) user_agent: String,
}

impl Context {
    /// Requests from the platform `provider`, to ACHQ's endpoints, for no merchant.
    pub fn new(provider: ProviderCredentials) -> Self {
        Self {
            provider,
            merchant: None,
            endpoints: Endpoints::default(),
            test_mode: false,
            user_agent: DEFAULT_USER_AGENT.to_owned(),
        }
    }

    /// The merchant whose payments the commands act on. Merchant commands
    /// (payments, tokens, verifications) need one; reports do not.
    pub fn merchant(mut self, merchant: MerchantCredentials) -> Self {
        self.merchant = Some(merchant);
        self
    }

    /// Acts for no merchant.
    pub fn without_merchant(mut self) -> Self {
        self.merchant = None;
        self
    }

    /// Where requests go. The default is ACHQ's.
    pub fn endpoints(mut self, endpoints: Endpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    /// Sends `TestMode=On` with every command: the gateway answers as it would,
    /// but records nothing. For trying production credentials without moving
    /// money.
    pub fn test_mode(mut self, test_mode: bool) -> Self {
        self.test_mode = test_mode;
        self
    }

    /// The `User-Agent` header. The default is `achq-rs/<version>`.
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// The platform's credentials.
    pub fn provider(&self) -> &ProviderCredentials {
        &self.provider
    }

    /// The merchant's credentials, if any.
    pub fn get_merchant(&self) -> Option<&MerchantCredentials> {
        self.merchant.as_ref()
    }

    /// Where requests go.
    pub fn get_endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    /// Whether commands are sent with `TestMode=On`.
    pub fn is_test_mode(&self) -> bool {
        self.test_mode
    }

    /// The `User-Agent` header.
    pub fn get_user_agent(&self) -> &str {
        &self.user_agent
    }
}

/// Validates `op`, then builds its HTTP request.
///
/// Fails with [`Error::Validation`] if the command's fields break the API's
/// constraints, or if it acts on a merchant's payments and `context` has no
/// merchant.
pub fn encode<O: Operation>(op: &O, context: &Context) -> Result<HttpRequest, Error> {
    let mut violations = op
        .validate()
        .err()
        .map(ValidationError::into_violations)
        .unwrap_or_default();
    if O::SCOPE == Scope::Merchant && context.merchant.is_none() {
        violations.push(Violation {
            path: "merchant credentials".to_owned(),
            message: format!("are required for {}", O::NAME),
        });
    }
    if let Some(err) = ValidationError::from_violations(violations) {
        return Err(err.into());
    }
    encode_unvalidated(op, context)
}

/// Like [`encode`], without validating `op` first. A merchant command without a
/// merchant is sent without merchant credentials, for the gateway to refuse.
///
/// Fails only if an onboarding API body cannot be written as JSON.
pub fn encode_unvalidated<O: Operation>(op: &O, context: &Context) -> Result<HttpRequest, Error> {
    match O::ENDPOINT {
        Endpoint::Gateway => Ok(HttpRequest {
            method: Method::Post,
            url: context.endpoints.gateway().to_owned(),
            headers: vec![
                ("accept", "application/json".to_owned()),
                (
                    "content-type",
                    "application/x-www-form-urlencoded".to_owned(),
                ),
                ("user-agent", context.user_agent.clone()),
            ],
            body: form(op, context).encode().into_bytes(),
            timeout: None,
        }),
        Endpoint::Api(path) => {
            let body = op
                .json_body()
                .unwrap_or_else(|| Ok(b"{}".to_vec()))
                .map_err(Error::Encode)?;
            let provider = &context.provider;
            Ok(HttpRequest {
                method: Method::Post,
                url: format!("{}{path}", context.endpoints.api()),
                headers: vec![
                    ("accept", "application/json".to_owned()),
                    ("content-type", "application/json".to_owned()),
                    ("user-agent", context.user_agent.clone()),
                    ("providerid", provider.provider_id().to_owned()),
                    ("provider_gateid", provider.gate_id().to_owned()),
                    ("provider_gatekey", provider.expose_key().to_owned()),
                ],
                body,
                timeout: None,
            })
        }
    }
}

/// The form body of a gateway command: the credentials, the command, then its
/// fields. (The onboarding API's commands send JSON instead.)
///
/// The gate keys are marked sensitive, so [`FormWriter::redacted`] hides them.
pub fn form<O: Operation>(op: &O, context: &Context) -> FormWriter {
    let mut form = FormWriter::new();
    let provider = &context.provider;
    form.push("ProviderID", provider.provider_id());
    form.push("Provider_GateID", provider.gate_id());
    form.push_sensitive("Provider_GateKey", provider.expose_key());
    if O::SCOPE == Scope::Merchant
        && let Some(merchant) = &context.merchant
    {
        form.push("MerchantID", merchant.merchant_id().as_str());
        form.push("Merchant_GateID", merchant.gate_id());
        form.push_sensitive("Merchant_GateKey", merchant.expose_key());
    }
    form.push("Command", O::COMMAND);
    form.push("CommandVersion", COMMAND_VERSION);
    if context.test_mode {
        form.push("TestMode", "On");
    }
    for (name, value) in O::EXTRA {
        form.push(name, value);
    }
    op.write_form(&mut form);
    form
}

/// Reads the response to `O`.
///
/// Any status other than 2xx fails with [`Error::Status`], except the onboarding
/// API's error responses, which fail with [`Error::Onboarding`]. The gateway
/// reports whether a command succeeded in the body: a `CommandStatus` other than
/// `Approved` fails with [`Error::Api`].
pub fn decode<O: Operation>(response: &HttpResponse) -> Result<O::Response, Error> {
    if (200..300).contains(&response.status) {
        return O::decode(response);
    }
    if let Endpoint::Api(_) = O::ENDPOINT
        && is_json_object(&response.body)
        && let Ok(error) = serde_json::from_slice::<OnboardingError>(&response.body)
        && error.has_content()
    {
        return Err(Error::Onboarding(Box::new(
            error.with_status(response.status),
        )));
    }
    Err(Error::Status {
        status: response.status,
        body: body_excerpt(&response.body),
    })
}

/// Reads a JSON body as `T`, whatever its `Content-Type` says.
#[doc(hidden)]
pub fn decode_json<T: DeserializeOwned>(response: &HttpResponse) -> Result<T, Error> {
    if !is_json_object(&response.body) {
        return Err(Error::UnexpectedContentType {
            status: response.status,
            expected: "JSON",
            content_type: response
                .content_type()
                .unwrap_or_else(|| "an unlabeled body".to_owned()),
            body: body_excerpt(&response.body),
        });
    }
    serde_json::from_slice(&response.body)
        .map_err(|e| Error::decode(e, response.status, &response.body))
}

/// Whether a body is a JSON object, judged by its first byte rather than its
/// `Content-Type`, which the gateway does not set reliably.
pub(crate) fn is_json_object(body: &[u8]) -> bool {
    body.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{')
}

/// Reads the gateway's JSON envelope, failing with [`Error::Api`] unless the
/// command was approved, and reads its `ResponseData` as `D`.
#[doc(hidden)]
pub fn decode_envelope<D: DeserializeOwned>(response: &HttpResponse) -> Result<Envelope<D>, Error> {
    if !is_json_object(&response.body) {
        return Err(Error::UnexpectedContentType {
            status: response.status,
            expected: "JSON",
            content_type: response
                .content_type()
                .unwrap_or_else(|| "an unlabeled body".to_owned()),
            body: body_excerpt(&response.body),
        });
    }
    let envelope: Envelope<serde_json::Value> = serde_json::from_slice(&response.body)
        .map_err(|e| Error::decode(e, response.status, &response.body))?;
    check_approved(response.status, envelope)?.map_data(|data| {
        serde_json::from_value(data).map_err(|e| Error::decode(e, response.status, &response.body))
    })
}

/// The envelope, if the command was approved; [`Error::Api`] otherwise.
pub(crate) fn check_approved(
    status: u16,
    envelope: Envelope<serde_json::Value>,
) -> Result<Envelope<serde_json::Value>, Error> {
    if envelope.is_approved() {
        Ok(envelope)
    } else {
        Err(ApiError::new(status, envelope).into())
    }
}

/// The ID the gateway gave the command, which every approved command has.
#[doc(hidden)]
pub fn reference_id<D>(envelope: &Envelope<D>, response: &HttpResponse) -> Result<String, Error> {
    envelope.reference().map(str::to_owned).ok_or_else(|| {
        Error::decode(
            "the response has no TransAct_ReferenceID",
            response.status,
            &response.body,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_objects_are_judged_by_their_first_byte() {
        assert!(is_json_object(b"  \n{\"a\": 1}"));
        assert!(!is_json_object(b"<html>"));
        assert!(!is_json_object(b""));
        assert!(!is_json_object(b"14447204,,2001,Created"));
    }
}
