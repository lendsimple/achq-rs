//! Payments: creating, changing, cancelling, holding and refunding them.
//!
//! A payment is an ACH debit ([`PaymentDirection::FromCustomer`], a charge) or
//! credit ([`PaymentDirection::ToCustomer`], a payout) of a customer's bank
//! account, which the gateway sends to the Federal Reserve in a later batch. Its
//! outcome arrives over the following days through
//! [event tracking](crate::events).
//!
//! The [`PaymentId`] returned when a payment is created identifies it from then on.
//! Updates, cancellations and holds return a [`CommandReceipt`] with an ID of
//! their own, which does not replace it.

use std::net::IpAddr;

use serde::de::IgnoredAny;
use time::Date;

use crate::Model;
use crate::bank::{BankAccount, CheckType};
use crate::envelope::ExpressVerify;
use crate::error::Error;
use crate::form::{FieldMeta, FormWriter, ToForm, join_prefix};
use crate::macros::string_enum;
use crate::operation::Decode;
use crate::protocol::{decode_envelope, reference_id};
use crate::transport::HttpResponse;
use crate::types::{
    AccountToken, AchqToken, Amount, CommandId, MerchantReferenceId, PaymentId,
    ProviderTransactionId,
};
use crate::validate::{Path, Validate, Violation, check_length, violation};

string_enum! {
    case_insensitive;
    /// Which way the money moves.
    pub enum PaymentDirection {
        /// An ACH debit: the merchant collects from the customer's account.
        FromCustomer = "FromCustomer",
        /// An ACH credit: the merchant pays into the customer's account.
        ToCustomer = "ToCustomer",
    }
}

string_enum! {
    case_insensitive;
    /// How the customer authorized the payment (the NACHA Standard Entry Class).
    pub enum SecCode {
        /// Prearranged: a consumer's signed authorization.
        Ppd = "PPD",
        /// Corporate: a business's signed authorization.
        Ccd = "CCD",
        /// Authorized online or in a mobile app. Needs the customer's IP address.
        Web = "WEB",
        /// Authorized over the telephone.
        Tel = "TEL",
    }
}

string_enum! {
    case_insensitive;
    /// When the gateway sends the payment to the Federal Reserve.
    pub enum DeliveryWindow {
        /// The next window, including Same-Day ACH windows (if the merchant is set
        /// up for them) before the last same-day cut-off.
        FirstAvailable = "FirstAvailable",
        /// The end-of-day window, effective the next business day.
        Standard = "Standard",
    }
}

string_enum! {
    case_insensitive;
    /// Who issued a payment's [`AccountToken`].
    pub enum TokenSource {
        /// ACHQ: an [`AchqToken`].
        Achq = "ACHQ",
        /// A Plaid processor token.
        Plaid = "Plaid",
        /// An MX processor token.
        Mx = "MX",
    }
}

/// The customer whose account is debited or credited, sent as the `Billing_`
/// fields.
#[derive(Clone, PartialEq, Model, bon::Builder)]
#[builder(on(String, into))]
#[achq(form)]
#[non_exhaustive]
pub struct Customer {
    /// The name on the account.
    #[achq(form = "CustomerName", min_length = 1, max_length = 80)]
    pub name: String,
    /// The company on the account; required for a business account.
    #[achq(form = "Company", max_length = 80)]
    pub company: Option<String>,
    /// The merchant's ID of the customer.
    #[achq(form = "CustomerID", max_length = 20)]
    pub customer_id: Option<String>,
    /// The street address on the account.
    #[achq(form = "Address1", min_length = 1, max_length = 70)]
    pub address1: String,
    /// More of the street address.
    #[achq(form = "Address2", max_length = 40)]
    pub address2: Option<String>,
    /// The city.
    #[achq(form = "City", min_length = 1, max_length = 70)]
    pub city: String,
    /// The state.
    #[achq(form = "State", min_length = 1, max_length = 30)]
    pub state: String,
    /// `#####` or `#####-####`.
    #[achq(form = "Zip", rule = "zip")]
    pub zip: String,
    /// The two-letter country code (ISO 3166). The gateway's default is `US`.
    #[achq(form = "Country", min_length = 2, max_length = 2)]
    pub country: Option<String>,
    /// The phone number.
    #[achq(form = "Phone", min_length = 1, max_length = 20)]
    pub phone: String,
    /// The email address, to which ACHQ sends notices when asked to.
    #[achq(form = "Email", max_length = 80, rule = "email")]
    pub email: String,
}

