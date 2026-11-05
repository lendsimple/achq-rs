//! Decoding event tracking's CSV responses (see `tests/data/README.md`).

use achq::envelope::{ResponseCode, VerificationCode, VerificationStatus};
use achq::events::{EventName, PaymentStatus, ReturnCode};
use achq::operations::events::TrackEvents;
use achq::protocol;
use achq::transport::HttpResponse;
use achq::{Error, events::EventBatch};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/data/events/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn decode_body(body: impl Into<Vec<u8>>) -> Result<EventBatch, Error> {
    let response = HttpResponse::new(
        200,
        vec![("Content-Type".into(), "text/plain".into())],
        body,
    );
    protocol::decode::<TrackEvents>(&response)
}

fn decode(name: &str) -> Result<EventBatch, Error> {
    decode_body(fixture(name))
}

#[test]
fn the_documented_sample() {
    let batch = decode("docs_sample.csv").unwrap();
    assert_eq!(batch.events.len(), 89);
    assert_eq!(batch.command_id, None);

    let first = &batch.events[0];
    assert_eq!(first.payment_id.as_str(), "14447204");
    assert_eq!(
        first.provider_transaction_id.as_ref().unwrap().as_str(),
        "payment_transaction#228"
    );
    assert_eq!(first.merchant_id.as_ref().unwrap().as_str(), "2001");
    assert_eq!(first.event, EventName::Created);
    assert_eq!(first.occurred_at.to_string(), "2022-06-17T05:00:59");
    assert_eq!(first.status, PaymentStatus::Scheduled);
    assert_eq!(first.return_code, None);
    assert_eq!(first.verification, None);

    // A blank Provider_TransactionID.
    assert_eq!(batch.events[3].provider_transaction_id, None);
    // The undocumented authorization event, and `In-Process`.
    assert!(
        batch
            .events
            .iter()
            .any(|e| e.event == EventName::Authorization && e.status == PaymentStatus::Authorized)
    );
    assert!(
        batch
            .events
            .iter()
            .any(|e| e.status == PaymentStatus::InProcess)
    );
    // Every event and status is one the crate names.
    for event in &batch.events {
        assert!(!matches!(event.event, EventName::Other(_)), "{event:?}");
        assert!(
            !matches!(event.status, PaymentStatus::Other(_)),
            "{event:?}"
        );
    }
}

#[test]
fn ten_fields_and_the_command_response_row() {
    let batch = decode("ten_fields_with_trailer.csv").unwrap();
    assert_eq!(batch.events.len(), 3);
    assert_eq!(batch.command_id.unwrap().as_str(), "12345");
    let first = &batch.events[0];
    assert_eq!(first.payment_id.as_str(), "12345");
    assert_eq!(first.merchant_id, None);
    assert_eq!(first.event, EventName::Cancelled);
}

#[test]
fn returns_with_codes_explanations_and_verification() {
    let batch = decode("returns.csv").unwrap();
    assert_eq!(batch.events.len(), 3);
    let nsf = &batch.events[0];
    assert_eq!(nsf.status, PaymentStatus::ReturnedNsf);
    assert!(nsf.status.is_returned());
    assert_eq!(nsf.return_code, Some(ReturnCode::R01));
    assert_eq!(
        nsf.return_explanation.as_deref(),
        Some("Insufficient Funds, retry later")
    );
    let verification = nsf.verification.as_ref().unwrap();
    assert_eq!(verification.status, Some(VerificationStatus::Positive));
    assert_eq!(verification.code, Some(VerificationCode::PassAv));
    assert_eq!(verification.description.as_deref(), Some("Pass AV"));

    let charge_back = &batch.events[1];
    assert_eq!(charge_back.event, EventName::ChargedBack);
    assert_eq!(charge_back.return_code, Some(ReturnCode::R10));
    assert_eq!(charge_back.verification, None);
    assert_eq!(batch.events[2].event, EventName::HoldReleased);
}

#[test]
fn a_declined_query_is_an_api_error() {
    let err = decode("declined.csv").unwrap_err();
    let api = err.as_api().expect("an API error");
    assert_eq!(api.code(), Some(ResponseCode::INVALID_CREDENTIALS));
    assert_eq!(api.field(), Some("Provider_Credentials"));
    assert_eq!(api.reference_id().unwrap().as_str(), "12346");
}

#[test]
fn a_json_error_is_an_api_error() {
    let err = decode("json_error.json").unwrap_err();
    let api = err.as_api().expect("an API error");
    assert_eq!(api.code(), Some(ResponseCode::REQUIRED_FIELD_MISSING));
    assert_eq!(api.field(), Some("TrackingDate"));
}

#[test]
fn no_events() {
    assert_eq!(decode_body("").unwrap(), EventBatch::default());
    let only_trailer =
        decode_body("Command Response,Approved,000,Command Successful. Approved.,,7,,,\r\n")
            .unwrap();
    assert!(only_trailer.events.is_empty());
    assert_eq!(only_trailer.command_id.unwrap().as_str(), "7");
}

#[test]
fn malformed_rows_cannot_be_read() {
    let short = decode_body("14447204,,2001,Created\n").unwrap_err();
    assert!(
        short
            .to_string()
            .contains("line 1: expected at least 6 fields"),
        "{short}"
    );
    let bad_time = decode_body("14447204,,2001,Created,yesterday,Scheduled,,,,,\n").unwrap_err();
    assert!(
        bad_time.to_string().contains("line 1: invalid date-time"),
        "{bad_time}"
    );
    let html = decode_body("<html></html>").unwrap_err();
    assert!(matches!(
        html,
        Error::UnexpectedContentType {
            expected: "CSV",
            ..
        }
    ));
}
