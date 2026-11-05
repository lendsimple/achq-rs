//! Settlements: the transfers into and out of merchants' bank accounts that their
//! payments add up to, for reconciling payments with bank statements.
//!
//! [`SettlementQuery`] lists the settlements in a date range;
//! [`SettlementDetailsQuery`] lists the payments in one, a page at a time. Both
//! need only the platform's credentials.

use serde::Deserialize;
use time::Date;

use crate::Model;
use crate::error::Error;
use crate::events::PaymentStatus;
use crate::macros::string_enum;
use crate::operation::Decode;
use crate::payments::PaymentDirection;
use crate::protocol::decode_envelope;
use crate::transport::HttpResponse;
use crate::types::{
    Amount, LocalDateTime, MerchantId, MerchantReferenceId, PaymentId, ProviderTransactionId,
    SettlementId,
};

/// The largest page of settlement details the gateway returns.
pub const MAX_PAGE_SIZE: u16 = 500;

/// The settlements of a date range.
#[derive(Clone, PartialEq, Model, bon::Builder)]
#[achq(form)]
#[non_exhaustive]
pub struct SettlementQuery {
    /// The first day of the range.
    #[achq(form = "DateFrom", with = "iso_date")]
    pub date_from: Date,
    /// The last day of the range.
    #[achq(form = "DateTo", with = "iso_date")]
    pub date_to: Date,
    /// One merchant's settlements only; every merchant of the platform's if not
    /// set.
    #[builder(into)]
    #[achq(form = "MerchantID")]
    pub merchant_id: Option<MerchantId>,
}

impl SettlementQuery {
    /// Every merchant's settlements from `date_from` to `date_to`.
    pub fn between(date_from: Date, date_to: Date) -> Self {
        Self {
            date_from,
            date_to,
            merchant_id: None,
        }
    }

    /// Only `merchant_id`'s settlements.
    pub fn merchant(mut self, merchant_id: impl Into<MerchantId>) -> Self {
        self.merchant_id = Some(merchant_id.into());
        self
    }
}

/// A page of the payments in a settlement.
#[derive(Clone, PartialEq, Model, bon::Builder)]
#[achq(form)]
#[non_exhaustive]
pub struct SettlementDetailsQuery {
    /// The settlement, from [`Settlement::settlement_id`].
    #[builder(into)]
    #[achq(form = "SettlementID", max_length = 50)]
    pub settlement_id: SettlementId,
    /// Payments per page, up to [`MAX_PAGE_SIZE`]; the gateway's default is 500.
    #[achq(form = "PageSize", min = "1", max = "500")]
    pub page_size: Option<u16>,
    /// The page, from 1.
    #[achq(form = "PageNumber", min = "1")]
    pub page_number: Option<u16>,
}

impl SettlementDetailsQuery {
    /// The first page of `settlement_id`'s payments, of the largest size.
    pub fn new(settlement_id: impl Into<SettlementId>) -> Self {
        Self {
            settlement_id: settlement_id.into(),
            page_size: Some(MAX_PAGE_SIZE),
            page_number: Some(1),
        }
    }

    /// The page `page_number`.
    pub fn page(mut self, page_number: u16) -> Self {
        self.page_number = Some(page_number);
        self
    }
}

string_enum! {
    case_insensitive;
    /// Which way a settlement moves money, from the merchant's point of view.
    pub enum TransferDirection {
        /// ACHQ deposits funds in the merchant's account.
        Credit = "Credit",
        /// ACHQ withdraws funds from the merchant's account.
        Debit = "Debit",
    }
}

string_enum! {
    case_insensitive;
    /// Why a settlement moved money.
    pub enum SettlementType {
        /// The merchant's cleared debits (charges), deposited.
        DebitSettlement = "Settlement for ACH Debits",
        /// The funds for the merchant's credits (payouts and refunds), withdrawn.
        CreditOffset = "Offset for ACH Credits",
        /// Debits returned after they were settled, withdrawn.
        ChargeBack = "Charge Back for Late Disputes/Returns",
        /// The funds for credits that failed, deposited back.
        OffsetReturn = "Offset Return",
    }
}

/// A transfer into or out of a merchant's bank account.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct Settlement {
    #[serde(rename = "SettlementID", default)]
    pub settlement_id: Option<SettlementId>,
    #[serde(rename = "MerchantID", default)]
    pub merchant_id: Option<MerchantId>,
    #[serde(rename = "SettlementType", default)]
    pub settlement_type: Option<SettlementType>,
    /// The day the transfer should post to the merchant's account.
    #[serde(rename = "EffectiveDate", default)]
    pub effective_date: Option<LocalDateTime>,
    #[serde(rename = "Amount", default)]
    pub amount: Option<Amount>,
    #[serde(rename = "TransferDirection", alias = "PaymentDirection", default)]
    pub direction: Option<TransferDirection>,
    /// How many payments the settlement covers.
    #[serde(rename = "ItemCount", default)]
    pub item_count: Option<u32>,
    /// A transfer ID that may appear on the merchant's bank statement.
    #[serde(rename = "BankDescriptorID", default)]
    pub bank_descriptor_id: Option<String>,
}

impl Decode for Vec<Settlement> {
    fn decode(response: &HttpResponse) -> Result<Self, Error> {
        let envelope = decode_envelope::<Vec<Settlement>>(response)?;
        Ok(envelope.response_data.unwrap_or_default())
    }
}

