//! Tokens: an [`AchqToken`] stands in for a bank account, so that later payments
//! need not send (and your platform need not store) its numbers.
//!
//! A token can be created on its own, as here, or with a payment
//! ([`PaymentSource::tokenize`](crate::payments::PaymentSource::tokenize)).

use serde::de::IgnoredAny;

use crate::Model;
use crate::bank::BankAccount;
use crate::envelope::ExpressVerify;
use crate::error::Error;
use crate::operation::Decode;
use crate::protocol::{decode_envelope, reference_id};
use crate::transport::HttpResponse;
use crate::types::{AchqToken, CommandId, ProviderTransactionId};

/// A bank account to create a token for.
#[derive(Clone, PartialEq, Model, bon::Builder)]
#[achq(form)]
#[non_exhaustive]
pub struct TokenRequest {
    /// Your platform's ID of the request, echoed in the response.
    #[builder(into)]
    #[achq(form = "Provider_TransactionID", max_length = 50)]
    pub provider_transaction_id: Option<ProviderTransactionId>,
    /// The account.
    #[achq(flatten)]
    pub account: BankAccount,
}

impl From<BankAccount> for TokenRequest {
    fn from(account: BankAccount) -> Self {
        Self {
            provider_transaction_id: None,
            account,
        }
    }
}

/// A token the gateway created.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TokenCreated {
    /// The token. Store it as you would the account number.
    pub token: AchqToken,
    /// The gateway's ID of the request.
    pub command_id: CommandId,
    /// Your platform's ID, echoed.
    pub provider_transaction_id: Option<ProviderTransactionId>,
    /// The bank account verification's result, if one ran.
    pub express_verify: Option<ExpressVerify>,
}

impl Decode for TokenCreated {
    fn decode(response: &HttpResponse) -> Result<Self, Error> {
        let envelope = decode_envelope::<IgnoredAny>(response)?;
        let command_id = CommandId::new(reference_id(&envelope, response)?);
        let token = envelope
            .achq_token
            .filter(|token| !token.as_str().trim().is_empty())
            .ok_or_else(|| {
                Error::decode(
                    "the response has no ACHQToken",
                    response.status,
                    &response.body,
                )
            })?;
        Ok(Self {
            token,
            command_id,
            provider_transaction_id: envelope.provider_transaction_id,
            express_verify: envelope.express_verify.filter(|v| !v.is_empty()),
        })
    }
}
