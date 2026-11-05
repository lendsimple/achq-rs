# Changelog

All notable changes to `achq` and `achq-derive` are recorded here. The two crates
are released together, with the same version.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
the crates follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-05

### Added

- `Client`, the async client, with `ReqwestTransport` (feature `reqwest`, on by
  default; `fetch` on WebAssembly), and `blocking::Client` with `UreqTransport`
  (feature `ureq`). Both take any `Transport` instead, and build for
  `wasm32-unknown-unknown` and `wasm32-wasip2`.
- `ProviderCredentials` for the platform and `MerchantCredentials` for a merchant,
  with the public sandbox's as `sandbox()`. The `Client` runs the platform's
  reports; `Client::merchant` gives a `Merchant`, which runs the commands on that
  merchant's payments, so they cannot be sent without the merchant's credentials.
- Every gateway command, with typed requests and results, through resource handles
  such as `merchant.payments().cancel(payment_id)`:
  - `tokens().create`: an `AchqToken` for a bank account;
  - `payments().create`, `update`, `cancel`, `hold` and `refund`, from a
    `bank::BankAccount` or a token (`payments::PaymentSource`);
  - `insights().account_status` and `account_owner`: bank account verification;
  - `settlements().list` and `details` (with paging);
  - `events().query`: the day's payment events, read from the gateway's CSV.
- The onboarding API (feature `onboarding`, on by default):
  `onboarding().create_merchant` with an `onboarding::MerchantApplication`, and
  `create_fee_profile` with a `FeeSchedule`; and the API's industry table as
  `Industry` and `IndustryCategory`.
- Requests are checked before they are sent: lengths, routing number checksums,
  amounts with at most two decimal places, and the API's conditional rules.
  `validate::Validate` reports every violation with the path of its field.
- `Error`, with `ApiError` for commands the gateway refused, which tells
  credential, duplicate, validation, rejected-account, queued and retryable
  failures apart; and `OnboardingError` for the onboarding API.
- Exact money `types::Amount`s, the gateway's `LocalDateTime`s, and typed IDs that
  keep a payment's `PaymentId` apart from the `CommandId` of a change to it.
- `protocol::encode` and `protocol::decode`, to drive any HTTP client without the
  clients.
- `tracing` logs in an `achq` span, with request and response bodies at TRACE level;
  gate keys, account numbers, tokens, SSNs, dates of birth and tax IDs are never
  logged, nor shown by `Debug`.
- Builder settings for the endpoints (`Endpoints::custom`, for a white-labeled
  gateway), test mode, timeout (60 seconds by default), user agent, and request
  validation.
- `achq-derive`: `#[derive(Model)]`, the crate's validation, redacted `Debug` and
  form encoding.
- ACHQ's API reference vendored in `schema/`, with the differences from ACHQ's
  guides that the request types keep listed in `schema/overrides.toml`; responses
  recorded from ACHQ's public sandbox as test fixtures; and an opt-in test suite
  against the sandbox.

[Unreleased]: https://github.com/lendsimple/achq-rs/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/lendsimple/achq-rs/releases/tag/v0.1.0
