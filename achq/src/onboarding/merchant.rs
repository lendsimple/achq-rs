//! A merchant's application for an ACHQ account.
//!
//! The types mirror the onboarding API's schema, with its conditional flags made
//! structural: payment limits are given only for the services enabled (which
//! enables them), the Standard Entry Classes are a list, and a funding account is
//! a bank account or a Plaid token, never both.
//!
//! The lengths checked are the schema's, which the API enforces.

use std::fmt;
use std::net::IpAddr;

use serde::ser::{SerializeMap, SerializeStruct};
use serde::{Deserialize, Serialize, Serializer};
use time::Date;

use super::{FeeSchedule, Industry};
use crate::Model;
use crate::error::Error;
use crate::macros::string_enum;
use crate::operation::Decode;
use crate::payments::SecCode;
use crate::protocol::decode_json;
use crate::transport::HttpResponse;
use crate::types::date::serialize_mdy_slash;
use crate::types::{FeeProfileId, OnboardingId};
use crate::validate::{Path, Validate, Violation, check_length, check_rule, rules, violation};

/// A merchant's application: the business, its owner, the payment services it
/// needs, its bank accounts, and its fees.
#[derive(Clone, PartialEq, Serialize, Model, bon::Builder)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MerchantApplication {
    /// The business being enrolled.
    pub business_entity: BusinessEntity,
    /// The business's principal owner.
    pub principal_owner: PrincipalOwner,
    /// Another representative of the company, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorized_representative: Option<AuthorizedRepresentative>,
    /// The payment services the merchant needs.
    pub payment_profile: PaymentProfile,
    /// The merchant's bank accounts.
    pub bank_accounts: BankAccounts,
    /// Who performs know-your-customer checks. Both are ACHQ's unless your
    /// account manager agreed otherwise.
    #[builder(default)]
    pub onboard_functions: OnboardFunctions,
    /// A fee profile by ID, or fees given inline.
    #[serde(flatten)]
    pub fees: FeeChoice,
}

string_enum! {
    case_insensitive;
    /// The legal form of a business.
    pub enum BusinessType {
        Corporation = "Corporation",
        Llc = "LLC",
        SoleProprietor = "Sole Proprietor",
        Partnership = "Partnership",
        NonProfit = "Non-profit",
    }
}

/// The business being enrolled.
#[derive(Clone, PartialEq, Serialize, Model, bon::Builder)]
#[serde(rename_all = "camelCase")]
#[builder(on(String, into))]
#[non_exhaustive]
pub struct BusinessEntity {
    /// The legal name, matching the tax ID.
    #[achq(min_length = 1, max_length = 255)]
    pub legal_business_name: String,
    /// The Employer Identification Number (or a sole proprietor's SSN): 9 digits.
    #[achq(sensitive, min_length = 9, max_length = 9, rule = "digits")]
    pub tax_id: String,
    /// The name customers see on their statements.
    #[serde(rename = "companyDBA")]
    #[achq(min_length = 1, max_length = 255)]
    pub company_dba: String,
    /// The business's industry, from the API's list.
    #[builder(into)]
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub industry: Option<IndustryChoice>,
    /// The four-digit merchant category code, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[achq(min_length = 4, max_length = 4, rule = "digits")]
    pub mcc: Option<String>,
    /// The legal form.
    #[serde(rename = "type")]
    pub kind: BusinessType,
    /// The business's website.
    #[achq(min_length = 1, max_length = 255)]
    pub website: String,
    /// Ten digits, without punctuation.
    #[achq(min_length = 10, max_length = 10, rule = "digits")]
    pub phone: String,
    /// The business model, and its products or services.
    #[achq(min_length = 1, max_length = 255)]
    pub business_description: String,
    /// The business's street address.
    #[achq(min_length = 1, max_length = 255)]
    pub address1: String,
    /// At most 9 characters, as the API's schema says (such as `Suite 100`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[achq(max_length = 9)]
    pub address2: Option<String>,
    /// The business's city.
    #[achq(min_length = 1, max_length = 255)]
    pub city: String,
    /// A state code or name.
    #[achq(rule = "us_state")]
    pub state: String,
    /// Five digits.
    #[achq(min_length = 5, max_length = 5, rule = "digits")]
    pub zip: String,
    /// `US`: the API accepts only the United States.
    #[builder(default = "US".to_owned())]
    #[achq(min_length = 1, max_length = 255)]
    pub country: String,
    /// Your platform's ID of the merchant.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[achq(max_length = 50)]
    pub external_id: Option<String>,
}

