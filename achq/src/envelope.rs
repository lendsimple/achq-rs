//! The JSON envelope every gateway command answers with, and its parts.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::macros::string_enum;
use crate::types::{AchqToken, LocalDateTime, ProviderTransactionId};

/// The gateway's answer to a command, before it is read as the command's result.
///
/// Every field is optional: the gateway leaves some out (the settlement details
/// sample has no `ResponseCode`). `response_data` holds the command's own result,
/// such as the settlements of a report.
///
/// Keys the envelope does not name, such as legacy `AVS`, `CVN` and `PaymentKey`
/// objects, are kept in [`other`](Self::other).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct Envelope<D> {
    /// Whether the command succeeded.
    #[serde(rename = "CommandStatus", default)]
    pub command_status: Option<CommandStatus>,
    /// Why the command succeeded or failed.
    #[serde(rename = "ResponseCode", default)]
    pub response_code: Option<ResponseCode>,
    /// The description of the response code.
    #[serde(rename = "Description", default)]
    pub description: Option<String>,
    /// What caused a failure, often the name of the offending field.
    #[serde(rename = "ErrorInformation", default)]
    pub error_information: Option<String>,
    /// The bank account verification's result, if one ran.
    #[serde(rename = "ExpressVerify", default)]
    pub express_verify: Option<ExpressVerify>,
    /// The command's own result.
    #[serde(rename = "ResponseData", default = "none")]
    pub response_data: Option<D>,
    /// A token for the bank account, when one was asked for.
    #[serde(rename = "ACHQToken", default)]
    pub achq_token: Option<AchqToken>,
    /// The platform's ID sent with the command, echoed.
    #[serde(rename = "Provider_TransactionID", default)]
    pub provider_transaction_id: Option<ProviderTransactionId>,
    /// The ID the gateway gave this command (`TransAct_ReferenceID`).
    #[serde(
        rename = "TransAct_ReferenceID",
        alias = "Transact_ReferenceID",
        default
    )]
    pub reference_id: Option<String>,
    /// Every other key, as sent.
    #[serde(flatten)]
    pub other: BTreeMap<String, serde_json::Value>,
}

fn none<D>() -> Option<D> {
    None
}

impl<D> Envelope<D> {
    /// Whether the command was approved.
    pub fn is_approved(&self) -> bool {
        self.command_status == Some(CommandStatus::Approved)
    }

    /// The envelope with its `ResponseData` read by `f`.
    pub(crate) fn map_data<T, E>(
        self,
        f: impl FnOnce(D) -> Result<T, E>,
    ) -> Result<Envelope<T>, E> {
        Ok(Envelope {
            command_status: self.command_status,
            response_code: self.response_code,
            description: self.description,
            error_information: self.error_information,
            express_verify: self.express_verify,
            response_data: self.response_data.map(f).transpose()?,
            achq_token: self.achq_token,
            provider_transaction_id: self.provider_transaction_id,
            reference_id: self.reference_id,
            other: self.other,
        })
    }

    /// The reference ID, with blank values treated as missing.
    pub(crate) fn reference(&self) -> Option<&str> {
        self.reference_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
    }
}

string_enum! {
    case_insensitive;
    /// Whether a command succeeded.
    pub enum CommandStatus {
        /// The command succeeded.
        Approved = "Approved",
        /// The gateway refused the command, such as for invalid credentials or
        /// fields.
        Declined = "Declined",
        /// The command failed.
        Error = "Error",
    }
}

/// The gateway's code for why a command succeeded or failed, such as `000` or
/// `150` (*Required Field Missing*).
///
/// Sent as a string of digits (`"000"`); displayed with three digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResponseCode(u16);

macro_rules! response_codes {
    ($($name:ident = $code:literal, $description:literal;)+) => {
        impl ResponseCode {
            $(
                #[doc = concat!("`", stringify!($code), "`: ", $description, ".")]
                pub const $name: Self = Self($code);
            )+

            /// The code's documented meaning, if it has one.
            pub fn description(self) -> Option<&'static str> {
                match self.0 {
                    $($code => Some($description),)+
                    _ => None,
                }
            }
        }
    };
}