/// One payment in a settlement.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct SettlementItem {
    /// The payment's ID, as returned when it was created.
    #[serde(
        rename = "Transact_ReferenceID",
        alias = "TransAct_ReferenceID",
        default
    )]
    pub payment_id: Option<PaymentId>,
    #[serde(rename = "CustomerName", default)]
    pub customer_name: Option<String>,
    #[serde(rename = "Company", default)]
    pub company: Option<String>,
    /// The day the Federal Reserve processed the payment.
    #[serde(rename = "ProcessDate", default)]
    pub process_date: Option<LocalDateTime>,
    /// Whether it was processed as a Same-Day ACH payment.
    #[serde(rename = "SameDayACH", default)]
    pub same_day_ach: Option<bool>,
    #[serde(rename = "Amount", default)]
    pub amount: Option<Amount>,
    #[serde(rename = "PaymentDirection", default)]
    pub direction: Option<PaymentDirection>,
    #[serde(rename = "Description", default)]
    pub description: Option<String>,
    #[serde(
        rename = "MerchantReferenceID",
        alias = "Merchant_ReferenceID",
        default
    )]
    pub merchant_reference_id: Option<MerchantReferenceId>,
    #[serde(rename = "Provider_TransactionID", default)]
    pub provider_transaction_id: Option<ProviderTransactionId>,
    /// The payment's status now. Undocumented; the gateway sends it.
    #[serde(rename = "CurrentStatus", default)]
    pub current_status: Option<PaymentStatus>,
    /// What the payment's funds were for, such as `settlement`. Undocumented.
    #[serde(rename = "funding_action", default)]
    pub funding_action: Option<String>,
    /// Which way the payment's funds moved, such as `deposit`. Undocumented.
    #[serde(rename = "funding_direction", default)]
    pub funding_direction: Option<String>,
}

/// A page of the payments in a settlement, with the settlement's summary.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct SettlementDetailsPage {
    #[serde(flatten)]
    pub settlement: Settlement,
    #[serde(rename = "PageNumber", default)]
    pub page_number: Option<u32>,
    #[serde(rename = "PageSize", default)]
    pub page_size: Option<u32>,
    /// The payments on this page.
    #[serde(rename = "SettlementItems", default)]
    pub items: Vec<SettlementItem>,
}

impl SettlementDetailsPage {
    /// Whether the settlement has payments beyond this page.
    pub fn has_next_page(&self) -> bool {
        match (self.page_number, self.page_size, self.settlement.item_count) {
            (Some(number), Some(size), Some(count)) => {
                u64::from(number) * u64::from(size) < u64::from(count)
            }
            // Without the counts, a full page may be followed by another.
            _ => self
                .page_size
                .is_some_and(|size| self.items.len() as u64 >= u64::from(size) && size > 0),
        }
    }

    /// The query for the next page, if [`has_next_page`](Self::has_next_page).
    pub fn next_page(&self, query: &SettlementDetailsQuery) -> Option<SettlementDetailsQuery> {
        if !self.has_next_page() {
            return None;
        }
        let current = self
            .page_number
            .and_then(|n| u16::try_from(n).ok())
            .or(query.page_number)
            .unwrap_or(1);
        Some(query.clone().page(current.checked_add(1)?))
    }
}

impl Decode for SettlementDetailsPage {
    fn decode(response: &HttpResponse) -> Result<Self, Error> {
        let envelope = decode_envelope::<SettlementDetailsPage>(response)?;
        Ok(envelope.response_data.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use time::macros::date;

    use super::*;
    use crate::form::ToForm;
    use crate::validate::Validate;

    #[test]
    fn queries_write_their_fields() {
        let query =
            SettlementQuery::between(date!(2023 - 12 - 01), date!(2023 - 12 - 08)).merchant("2001");
        assert_eq!(
            query.to_form().encode(),
            "DateFrom=2023-12-01&DateTo=2023-12-08&MerchantID=2001"
        );
        let details = SettlementDetailsQuery::new("64b7811c").page(2);
        assert_eq!(
            details.to_form().encode(),
            "SettlementID=64b7811c&PageSize=500&PageNumber=2"
        );
    }

    #[test]
    fn page_sizes_are_bounded() {
        let mut details = SettlementDetailsQuery::new("x");
        details.page_size = Some(501);
        details.page_number = Some(0);
        let messages: Vec<String> = details
            .validate()
            .unwrap_err()
            .violations()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            messages,
            [
                "page_size: must be at most 500, not 501",
                "page_number: must be at least 1, not 0"
            ]
        );
    }

    #[test]
    fn pages_follow_the_item_count() {
        let page = |number, size, count| SettlementDetailsPage {
            settlement: Settlement {
                item_count: Some(count),
                ..Settlement::default()
            },
            page_number: Some(number),
            page_size: Some(size),
            items: Vec::new(),
        };
        let query = SettlementDetailsQuery::new("x");
        assert!(page(1, 2, 21).has_next_page());
        assert_eq!(
            page(1, 2, 21).next_page(&query).unwrap().page_number,
            Some(2)
        );
        assert!(!page(11, 2, 21).has_next_page());
        assert!(!page(1, 500, 21).has_next_page());
        assert_eq!(page(1, 500, 21).next_page(&query), None);
    }
}