/// A business's industry: one of the [documented pairs](Industry), or another
/// pair the API's list has since gained.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum IndustryChoice {
    /// A pair from the API's list.
    Listed(Industry),
    /// A pair the API's list has gained since this crate's table.
    Custom {
        /// The general industry (`industryCategory`).
        category: String,
        /// The specific industry (`specificIndustry`).
        specific: String,
    },
}

impl From<Industry> for IndustryChoice {
    fn from(industry: Industry) -> Self {
        Self::Listed(industry)
    }
}

/// Written as `industryCategory` and `specificIndustry`.
impl Serialize for IndustryChoice {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (category, specific) = match self {
            Self::Listed(industry) => (industry.category().name(), industry.name()),
            Self::Custom { category, specific } => (category.as_str(), specific.as_str()),
        };
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("industryCategory", category)?;
        map.serialize_entry("specificIndustry", specific)?;
        map.end()
    }
}

impl Validate for IndustryChoice {
    fn validate_into(&self, path: &mut Path, out: &mut Vec<Violation>) {
        if let Self::Custom { category, specific } = self {
            for (name, value) in [("category", category), ("specific", specific)] {
                path.push_field(name);
                check_length(value, Some(1), Some(255), path, out);
                path.pop();
            }
        }
    }
}

/// An owner with more than 25% of the business, or a significant manager (such
/// as the CFO).
#[derive(Clone, PartialEq, Serialize, Model, bon::Builder)]
#[serde(rename_all = "camelCase")]
#[builder(on(String, into))]
#[non_exhaustive]
pub struct PrincipalOwner {
    /// The owner's first name.
    #[achq(min_length = 1, max_length = 255)]
    pub first_name: String,
    /// The owner's last name.
    #[achq(min_length = 1, max_length = 255)]
    pub last_name: String,
    /// The owner's role, such as CEO.
    #[achq(min_length = 1, max_length = 255)]
    pub job_title: String,
    /// The owner's company email address.
    #[achq(max_length = 255, rule = "email")]
    pub email: String,
    /// Ten digits, without punctuation.
    #[achq(min_length = 10, max_length = 10, rule = "digits")]
    pub mobile_phone: String,
    /// The Social Security number: 9 digits.
    #[achq(sensitive, min_length = 9, max_length = 9, rule = "digits")]
    pub ssn: String,
    /// The share of the business the owner holds or controls, in percent.
    #[achq(max = "100")]
    #[serde(serialize_with = "as_text")]
    pub ownership_percent: u8,
    /// The date of birth.
    #[achq(sensitive)]
    #[serde(serialize_with = "serialize_mdy_slash")]
    pub dob: Date,
    /// The owner's home street address.
    #[achq(min_length = 1, max_length = 255)]
    pub address1: String,
    /// More of the owner's home street address.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[achq(max_length = 30)]
    pub address2: Option<String>,
    /// The owner's home city.
    #[achq(min_length = 1, max_length = 255)]
    pub city: String,
    /// A state or territory, by code or name.
    #[achq(rule = "us_state")]
    pub state: String,
    /// Five digits.
    #[achq(min_length = 5, max_length = 5, rule = "digits")]
    pub zip: String,
    /// The IP address the application was submitted from.
    #[builder(into)]
    pub ip_address: IpAddr,
}