response_codes! {
    APPROVED = 0, "Request successful, approved";
    INVALID_CREDENTIALS = 100, "Invalid gateway credentials";
    INVALID_COMMAND = 101, "Invalid gateway command";
    DUPLICATE_COMMAND = 102, "Duplicate command not processed";
    TRANSACTION_NOT_MODIFIABLE = 103, "Transaction cannot be modified";
    BATCH_NOT_MODIFIABLE = 104, "Batch cannot be modified";
    INVALID_REFERENCE_ID = 105, "Invalid Transact_ReferenceID";
    INVALID_BATCH_ID = 106, "Invalid BatchID";
    NON_UNIQUE_ID = 107, "Non-unique reference/transaction ID";
    INVALID_ID = 108, "Invalid reference/transaction ID";
    INVALID_SOURCE_IP = 109, "Invalid source IP";
    INVALID_VALUE = 110, "Invalid value in message";
    REQUIRED_FIELD_MISSING = 150, "Required field missing";
    INVALID_FIELD_VALUE = 151, "Field value is not valid";
    FIELD_TOO_LONG = 152, "Field value exceeds maximum length";
    FAILED_EXPRESS_VERIFY = 202, "Failed Express Verify (undocumented; seen in the sandbox)";
    INVALID_ROUTING_NUMBER = 209, "Invalid routing number";
    INVALID_ACCOUNT_NUMBER = 210, "Invalid bank account number";
    INVALID_ACHQ_TOKEN = 212, "Invalid ACHQToken";
    DECLINED_BY_PROCESSOR = 300, "Transaction was declined by processor";
    REJECTED_BY_GATEWAY = 301, "Transaction was rejected by gateway";
    INVALID_ACCOUNT_TYPE = 304, "Invalid account type";
    ACCOUNT_CLOSED = 305, "Account closed";
    ACCOUNT_INACTIVE = 306, "Account inactive";
    INTERNAL_GATEWAY_ERROR = 600, "Internal gateway error";
    INTERNAL_PROCESSOR_ERROR = 601, "Internal processor error";
    PROCESSOR_COMMUNICATION_ERROR = 603, "Communication error with processor";
    PROCESSOR_FEATURE_UNAVAILABLE = 604, "Processor feature not available";
    PROCESSOR_FORMAT_ERROR = 605, "Processor format error";
    MERCHANT_NOT_SET_UP = 607, "Merchant not set up";
    MERCHANT_INACTIVE = 608, "Merchant account is inactive";
    INVALID_MERCHANT_CONFIGURATION = 609, "Invalid merchant configuration";
}

impl ResponseCode {
    /// Wraps a code.
    pub const fn new(code: u16) -> Self {
        Self(code)
    }

    /// The code's value.
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl fmt::Display for ResponseCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:03}", self.0)
    }
}

impl Serialize for ResponseCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ResponseCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ResponseCodeVisitor)
    }
}

struct ResponseCodeVisitor;

impl Visitor<'_> for ResponseCodeVisitor {
    type Value = ResponseCode;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a response code, as digits")
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<ResponseCode, E> {
        u16::try_from(value)
            .map(ResponseCode)
            .map_err(|_| E::custom(format!("response code {value} is out of range")))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<ResponseCode, E> {
        u16::try_from(value)
            .map(ResponseCode)
            .map_err(|_| E::custom(format!("response code {value} is out of range")))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<ResponseCode, E> {
        value
            .trim()
            .parse()
            .map(ResponseCode)
            .map_err(|_| E::custom(format!("invalid response code {value:?}")))
    }
}

/// The result of verifying a bank account (*Express Verify*), and, for
/// `ECheck.VerifyPlus`, what the verification network knows about it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct ExpressVerify {
    #[serde(rename = "Status", default)]
    pub status: Option<VerificationStatus>,
    #[serde(rename = "Code", default)]
    pub code: Option<VerificationCode>,
    #[serde(rename = "Description", default)]
    pub description: Option<String>,
    /// When the account joined the real-time verification network.
    #[serde(rename = "AccountCreated", default)]
    pub account_created: Option<LocalDateTime>,
    /// When the network last updated the account's status.
    #[serde(rename = "AccountLastSeen", default)]
    pub account_last_seen: Option<LocalDateTime>,
    /// The bank of the routing number.
    #[serde(rename = "BankName", default)]
    pub bank_name: Option<String>,
}

