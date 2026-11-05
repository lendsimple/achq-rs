//! Event tracking: what happened to payments on a given day.
//!
//! ACH payments settle over days, so their outcomes (cleared, returned, charged
//! back) arrive as events. A query for a day returns only the payments that had
//! an event that day, which spares checking every payment for months.
//!
//! ACHQ recommends querying at 9 AM, 1 PM, 7 PM and midnight Eastern, and at least
//! once after midnight Central for the whole previous day. Returns can arrive up
//! to 60 days after a payment.
//!
//! The gateway answers this command in CSV, not JSON: one row per event, then a
//! `Command Response` row with the command's status.

use serde::de::IgnoredAny;
use time::Date;

use crate::Model;
use crate::envelope::{Envelope, VerificationCode, VerificationStatus};
use crate::error::{ApiError, Error, body_excerpt};
use crate::macros::string_enum;
use crate::operation::Decode;
use crate::protocol::{decode_envelope, is_json_object};
use crate::transport::HttpResponse;
use crate::types::{CommandId, LocalDateTime, MerchantId, PaymentId, ProviderTransactionId};

/// The events of a day.
#[derive(Clone, PartialEq, Model, bon::Builder)]
#[achq(form)]
#[non_exhaustive]
pub struct EventQuery {
    /// The day the events happened.
    #[achq(form = "TrackingDate", with = "mdy")]
    pub tracking_date: Date,
    /// One merchant's events only; every merchant of the platform's if not set.
    #[builder(into)]
    #[achq(form = "MerchantID")]
    pub merchant_id: Option<MerchantId>,
}

impl EventQuery {
    /// Every merchant's events on `tracking_date`.
    pub fn on(tracking_date: Date) -> Self {
        Self {
            tracking_date,
            merchant_id: None,
        }
    }

    /// Only `merchant_id`'s events.
    pub fn merchant(mut self, merchant_id: impl Into<MerchantId>) -> Self {
        self.merchant_id = Some(merchant_id.into());
        self
    }
}

string_enum! {
    case_insensitive;
    /// What happened to a payment.
    pub enum EventName {
        /// The payment was created: now `Scheduled`.
        Created = "Created",
        /// The payment was cancelled before it was sent.
        Cancelled = "Cancelled",
        /// The bank account failed verification: now `Rejected`.
        FailedVerification = "Failed Verification",
        /// The payment was rejected before it was sent.
        Rejected = "Rejected",
        /// The payment was sent to the Federal Reserve: now `In-Process`.
        Submitted = "Submitted",
        /// The customer's bank returned the payment for insufficient funds.
        ReturnedNsf = "Returned-NSF",
        /// The customer's bank returned the payment for another reason, such as a
        /// closed or frozen account.
        ReturnedOther = "Returned-Other",
        /// The payment cleared, and the merchant or receiver was paid.
        Cleared = "Cleared",
        /// A return arrived after the payment had been settled.
        ChargedBack = "Charged Back",
        /// The merchant or platform held the payment.
        HeldByMerchant = "Held by Merchant",
        /// ACHQ held the payment for review.
        HeldByProcessor = "Held by Processor",
        /// A hold was removed.
        HoldReleased = "Hold Released",
        /// Undocumented; seen in ACHQ's own sample, resulting in `Authorized`.
        Authorization = "Authorization",
    }
}

string_enum! {
    case_insensitive;
    /// A payment's status after an event.
    pub enum PaymentStatus {
        /// Waiting to be sent to the Federal Reserve. It can still be updated,
        /// cancelled or held.
        Scheduled = "Scheduled",
        /// Cancelled before it was sent.
        Cancelled = "Cancelled",
        /// Its bank account failed verification.
        Rejected = "Rejected",
        /// Its bank account failed verification (as some responses name it).
        FailedVerification = "Failed Verification",
        /// Sent to the Federal Reserve.
        InProcess = "In-Process",
        /// Returned for insufficient funds. A new payment may be tried.
        ReturnedNsf = "Returned-NSF",
        /// Returned for another reason; contact the customer.
        ReturnedOther = "Returned-Other",
        /// Paid. It can be refunded, and can still be charged back.
        Cleared = "Cleared",
        /// Returned after it was settled; contact the customer.
        ChargedBack = "Charged Back",
        /// Held by the merchant or platform. It can still be updated or
        /// cancelled.
        MerchantHold = "Merchant Hold",
        /// Held by ACHQ for review. It can still be cancelled.
        ProcessorHold = "Processor Hold",
        /// A hold was removed (a momentary status).
        HoldReleased = "Hold Released",
        /// Undocumented; seen in ACHQ's own sample.
        Authorized = "Authorized",
    }
}

