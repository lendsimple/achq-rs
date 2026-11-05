//! Gateway credentials, and where requests go.

use std::fmt;

use secrecy::{ExposeSecret, SecretString};

use crate::types::MerchantId;

/// A platform's credentials: its Provider ID, gate ID and gate key, sent with every
/// request.
///
/// The gate key is held in a [`SecretString`] and never appears in `Debug` output.
/// It lets the holder move money for every merchant of the platform: keep it on a
/// server, and never ship it to a browser.
///
/// ```
/// use achq::ProviderCredentials;
///
/// let credentials = ProviderCredentials::new("99", "test", "s3cret");
/// assert_eq!(credentials.provider_id(), "99");
/// assert!(!format!("{credentials:?}").contains("s3cret"));
/// ```
#[derive(Clone)]
pub struct ProviderCredentials {
    provider_id: String,
    gate_id: String,
    gate_key: SecretString,
}

impl ProviderCredentials {
    /// The `ProviderID`, `Provider_GateID` and `Provider_GateKey` ACHQ assigned.
    pub fn new(
        provider_id: impl Into<String>,
        gate_id: impl Into<String>,
        gate_key: impl Into<SecretString>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            gate_id: gate_id.into(),
            gate_key: gate_key.into(),
        }
    }

    /// The public sandbox platform: Provider ID `99`, gate ID and key `test`.
    ///
    /// Payments sent with it are stored and reported but never processed. Its
    /// data is visible to anyone using the sandbox, so send only made-up names,
    /// addresses and accounts.
    pub fn sandbox() -> Self {
        Self::new("99", "test", "test")
    }

    /// The `ProviderID`.
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// The `Provider_GateID`.
    pub fn gate_id(&self) -> &str {
        &self.gate_id
    }

    /// The `Provider_GateKey`.
    pub fn gate_key(&self) -> &SecretString {
        &self.gate_key
    }

    pub(crate) fn expose_key(&self) -> &str {
        self.gate_key.expose_secret()
    }
}

impl fmt::Debug for ProviderCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderCredentials")
            .field("provider_id", &self.provider_id)
            .field("gate_id", &self.gate_id)
            .finish_non_exhaustive()
    }
}

/// A merchant's credentials: its Merchant ID, gate ID and gate key, sent with the
/// commands that act on the merchant's payments.
///
/// As with [`ProviderCredentials`], the gate key never appears in `Debug` output.
#[derive(Clone)]
pub struct MerchantCredentials {
    merchant_id: MerchantId,
    gate_id: String,
    gate_key: SecretString,
}

impl MerchantCredentials {
    /// The `MerchantID`, `Merchant_GateID` and `Merchant_GateKey` ACHQ assigned.
    pub fn new(
        merchant_id: impl Into<MerchantId>,
        gate_id: impl Into<String>,
        gate_key: impl Into<SecretString>,
    ) -> Self {
        Self {
            merchant_id: merchant_id.into(),
            gate_id: gate_id.into(),
            gate_key: gate_key.into(),
        }
    }

    /// The public sandbox merchant: Merchant ID `2001`, gate ID and key `test`.
    /// See [`ProviderCredentials::sandbox`].
    pub fn sandbox() -> Self {
        Self::new("2001", "test", "test")
    }

    /// The `MerchantID`.
    pub fn merchant_id(&self) -> &MerchantId {
        &self.merchant_id
    }

    /// The `Merchant_GateID`.
    pub fn gate_id(&self) -> &str {
        &self.gate_id
    }

    /// The `Merchant_GateKey`.
    pub fn gate_key(&self) -> &SecretString {
        &self.gate_key
    }

    pub(crate) fn expose_key(&self) -> &str {
        self.gate_key.expose_secret()
    }
}

impl fmt::Debug for MerchantCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MerchantCredentials")
            .field("merchant_id", &self.merchant_id)
            .field("gate_id", &self.gate_id)
            .finish_non_exhaustive()
    }
}

/// The gateway, which every payment, token, verification and report command goes
/// to. The sandbox and production share it; the credentials decide which a
/// request uses.
pub const GATEWAY_URL: &str = "https://www.speedchex.com/datalinks/transact.aspx";

/// The base URL of the onboarding API.
pub const API_URL: &str = "https://www.speedchex.com/api";

/// Where requests go: the gateway, and the onboarding API.
///
/// The defaults are ACHQ's ([`GATEWAY_URL`] and [`API_URL`]). Use
/// [`Endpoints::custom`] for a white-labeled gateway or a mock server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    gateway: String,
    api: String,
}

impl Endpoints {
    /// The gateway's URL (the full URL of `transact.aspx`) and the onboarding API's
    /// base URL.
    pub fn custom(gateway: impl Into<String>, api: impl Into<String>) -> Self {
        Self {
            gateway: gateway.into(),
            api: api.into().trim_end_matches('/').to_owned(),
        }
    }

    /// The gateway's URL.
    pub fn gateway(&self) -> &str {
        &self.gateway
    }

    /// The onboarding API's base URL, without a trailing `/`.
    pub fn api(&self) -> &str {
        &self.api
    }
}

impl Default for Endpoints {
    fn default() -> Self {
        Self::custom(GATEWAY_URL, API_URL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_the_gate_keys() {
        let provider = format!("{:?}", ProviderCredentials::new("7", "gate", "s3cret"));
        assert_eq!(
            provider,
            r#"ProviderCredentials { provider_id: "7", gate_id: "gate", .. }"#
        );
        let merchant = format!("{:?}", MerchantCredentials::new("2001", "gate", "s3cret"));
        assert_eq!(
            merchant,
            r#"MerchantCredentials { merchant_id: MerchantId("2001"), gate_id: "gate", .. }"#
        );
    }

    #[test]
    fn sandbox_credentials() {
        let provider = ProviderCredentials::sandbox();
        assert_eq!(
            (
                provider.provider_id(),
                provider.gate_id(),
                provider.expose_key()
            ),
            ("99", "test", "test")
        );
        let merchant = MerchantCredentials::sandbox();
        assert_eq!(merchant.merchant_id().as_str(), "2001");
        assert_eq!(
            (merchant.gate_id(), merchant.expose_key()),
            ("test", "test")
        );
    }

    #[test]
    fn endpoints() {
        let default = Endpoints::default();
        assert_eq!(default.gateway(), GATEWAY_URL);
        assert_eq!(default.api(), API_URL);
        let custom = Endpoints::custom(
            "http://localhost:1/transact.aspx",
            "http://localhost:1/api/",
        );
        assert_eq!(custom.api(), "http://localhost:1/api");
    }
}
