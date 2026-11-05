//! Bank accounts, as payments and tokens describe them.

use crate::Model;
use crate::macros::string_enum;

string_enum! {
    case_insensitive;
    /// The kind of deposit account.
    pub enum AccountType {
        Checking = "Checking",
        Savings = "Savings",
    }
}

string_enum! {
    case_insensitive;
    /// Whose account it is: a person's or a business's.
    pub enum CheckType {
        /// A consumer account.
        Personal = "Personal",
        /// A business account. Payments from or to one name the company
        /// (`Billing_Company`).
        Business = "Business",
    }
}

/// A bank account, by routing and account number.
///
/// ```
/// use achq::bank::{BankAccount, CheckType};
///
/// let account = BankAccount::checking("123123123", "12345678").business();
/// assert_eq!(account.check_type, CheckType::Business);
/// assert!(!format!("{account:?}").contains("12345678"));
/// ```
#[derive(Clone, PartialEq, Model, bon::Builder)]
#[builder(on(String, into))]
#[achq(form)]
#[non_exhaustive]
pub struct BankAccount {
    /// The ABA routing number: 9 digits with a valid checksum.
    #[achq(form = "RoutingNumber", rule = "routing_number")]
    pub routing_number: String,
    /// The account number. NACHA allows letters and hyphens in it, so only its
    /// length is checked here.
    #[achq(form = "AccountNumber", sensitive, max_length = 30)]
    pub account_number: String,
    /// Checking or savings.
    #[achq(form = "AccountType")]
    pub account_type: AccountType,
    /// Personal or business.
    #[builder(default = CheckType::Personal)]
    #[achq(form = "CheckType")]
    pub check_type: CheckType,
}

impl BankAccount {
    /// A personal checking account.
    pub fn checking(routing_number: impl Into<String>, account_number: impl Into<String>) -> Self {
        Self::new(routing_number, account_number, AccountType::Checking)
    }

    /// A personal savings account.
    pub fn savings(routing_number: impl Into<String>, account_number: impl Into<String>) -> Self {
        Self::new(routing_number, account_number, AccountType::Savings)
    }

    fn new(
        routing_number: impl Into<String>,
        account_number: impl Into<String>,
        account_type: AccountType,
    ) -> Self {
        Self {
            routing_number: routing_number.into(),
            account_number: account_number.into(),
            account_type,
            check_type: CheckType::Personal,
        }
    }

    /// The same account, held by a business.
    pub fn business(mut self) -> Self {
        self.check_type = CheckType::Business;
        self
    }

    /// The same account, held by a person.
    pub fn personal(mut self) -> Self {
        self.check_type = CheckType::Personal;
        self
    }

    /// The last four digits of the account number, to show the account to its
    /// owner without revealing it.
    pub fn last_four(&self) -> &str {
        let len = self.account_number.len();
        self.account_number
            .get(len.saturating_sub(4)..)
            .unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use crate::form::ToForm;
    use crate::validate::Validate;

    use super::*;

    #[test]
    fn writes_its_fields() {
        let account = BankAccount::savings("123123123", "12345678").business();
        assert_eq!(
            account.to_form().redacted(),
            "RoutingNumber=123123123&AccountNumber=[REDACTED]&AccountType=Savings&CheckType=Business"
        );
        assert_eq!(account.last_four(), "5678");
        assert_eq!(BankAccount::checking("123123123", "12").last_four(), "12");
    }

    #[test]
    fn validates_the_numbers() {
        assert!(
            BankAccount::checking("123123123", "12345678")
                .validate()
                .is_ok()
        );
        assert!(
            BankAccount::checking("123123123", "1234-56A")
                .validate()
                .is_ok()
        );
        let error = BankAccount::checking("123456789", "1".repeat(31))
            .validate()
            .unwrap_err();
        let messages: Vec<String> = error.violations().iter().map(ToString::to_string).collect();
        assert_eq!(
            messages,
            [
                "routing_number: is not a valid routing number (checksum mismatch)",
                "account_number: must be at most 30 characters, not 31",
            ]
        );
    }

    #[test]
    fn the_builder_defaults_to_personal() {
        let account = BankAccount::builder()
            .routing_number("123123123")
            .account_number("1")
            .account_type(AccountType::Checking)
            .build();
        assert_eq!(account.check_type, CheckType::Personal);
    }
}