impl PaymentStatus {
    /// Whether the customer's bank returned the payment, before or after it was
    /// settled.
    pub fn is_returned(&self) -> bool {
        matches!(
            self,
            Self::ReturnedNsf | Self::ReturnedOther | Self::ChargedBack
        )
    }

    /// Whether the payment can still be updated, cancelled or held.
    pub fn can_modify(&self) -> bool {
        matches!(self, Self::Scheduled | Self::MerchantHold)
    }

    /// Whether the payment can be refunded.
    pub fn can_refund(&self) -> bool {
        matches!(self, Self::Cleared)
    }
}

string_enum! {
    case_insensitive;
    /// Why a customer's bank returned a payment (the NACHA return code). ACHQ
    /// documents the most common; others are kept as they are sent.
    pub enum ReturnCode {
        /// Insufficient funds.
        R01 = "R01",
        /// Account closed.
        R02 = "R02",
        /// No account, or unable to locate the account.
        R03 = "R03",
        /// Invalid account number.
        R04 = "R04",
        /// Unauthorized debit to a consumer account using a corporate SEC code.
        R05 = "R05",
        /// Authorization revoked by the customer.
        R07 = "R07",
        /// Payment stopped.
        R08 = "R08",
        /// Uncollected funds.
        R09 = "R09",
        /// The customer does not know the originator, or did not authorize the
        /// debit.
        R10 = "R10",
        /// The debit was not in accordance with the terms of the authorization.
        R11 = "R11",
        /// Account frozen.
        R16 = "R16",
        /// Non-transaction account.
        R20 = "R20",
        /// The corporate customer advises the debit was not authorized.
        R29 = "R29",
    }
}

/// The bank account verification reported with an event, if one ran.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EventVerification {
    pub status: Option<VerificationStatus>,
    pub code: Option<VerificationCode>,
    pub description: Option<String>,
}

/// One event of a payment.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct PaymentEvent {
    /// The payment, by the ID returned when it was created.
    pub payment_id: PaymentId,
    /// Your platform's ID of the payment.
    pub provider_transaction_id: Option<ProviderTransactionId>,
    pub merchant_id: Option<MerchantId>,
    /// What happened.
    pub event: EventName,
    /// When, in US Central time.
    pub occurred_at: LocalDateTime,
    /// The payment's status after the event.
    pub status: PaymentStatus,
    /// For a returned or charged back payment, why.
    pub return_code: Option<ReturnCode>,
    /// The bank's explanation of the return.
    pub return_explanation: Option<String>,
    pub verification: Option<EventVerification>,
}

/// The events of an [`EventQuery`], in the order the gateway lists them.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct EventBatch {
    pub events: Vec<PaymentEvent>,
    /// The gateway's ID of the query, from its `Command Response` row.
    pub command_id: Option<CommandId>,
}

impl Decode for EventBatch {
    fn decode(response: &HttpResponse) -> Result<Self, Error> {
        if is_json_object(&response.body) {
            // An error, or an empty result, as a JSON envelope.
            let envelope = decode_envelope::<IgnoredAny>(response)?;
            return Ok(Self {
                events: Vec::new(),
                command_id: envelope.reference().map(CommandId::from),
            });
        }
        if response.body.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'<') {
            return Err(Error::UnexpectedContentType {
                status: response.status,
                expected: "CSV",
                content_type: response
                    .content_type()
                    .unwrap_or_else(|| "an unlabeled body".to_owned()),
                body: body_excerpt(&response.body),
            });
        }
        parse_csv(response)
    }
}

/// The first field of the row with the command's status.
const TRAILER: &str = "Command Response";

