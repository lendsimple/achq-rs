//! Decoding the gateway's responses, from the samples in ACHQ's guides and the
//! orkestra client's fixtures (see `tests/data/README.md`).

use achq::envelope::{CommandStatus, ResponseCode, VerificationCode, VerificationStatus};
use achq::operations::{insights, payments, settlements, tokens};
use achq::payments::PaymentDirection;
use achq::protocol;
use achq::settlements::{SettlementType, TransferDirection};
use achq::transport::HttpResponse;
use achq::{Error, Operation, dec};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/data/responses/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn response(name: &str) -> HttpResponse {
    HttpResponse::new(
        200,
        vec![(
            "Content-Type".into(),
            "application/json; charset=utf-8".into(),
        )],
        fixture(name),
    )
}

fn decode<O: Operation>(name: &str) -> Result<O::Response, Error> {
    protocol::decode::<O>(&response(name))
}

#[test]
fn a_token() {
    let created = decode::<tokens::CreateToken>("token_created.json").unwrap();
    assert_eq!(created.token.as_str(), "achq-sandbox-3ff6d-b64fd-5a8a5");
    assert_eq!(created.command_id.as_str(), "14469220");
    assert_eq!(
        created
            .provider_transaction_id
            .as_ref()
            .map(|id| id.as_str()),
        Some("21321312")
    );
    assert_eq!(created.express_verify, None);
}

#[test]
fn a_bank_account_payment_with_a_token() {
    let created = decode::<payments::CreatePayment>("payment_bank.json").unwrap();
    assert_eq!(created.payment_id.as_str(), "14476929");
    assert_eq!(
        created.token.as_ref().map(|t| t.as_str()),
        Some("achq-sandbox-127a3-ee432-2a07e")
    );
}

#[test]
fn a_token_payment() {
    let created = decode::<payments::CreatePayment>("payment_token.json").unwrap();
    assert_eq!(created.payment_id.as_str(), "14476924");
    assert_eq!(created.token, None);
}

#[test]
fn updates_cancellations_and_holds_return_their_own_id() {
    for (name, id) in [
        ("update.json", "14477132"),
        ("cancel.json", "14477176"),
        ("hold.json", "14477210"),
    ] {
        let receipt = decode::<payments::CancelPayment>(name).unwrap();
        assert_eq!(receipt.command_id.as_str(), id, "{name}");
    }
    let update = decode::<payments::UpdatePayment>("update.json").unwrap();
    assert_eq!(update.provider_transaction_id, None);
}

#[test]
fn a_refund_is_a_new_payment() {
    let refund = decode::<payments::RefundPayment>("refund.json").unwrap();
    assert_eq!(refund.payment_id.as_str(), "14477272");
    assert_eq!(
        refund
            .provider_transaction_id
            .as_ref()
            .map(|id| id.as_str()),
        Some("1233")
    );
}

#[test]
fn settlements() {
    let list = decode::<settlements::ListSettlements>("settlements.json").unwrap();
    assert_eq!(list.len(), 2);
    let first = &list[0];
    assert_eq!(
        first.settlement_id.as_ref().unwrap().as_str(),
        "64b7811c-f631-4bc6-9eaf-d2f45356cf9e"
    );
    assert_eq!(first.settlement_type, Some(SettlementType::DebitSettlement));
    assert_eq!(first.amount.unwrap().as_decimal(), dec!(55300.19));
    assert_eq!(first.direction, Some(TransferDirection::Credit));
    assert_eq!(first.item_count, Some(21));
    assert_eq!(
        first.effective_date.unwrap().to_string(),
        "2023-12-01T00:00:00"
    );
    assert_eq!(list[1].amount.unwrap().as_decimal(), dec!(36.4));
    assert_eq!(list[1].settlement_type, Some(SettlementType::CreditOffset));
}

#[test]
fn settlement_details_as_published() {
    let page = decode::<settlements::SettlementDetails>("settlement_details.json").unwrap();
    // `" 2001"` in the sample, and `PaymentDirection` in place of
    // `TransferDirection`.
    assert_eq!(
        page.settlement.merchant_id.as_ref().unwrap().as_str(),
        "2001"
    );
    assert_eq!(page.settlement.direction, Some(TransferDirection::Credit));
    assert_eq!(page.page_number, Some(1));
    assert_eq!(page.page_size, Some(2));
    assert!(page.has_next_page());
    assert_eq!(page.items.len(), 2);
    let item = &page.items[0];
    assert_eq!(item.payment_id.as_ref().unwrap().as_str(), "18127667");
    assert_eq!(item.customer_name.as_deref(), Some("Alberta Smith"));
    assert_eq!(item.same_day_ach, Some(false));
    assert_eq!(item.amount.unwrap().as_decimal(), dec!(623.67));
    assert_eq!(item.direction, Some(PaymentDirection::FromCustomer));
    assert_eq!(
        item.merchant_reference_id.as_ref().unwrap().as_str(),
        "8c01e663-0490-408e-9760-0deb081c12ed"
    );
}