impl ExpressVerify {
    /// Whether every field is empty, as when no verification ran.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

string_enum! {
    case_insensitive;
    /// The outcome of a bank account verification or an ownership check.
    pub enum VerificationStatus {
        /// Positive: the account was found in good standing, or the owner
        /// matched.
        Positive = "POS",
        /// Negative: the account is invalid or in negative standing, or the owner
        /// did not match.
        Negative = "NEG",
        /// Unknown: the bank does not report to the verification network.
        Unknown = "UNK",
        /// The account could not be verified because of a system error.
        Error = "ERR",
    }
}

string_enum! {
    case_insensitive;
    /// Why a bank account verification or ownership check had its outcome.
    pub enum VerificationCode {
        /// POS: an open and valid checking account.
        PassAv = "1111",
        /// NEG: an open and valid American Express account.
        PassAmex = "2222",
        /// POS: a non-participating bank reported acceptable recent activity.
        PassNpp = "3333",
        /// POS: an open and valid savings account.
        PassSav = "5555",
        /// POS: an open checking account with a positive history.
        PassAvHistory = "7777",
        /// POS: an open savings account with a positive history.
        PassSavHistory = "8888",
        /// POS: acceptable data across multiple recent transactions.
        PassNppHistory = "9999",
        /// NEG: negative information was found.
        NegativeData = "GN01",
        /// NEG: the routing number is not assigned to a bank.
        UnassignedRoutingNumber = "GN05",
        /// NEG: a known bad check writer.
        PrivateBadChecksList = "GP01",
        /// NEG: the routing number is not in a valid format.
        InvalidRoutingNumber = "GS01",
        /// NEG: the account number is not in a valid format.
        InvalidAccountNumber = "GS02",
        /// UNK: no information has been reported on the account.
        NoData = "ND00",
        /// NEG: no information; the routing number is for US government banks
        /// only.
        NoDataUsGovernment = "ND01",
        /// NEG: the routing number looks right, but nothing is known about the
        /// account; check it with the customer.
        NoInformationFound = "RT00",
        /// NEG: the account should be returned for the risk reported.
        Declined = "RT01",
        /// NEG: the item should be returned for the risk reported.
        RejectItem = "RT02",
        /// NEG: current negative data, such as NSF status; accept with risk.
        AcceptWithRiskCurrent = "RT03",
        /// NEG: not a demand deposit account (credit card check, line of credit,
        /// brokerage).
        NonDemandDeposit = "RT04",
        /// NEG: recent negative data; accept with risk.
        AcceptWithRiskRecent = "RT05",
        /// POS: the account was validated. Undocumented; the sandbox reports it for
        /// payments' Express Verify.
        Validated = "P70",
        /// NEG: negative information was found. Undocumented; the sandbox reports
        /// it for payments' Express Verify.
        NegativeInfo = "P41",
        /// UNK: no ownership check was made for the account.
        NoOwnerData = "0",
        /// NEG: the account failed verification, so its owner was not checked.
        OwnerAccountFailed = "CA00",
        /// NEG: the owner information failed authentication.
        OwnerDeclined = "CA01",
        /// POS: the owner passed authentication.
        OwnerPass = "CA11",
        /// NEG: the customer or business name did not match.
        OwnerNameMismatch = "CA21",
        /// NEG: the tax ID did not match.
        OwnerTaxIdMismatch = "CA22",
        /// POS: the address did not match; accept with risk.
        OwnerAddressMismatch = "CA23",
        /// POS: the phone number did not match; accept with risk.
        OwnerPhoneMismatch = "CA24",
        /// POS: the date of birth or ID did not match; accept with risk.
        OwnerDobMismatch = "CA25",
        /// NEG: several secondary data points did not match.
        OwnerMultipleMismatches = "CA30",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    type Plain = Envelope<serde_json::Value>;

    fn read(value: serde_json::Value) -> Plain {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn reads_the_documented_envelope() {
        let envelope = read(json!({
            "CommandStatus": "Approved",
            "Description": "Command Successful.  Approved.",
            "ErrorInformation": null,
            "ExpressVerify": {"Status": null, "Code": null, "Description": null},
            "ResponseData": null,
            "ACHQToken": "achq-sandbox-3ff6d-b64fd-5a8a5",
            "Provider_TransactionID": "21321312",
            "TransAct_ReferenceID": "14469220",
            "ResponseCode": "000"
        }));
        assert!(envelope.is_approved());
        assert_eq!(envelope.response_code, Some(ResponseCode::APPROVED));
        assert_eq!(envelope.reference(), Some("14469220"));
        assert_eq!(
            envelope.achq_token.as_ref().map(AchqToken::as_str),
            Some("achq-sandbox-3ff6d-b64fd-5a8a5")
        );
        assert!(envelope.express_verify.unwrap().is_empty());
        assert!(envelope.other.is_empty());
    }

    #[test]
    fn reads_the_other_reference_id_spelling() {
        let envelope = read(json!({"Transact_ReferenceID": "1"}));
        assert_eq!(envelope.reference(), Some("1"));
        assert!(envelope.other.is_empty());
    }

    #[test]
    fn missing_fields_are_none() {
        let envelope = read(json!({"CommandStatus": "Approved"}));
        assert_eq!(envelope.response_code, None);
        assert_eq!(envelope.reference(), None);
        assert_eq!(read(json!({"TransAct_ReferenceID": " "})).reference(), None);
    }

    #[test]
    fn keeps_unknown_keys() {
        let envelope = read(json!({
            "AVS": {"Description": null, "Code": null},
            "PaymentKey": null
        }));
        assert_eq!(
            envelope.other.keys().collect::<Vec<_>>(),
            ["AVS", "PaymentKey"]
        );
    }

    #[test]
    fn reads_response_codes_in_any_form() {
        let code = |v: serde_json::Value| serde_json::from_value::<ResponseCode>(v).unwrap();
        assert_eq!(code(json!("000")), ResponseCode::APPROVED);
        assert_eq!(code(json!("0")), ResponseCode::APPROVED);
        assert_eq!(code(json!(150)), ResponseCode::REQUIRED_FIELD_MISSING);
        assert_eq!(code(json!(" 100 ")), ResponseCode::INVALID_CREDENTIALS);
        assert!(serde_json::from_value::<ResponseCode>(json!("abc")).is_err());
        assert!(serde_json::from_value::<ResponseCode>(json!(70000)).is_err());
    }

    #[test]
    fn response_codes_display_with_three_digits() {
        assert_eq!(ResponseCode::APPROVED.to_string(), "000");
        assert_eq!(ResponseCode::new(42).to_string(), "042");
        assert_eq!(
            ResponseCode::FIELD_TOO_LONG.description(),
            Some("Field value exceeds maximum length")
        );
        assert_eq!(ResponseCode::new(999).description(), None);
    }

    #[test]
    fn command_status_ignores_case() {
        assert_eq!(CommandStatus::from("approved"), CommandStatus::Approved);
        assert_eq!(
            CommandStatus::from("Pending"),
            CommandStatus::Other("Pending".into())
        );
    }

    #[test]
    fn reads_verification_results() {
        let verify: ExpressVerify = serde_json::from_value(json!({
            "Status": "NEG", "Code": "RT03", "Description": "Accept With Risk",
            "BankName": "FIRST BANK"
        }))
        .unwrap();
        assert_eq!(verify.status, Some(VerificationStatus::Negative));
        assert_eq!(verify.code, Some(VerificationCode::AcceptWithRiskCurrent));
        assert_eq!(verify.bank_name.as_deref(), Some("FIRST BANK"));
        assert!(!verify.is_empty());
    }
}