fn parse_csv(response: &HttpResponse) -> Result<EventBatch, Error> {
    let fail = |message: String| Error::decode(message, response.status, &response.body);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(response.body.as_slice());
    let mut batch = EventBatch::default();
    for (index, record) in reader.records().enumerate() {
        let record = record.map_err(|e| Error::decode(e, response.status, &response.body))?;
        let line = index + 1;
        let cell = |i: usize| record.get(i).filter(|v| !v.is_empty());
        if record.iter().all(str::is_empty) {
            continue;
        }
        if cell(0).is_some_and(|first| first.eq_ignore_ascii_case(TRAILER)) {
            let envelope = trailer_envelope(&record);
            batch.command_id = envelope.reference().map(CommandId::from);
            if !envelope.is_approved() {
                return Err(ApiError::new(response.status, envelope).into());
            }
            continue;
        }
        if record.len() < 6 {
            return Err(fail(format!(
                "line {line}: expected at least 6 fields, found {}",
                record.len()
            )));
        }
        // ACHQ documents 11 fields, with the return's explanation after its code;
        // older responses have 10, without the explanation.
        let (explanation, verification) = if record.len() == 10 {
            (None, 7)
        } else {
            (cell(7), 8)
        };
        let required = |i: usize, name: &str| {
            cell(i).ok_or_else(|| fail(format!("line {line}: the {name} is missing")))
        };
        let occurred_at = required(4, "event time")?;
        let occurred_at = occurred_at
            .parse()
            .map_err(|e| fail(format!("line {line}: {e}")))?;
        let verification = EventVerification {
            status: cell(verification).map(VerificationStatus::from),
            code: cell(verification + 1).map(VerificationCode::from),
            description: cell(verification + 2).map(str::to_owned),
        };
        batch.events.push(PaymentEvent {
            payment_id: PaymentId::new(required(0, "payment ID")?),
            provider_transaction_id: cell(1).map(ProviderTransactionId::from),
            merchant_id: cell(2).map(MerchantId::from),
            event: EventName::from(required(3, "event name")?),
            occurred_at,
            status: PaymentStatus::from(required(5, "resulting status")?),
            return_code: cell(6).map(ReturnCode::from),
            return_explanation: explanation.map(str::to_owned),
            verification: (verification != EventVerification::default()).then_some(verification),
        });
    }
    Ok(batch)
}

/// The `Command Response,<status>,<code>,<description>,<error>,<reference ID>`
/// row, as an envelope.
fn trailer_envelope(record: &csv::StringRecord) -> Envelope<serde_json::Value> {
    let cell = |i: usize| record.get(i).filter(|v| !v.is_empty());
    let json = serde_json::json!({
        "CommandStatus": cell(1),
        "ResponseCode": cell(2),
        "Description": cell(3),
        "ErrorInformation": cell(4),
        "TransAct_ReferenceID": cell(5),
    });
    serde_json::from_value(json).unwrap_or_else(|_| {
        // A malformed response code: keep the rest.
        let mut envelope: Envelope<serde_json::Value> =
            serde_json::from_value(serde_json::json!({})).expect("an empty envelope reads");
        envelope.command_status = cell(1).map(Into::into);
        envelope.description = cell(3).map(str::to_owned);
        envelope.error_information = cell(4).map(str::to_owned);
        envelope.reference_id = cell(5).map(str::to_owned);
        envelope
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_ignore_case_and_know_what_they_allow() {
        assert_eq!(PaymentStatus::from("In-process"), PaymentStatus::InProcess);
        assert!(PaymentStatus::from("charged back").is_returned());
        assert!(PaymentStatus::MerchantHold.can_modify());
        assert!(!PaymentStatus::InProcess.can_modify());
        assert!(PaymentStatus::Cleared.can_refund());
        assert_eq!(
            EventName::from("HELD BY MERCHANT"),
            EventName::HeldByMerchant
        );
        assert_eq!(ReturnCode::from("r01"), ReturnCode::R01);
        assert_eq!(ReturnCode::from("R85"), ReturnCode::Other("R85".into()));
    }

    #[test]
    fn queries_write_their_fields() {
        use crate::form::ToForm;
        let query = EventQuery::on(time::macros::date!(2022 - 06 - 17)).merchant("2001");
        assert_eq!(
            query.to_form().encode(),
            "TrackingDate=06172022&MerchantID=2001"
        );
    }
}