/// Another representative of the company.
#[derive(Clone, PartialEq, Serialize, Model, bon::Builder)]
#[serde(rename_all = "camelCase")]
#[builder(on(String, into))]
#[non_exhaustive]
pub struct AuthorizedRepresentative {
    /// The representative's first name.
    #[achq(min_length = 1, max_length = 255)]
    pub first_name: String,
    /// The representative's last name.
    #[achq(min_length = 1, max_length = 255)]
    pub last_name: String,
    /// The representative's business title.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[achq(max_length = 255)]
    pub title: Option<String>,
    /// The representative's business email address.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[achq(max_length = 255, rule = "email")]
    pub email: Option<String>,
    /// Ten digits, without punctuation.
    #[serde(rename = "authRepPhone", skip_serializing_if = "Option::is_none")]
    #[achq(min_length = 10, max_length = 10, rule = "digits")]
    pub phone: Option<String>,
    /// The share of the business the representative holds or controls, in
    /// percent. (The API spells it `ownerShipPercent` here.)
    #[serde(
        rename = "ownerShipPercent",
        serialize_with = "option_as_text",
        skip_serializing_if = "Option::is_none"
    )]
    #[achq(max = "100")]
    pub ownership_percent: Option<u8>,
}

fn as_text<S: Serializer>(value: &u8, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

fn option_as_text<S: Serializer>(value: &Option<u8>, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => serializer.collect_str(value),
        None => serializer.serialize_none(),
    }
}

/// Monthly limits on a payment service: charges or payouts.
#[derive(Clone, Copy, PartialEq, Eq, Model)]
#[non_exhaustive]
pub struct Limits {
    /// The most dollars a month (up to 10 digits).
    #[achq(max = "9999999999")]
    pub dollars_per_month: u64,
    /// The most payments a month (up to 7 digits).
    #[achq(max = "9999999")]
    pub count_per_month: u32,
    /// The most dollars in one payment (up to 6 digits).
    #[achq(max = "999999")]
    pub max_amount: u32,
}

impl Limits {
    /// Limits of `dollars_per_month` and `count_per_month`, and of `max_amount` for one
    /// payment.
    pub fn new(dollars_per_month: u64, count_per_month: u32, max_amount: u32) -> Self {
        Self {
            dollars_per_month,
            count_per_month,
            max_amount,
        }
    }
}

/// The payment services a merchant needs, and how its payments appear to
/// customers.
///
/// Charges (debits) and payouts (credits, including refunds) are enabled by
/// giving their [`Limits`]; ACHQ advises enabling payouts so that the merchant
/// can refund.
#[derive(Clone, PartialEq, Model, bon::Builder)]
#[builder(on(String, into))]
#[achq(check = "services")]
#[non_exhaustive]
pub struct PaymentProfile {
    /// The limits on charges, which enables them.
    pub charges: Option<Limits>,
    /// The limits on payouts, which enables them.
    pub payouts: Option<Limits>,
    /// Whether the merchant may transact with consumer accounts.
    #[builder(default)]
    pub consumers: bool,
    /// Whether the merchant may transact with business accounts.
    #[builder(default)]
    pub businesses: bool,
    /// The authorizations the merchant collects: PPD, CCD, WEB and TEL.
    pub sec_codes: Vec<SecCode>,
    /// Up to 10 characters shown on customers' statements, besides the company
    /// name and phone, which ACHQ adds.
    #[achq(min_length = 1, max_length = 10)]
    pub statement_descriptor: String,
    /// Ten digits, shown on customers' statements.
    #[achq(min_length = 10, max_length = 10, rule = "digits")]
    pub customer_service_phone: String,
    /// The customer service email address.
    #[achq(max_length = 255, rule = "email")]
    pub customer_service_email: String,
    /// What the merchant uses the payments for, such as payroll or tuition.
    #[achq(min_length = 1, max_length = 255)]
    pub service_use_case: String,
}

impl PaymentProfile {
    fn services(&self, path: &mut Path, out: &mut Vec<Violation>) {
        if self.charges.is_none() && self.payouts.is_none() {
            violation(path, out, "enable charges, payouts or both".into());
        }
        if !self.consumers && !self.businesses {
            violation(path, out, "allow consumers, businesses or both".into());
        }
        path.push_field("sec_codes");
        if self.sec_codes.is_empty() {
            violation(path, out, "list at least one SEC code".into());
        }
        for code in &self.sec_codes {
            if let SecCode::Other(other) = code {
                violation(path, out, format!("{other:?} is not PPD, CCD, WEB or TEL"));
            }
        }
        path.pop();
    }
}