/// Changes to a payment's customer: only the fields set are sent.
#[derive(Clone, Default, PartialEq, Model, bon::Builder)]
#[builder(on(String, into))]
#[achq(form)]
#[non_exhaustive]
pub struct CustomerUpdate {
    /// The name on the account.
    #[achq(form = "CustomerName", min_length = 1, max_length = 80)]
    pub name: Option<String>,
    /// The company on the account.
    #[achq(form = "Company", max_length = 80)]
    pub company: Option<String>,
    /// The merchant's ID of the customer.
    #[achq(form = "CustomerID", max_length = 20)]
    pub customer_id: Option<String>,
    /// The street address.
    #[achq(form = "Address1", min_length = 1, max_length = 70)]
    pub address1: Option<String>,
    /// More of the street address.
    #[achq(form = "Address2", max_length = 40)]
    pub address2: Option<String>,
    /// The city.
    #[achq(form = "City", min_length = 1, max_length = 70)]
    pub city: Option<String>,
    /// The state.
    #[achq(form = "State", min_length = 1, max_length = 30)]
    pub state: Option<String>,
    /// `#####` or `#####-####`.
    #[achq(form = "Zip", rule = "zip")]
    pub zip: Option<String>,
    /// The two-letter country code (ISO 3166).
    #[achq(form = "Country", min_length = 2, max_length = 2)]
    pub country: Option<String>,
    /// The phone number.
    #[achq(form = "Phone", min_length = 1, max_length = 20)]
    pub phone: Option<String>,
    /// The email address.
    #[achq(form = "Email", max_length = 80, rule = "email")]
    pub email: Option<String>,
}

/// The bank account a payment draws on or pays into: the account itself, or a
/// token standing in for it.
///
/// ```
/// use achq::bank::BankAccount;
/// use achq::payments::{PaymentSource, TokenSource};
///
/// // Keep a token for the account, to charge it later without its numbers.
/// let bank = PaymentSource::bank(BankAccount::checking("123123123", "12345678")).tokenize();
/// let token = PaymentSource::token("processor-sandbox-74fb6a84", TokenSource::Plaid);
/// # let _ = (bank, token);
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum PaymentSource {
    /// A bank account by its numbers.
    Bank {
        /// The account.
        account: BankAccount,
        /// Whether to create an [`AchqToken`] for the account, returned with the
        /// payment (`Create_ACHQToken`).
        create_token: bool,
    },
    /// A token standing in for a bank account.
    Token {
        /// The token.
        token: AccountToken,
        /// Who issued the token.
        source: TokenSource,
    },
}

impl PaymentSource {
    /// A bank account by its numbers, without creating a token for it.
    pub fn bank(account: BankAccount) -> Self {
        Self::Bank {
            account,
            create_token: false,
        }
    }

    /// A token from `source`.
    pub fn token(token: impl Into<AccountToken>, source: TokenSource) -> Self {
        Self::Token {
            token: token.into(),
            source,
        }
    }

    /// An ACHQ token, as returned when a token was created.
    pub fn achq_token(token: &AchqToken) -> Self {
        Self::token(token, TokenSource::Achq)
    }

    /// For a bank account, also creates an [`AchqToken`] for it; a token source is
    /// unchanged.
    pub fn tokenize(mut self) -> Self {
        if let Self::Bank { create_token, .. } = &mut self {
            *create_token = true;
        }
        self
    }

    fn check_type(&self) -> Option<&CheckType> {
        match self {
            Self::Bank { account, .. } => Some(&account.check_type),
            Self::Token { .. } => None,
        }
    }
}

impl ToForm for PaymentSource {
    fn write_form(&self, form: &mut FormWriter, prefix: &str) {
        match self {
            Self::Bank {
                account,
                create_token,
            } => {
                account.write_form(form, prefix);
                if *create_token {
                    form.push(&join_prefix(prefix, "Create_ACHQToken"), "Yes");
                }
            }
            Self::Token { token, source } => {
                form.push_sensitive(&join_prefix(prefix, "AccountToken"), token.as_str());
                form.push(&join_prefix(prefix, "TokenSource"), source.as_str());
            }
        }
    }

    /// The fields of both kinds of source: each is required when its kind is
    /// chosen.
    fn form_fields(prefix: &str, out: &mut Vec<FieldMeta>) {
        BankAccount::form_fields(prefix, out);
        let field = |name, required, max_length, sensitive| {
            FieldMeta::new(join_prefix(prefix, name), required, max_length, sensitive)
        };
        out.push(field("Create_ACHQToken", false, Some(3), false));
        out.push(field("AccountToken", true, Some(100), true));
        out.push(field("TokenSource", true, Some(50), false));
    }
}

