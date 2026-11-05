//! The blocking client, which needs no async runtime, and its resource handles.
//!
//! Do not call it from async code: it blocks the thread. In a tokio task, use the
//! async [`Client`](crate::Client), or wrap calls in `tokio::task::spawn_blocking`.

use std::fmt;
use std::sync::Arc;

use crate::config::{Config, common_builder_methods, request_span};
use crate::credentials::{MerchantCredentials, ProviderCredentials};
use crate::error::{Error, TransportError};
use crate::operation::Operation;
use crate::operations::{blocking_handles, operation_table};
use crate::transport::{HttpRequest, HttpResponse};

/// Sends requests for the blocking [`Client`].
///
/// Return a [`TransportError`] only when no response was received (a connection,
/// TLS or timeout failure); an HTTP error status is a response. See
/// [`transport`](crate::transport) for the async counterpart and an example.
#[cfg(not(target_arch = "wasm32"))]
pub trait Transport: Send + Sync {
    fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError>;
}

/// Sends requests for the blocking [`Client`].
///
/// On WebAssembly the transport need not be `Send` or `Sync`: there are no threads
/// to share it with, and host HTTP APIs (such as WASI's) are often single-threaded.
#[cfg(target_arch = "wasm32")]
pub trait Transport {
    fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError>;
}

/// A blocking ACHQ client, acting for a platform.
///
/// It sends requests through a [`Transport`], by default a [`UreqTransport`]
/// (with the `ureq` feature). Use [`with_transport`](Client::with_transport) to
/// send them with another HTTP client.
///
/// As with the async [`Client`](crate::Client), it runs the platform's reports,
/// and commands on a merchant's payments go through [`merchant`](Self::merchant).
/// Cloning is cheap and clones share one transport.
///
/// ```no_run
/// # #[cfg(feature = "ureq")]
/// # fn run() -> Result<(), achq::Error> {
/// use achq::blocking::Client;
/// use achq::{MerchantCredentials, ProviderCredentials};
///
/// let client = Client::new(ProviderCredentials::new("1234", "gate-id", "gate-key"));
/// let merchant = client.merchant(MerchantCredentials::new("5678", "gate-id", "gate-key"));
/// let receipt = merchant.payments().hold("14477207")?;
/// println!("held; quote {} to ACHQ support", receipt.command_id);
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "ureq")]
pub struct Client<T = UreqTransport> {
    transport: Arc<T>,
    config: Arc<Config>,
}

/// A blocking ACHQ client, acting for a platform.
///
/// It sends requests through a [`Transport`]; enable the `ureq` feature for a
/// default one. Cloning is cheap and clones share one transport.
#[cfg(not(feature = "ureq"))]
pub struct Client<T> {
    transport: Arc<T>,
    config: Arc<Config>,
}

#[cfg(feature = "ureq")]
impl Client {
    /// A client with default settings and a [`UreqTransport`].
    pub fn new(provider: ProviderCredentials) -> Self {
        Self::builder(provider).build()
    }

    /// A builder to configure the endpoints, timeout, test mode and so on.
    pub fn builder(provider: ProviderCredentials) -> ClientBuilder {
        ClientBuilder::new(provider, UreqTransport::default())
    }
}

impl<T: Transport> Client<T> {
    /// A client with default settings, sending requests through `transport`. Use
    /// [`ClientBuilder::new`] to change the settings.
    pub fn with_transport(provider: ProviderCredentials, transport: T) -> Self {
        ClientBuilder::new(provider, transport).build()
    }

    /// The transport requests are sent through.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Acts for a merchant: its payments, tokens and account checks. The
    /// [`Merchant`] shares this client's transport and settings.
    pub fn merchant(&self, merchant: MerchantCredentials) -> Merchant<T> {
        let mut config = Config::clone(&self.config);
        config.context.merchant = Some(merchant);
        Merchant {
            client: Self {
                transport: Arc::clone(&self.transport),
                config: Arc::new(config),
            },
        }
    }

    /// Sends any operation. The resource methods, such as
    /// [`events`](Self::events), call this.
    ///
    /// A command on a merchant's payments fails validation here: send it from a
    /// [`merchant`](Self::merchant).
    pub fn execute<O: Operation>(&self, op: &O) -> Result<O::Response, Error> {
        let _span = request_span::<O>().entered();
        let request = self.config.encode(op)?;
        let response = self.transport.send(request)?;
        self.config.decode::<O>(&response)
    }
}

