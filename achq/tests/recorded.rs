//! Real responses from ACHQ's public sandbox, recorded by `cargo xtask record`
//! (see `tests/data/recorded/README.md`), read as each command reads them.

use achq::envelope::{CommandStatus, ResponseCode, VerificationCode, VerificationStatus};
use achq::events::{EventName, PaymentStatus};
use achq::operations::{events, insights, payments, settlements, tokens};
use achq::protocol;
use achq::settlements::{SettlementType, TransferDirection};
use achq::transport::HttpResponse;
use achq::{Error, Operation};

fn recorded(name: &str, status: u16) -> HttpResponse {
    let path = format!("{}/tests/data/recorded/{name}", env!("CARGO_MANIFEST_DIR"));
    let body = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    HttpResponse::new(
        status,
        vec![(
            "Content-Type".into(),
            "application/json; charset=utf-8".into(),
        )],
        body,
    )
}

fn decode<O: Operation>(name: &str) -> Result<O::Response, Error> {
    protocol::decode::<O>(&recorded(name, 200))
}

fn api_error<O: Operation>(name: &str) -> achq::ApiError
where
    O::Response: std::fmt::Debug,
{
    let err = decode::<O>(name).unwrap_err();
    err.as_api()
        .unwrap_or_else(|| panic!("{name}: expected an API error, got {err:?}"))
        .clone()
}

#[test]
fn tokens_and_payments() {
    let token = decode::<tokens::CreateToken>("token_created.json").unwrap();
    assert!(token.token.as_str().starts_with("achq-sandbox-"));

    let bank = decode::<payments::CreatePayment>("payment_bank.json").unwrap();
    assert!(bank.token.is_some(), "a tokenized payment returns a token");
    // Express Verify ran, with a code the documentation does not list.
    let verify = bank.express_verify.unwrap();
    assert_eq!(verify.status, Some(VerificationStatus::Positive));
    assert_eq!(verify.code, Some(VerificationCode::Validated));

    let by_token = decode::<payments::CreatePayment>("payment_achq_token.json").unwrap();
    assert!(!by_token.payment_id.as_str().is_empty());

    let plaid = decode::<payments::CreatePayment>("payment_plaid_token.json").unwrap();
    let verify = plaid.express_verify.unwrap();
    assert_eq!(verify.status, None);
    assert_eq!(
        verify.description.as_deref(),
        Some("Express Verify is not executed on third party tokens")
    );
}

#[test]
fn a_rejected_account_is_declined_with_its_verification() {
    let err = api_error::<payments::CreatePayment>("payment_express_verify_reject.json");
    assert_eq!(err.command_status(), Some(&CommandStatus::Declined));
    assert_eq!(err.code(), Some(ResponseCode::FAILED_EXPRESS_VERIFY));
    assert!(err.is_account_rejected());
    let verify = err.express_verify().unwrap();
    assert_eq!(verify.status, Some(VerificationStatus::Negative));
    assert_eq!(verify.code, Some(VerificationCode::NegativeInfo));
}

#[test]
fn changes_return_their_own_ids() {
    let payment = decode::<payments::CreatePayment>("payment_bank.json")
        .unwrap()
        .payment_id;
    for name in [
        "update.json",
        "hold.json",
        "cancel.json",
        "cancel_again.json",
    ] {
        let receipt = decode::<payments::CancelPayment>(name).unwrap();
        assert_ne!(receipt.command_id.as_str(), payment.as_str(), "{name}");
    }
}

#[test]
fn refusals() {
    // The refund's `TransAct_` spelling is understood: it is refused for the
    // payment's status, which the gateway names as the error information.
    let err = api_error::<payments::RefundPayment>("refund_not_cleared.json");
    assert_eq!(err.code(), Some(ResponseCode::TRANSACTION_NOT_MODIFIABLE));
    assert_eq!(err.field(), Some("Scheduled"));

    let err = api_error::<payments::CancelPayment>("cancel_unknown.json");
    assert_eq!(err.code(), Some(ResponseCode::INVALID_REFERENCE_ID));

    let err = api_error::<payments::CancelPayment>("cancel_bad_credentials.json");
    assert!(err.is_credentials());
    assert_eq!(err.command_status(), Some(&CommandStatus::Error));
}