impl Validate for PaymentSource {
    fn validate_into(&self, path: &mut Path, out: &mut Vec<Violation>) {
        match self {
            Self::Bank { account, .. } => {
                path.push_field("account");
                account.validate_into(path, out);
                path.pop();
            }
            Self::Token { token, .. } => {
                path.push_field("token");
                check_length(token.as_str(), Some(1), Some(100), path, out);
                path.pop();
            }
        }
    }
}

/// A new payment.
///
/// The customer's IP address is required for [`SecCode::Web`] payments, and the
/// customer's company for business accounts; validation checks both.
#[derive(Clone, PartialEq, Model, bon::Builder)]
#[builder(on(String, into))]
#[achq(form, check = "conditional_fields")]
#[non_exhaustive]
pub struct PaymentRequest {
    /// Your platform's ID of the payment, echoed in responses and reports. The
    /// gateway refuses a second payment with the same ID (code 107), which makes
    /// retries safe: set it.
    #[builder(into)]
    #[achq(form = "Provider_TransactionID", max_length = 50)]
    pub provider_transaction_id: Option<ProviderTransactionId>,
    /// The merchant's ID of the payment.
    #[builder(into)]
    #[achq(form = "Merchant_ReferenceID", max_length = 50)]
    pub merchant_reference_id: Option<MerchantReferenceId>,
    /// The amount, with at most 2 decimal places.
    #[builder(into)]
    #[achq(form = "Amount")]
    pub amount: Amount,
    /// A debit from the customer, or a credit to them.
    #[achq(form = "PaymentDirection")]
    pub direction: PaymentDirection,
    /// A description for the merchant.
    #[achq(form = "Description", max_length = 100)]
    pub description: Option<String>,
    /// How the customer authorized the payment.
    #[achq(form = "SECCode")]
    pub sec_code: SecCode,
    /// The day to process the payment; the day it is created if not set.
    #[achq(form = "DateScheduled", with = "mdy_slash")]
    pub date_scheduled: Option<Date>,
    /// When to send the payment to the Federal Reserve.
    #[builder(default = DeliveryWindow::Standard)]
    #[achq(form = "DeliveryWindow")]
    pub delivery_window: DeliveryWindow,
    /// The customer, sent as the `Billing_` fields.
    #[achq(flatten, prefix = "Billing_")]
    pub customer: Customer,
    /// The customer's IP address; required for [`SecCode::Web`].
    #[builder(into)]
    #[achq(form = "Customer_IPAddress")]
    pub customer_ip: Option<IpAddr>,
    /// Whether ACHQ emails the customer about the payment.
    #[builder(default)]
    #[achq(form = "SendEmailToCustomer", with = "yes_no")]
    pub send_email: bool,
    /// Whether to verify the bank account first (*Express Verify*). The merchant's
    /// settings apply when not set.
    #[achq(form = "Run_ExpressVerify", with = "yes_no")]
    pub run_express_verify: Option<bool>,
    /// The bank account, or a token for it.
    #[achq(flatten)]
    pub source: PaymentSource,
}

impl PaymentRequest {
    fn conditional_fields(&self, path: &mut Path, out: &mut Vec<Violation>) {
        if self.source.check_type() == Some(&CheckType::Business) && self.customer.company.is_none()
        {
            path.push_field("customer");
            path.push_field("company");
            violation(path, out, "is required for a business account".into());
            path.pop();
            path.pop();
        }
        check_customer_ip(Some(&self.sec_code), self.customer_ip.as_ref(), path, out);
    }
}

/// `Customer_IPAddress` is required for WEB payments, and the gateway takes at
/// most 15 characters: an IPv4 address.
fn check_customer_ip(
    sec_code: Option<&SecCode>,
    ip: Option<&IpAddr>,
    path: &mut Path,
    out: &mut Vec<Violation>,
) {
    path.push_field("customer_ip");
    match ip {
        None if sec_code == Some(&SecCode::Web) => {
            violation(path, out, "is required for WEB payments".into());
        }
        Some(ip) if ip.to_string().len() > 15 => {
            violation(
                path,
                out,
                format!("must be at most 15 characters, as an IPv4 address is, not {ip}"),
            );
        }
        _ => {}
    }
    path.pop();
}

