//! Both clients with custom, in-memory transports: no HTTP, no cargo features, and
//! for the async client no tokio (a minimal executor drives it).

mod common;

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake};
use std::time::Duration;

use achq::operations::payments::CancelPayment;
use achq::protocol::DEFAULT_USER_AGENT;
use achq::transport::{HttpRequest, HttpResponse, Method};
use achq::{
    ClientBuilder, DEFAULT_TIMEOUT, Endpoints, Error, GATEWAY_URL, TransportError, blocking,
};
use common::{MERCHANT_KEY, PROVIDER_KEY, merchant_credentials, provider};

type Client = achq::Client<FakeTransport>;

/// Answers every request with the same result and records what was sent.
#[derive(Clone)]
struct FakeTransport {
    answer: Arc<dyn Fn() -> Result<HttpResponse, TransportError> + Send + Sync>,
    sent: Arc<Mutex<Vec<HttpRequest>>>,
}

impl FakeTransport {
    fn json(body: &str) -> Self {
        let body = body.to_owned();
        Self::with(move || {
            Ok(HttpResponse::new(
                200,
                vec![("content-type".into(), "application/json".into())],
                body.clone(),
            ))
        })
    }

    fn with(
        answer: impl Fn() -> Result<HttpResponse, TransportError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            answer: Arc::new(answer),
            sent: Arc::default(),
        }
    }

    fn answer(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
        self.sent.lock().unwrap().push(request);
        (self.answer)()
    }

    fn only_request(&self) -> HttpRequest {
        let sent = self.sent.lock().unwrap();
        assert_eq!(sent.len(), 1, "expected exactly one request");
        sent[0].clone()
    }

    fn sent(&self) -> usize {
        self.sent.lock().unwrap().len()
    }
}

impl achq::transport::Transport for FakeTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
        self.answer(request)
    }
}

impl blocking::Transport for FakeTransport {
    fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
        self.answer(request)
    }
}

/// Runs a future to completion on the current thread, without an async runtime.
fn block_on<F: Future>(future: F) -> F::Output {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Arc::new(Unpark(std::thread::current())).into();
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return output;
        }
        std::thread::park();
    }
}

const RECEIPT: &str =
    r#"{"CommandStatus": "Approved", "ResponseCode": "000", "TransAct_ReferenceID": "14477176"}"#;

fn field(request: &HttpRequest, name: &str) -> Option<String> {
    common::form_fields(&request.body)
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v)
}

