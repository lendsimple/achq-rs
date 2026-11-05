//! Fee profiles: the fees ACHQ charges a merchant for its services.

use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize, Serializer};

use crate::Decimal;
use crate::Model;
use crate::error::Error;
use crate::operation::Decode;
use crate::protocol::decode_json;
use crate::transport::HttpResponse;
use crate::types::{Amount, FeeProfileId};

/// A merchant's fees: created on its own with
/// [`create_fee_profile`](crate::operations::onboarding::CreateFeeProfile) and
/// assigned by ID, or given inline with an application.
///
/// Every fee is sent, as zero (not charged) unless set: the API requires them
/// all, although its schema marks them optional.
#[derive(Clone, Default, PartialEq, Serialize, Model, bon::Builder)]
#[serde(rename_all = "camelCase")]
#[builder(on(String, into))]
#[non_exhaustive]
pub struct FeeSchedule {
    /// A name for the profile.
    #[serde(rename = "feeName", skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The monthly service fee.
    #[builder(into, default)]
    pub monthly_fee: Amount,
    /// The fee for each payment.
    #[builder(into, default)]
    pub transaction_fee: Amount,
    /// The fee for each returned payment.
    #[builder(into, default)]
    pub return_fee: Amount,
    /// The fee for each disputed payment.
    #[builder(into, default)]
    pub dispute_fee: Amount,
    /// A percentage of each payment's amount, such as `0.75` for 0.75%.
    #[builder(default)]
    #[achq(min = "0", max = "100")]
    #[serde(serialize_with = "percent")]
    pub percent_fee: Decimal,
    /// The most the percentage fee charges for one payment.
    #[builder(into, default)]
    pub percent_cap: Amount,
    /// The fee for each account status check.
    #[builder(into, default)]
    pub bank_status_fee: Amount,
    /// The fee for each account owner check.
    #[builder(into, default)]
    pub bank_id_fee: Amount,
    /// Charged if the merchant's account is activated.
    #[builder(into, default)]
    pub setup_fee: Amount,
    /// Free-form key-value pairs to keep with the profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta_data: Option<String>,
}

/// A percentage, sent as a JSON number.
fn percent<S: Serializer>(value: &Decimal, serializer: S) -> Result<S::Ok, S::Error> {
    let number = value
        .to_f64()
        .ok_or_else(|| serde::ser::Error::custom(format!("{value} does not fit a JSON number")))?;
    serializer.serialize_f64(number)
}

/// A fee profile the API created.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FeeProfileCreated {
    /// The profile's ID, to assign it to merchants.
    pub fee_profile_id: FeeProfileId,
    pub status: Option<String>,
    /// The API's ID of the request, to quote to ACHQ support.
    pub api_request_id: Option<String>,
    /// When the API handled the request, as it wrote it.
    pub api_request_timestamp: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FeeProfileResponse {
    #[serde(default)]
    fee_profile_id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    api_request_id: Option<String>,
    #[serde(default)]
    api_request_time_stamp: Option<String>,
}

impl Decode for FeeProfileCreated {
    fn decode(response: &HttpResponse) -> Result<Self, Error> {
        let body: FeeProfileResponse = decode_json(response)?;
        let fee_profile_id = body
            .fee_profile_id
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| {
                Error::decode(
                    "the response has no feeProfileId",
                    response.status,
                    &response.body,
                )
            })?;
        Ok(Self {
            fee_profile_id: FeeProfileId::new(fee_profile_id),
            status: body.status,
            api_request_id: body.api_request_id,
            api_request_timestamp: body.api_request_time_stamp,
        })
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;
    use serde_json::json;

    use super::*;
    use crate::validate::Validate;

    #[test]
    fn writes_every_fee() {
        let fees = FeeSchedule::builder()
            .name("Standard")
            .transaction_fee(dec!(0.25))
            .percent_fee(dec!(0.75))
            .percent_cap(dec!(10))
            .build();
        assert_eq!(
            serde_json::to_value(&fees).unwrap(),
            json!({
                "feeName": "Standard", "monthlyFee": 0.0, "transactionFee": 0.25,
                "returnFee": 0.0, "disputeFee": 0.0, "percentFee": 0.75, "percentCap": 10.0,
                "bankStatusFee": 0.0, "bankIdFee": 0.0, "setupFee": 0.0
            })
        );
    }

    #[test]
    fn validates_amounts_and_the_percentage() {
        let fees = FeeSchedule::builder()
            .transaction_fee(dec!(0.255))
            .percent_fee(dec!(101))
            .build();
        let messages: Vec<String> = fees
            .validate()
            .unwrap_err()
            .violations()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            messages,
            [
                "transaction_fee: must have at most 2 decimal places, not 0.255",
                "percent_fee: must be at most 100, not 101",
            ]
        );
    }
}
