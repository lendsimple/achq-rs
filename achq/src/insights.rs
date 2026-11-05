//! Account insights: whether a bank account is open and in good standing, and
//! whether it belongs to the person or business named.
//!
//! NACHA requires WEB debits to validate an account the first time it is used.
//! These checks do that in real time, without micro-deposits.
//!
//! Both are the gateway's `ECheck.VerifyPlus` command; the ownership check adds
//! `OwnerAuth=On`. The gateway documents no sample response for them, so
//! [`AccountVerification`] and [`OwnerVerification`] keep the whole
//! [`Envelope`] too.

use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::Model;
use crate::envelope::{Envelope, ExpressVerify, VerificationCode, VerificationStatus};
use crate::error::Error;
use crate::operation::Decode;
use crate::protocol::{decode_envelope, reference_id};
use crate::transport::HttpResponse;
use crate::types::{CommandId, ProviderTransactionId};

/// A bank account to check the status of.
#[derive(Clone, PartialEq, Model, bon::Builder)]
#[builder(on(String, into))]
#[achq(form)]
#[non_exhaustive]
pub struct AccountStatusRequest {
    /// Your platform's ID of the request, echoed in the response.
    #[builder(into)]
    #[achq(form = "Provider_TransactionID", max_length = 50)]
    pub provider_transaction_id: Option<ProviderTransactionId>,
    #[achq(form = "RoutingNumber", rule = "routing_number")]
    pub routing_number: String,
    #[achq(form = "AccountNumber", sensitive, max_length = 30)]
    pub account_number: String,
}

/// A bank account and who should own it, to check that they do.
///
/// Name a person (`customer_name`) for a personal account, or a company for a
/// business one. The more of the address, phone and tax ID given, the more the
/// check can confirm.
#[derive(Clone, PartialEq, Model, bon::Builder)]
#[builder(on(String, into))]
#[achq(form)]
#[non_exhaustive]
pub struct OwnerAuthRequest {
    #[builder(into)]
    #[achq(form = "Provider_TransactionID", max_length = 50)]
    pub provider_transaction_id: Option<ProviderTransactionId>,
    #[achq(form = "RoutingNumber", rule = "routing_number")]
    pub routing_number: String,
    #[achq(form = "AccountNumber", sensitive, max_length = 30)]
    pub account_number: String,
    /// The person on a personal account.
    #[achq(form = "CustomerName")]
    pub customer_name: Option<String>,
    /// The company on a business account.
    #[achq(form = "Company")]
    pub company: Option<String>,
    #[achq(form = "Address1")]
    pub address1: Option<String>,
    #[achq(form = "Address2")]
    pub address2: Option<String>,
    #[achq(form = "City")]
    pub city: Option<String>,
    #[achq(form = "State")]
    pub state: Option<String>,
    #[achq(form = "Zip", rule = "zip")]
    pub zip: Option<String>,
    /// `US` or `CA`.
    #[achq(form = "Country", min_length = 2, max_length = 2)]
    pub country: Option<String>,
    /// Ten digits, without the country code.
    #[achq(
        form = "Phone Number",
        min_length = 10,
        max_length = 10,
        rule = "digits"
    )]
    pub phone: Option<String>,
    /// A Social Security number (or its last four digits), or an Employer
    /// Identification Number.
    #[achq(form = "TaxID", sensitive, rule = "digits")]
    pub tax_id: Option<String>,
}

/// The status of a bank account.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct AccountVerification {
    /// The gateway's ID of the request.
    pub command_id: CommandId,
    /// What the verification network knows of the account.
    pub express_verify: ExpressVerify,
    /// The whole response, for fields this type does not name yet.
    pub envelope: Envelope<IgnoredAny>,
}

impl AccountVerification {
    /// Whether the account was found open and in good standing.
    pub fn is_positive(&self) -> bool {
        self.express_verify.status == Some(VerificationStatus::Positive)
    }
}

impl Decode for AccountVerification {
    fn decode(response: &HttpResponse) -> Result<Self, Error> {
        let envelope = decode_envelope::<IgnoredAny>(response)?;
        Ok(Self {
            command_id: CommandId::new(reference_id(&envelope, response)?),
            express_verify: envelope.express_verify.clone().unwrap_or_default(),
            envelope,
        })
    }
}

/// The result of an ownership check (`OwnerAuth`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct OwnerAuth {
    #[serde(rename = "Status", default)]
    pub status: Option<VerificationStatus>,
    #[serde(rename = "Code", default)]
    pub code: Option<VerificationCode>,
    #[serde(rename = "Description", default)]
    pub description: Option<String>,
}

/// The status of a bank account, and whether the owner named owns it.
///
/// The owner is checked only when the account passes verification (codes 1111,
/// 5555, RT02 or RT03) at a bank on the real-time network; otherwise
/// [`owner_auth`](Self::owner_auth) is unknown or missing.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct OwnerVerification {
    /// The gateway's ID of the request.
    pub command_id: CommandId,
    /// What the verification network knows of the account.
    pub express_verify: ExpressVerify,
    /// The ownership check, read from the response's `OwnerAuth` object.
    pub owner_auth: Option<OwnerAuth>,
    /// The whole response, for fields this type does not name yet.
    pub envelope: Envelope<IgnoredAny>,
}

impl OwnerVerification {
    /// Whether the owner was confirmed (perhaps with a minor mismatch, such as the
    /// address).
    pub fn is_owner_confirmed(&self) -> bool {
        self.owner_auth
            .as_ref()
            .is_some_and(|auth| auth.status == Some(VerificationStatus::Positive))
    }
}

impl Decode for OwnerVerification {
    fn decode(response: &HttpResponse) -> Result<Self, Error> {
        let envelope = decode_envelope::<IgnoredAny>(response)?;
        let owner_auth = match envelope.other.get("OwnerAuth") {
            None | Some(serde_json::Value::Null) => None,
            Some(value) => Some(
                serde_json::from_value(value.clone())
                    .map_err(|e| Error::decode(e, response.status, &response.body))?,
            ),
        };
        Ok(Self {
            command_id: CommandId::new(reference_id(&envelope, response)?),
            express_verify: envelope.express_verify.clone().unwrap_or_default(),
            owner_auth,
            envelope,
        })
    }
}
