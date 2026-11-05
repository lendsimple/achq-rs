# Changelog

All notable changes to `achq` and `achq-derive` are recorded here. The two crates
are released together, with the same version.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
the crates follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `types::Amount`, an exact money amount: sent in form bodies with exactly two
  decimal places (`42.00`), and read from a JSON number or string without float
  rounding. Validation rejects negative amounts and more than two decimal places.
- `types::LocalDateTime`, the gateway's offset-free date-times, read from both
  `2023-12-01T00:00:00` and `06/17/2022 05:00:59`.
- Typed IDs: `PaymentId` and `CommandId` (both from `TransAct_ReferenceID`, kept
  apart because only a payment's ID identifies it later), `ProviderTransactionId`,
  `MerchantReferenceId`, `MerchantId`, `SettlementId`, `FeeProfileId`,
  `OnboardingId`, and `AchqToken`, which `Debug` hides. IDs are read trimmed.
- `form`: `FormWriter`, the gateway's `application/x-www-form-urlencoded` bodies
  with sensitive values hidden by `redacted()`; the `ToForm` and `FormValue`
  traits; and `form::with` formats for Yes/No, On/Off and the API's three date
  formats.
- `validate`: `Validate`, reporting every violation with its field path, and the
  rules `digits`, `routing_number` (with the ABA checksum), `zip`, `email` and
  `us_state`.
- `achq-derive`: `#[derive(Model)]`, implementing `Validate` and a redacting
  `Debug`, and with `#[achq(form)]` also `ToForm`: wire names, flattened structs
  with a prefix (`Billing_`), `with` formats, and a description of each field.
- `onboarding::Industry` and `onboarding::IndustryCategory` (feature `onboarding`,
  on by default): the onboarding API's 150 documented industries in 13
  categories, generated from its documentation, with their names, each
  industry's category, and parsing by name.
- The ACHQ API reference's OpenAPI fragments and industry table, vendored in
  `schema/` with their sources and checksums.