/// Changes to a scheduled payment: only the fields set are sent.
///
/// A payment can be changed until the gateway sends it to the Federal Reserve
/// (while its status is `Scheduled` or `Merchant Hold`).
#[derive(Clone, Default, PartialEq, Model, bon::Builder)]
#[builder(on(String, into))]
#[achq(form, check = "has_changes")]
#[non_exhaustive]
pub struct PaymentUpdate {
    /// Your platform's new ID of the payment.
    #[builder(into)]
    #[achq(form = "Provider_TransactionID", max_length = 50)]
    pub provider_transaction_id: Option<ProviderTransactionId>,
    /// The merchant's new ID of the payment.
    #[builder(into)]
    #[achq(form = "Merchant_ReferenceID", max_length = 50)]
    pub merchant_reference_id: Option<MerchantReferenceId>,
    /// The new amount.
    #[builder(into)]
    #[achq(form = "Amount")]
    pub amount: Option<Amount>,
    /// The new direction.
    #[achq(form = "PaymentDirection")]
    pub direction: Option<PaymentDirection>,
    /// The new description.
    #[achq(form = "Description", max_length = 100)]
    pub description: Option<String>,
    /// The new authorization type.
    #[achq(form = "SECCode")]
    pub sec_code: Option<SecCode>,
    /// The new day to process the payment.
    #[achq(form = "DateScheduled", with = "mdy_slash")]
    pub date_scheduled: Option<Date>,
    /// The new delivery window.
    #[achq(form = "DeliveryWindow")]
    pub delivery_window: Option<DeliveryWindow>,
    /// Changes to the customer, sent as the `Billing_` fields.
    #[achq(flatten, prefix = "Billing_")]
    pub customer: Option<CustomerUpdate>,
    /// The customer's IP address; required for a change to WEB.
    #[builder(into)]
    #[achq(form = "Customer_IPAddress")]
    pub customer_ip: Option<IpAddr>,
    /// A different bank account, or a token for one.
    #[achq(flatten)]
    pub source: Option<PaymentSource>,
}

impl PaymentUpdate {
    fn has_changes(&self, path: &mut Path, out: &mut Vec<Violation>) {
        if *self == Self::default() {
            violation(path, out, "set at least one field to change".into());
        }
        check_customer_ip(self.sec_code.as_ref(), self.customer_ip.as_ref(), path, out);
    }
}

/// A refund of a cleared payment: a new credit to the customer.
///
/// Without an amount, the whole payment is refunded.
#[derive(Clone, Default, PartialEq, Model, bon::Builder)]
#[builder(on(String, into))]
#[achq(form)]
#[non_exhaustive]
pub struct RefundRequest {
    /// Your platform's ID of the refund.
    #[builder(into)]
    #[achq(form = "Provider_TransactionID", max_length = 50)]
    pub provider_transaction_id: Option<ProviderTransactionId>,
    /// The merchant's ID of the refund.
    #[builder(into)]
    #[achq(form = "Merchant_ReferenceID", max_length = 50)]
    pub merchant_reference_id: Option<MerchantReferenceId>,
    /// Less than the payment, for a partial refund; the whole payment if not set.
    #[builder(into)]
    #[achq(form = "Amount")]
    pub amount: Option<Amount>,
    /// A description for the merchant.
    #[achq(form = "Description", max_length = 100)]
    pub description: Option<String>,
    /// The day to send the refund; the day it is created if not set.
    #[achq(form = "DateScheduled", with = "mdy_slash")]
    pub date_scheduled: Option<Date>,
}

impl RefundRequest {
    /// A refund of the whole payment.
    pub fn full() -> Self {
        Self::default()
    }

    /// A refund of part of the payment.
    pub fn partial(amount: impl Into<Amount>) -> Self {
        Self {
            amount: Some(amount.into()),
            ..Self::default()
        }
    }
}

/// A payment or refund the gateway accepted.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct PaymentCreated {
    /// The payment's ID. Keep it: updates, cancellations, holds, refunds, event
    /// tracking and settlement reports refer to the payment by it.
    pub payment_id: PaymentId,
    /// A token for the bank account, when one was asked for.
    pub token: Option<AchqToken>,
    /// Your platform's ID of the payment, echoed.
    pub provider_transaction_id: Option<ProviderTransactionId>,
    /// The bank account verification's result, if one ran.
    pub express_verify: Option<ExpressVerify>,
}

