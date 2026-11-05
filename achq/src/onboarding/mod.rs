//! The onboarding API, for platforms enrolling their merchants with ACHQ.
//!
//! This release has the industry table a merchant application classifies the
//! business with: an [`Industry`] (the specific industry) and its
//! [`IndustryCategory`]. Creating merchants and fee profiles comes in a later
//! release.

/// Defines [`IndustryCategory`] and [`Industry`] from the generated table in
/// `industry.rs`.
macro_rules! industry_table {
    (
        categories {
            $($category:ident = $category_name:literal,)+
        }
        industries {
            $($industry:ident = $industry_name:literal in $industry_category:ident,)+
        }
    ) => {
        /// The general industry of a business, as the onboarding API's
        /// `industryCategory` names it.
        #[non_exhaustive]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum IndustryCategory {
            $(
                #[doc = concat!("`\"", $category_name, "\"`")]
                $category,
            )+
        }

        impl IndustryCategory {
            /// Every category, in the documentation's order.
            pub const ALL: &'static [Self] = &[$(Self::$category,)+];

            /// The name, as sent to the API.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$category => $category_name,)+
                }
            }

            /// The specific industries in this category.
            pub fn industries(self) -> impl Iterator<Item = Industry> {
                Industry::ALL.iter().copied().filter(move |i| i.category() == self)
            }
        }

        /// A specific industry, as the onboarding API's `specificIndustry` names
        /// it. Each belongs to one [`IndustryCategory`].
        #[non_exhaustive]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Industry {
            $(
                #[doc = concat!("`\"", $industry_name, "\"`")]
                $industry,
            )+
        }

        impl Industry {
            /// Every industry, in the documentation's order.
            pub const ALL: &'static [Self] = &[$(Self::$industry,)+];

            /// The name, as sent to the API.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$industry => $industry_name,)+
                }
            }

            /// The category this industry belongs to.
            pub const fn category(self) -> IndustryCategory {
                match self {
                    $(Self::$industry => IndustryCategory::$industry_category,)+
                }
            }
        }
    };
}

#[rustfmt::skip]
mod industry;

use std::fmt;
use std::str::FromStr;

/// The text names no documented industry or category.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown {kind} {name:?}")]
pub struct UnknownIndustry {
    kind: &'static str,
    name: String,
}

pub use industry::{Industry, IndustryCategory};

impl fmt::Display for IndustryCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl fmt::Display for Industry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Reads a category by its documented name, ignoring case.
impl FromStr for IndustryCategory {
    type Err = UnknownIndustry;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|c| c.name().eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| UnknownIndustry {
                kind: "industry category",
                name: name.into(),
            })
    }
}

/// Reads an industry by its documented (specific) name, ignoring case.
impl FromStr for Industry {
    type Err = UnknownIndustry;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|i| i.name().eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| UnknownIndustry {
                kind: "industry",
                name: name.into(),
            })
    }
}

crate::validate::no_constraints!(IndustryCategory, Industry);

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn the_table_is_complete() {
        assert_eq!(Industry::ALL.len(), 150);
        assert_eq!(IndustryCategory::ALL.len(), 13);
        let total: usize = IndustryCategory::ALL
            .iter()
            .map(|c| c.industries().count())
            .sum();
        assert_eq!(total, 150);
        for category in IndustryCategory::ALL {
            assert!(
                category.industries().next().is_some(),
                "{category} is empty"
            );
        }
    }

    #[test]
    fn names_are_unique() {
        let industries: BTreeSet<&str> = Industry::ALL.iter().map(|i| i.name()).collect();
        assert_eq!(industries.len(), Industry::ALL.len());
        let categories: BTreeSet<&str> = IndustryCategory::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(categories.len(), IndustryCategory::ALL.len());
    }

    #[test]
    fn industries_know_their_category() {
        assert_eq!(Industry::Bnpl.name(), "BNPL");
        assert_eq!(
            Industry::Bnpl.category(),
            IndustryCategory::FinancialServices
        );
        assert_eq!(
            Industry::NftsDigitalCollectible.name(),
            "NFTs / Digital Collectible"
        );
        assert_eq!(
            Industry::NftsDigitalCollectible.category().name(),
            "Digital Goods and Software"
        );
    }

    #[test]
    fn reads_names_ignoring_case() {
        for industry in Industry::ALL {
            assert_eq!(industry.name().parse::<Industry>(), Ok(*industry));
        }
        assert_eq!(
            "saas and software applications".parse(),
            Ok(Industry::SaasAndSoftwareApplications)
        );
        assert_eq!(
            " TRAVEL / LODGING ".parse(),
            Ok(IndustryCategory::TravelLodging)
        );
        assert_eq!(
            "Underwater Basket Weaving"
                .parse::<Industry>()
                .unwrap_err()
                .to_string(),
            "unknown industry \"Underwater Basket Weaving\""
        );
        assert!("Retail".parse::<IndustryCategory>().is_err());
    }

    #[test]
    fn displays_the_documented_name() {
        assert_eq!(Industry::Bnpl.to_string(), "BNPL");
        assert_eq!(
            IndustryCategory::ECommerceAndRetail.to_string(),
            "eCommerce and Retail"
        );
    }
}