impl Serialize for PaymentProfile {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("enableCharges", &self.charges.is_some())?;
        map.serialize_entry("enablePayouts", &self.payouts.is_some())?;
        for (limits, [dollars, count, max]) in [
            (
                &self.charges,
                ["chargeDollarsMonth", "chargesPerMonth", "chargesMaxAmount"],
            ),
            (
                &self.payouts,
                ["payoutsDollarsMonth", "payoutsPerMonth", "payoutsMaxAmount"],
            ),
        ] {
            if let Some(limits) = limits {
                map.serialize_entry(dollars, &limits.dollars_per_month.to_string())?;
                map.serialize_entry(count, &limits.count_per_month.to_string())?;
                map.serialize_entry(max, &limits.max_amount.to_string())?;
            }
        }
        map.serialize_entry("customerTypeConsumer", &self.consumers)?;
        map.serialize_entry("customerTypeBusiness", &self.businesses)?;
        for (key, code) in [
            ("enablePPD", SecCode::Ppd),
            ("enableCCD", SecCode::Ccd),
            ("enableWEB", SecCode::Web),
            ("enableTEL", SecCode::Tel),
        ] {
            map.serialize_entry(key, &self.sec_codes.contains(&code))?;
        }
        map.serialize_entry("statementDescriptor", &self.statement_descriptor)?;
        map.serialize_entry("customerServicePhone", &self.customer_service_phone)?;
        map.serialize_entry("customerServiceEmail", &self.customer_service_email)?;
        map.serialize_entry("serviceUseCase", &self.service_use_case)?;
        map.end()
    }
}

/// A merchant bank account ACHQ transfers to or from: by its numbers, or a Plaid
/// processor token for it.
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FundingAccount {
    /// A bank account by its numbers.
    Bank {
        /// The bank's name.
        bank_name: String,
        /// The ABA routing number.
        routing_number: String,
        /// The account number.
        account_number: String,
    },
    /// A Plaid processor token for the account.
    Plaid {
        /// The token.
        token: String,
    },
}

impl FundingAccount {
    /// A bank account by its bank's name, routing number and account number.
    pub fn bank(
        bank_name: impl Into<String>,
        routing_number: impl Into<String>,
        account_number: impl Into<String>,
    ) -> Self {
        Self::Bank {
            bank_name: bank_name.into(),
            routing_number: routing_number.into(),
            account_number: account_number.into(),
        }
    }

    /// A Plaid processor token for the account.
    pub fn plaid(token: impl Into<String>) -> Self {
        Self::Plaid {
            token: token.into(),
        }
    }

    /// Writes the account with the key names of `role`.
    fn serialize_as<S: Serializer>(&self, role: Role, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Bank {
                bank_name,
                routing_number,
                account_number,
            } => {
                let mut s = serializer.serialize_struct("FundingAccount", 3)?;
                s.serialize_field("bankName", bank_name)?;
                s.serialize_field("routingNumber", routing_number)?;
                s.serialize_field("accountNumber", account_number)?;
                s.end()
            }
            Self::Plaid { token } => {
                let (type_key, token_key) = match role {
                    Role::Transfers => ("tokenType", "transferProcessorToken"),
                    Role::Billing => ("billingTokenType", "billingProcessorToken"),
                };
                let mut s = serializer.serialize_struct("FundingAccount", 2)?;
                s.serialize_field(type_key, "Plaid")?;
                s.serialize_field(token_key, token)?;
                s.end()
            }
        }
    }
}

impl fmt::Debug for FundingAccount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted = crate::__private::Redacted::new(true);
        match self {
            Self::Bank {
                bank_name,
                routing_number,
                ..
            } => f
                .debug_struct("Bank")
                .field("bank_name", bank_name)
                .field("routing_number", routing_number)
                .field("account_number", &redacted)
                .finish(),
            Self::Plaid { .. } => f.debug_struct("Plaid").field("token", &redacted).finish(),
        }
    }
}