#[test]
fn account_insights() {
    let status = decode::<insights::VerifyAccount>("account_status.json").unwrap();
    assert!(status.is_positive());
    assert_eq!(status.express_verify.code, Some(VerificationCode::PassAv));
    assert_eq!(
        status.express_verify.bank_name.as_deref(),
        Some("Sandbox Test Bank")
    );
    assert_eq!(
        status.express_verify.account_created.unwrap().to_string(),
        "1980-01-01T00:00:00"
    );

    let owner = decode::<insights::VerifyOwner>("account_owner.json").unwrap();
    assert!(owner.is_owner_confirmed());
    assert_eq!(
        owner.owner_auth.unwrap().code,
        Some(VerificationCode::OwnerPass)
    );

    // A failed ownership check is an approved command with a negative result.
    let fails = decode::<insights::VerifyOwner>("account_owner_fails.json").unwrap();
    assert!(!fails.is_owner_confirmed());
    let auth = fails.owner_auth.unwrap();
    assert_eq!(auth.status, Some(VerificationStatus::Negative));
    assert_eq!(auth.code, Some(VerificationCode::OwnerNameMismatch));
}

#[test]
fn settlements_and_their_details() {
    let list = decode::<settlements::ListSettlements>("settlements.json").unwrap();
    assert!(!list.is_empty());
    for settlement in &list {
        // `MerchantID` is a number here; the sandbox platform has more than one.
        let merchant = settlement.merchant_id.as_ref().unwrap().as_str();
        assert!(merchant.bytes().all(|b| b.is_ascii_digit()), "{merchant}");
        assert!(!matches!(
            settlement.settlement_type,
            Some(SettlementType::Other(_)) | None
        ));
        assert!(settlement.amount.is_some() && settlement.effective_date.is_some());
    }

    let page = decode::<settlements::SettlementDetails>("settlement_details.json").unwrap();
    assert_eq!(page.settlement.direction, Some(TransferDirection::Credit));
    assert_eq!(page.page_size, Some(5));
    assert!(page.has_next_page());
    assert_eq!(page.items.len(), 5);
    for item in &page.items {
        // `Transact_ReferenceID` is a number here.
        assert!(!item.payment_id.as_ref().unwrap().as_str().is_empty());
        assert!(item.current_status.is_some());
        assert!(item.funding_direction.is_some());
    }
}

#[test]
fn events() {
    let today = decode::<events::TrackEvents>("events_today.csv").unwrap();
    assert_eq!(today.events.len(), 40);
    assert_eq!(today.command_id, None, "an approved report has no trailer");
    for event in &today.events {
        assert!(!matches!(event.event, EventName::Other(_)), "{event:?}");
        assert!(
            !matches!(event.status, PaymentStatus::Other(_)),
            "{event:?}"
        );
    }
    // The recorder's rejected payment: the event is "Failed Verification", the
    // status "Rejected".
    let rejected = today
        .events
        .iter()
        .find(|e| e.event == EventName::FailedVerification)
        .expect("the rejected payment's event");
    assert_eq!(rejected.status, PaymentStatus::Rejected);
    let verification = rejected.verification.as_ref().unwrap();
    assert_eq!(verification.code, Some(VerificationCode::NegativeInfo));

    let yesterday = decode::<events::TrackEvents>("events_yesterday.csv").unwrap();
    assert_eq!(yesterday.events.len(), 40);
}

#[test]
fn a_failed_event_query_answers_in_plain_text() {
    let err = api_error::<events::TrackEvents>("events_bad_credentials.txt");
    assert_eq!(err.code(), Some(ResponseCode::INVALID_CREDENTIALS));
    assert_eq!(err.field(), Some("Provider_Credentials"));
}

#[cfg(feature = "onboarding")]
mod onboarding {
    use achq::operations::onboarding::{CreateFeeProfile, CreateMerchant};

    use super::*;

    #[test]
    fn a_fee_profile() {
        let created = decode::<CreateFeeProfile>("fee_profile_created.json").unwrap();
        assert_eq!(created.fee_profile_id.as_str(), "36438685");
        assert_eq!(created.status.as_deref(), Some("Success"));
        assert_eq!(
            created.api_request_timestamp.as_deref(),
            Some("2026-11-05T15:55:29.127")
        );
    }

    #[test]
    fn an_invalid_application_is_an_onboarding_error() {
        let err = protocol::decode::<CreateMerchant>(&recorded("onboarding_invalid.json", 400))
            .unwrap_err();
        let Error::Onboarding(err) = err else {
            panic!("expected an onboarding error, got {err:?}");
        };
        assert_eq!(err.error_code(), Some("400"));
        assert_eq!(err.message(), "The object OnboardFunctions is required.");
        assert_eq!(
            err.to_string(),
            "HTTP 400 [400] The object OnboardFunctions is required."
        );
    }
}
