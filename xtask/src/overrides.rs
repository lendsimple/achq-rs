//! `schema/overrides.toml`: the intentional differences between the vendored
//! documentation and the crate.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    /// Gateway operations with no request type registered yet.
    #[serde(default)]
    pub unregistered: Vec<String>,
    #[serde(default)]
    pub industry: IndustryOverrides,
    /// Accepted differences per gateway operation, by fragment name.
    #[serde(default)]
    pub gateway: BTreeMap<String, OpOverrides>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndustryOverrides {
    /// Variant names, by the documented name, where heck's would read badly.
    #[serde(default)]
    pub names: BTreeMap<String, String>,
}

/// Each map is wire name to the reason for the difference.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpOverrides {
    /// In the fragment, but deliberately not in the request type.
    #[serde(default)]
    pub missing: BTreeMap<String, String>,
    /// In the request type, though the fragment omits it.
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
    /// In both, but the type disagrees with the fragment on whether it is required.
    #[serde(default)]
    pub required: BTreeMap<String, String>,
}

impl Overrides {
    pub fn parse(text: &str) -> Result<Self, String> {
        let overrides: Self =
            toml::from_str(text).map_err(|e| format!("schema/overrides.toml: {e}"))?;
        for (op, entries) in &overrides.gateway {
            for (kind, map) in [
                ("missing", &entries.missing),
                ("extra", &entries.extra),
                ("required", &entries.required),
            ] {
                if let Some((wire, _)) = map.iter().find(|(_, reason)| reason.trim().is_empty()) {
                    return Err(format!(
                        "schema/overrides.toml: gateway.{op}.{kind}.{wire:?} needs a reason"
                    ));
                }
            }
        }
        Ok(overrides)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_section() {
        let overrides = Overrides::parse(
            r#"
            unregistered = ["holdpayment"]
            [industry.names]
            "NFTs / Digital Collectible" = "NftsDigitalCollectible"
            [gateway.bankpayment]
            extra = { Run_ExpressVerify = "documented in the guide only" }
            "#,
        )
        .unwrap();
        assert_eq!(overrides.unregistered, ["holdpayment"]);
        assert_eq!(overrides.industry.names.len(), 1);
        assert_eq!(overrides.gateway["bankpayment"].extra.len(), 1);
    }

    #[test]
    fn rejects_unknown_keys_and_missing_reasons() {
        assert!(Overrides::parse("unregisterd = []").is_err());
        assert!(Overrides::parse("[gateway.x]\nrenamed = {}").is_err());
        let error = Overrides::parse("[gateway.x]\nextra = { A = \" \" }").unwrap_err();
        assert_eq!(
            error,
            "schema/overrides.toml: gateway.x.extra.\"A\" needs a reason"
        );
    }
}
