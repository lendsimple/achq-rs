//! The HTTP requests and responses the protocol exchanges.
//!
//! [`protocol::encode`](crate::protocol::encode) turns an operation into an
//! [`HttpRequest`]; any HTTP client can send it, and
//! [`protocol::decode`](crate::protocol::decode) reads the [`HttpResponse`].

use std::fmt;
use std::time::Duration;

/// The HTTP methods the API uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Method {
    Get,
    Post,
}

impl Method {
    /// The method's name, such as `"POST"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A request to the API, ready to send.
///
/// The gateway's credentials are in the body, so `Debug` output shows only the
/// body's length, and hides secret headers.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HttpRequest {
    pub method: Method,
    /// The full URL.
    pub url: String,
    /// Header names (lowercase) and values.
    pub headers: Vec<(&'static str, String)>,
    /// The body; empty when the request has none.
    pub body: Vec<u8>,
    /// How long the request may take in total, including reading the response.
    /// The clients always set it; `None` from [`protocol::encode`](crate::protocol::encode).
    pub timeout: Option<Duration>,
}

impl HttpRequest {
    /// The value of the header `name` (ignoring case), if present.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Headers whose values `Debug` output hides: the onboarding API's gate key.
pub(crate) const SECRET_HEADERS: &[&str] = &["provider_gatekey"];

impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let headers: Vec<(&str, &str)> = self
            .headers
            .iter()
            .map(|(name, value)| {
                let secret = SECRET_HEADERS.iter().any(|s| s.eq_ignore_ascii_case(name));
                (*name, if secret { "[REDACTED]" } else { value.as_str() })
            })
            .collect();
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &headers)
            .field("body", &format_args!("{} bytes", self.body.len()))
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// The API's answer to an [`HttpRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HttpResponse {
    pub status: u16,
    /// Header names and values, as received.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn new(status: u16, headers: Vec<(String, String)>, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers,
            body: body.into(),
        }
    }

    /// The value of the header `name` (ignoring case), if present.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The media type, without parameters and in lowercase, such as
    /// `application/json`.
    pub fn content_type(&self) -> Option<String> {
        self.header("content-type")
            .map(|v| {
                v.split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
            })
            .filter(|v| !v.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_the_body_and_secret_headers() {
        let request = HttpRequest {
            method: Method::Post,
            url: "https://example.com".into(),
            headers: vec![
                ("accept", "application/json".into()),
                ("provider_gatekey", "s3cret".into()),
            ],
            body: b"Provider_GateKey=s3cret".to_vec(),
            timeout: None,
        };
        let debug = format!("{request:?}");
        assert!(!debug.contains("s3cret"), "{debug}");
        assert!(debug.contains("23 bytes"), "{debug}");
        assert_eq!(request.header("Accept"), Some("application/json"));
    }

    #[test]
    fn content_type_drops_parameters() {
        let response = HttpResponse::new(
            200,
            vec![("Content-Type".into(), "Text/HTML; charset=utf-8".into())],
            "",
        );
        assert_eq!(response.content_type().as_deref(), Some("text/html"));
        assert_eq!(HttpResponse::new(200, vec![], "").content_type(), None);
    }
}
