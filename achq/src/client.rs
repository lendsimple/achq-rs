//! The async client and its resource handles.

use std::fmt;
use std::sync::Arc;

use tracing::Instrument;

use crate::config::{Config, common_builder_methods, request_span};
use crate::credentials::{MerchantCredentials, ProviderCredentials};
use crate::error::Error;
use crate::operation::Operation;
use crate::operations::{async_handles, operation_table};
#[cfg(feature = "reqwest")]
use crate::transport::ReqwestTransport;
use crate::transport::Transport;

/// An async ACHQ client, acting for a platform.
///
/// It sends requests through a [`Transport`], by default a [`ReqwestTransport`]
/// (with the `reqwest` feature). Use [`with_transport`](Client::with_transport) to
/// send them with another HTTP client.
///
/// The client runs the platform's reports, [`settlements`](Self::settlements)
/// and [`events`](Self::events). Commands on a merchant's payments go through
/// [`merchant`](Self::merchant), with the merchant's credentials.
///
/// Cloning is cheap and clones share one transport, so a client can be shared
/// across tasks.
///
/// ```no_run
/// # #[cfg(feature = "reqwest")]
/// # async fn run() -> Result<(), achq::Error> {
/// use achq::events::EventQuery;
/// use achq::{Client, MerchantCredentials, ProviderCredentials};
///
/// let client = Client::new(ProviderCredentials::new("1234", "gate-id", "gate-key"));
/// let merchant = client.merchant(MerchantCredentials::new("5678", "gate-id", "gate-key"));
/// merchant.payments().cancel("14477175").await?;
///
/// let yesterday = time::OffsetDateTime::now_utc().date().previous_day().unwrap();
/// let batch = client.events().query(&EventQuery::on(yesterday)).await?;
/// for event in batch.events {
///     println!("{}: {} -> {}", event.payment_id, event.event, event.status);
/// }
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "reqwest")]
pub struct Client<T = ReqwestTransport> {
    transport: Arc<T>,
    config: Arc<Config>,
}

/// An async ACHQ client, acting for a platform.
///
/// It sends requests through a [`Transport`]; enable the `reqwest` feature for a
/// default one. Cloning is cheap and clones share one transport.
#[cfg(not(feature = "reqwest"))]
pub struct Client<T> {
    transport: Arc<T>,
    config: Arc<Config>,
}

#[cfg(feature = "reqwest")]
impl Client {
    /// A client with default settings and a [`ReqwestTransport`].
    ///
    /// # Panics
    ///
    /// Like `reqwest::Client::new`, if the TLS backend cannot be initialized.
    pub fn new(provider: ProviderCredentials) -> Self {
        Self::builder(provider).build()
    }

    /// A builder to configure the endpoints, timeout, test mode and so on.
    pub fn builder(provider: ProviderCredentials) -> ClientBuilder {
        ClientBuilder::new(provider, ReqwestTransport::default())
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
    pub async fn execute<O: Operation + Sync>(&self, op: &O) -> Result<O::Response, Error> {
        let config = &self.config;
        async move {
            let request = config.encode(op)?;
            let response = self.transport.send(request).await?;
            config.decode::<O>(&response)
        }
        .instrument(request_span::<O>())
        .await
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

/// A [`Client`] acting for one merchant, from [`Client::merchant`]: the commands
/// on the merchant's payments, tokens and bank account checks.
///
/// It is cheap to create and to clone, and shares the client's transport; it
/// owns what it needs, so it can be kept or moved into a task.
///
/// ```no_run
/// # #[cfg(feature = "reqwest")]
/// # async fn run(client: achq::Client, credentials: achq::MerchantCredentials) -> Result<(), achq::Error> {
/// use achq::bank::BankAccount;
///
/// let merchant = client.merchant(credentials);
/// let token = merchant
///     .tokens()
///     .create(&BankAccount::checking("123123123", "12345678").into())
///     .await?
///     .token;
/// # let _ = token;
/// # Ok(())
/// # }
/// ```
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
    pub async fn execute<O: Operation + Sync>(&self, op: &O) -> Result<O::Response, Error> {
        self.client.execute(op).await
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

/// Configures a [`Client`].
#[cfg(feature = "reqwest")]
#[must_use]
pub struct ClientBuilder<T = ReqwestTransport> {
    config: Config,
    transport: T,
}

/// Configures a [`Client`].
#[cfg(not(feature = "reqwest"))]
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

operation_table!(async_handles);