impl Validate for FundingAccount {
    fn validate_into(&self, path: &mut Path, out: &mut Vec<Violation>) {
        match self {
            Self::Bank {
                bank_name,
                routing_number,
                account_number,
            } => {
                path.push_field("bank_name");
                check_length(bank_name, None, Some(255), path, out);
                path.pop();
                path.push_field("routing_number");
                check_rule(routing_number, rules::routing_number, path, out);
                path.pop();
                path.push_field("account_number");
                check_length(account_number, Some(1), Some(30), path, out);
                path.pop();
            }
            Self::Plaid { token } => {
                // The schema allows a transfer token only 10 characters, shorter
                // than Plaid's; only the overall limit is checked (open item).
                path.push_field("token");
                check_length(token, Some(1), Some(255), path, out);
                path.pop();
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Role {
    Transfers,
    Billing,
}

/// The merchant's bank accounts.
#[derive(Clone, PartialEq, Model, bon::Builder)]
#[non_exhaustive]
pub struct BankAccounts {
    /// The account for deposits and payouts.
    pub transfers: Option<FundingAccount>,
    /// The account ACHQ debits its service fees from.
    pub billing: Option<FundingAccount>,
}

impl Serialize for BankAccounts {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct As<'a>(&'a FundingAccount, Role);
        impl Serialize for As<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.0.serialize_as(self.1, serializer)
            }
        }
        let mut map = serializer.serialize_map(None)?;
        if let Some(account) = &self.transfers {
            map.serialize_entry("transfers", &As(account, Role::Transfers))?;
        }
        if let Some(account) = &self.billing {
            map.serialize_entry("billing", &As(account, Role::Billing))?;
        }
        map.end()
    }
}

/// Who performs the onboarding checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct OnboardFunctions {
    /// ACHQ verifies the business and its owners (KYC). Keep it unless your
    /// account manager agreed otherwise.
    #[serde(rename = "requireKYC")]
    pub require_kyc: bool,
    /// ACHQ confirms the merchant's bank account with Plaid.
    #[serde(rename = "requirePlaidConnect")]
    pub require_plaid_connect: bool,
}

impl Default for OnboardFunctions {
    /// Both, as ACHQ asks.
    fn default() -> Self {
        Self {
            require_kyc: true,
            require_plaid_connect: true,
        }
    }
}

crate::validate::no_constraints!(OnboardFunctions);

/// The merchant's fees: a fee profile already created, by ID, or a schedule.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum FeeChoice {
    /// A fee profile from
    /// [`create_fee_profile`](crate::operations::onboarding::CreateFeeProfile).
    Profile(FeeProfileId),
    /// Fees given inline.
    Schedule(FeeSchedule),
}

impl From<FeeProfileId> for FeeChoice {
    fn from(id: FeeProfileId) -> Self {
        Self::Profile(id)
    }
}

impl From<FeeSchedule> for FeeChoice {
    fn from(schedule: FeeSchedule) -> Self {
        Self::Schedule(schedule)
    }
}

/// Written as `feeProfileId` or `feeProfile`.
impl Serialize for FeeChoice {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            Self::Profile(id) => map.serialize_entry("feeProfileId", id)?,
            Self::Schedule(schedule) => map.serialize_entry("feeProfile", schedule)?,
        }
        map.end()
    }
}

impl Validate for FeeChoice {
    fn validate_into(&self, path: &mut Path, out: &mut Vec<Violation>) {
        match self {
            Self::Profile(id) => {
                if id.as_str().trim().is_empty() {
                    violation(path, out, "the fee profile ID must not be empty".into());
                }
            }
            Self::Schedule(schedule) => schedule.validate_into(path, out),
        }
    }
}

/// A merchant application the API accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MerchantOnboarded {
    /// The application's ID.
    pub id: OnboardingId,
    /// The application's status, such as `Pending`.
    pub status: Option<String>,
    /// The API's ID of the request, to quote to ACHQ support.
    pub api_request_id: Option<String>,
    /// When the API handled the request, as it wrote it.
    pub api_request_timestamp: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OnboardingResponse {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    api_request_id: Option<String>,
    #[serde(default)]
    api_request_time_stamp: Option<String>,
}