impl Decode for PaymentCreated {
    fn decode(response: &HttpResponse) -> Result<Self, Error> {
        let envelope = decode_envelope::<IgnoredAny>(response)?;
        Ok(Self {
            payment_id: PaymentId::new(reference_id(&envelope, response)?),
            token: envelope.achq_token,
            provider_transaction_id: envelope.provider_transaction_id,
            express_verify: envelope.express_verify.filter(|v| !v.is_empty()),
        })
    }
}

/// An update, cancellation or hold the gateway accepted.
///
/// Its [`command_id`](Self::command_id) identifies this command only; the
/// payment keeps its own [`PaymentId`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct CommandReceipt {
    /// The gateway's ID of this command, to quote to ACHQ support.
    pub command_id: CommandId,
    /// Your platform's ID, echoed.
    pub provider_transaction_id: Option<ProviderTransactionId>,
}

impl Decode for CommandReceipt {
    fn decode(response: &HttpResponse) -> Result<Self, Error> {
        let envelope = decode_envelope::<IgnoredAny>(response)?;
        Ok(Self {
            command_id: CommandId::new(reference_id(&envelope, response)?),
            provider_transaction_id: envelope.provider_transaction_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;

    fn customer() -> Customer {
        Customer::builder()
            .name("Bob Loblaw")
            .address1("123 Street")
            .city("Town")
            .state("NY")
            .zip("12345")
            .phone("555-867-5309")
            .email("bob@example.com")
            .build()
    }

    fn payment(source: PaymentSource) -> PaymentRequest {
        PaymentRequest::builder()
            .amount(dec!(42))
            .direction(PaymentDirection::FromCustomer)
            .sec_code(SecCode::Web)
            .customer(customer())
            .customer_ip(IpAddr::from([203, 0, 113, 9]))
            .source(source)
            .build()
    }

    fn messages(value: &impl Validate) -> Vec<String> {
        match value.validate() {
            Ok(()) => Vec::new(),
            Err(error) => error.violations().iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn a_web_payment_needs_the_customer_ip() {
        let mut request = payment(PaymentSource::bank(BankAccount::checking("123123123", "1")));
        assert!(messages(&request).is_empty());
        request.customer_ip = None;
        assert_eq!(
            messages(&request),
            ["customer_ip: is required for WEB payments"]
        );
        request.sec_code = SecCode::Ppd;
        assert!(messages(&request).is_empty());
    }

    #[test]
    fn the_customer_ip_must_fit_15_characters() {
        let mut request = payment(PaymentSource::achq_token(&AchqToken::new("t")));
        request.customer_ip = Some("2001:db8::8a2e:370:7334".parse().unwrap());
        assert_eq!(
            messages(&request),
            [
                "customer_ip: must be at most 15 characters, as an IPv4 address is, not 2001:db8::8a2e:370:7334"
            ]
        );
    }

    #[test]
    fn a_business_account_needs_the_company() {
        let account = BankAccount::checking("123123123", "1").business();
        let mut request = payment(PaymentSource::bank(account));
        assert_eq!(
            messages(&request),
            ["customer.company: is required for a business account"]
        );
        request.customer.company = Some("Bluth Company".into());
        assert!(messages(&request).is_empty());
    }

    #[test]
    fn sources_write_their_fields() {
        let bank = PaymentSource::bank(BankAccount::checking("123123123", "12345678")).tokenize();
        assert_eq!(
            bank.to_form().redacted(),
            "RoutingNumber=123123123&AccountNumber=[REDACTED]&AccountType=Checking\
             &CheckType=Personal&Create_ACHQToken=Yes"
        );
        let token = PaymentSource::token("processor-sandbox-1", TokenSource::Plaid).tokenize();
        assert_eq!(
            token.to_form().redacted(),
            "AccountToken=[REDACTED]&TokenSource=Plaid"
        );
        assert!(!format!("{token:?}").contains("processor-sandbox-1"));
    }

    #[test]
    fn an_update_needs_a_change() {
        assert_eq!(
            messages(&PaymentUpdate::default()),
            ["set at least one field to change"]
        );
        let update = PaymentUpdate::builder().amount(dec!(142)).build();
        assert!(messages(&update).is_empty());
        assert_eq!(update.to_form().encode(), "Amount=142.00");
    }

    #[test]
    fn refunds_are_full_or_partial() {
        assert_eq!(RefundRequest::full().to_form().encode(), "");
        assert_eq!(
            RefundRequest::partial(dec!(23)).to_form().encode(),
            "Amount=23.00"
        );
    }
}