impl<T> Clone for Client<T> {
    fn clone(&self) -> Self {
        Self {
            transport: Arc::clone(&self.transport),
            config: Arc::clone(&self.config),
        }
    }
}

impl<T> fmt::Debug for Client<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// A blocking [`Client`] acting for one merchant, from [`Client::merchant`]: the
/// commands on the merchant's payments, tokens and bank account checks.
pub struct Merchant<T> {
    client: Client<T>,
}

impl<T: Transport> Merchant<T> {
    /// The merchant's credentials.
    pub fn credentials(&self) -> &MerchantCredentials {
        self.client
            .config
            .context
            .merchant
            .as_ref()
            .expect("a Merchant always has credentials")
    }

    /// The platform's client.
    pub fn client(&self) -> &Client<T> {
        &self.client
    }

    /// Sends any operation, with the merchant's credentials. The resource
    /// methods, such as [`payments`](Self::payments), call this.
    pub fn execute<O: Operation>(&self, op: &O) -> Result<O::Response, Error> {
        self.client.execute(op)
    }
}

impl<T> Clone for Merchant<T> {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
        }
    }
}

impl<T> fmt::Debug for Merchant<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Merchant")
            .field("config", &self.client.config)
            .finish_non_exhaustive()
    }
}

/// Configures a blocking [`Client`].
#[cfg(feature = "ureq")]
#[must_use]
pub struct ClientBuilder<T = UreqTransport> {
    config: Config,
    transport: T,
}

/// Configures a blocking [`Client`].
#[cfg(not(feature = "ureq"))]
#[must_use]
pub struct ClientBuilder<T> {
    config: Config,
    transport: T,
}

impl<T> ClientBuilder<T> {
    /// A builder for a client that sends requests through `transport`.
    pub fn new(provider: ProviderCredentials, transport: T) -> Self {
        Self {
            config: Config::new(provider),
            transport,
        }
    }

    common_builder_methods!();
}

impl<T: Transport> ClientBuilder<T> {
    pub fn build(self) -> Client<T> {
        Client {
            transport: Arc::new(self.transport),
            config: Arc::new(self.config),
        }
    }
}

impl<T> fmt::Debug for ClientBuilder<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientBuilder")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// A [`Transport`] built on [ureq](https://docs.rs/ureq).
#[cfg(feature = "ureq")]
#[derive(Debug, Clone)]
pub struct UreqTransport {
    agent: ureq::Agent,
}

/// Larger than any response: a page of 500 settlement items, or a busy day's
/// events, are far smaller.
#[cfg(feature = "ureq")]
const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;

#[cfg(feature = "ureq")]
impl Default for UreqTransport {
    /// A transport with its own connection pool.
    fn default() -> Self {
        Self {
            agent: ureq::Agent::new_with_defaults(),
        }
    }
}

#[cfg(feature = "ureq")]
impl From<ureq::Agent> for UreqTransport {
    /// Uses an existing ureq agent, for example to share its connection pool or
    /// proxy settings.
    fn from(agent: ureq::Agent) -> Self {
        Self { agent }
    }
}

#[cfg(feature = "ureq")]
impl Transport for UreqTransport {
    fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
        let mut builder = ureq::http::Request::builder()
            .method(request.method.as_str())
            .uri(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(*name, value);
        }
        // A request without a body sends none, rather than an empty one.
        let mut response = if request.body.is_empty() {
            self.run(builder.body(()), request.timeout)
        } else {
            self.run(builder.body(request.body), request.timeout)
        }?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
                (name.as_str().to_owned(), value)
            })
            .collect();
        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_vec()
            .map_err(ureq_error)?;
        Ok(HttpResponse::new(status, headers, body))
    }
}

#[cfg(feature = "ureq")]
impl UreqTransport {
    fn run<B: ureq::AsSendBody>(
        &self,
        request: Result<ureq::http::Request<B>, ureq::http::Error>,
        timeout: Option<std::time::Duration>,
    ) -> Result<ureq::http::Response<ureq::Body>, TransportError> {
        let request = self
            .agent
            .configure_request(request.map_err(TransportError::new)?)
            .http_status_as_error(false)
            .timeout_global(timeout)
            .build();
        self.agent.run(request).map_err(ureq_error)
    }
}

#[cfg(feature = "ureq")]
fn ureq_error(err: ureq::Error) -> TransportError {
    if matches!(err, ureq::Error::Timeout(_)) {
        TransportError::timeout(err)
    } else {
        TransportError::new(err)
    }
}

operation_table!(blocking_handles);