#[test]
fn bad_credentials_are_an_api_error() {
    let err = decode::<payments::CreatePayment>("declined_credentials.json").unwrap_err();
    let api = err.as_api().expect("an API error");
    assert_eq!(api.command_status(), Some(&CommandStatus::Declined));
    assert_eq!(api.code(), Some(ResponseCode::INVALID_CREDENTIALS));
    assert_eq!(api.field(), Some("Provider_Credentials"));
    assert_eq!(api.reference_id().unwrap().as_str(), "12345");
    assert!(api.is_credentials());
    assert!(api.envelope().other.contains_key("AVS"));
    assert_eq!(
        err.to_string(),
        "Declined [100] Invalid Gateway Credentials (Provider_Credentials)"
    );
}

#[test]
fn an_internal_error_may_be_retried() {
    let err = decode::<tokens::CreateToken>("internal_error.json").unwrap_err();
    let api = err.as_api().unwrap();
    assert!(api.is_retryable());
    assert_eq!(api.field(), None);
    assert_eq!(err.to_string(), "Error [600] Internal Gateway Error");
}

#[test]
fn a_rejected_account_carries_its_verification() {
    let err = decode::<payments::CreatePayment>("express_verify_rejected.json").unwrap_err();
    let verify = err.as_api().unwrap().express_verify().unwrap();
    assert_eq!(verify.status, Some(VerificationStatus::Negative));
    assert_eq!(verify.code, Some(VerificationCode::InvalidAccountNumber));
}

#[test]
fn legacy_keys_are_ignored() {
    let receipt = decode::<payments::HoldPayment>("legacy_keys.json").unwrap();
    assert_eq!(receipt.command_id.as_str(), "56789");
}

#[test]
fn verifications_keep_the_whole_envelope() {
    let response = HttpResponse::new(
        200,
        vec![],
        r#"{"CommandStatus": "Approved", "ResponseCode": "000", "TransAct_ReferenceID": "1",
            "ExpressVerify": {"Status": "POS", "Code": "1111", "Description": "Pass AV",
                              "BankName": "FIRST BANK"},
            "OwnerAuth": {"Status": "POS", "Code": "CA11", "Description": "Pass"}}"#,
    );
    let status = protocol::decode::<insights::VerifyAccount>(&response).unwrap();
    assert!(status.is_positive());
    assert_eq!(
        status.express_verify.bank_name.as_deref(),
        Some("FIRST BANK")
    );
    let owner = protocol::decode::<insights::VerifyOwner>(&response).unwrap();
    assert!(owner.is_owner_confirmed());
    assert_eq!(
        owner.owner_auth.unwrap().code,
        Some(VerificationCode::OwnerPass)
    );
}

#[test]
fn an_approved_response_without_its_id_cannot_be_read() {
    let response = HttpResponse::new(200, vec![], r#"{"CommandStatus": "Approved"}"#);
    let err = protocol::decode::<payments::CreatePayment>(&response).unwrap_err();
    assert!(matches!(&err, Error::Decode { status: 200, .. }), "{err:?}");
    assert!(err.to_string().contains("no TransAct_ReferenceID"), "{err}");
}

#[test]
fn an_html_page_is_not_json() {
    let page = HttpResponse::new(
        200,
        vec![("Content-Type".into(), "text/html".into())],
        fixture("iis_error.html"),
    );
    let err = protocol::decode::<payments::CreatePayment>(&page).unwrap_err();
    match err {
        Error::UnexpectedContentType {
            status,
            expected,
            content_type,
            body,
        } => {
            assert_eq!(
                (status, expected, content_type.as_str()),
                (200, "JSON", "text/html")
            );
            assert!(body.contains("Runtime Error"));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn json_labeled_as_html_is_still_read() {
    let mut mislabeled = response("cancel.json");
    mislabeled.headers = vec![("Content-Type".into(), "text/html".into())];
    assert!(protocol::decode::<payments::CancelPayment>(&mislabeled).is_ok());
}

#[test]
fn error_statuses_keep_the_body() {
    let response = HttpResponse::new(503, vec![], "Service Unavailable");
    let err = protocol::decode::<payments::CreatePayment>(&response).unwrap_err();
    match err {
        Error::Status { status, body } => {
            assert_eq!((status, body.as_str()), (503, "Service Unavailable"));
        }
        other => panic!("unexpected {other:?}"),
    }
}
