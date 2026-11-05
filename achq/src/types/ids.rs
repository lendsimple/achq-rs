//! Typed IDs, so that one kind of ID cannot be passed where another is expected.
//!
//! Every ID is text on the wire. The gateway gives each command it accepts its own
//! `TransAct_ReferenceID`; [`PaymentId`] and [`CommandId`] are both read from that
//! field, and are distinct types because only the ID from creating a payment (or a
//! refund) identifies the payment afterwards. The IDs of updates, cancellations and
//! holds identify only those commands.

use crate::macros::string_id;

string_id! {
    /// The gateway's ID of a payment: the `TransAct_ReferenceID` returned when the
    /// payment (or refund) was created. Keep it to update, cancel, hold or refund
    /// the payment, and to match event tracking and settlement records.
    PaymentId;
    /// The gateway's ID of one command, such as an update or a cancellation: its
    /// `TransAct_ReferenceID`. Quote it to ACHQ support about that command; it does
    /// not identify a payment.
    CommandId;
    /// Your platform's ID of a payment (`Provider_TransactionID`), echoed in
    /// responses and reports. Up to 50 characters.
    ProviderTransactionId;
    /// The merchant's ID of a payment (`Merchant_ReferenceID`), echoed in reports.
    /// Up to 50 characters.
    MerchantReferenceId;
    /// An ACHQ merchant account (`MerchantID`).
    MerchantId;
    /// A settlement, a UUID (`SettlementID`).
    SettlementId;
    /// A fee profile created through the onboarding API.
    FeeProfileId;
    /// A merchant onboarding application.
    OnboardingId;
}

string_id! {
    sensitive:
    /// An ACHQ token standing in for a bank account (`ACHQToken`), such as
    /// `achq-sandbox-3ff6d-b64fd-5a8a5`. It can move money from or to that
    /// account, so `Debug` hides it; store it as you would the account number.
    AchqToken;
    /// A token standing in for a bank account in a payment (`AccountToken`): an
    /// [`AchqToken`], or a Plaid or MX processor token. `Debug` hides it.
    AccountToken;
}

impl From<AchqToken> for AccountToken {
    fn from(token: AchqToken) -> Self {
        Self::new(token.as_str())
    }
}

impl From<&AchqToken> for AccountToken {
    fn from(token: &AchqToken) -> Self {
        Self::new(token.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_transparent_strings() {
        let id: PaymentId = serde_json::from_str("\"14469220\"").unwrap();
        assert_eq!(id.as_str(), "14469220");
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"14469220\"");
        assert_eq!(id, PaymentId::from("14469220"));
    }

    #[test]
    fn padded_ids_are_trimmed() {
        let id: MerchantId = serde_json::from_str("\" 2001\"").unwrap();
        assert_eq!(id, MerchantId::new("2001"));
    }

    #[test]
    fn tokens_are_hidden_in_debug() {
        let token = AchqToken::new("achq-sandbox-3ff6d-b64fd-5a8a5");
        assert_eq!(format!("{token:?}"), "AchqToken([REDACTED])");
        assert_eq!(token.as_str(), "achq-sandbox-3ff6d-b64fd-5a8a5");
    }
}
