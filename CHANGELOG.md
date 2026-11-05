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
- The protocol, without I/O: `protocol::encode` turns a command into an
  `HttpRequest` (every field, credentials included, in the form body) and
  `protocol::decode` reads the gateway's answer, so that any HTTP client can
  drive it. `Context` holds the `ProviderCredentials`, an optional
  `MerchantCredentials`, the `Endpoints` and test mode.
- `operations`: a type for each of the gateway's 11 commands, generated from one
  table: tokens, payments (create, update, cancel, hold, refund), account and
  owner verification, settlements and settlement details, and event tracking.
- Requests with builders and validation, including the API's cross-field rules:
  `payments::PaymentRequest` (from a `bank::BankAccount` or a token),
  `PaymentUpdate`, `RefundRequest`, `tokens::TokenRequest`,
  `insights::AccountStatusRequest` and `OwnerAuthRequest`,
  `settlements::SettlementQuery` and `SettlementDetailsQuery`, and
  `events::EventQuery`.
- Results: `PaymentCreated`, `CommandReceipt`, `TokenCreated`,
  `AccountVerification`, `OwnerVerification`, `Settlement`,
  `SettlementDetailsPage` (with paging), and `EventBatch`, read from the
  gateway's CSV event reports.
- `envelope`: the gateway's response `Envelope`, `CommandStatus`, `ResponseCode`
  and the bank verification codes; `Error` and `ApiError`, which tells
  credential, duplicate, validation and retryable failures apart.
- `Client`, the async client, with `ReqwestTransport` (feature `reqwest`, on by
  default; `fetch` on WebAssembly), and `blocking::Client` with `UreqTransport`
  (feature `ureq`). Both take any `Transport` instead. The client runs the
  platform's settlement and event reports; `Client::merchant` gives a
  `Merchant`, which runs the commands on that merchant's payments, tokens and
  bank account checks, so that a merchant command cannot be sent without a
  merchant's credentials.
- Resource handles generated from the command table, such as
  `merchant.payments().cancel(payment_id)` and `client.events().query(&query)`.
- Builder settings for the endpoints, test mode, timeout (60 seconds by
  default), user agent, and request validation.
- `tracing` logs in an `achq` span naming the operation and command, with request
  and response bodies at TRACE level; gate keys, account numbers, tokens and tax
  IDs are never logged.
- The onboarding API (feature `onboarding`): `client.onboarding().create_merchant`
  with an `onboarding::MerchantApplication`, and `create_fee_profile` with a
  `FeeSchedule`, sent as JSON with the platform's credentials in headers. Its
  errors are `Error::Onboarding`.
- `ApiError::is_account_rejected` (a payment refused for its bank account,
  including the undocumented code 202) and `ApiError::is_queued` (a new payment
  that cannot be changed yet).
- Responses recorded from ACHQ's public sandbox, as test fixtures, with
  `cargo xtask record`; and an opt-in suite against the sandbox
  (`ACHQ_SANDBOX=1`).