#[test]
fn async_client_sends_through_a_custom_transport() {
    let transport = FakeTransport::json(RECEIPT);
    let client = Client::with_transport(provider(), transport.clone());
    let receipt = block_on(
        client
            .merchant(merchant_credentials())
            .payments()
            .cancel("14477175"),
    )
    .unwrap();
    assert_eq!(receipt.command_id.as_str(), "14477176");

    let sent = transport.only_request();
    assert_eq!(sent.method, Method::Post);
    assert_eq!(sent.url, GATEWAY_URL);
    assert_eq!(sent.timeout, Some(DEFAULT_TIMEOUT));
    assert_eq!(sent.header("user-agent"), Some(DEFAULT_USER_AGENT));
    assert_eq!(
        sent.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(field(&sent, "Command").as_deref(), Some("ECheck.Void"));
    assert_eq!(
        field(&sent, "Provider_GateKey").as_deref(),
        Some(PROVIDER_KEY)
    );
    assert_eq!(
        field(&sent, "Merchant_GateKey").as_deref(),
        Some(MERCHANT_KEY)
    );
    assert_eq!(
        field(&sent, "Transact_ReferenceID").as_deref(),
        Some("14477175")
    );
}

#[test]
fn blocking_client_sends_through_a_custom_transport() {
    let transport = FakeTransport::json(RECEIPT);
    let client = blocking::Client::with_transport(provider(), transport.clone());
    let receipt = client
        .merchant(merchant_credentials())
        .payments()
        .hold("14477207")
        .unwrap();
    assert_eq!(receipt.command_id.as_str(), "14477176");
    let sent = transport.only_request();
    assert_eq!(field(&sent, "Command").as_deref(), Some("ECheck.Hold"));
}

#[test]
fn builder_settings_reach_the_request() {
    let transport = FakeTransport::json(RECEIPT);
    let client = ClientBuilder::new(provider(), transport.clone())
        .endpoints(Endpoints::custom(
            "https://gateway.example/transact.aspx",
            "https://gateway.example/api",
        ))
        .timeout(Duration::from_secs(5))
        .user_agent("my-app/1.0")
        .test_mode(true)
        .build();
    block_on(
        client
            .merchant(merchant_credentials())
            .payments()
            .cancel("1"),
    )
    .unwrap();

    let sent = transport.only_request();
    assert_eq!(sent.url, "https://gateway.example/transact.aspx");
    assert_eq!(sent.timeout, Some(Duration::from_secs(5)));
    assert_eq!(sent.header("user-agent"), Some("my-app/1.0"));
    assert_eq!(field(&sent, "TestMode").as_deref(), Some("On"));
}

#[test]
fn merchants_share_the_client_and_its_transport() {
    let transport = FakeTransport::json(RECEIPT);
    let client = Client::with_transport(provider(), transport.clone());
    let first = client.merchant(merchant_credentials());
    let second = client.merchant(achq::MerchantCredentials::new("3003", "gate", "other-key"));
    assert_eq!(first.credentials().merchant_id().as_str(), "2001");
    block_on(first.payments().cancel("1")).unwrap();
    block_on(second.clone().payments().cancel("2")).unwrap();

    let sent = transport.sent.lock().unwrap();
    let merchants: Vec<_> = sent.iter().map(|r| field(r, "MerchantID")).collect();
    assert_eq!(merchants, [Some("2001".into()), Some("3003".into())]);
    assert!(std::ptr::eq(first.client().transport(), client.transport()));
}

#[test]
fn a_merchant_command_needs_a_merchant() {
    let transport = FakeTransport::json(RECEIPT);
    let client = blocking::Client::with_transport(provider(), transport.clone());
    // The handles put merchant commands on a Merchant; `execute` accepts any.
    let err = client.execute(&CancelPayment::new("1")).unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid request: merchant credentials: are required for payments.cancel"
    );
    assert_eq!(transport.sent(), 0);
}

#[test]
fn an_invalid_request_is_not_sent() {
    let transport = FakeTransport::json(RECEIPT);
    let client = blocking::Client::with_transport(provider(), transport.clone());
    let mut payment = common::bank_payment();
    payment.customer.email = "not an email".into();
    let err = client
        .merchant(merchant_credentials())
        .payments()
        .create(&payment)
        .unwrap_err();
    assert!(matches!(err, Error::Validation(_)), "{err:?}");
    assert_eq!(transport.sent(), 0);
}

#[test]
fn validation_can_be_turned_off() {
    let transport = FakeTransport::json(RECEIPT);
    let client = blocking::ClientBuilder::new(provider(), transport.clone())
        .validate_requests(false)
        .build();
    client
        .merchant(merchant_credentials())
        .payments()
        .cancel(" ")
        .unwrap();
    assert_eq!(transport.sent(), 1);
}

#[test]
fn transport_errors_are_returned() {
    let transport = FakeTransport::with(|| Err(TransportError::timeout("too slow")));
    let client = Client::with_transport(provider(), transport);
    let err = block_on(client.events().query(&common::event_query())).unwrap_err();
    let Error::Transport(err) = err else {
        panic!("expected a transport error, got {err:?}");
    };
    assert!(err.is_timeout());
}

#[test]
fn debug_output_hides_the_gate_keys() {
    let client = Client::with_transport(provider(), FakeTransport::json("{}"));
    let merchant = client.merchant(merchant_credentials());
    let builder = blocking::ClientBuilder::new(provider(), FakeTransport::json("{}"));
    for debug in [
        format!("{client:?}"),
        format!("{merchant:?}"),
        format!("{builder:?}"),
        format!("{:?}", merchant.payments()),
    ] {
        assert!(!debug.contains(PROVIDER_KEY), "{debug}");
        assert!(!debug.contains(MERCHANT_KEY), "{debug}");
    }
    assert!(format!("{merchant:?}").contains("2001"));
}