impl Decode for MerchantOnboarded {
    fn decode(response: &HttpResponse) -> Result<Self, Error> {
        let body: OnboardingResponse = decode_json(response)?;
        let id = body.id.filter(|id| !id.trim().is_empty()).ok_or_else(|| {
            Error::decode("the response has no id", response.status, &response.body)
        })?;
        Ok(Self {
            id: OnboardingId::new(id),
            status: body.status,
            api_request_id: body.api_request_id,
            api_request_timestamp: body.api_request_time_stamp,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use pretty_assertions::assert_eq;
    use rust_decimal::dec;
    use serde_json::json;
    use time::macros::date;

    use super::*;

    fn business() -> BusinessEntity {
        BusinessEntity::builder()
            .legal_business_name("Bluth Company LLC")
            .tax_id("123456789")
            .company_dba("Bluth Bananas")
            .industry(Industry::SaasAndSoftwareApplications)
            .kind(BusinessType::Llc)
            .website("https://example.com")
            .phone("5558675309")
            .business_description("Frozen bananas")
            .address1("1 Pier Way")
            .city("Newport Beach")
            .state("CA")
            .zip("92660")
            .build()
    }

    fn owner() -> PrincipalOwner {
        PrincipalOwner::builder()
            .first_name("George")
            .last_name("Bluth")
            .job_title("CEO")
            .email("george@example.com")
            .mobile_phone("5558675309")
            .ssn("987654321")
            .ownership_percent(60)
            .dob(date!(1950 - 01 - 02))
            .address1("1 Pier Way")
            .city("Newport Beach")
            .state("California")
            .zip("92660")
            .ip_address(Ipv4Addr::new(203, 0, 113, 9))
            .build()
    }

    fn profile() -> PaymentProfile {
        PaymentProfile::builder()
            .charges(Limits::new(100_000, 500, 5_000))
            .consumers(true)
            .sec_codes(vec![SecCode::Web, SecCode::Ppd])
            .statement_descriptor("BANANAS")
            .customer_service_phone("5558675309")
            .customer_service_email("help@example.com")
            .service_use_case("Sale of goods")
            .build()
    }

    fn application() -> MerchantApplication {
        MerchantApplication::builder()
            .business_entity(business())
            .principal_owner(owner())
            .payment_profile(profile())
            .bank_accounts(
                BankAccounts::builder()
                    .transfers(FundingAccount::bank("First Bank", "123123123", "12345678"))
                    .billing(FundingAccount::plaid("processor-sandbox-1"))
                    .build(),
            )
            .fees(FeeChoice::Profile(FeeProfileId::new("fee-1")))
            .build()
    }

    fn messages(value: &impl Validate) -> Vec<String> {
        match value.validate() {
            Ok(()) => Vec::new(),
            Err(error) => error.violations().iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn writes_the_documented_json() {
        assert!(
            messages(&application()).is_empty(),
            "{:?}",
            messages(&application())
        );
        assert_eq!(
            serde_json::to_value(application()).unwrap(),
            json!({
                "businessEntity": {
                    "legalBusinessName": "Bluth Company LLC",
                    "taxId": "123456789",
                    "companyDBA": "Bluth Bananas",
                    "industryCategory": "Digital Goods and Software",
                    "specificIndustry": "SaaS and Software Applications",
                    "type": "LLC",
                    "website": "https://example.com",
                    "phone": "5558675309",
                    "businessDescription": "Frozen bananas",
                    "address1": "1 Pier Way",
                    "city": "Newport Beach",
                    "state": "CA",
                    "zip": "92660",
                    "country": "US"
                },
                "principalOwner": {
                    "firstName": "George",
                    "lastName": "Bluth",
                    "jobTitle": "CEO",
                    "email": "george@example.com",
                    "mobilePhone": "5558675309",
                    "ssn": "987654321",
                    "ownershipPercent": "60",
                    "dob": "01/02/1950",
                    "address1": "1 Pier Way",
                    "city": "Newport Beach",
                    "state": "California",
                    "zip": "92660",
                    "ipAddress": "203.0.113.9"
                },
                "paymentProfile": {
                    "enableCharges": true,
                    "enablePayouts": false,
                    "chargeDollarsMonth": "100000",
                    "chargesPerMonth": "500",
                    "chargesMaxAmount": "5000",
                    "customerTypeConsumer": true,
                    "customerTypeBusiness": false,
                    "enablePPD": true,
                    "enableCCD": false,
                    "enableWEB": true,
                    "enableTEL": false,
                    "statementDescriptor": "BANANAS",
                    "customerServicePhone": "5558675309",
                    "customerServiceEmail": "help@example.com",
                    "serviceUseCase": "Sale of goods"
                },
                "bankAccounts": {
                    "transfers": {
                        "bankName": "First Bank",
                        "routingNumber": "123123123",
                        "accountNumber": "12345678"
                    },
                    "billing": {
                        "billingTokenType": "Plaid",
                        "billingProcessorToken": "processor-sandbox-1"
                    }
                },
                "onboardFunctions": {"requireKYC": true, "requirePlaidConnect": true},
                "feeProfileId": "fee-1"
            })
        );
    }

    #[test]
    fn writes_the_other_variants() {
        let mut application = application();
        application.business_entity.industry = Some(IndustryChoice::Custom {
            category: "Space".into(),
            specific: "Rockets".into(),
        });
        application.bank_accounts.transfers = Some(FundingAccount::plaid("tok"));
        application.fees = FeeSchedule::builder().monthly_fee(dec!(25)).build().into();
        application.authorized_representative = Some(
            AuthorizedRepresentative::builder()
                .first_name("Lucille")
                .last_name("Bluth")
                .ownership_percent(40)
                .build(),
        );
        let json = serde_json::to_value(&application).unwrap();
        assert_eq!(json["businessEntity"]["industryCategory"], "Space");
        assert_eq!(
            json["bankAccounts"]["transfers"],
            json!({"tokenType": "Plaid", "transferProcessorToken": "tok"})
        );
        assert_eq!(json["feeProfile"]["monthlyFee"], 25.0);
        assert_eq!(json["feeProfile"]["setupFee"], 0.0);
        assert!(json.get("feeProfileId").is_none());
        assert_eq!(
            json["authorizedRepresentative"],
            json!({"firstName": "Lucille", "lastName": "Bluth", "ownerShipPercent": "40"})
        );
    }

    #[test]
    fn validates_the_schema_constraints() {
        let mut application = application();
        application.business_entity.tax_id = "12-3456789".into();
        application.business_entity.address2 = Some("Suite 1000".into());
        application.business_entity.zip = "92660-1234".into();
        application.principal_owner.ownership_percent = 101;
        application.principal_owner.state = "Ontario".into();
        application.payment_profile.statement_descriptor = "FROZEN BANANAS".into();
        assert_eq!(
            messages(&application),
            [
                "business_entity.tax_id: must be at most 9 characters, not 10",
                "business_entity.tax_id: must be digits only",
                "business_entity.address2: must be at most 9 characters, not 10",
                "business_entity.zip: must be at most 5 characters, not 10",
                "business_entity.zip: must be digits only",
                "principal_owner.ownership_percent: must be at most 100, not 101",
                "principal_owner.state: must be a US state code or name, such as NY or New York",
                "payment_profile.statement_descriptor: must be at most 10 characters, not 14",
            ]
        );
    }

    #[test]
    fn a_profile_needs_a_service_a_customer_type_and_a_sec_code() {
        let mut profile = profile();
        profile.charges = None;
        profile.consumers = false;
        profile.sec_codes = vec![SecCode::from("ARC")];
        assert_eq!(
            messages(&profile),
            [
                "enable charges, payouts or both",
                "allow consumers, businesses or both",
                "sec_codes: \"ARC\" is not PPD, CCD, WEB or TEL",
            ]
        );
        profile.sec_codes.clear();
        assert!(messages(&profile).contains(&"sec_codes: list at least one SEC code".to_owned()));
        let mut limits = profile.clone();
        limits.payouts = Some(Limits::new(1, 10_000_000, 1_000_000));
        assert!(messages(&limits).contains(
            &"payouts.count_per_month: must be at most 9999999, not 10000000".to_owned()
        ));
    }

    #[test]
    fn debug_hides_personal_and_account_data() {
        let debug = format!("{:?}", application());
        for secret in [
            "123456789",
            "987654321",
            "1950",
            "12345678\"",
            "processor-sandbox-1",
        ] {
            assert!(!debug.contains(secret), "{secret} in {debug}");
        }
        assert!(debug.contains("George"), "{debug}");
    }
}
