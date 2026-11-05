# achq-rs — Implementation Plan

## Context

This is an idiomatic Rust client for the **ACHQ platform API** (formerly SpeedChex): bank (ACH) payments, bank-account tokenization, account insights, settlements, payment event tracking, and merchant onboarding. It runs on native targets and on `wasm32`.

It mirrors the architecture of `~/Web/repay-rs`, which is itself derived from `~/Web/authorizenet-rs`:
- a sans-IO protocol core (`protocol::encode` / `protocol::decode`);
- pluggable transports: `Transport` traits with `Send` on native and no `Send` on wasm32, `ReqwestTransport` (async, also wasm32 via fetch) and `UreqTransport` (blocking);
- an `operation_table!` callback-macro DSL that generates operation structs and client handles;
- a `-derive` proc-macro crate (redacted `Debug` and `Validate`, plus form encoding here);
- `string_enum!`/ID macros with an `Other(String)` fallback, and `bon` builders;
- the same CI, deny, release and README conventions.

**Main difference.** Most of ACHQ is a single legacy ASP.NET endpoint (`transact.aspx`), driven by a `Command` field with **form-encoded** requests. It answers with a JSON envelope, except event tracking, which answers in CSV. A second, newer surface (Onboarding) is JSON/REST with header auth. So the core is closer to authorizenet-rs (one URL, command-dispatched) than to repay-rs (many paths), but it reuses repay-rs's serde/JSON runtime.

**Specs.** There is no single spec. The ReadMe docs embed one OpenAPI fragment per reference page, and `https://developers.achq.com/llms.txt` indexes every page as Markdown (`<page>.md`).

| Source | What it has |
|---|---|
| 14 gateway fragments (OpenAPI 3.1, `/reference/{banktoken,tokenizedpayment,bankpayment,updatepayment,cancelpayment,holdpayment,refundpayment,get-settlements,settlement-details,eventmonitor,bank-account-status,bankaccountid}.md`) | Request fields only (all `type: string`, with a handful of wrong types). **No response schemas.** Server `https://www.speedchex.com/datalinks/transact.aspx`. |
| 2 onboarding fragments (OpenAPI 3.0.1, `/reference/{merchant-onboarding,post_onboarding-fees}.md`) | Full request and response component schemas with `maxLength`/`required`. Server `https://www.speedchex.com/api`. |
| Guides (`/docs/*.md`) | Response samples, max-length tables, conditional-required rules, status/event tables, verification code tables. |
| `orkestra/orkestra-transactor` (PHP, `PaymentsXpress/AchTransactor.php`) | The **only open-source client found**: a white-label of the same gateway. It shows credentials in the POST body, an undocumented `ResponseType=JSON`, the CSV trailer row, and real envelope fixtures. |

No other open-source client exists. GitHub code search for `speedchex.com/datalinks`, `Merchant_GateKey` and `ECheck.ProcessPayment` found only orkestra.

### Decisions already made

| Topic | Decision |
|---|---|
| Scope | All gateway commands (tokens, the payment lifecycle, account insights, settlements, event tracking) **plus** the Onboarding API behind the `onboarding` feature (on by default). |
| Credentials model | `Client` holds the **platform** (Provider) credentials. `client.merchant(MerchantCredentials)` returns a cheap scoped `Merchant<'_, T>` handle for merchant commands. Reports and onboarding hang off `Client` directly. |
| Sandbox | Live calls to the public sandbox are allowed, **opt-in only**: `#[ignore]` tests gated on `ACHQ_SANDBOX=1`, plus a `cargo xtask record` fixture recorder. They never run in the default `cargo test`. |
| Codegen | Request and onboarding models are **hand-written**, because the fragments are too small and inaccurate to generate from. A **conformance test** checks them against the vendored fragments, with intentional deviations listed in `schema/overrides.toml`. The xtask generates only the 150-row industry-category table. |
| Money | `Amount(Decimal)`, sent as a form string with exactly 2 decimals (`"42.00"`) and read losslessly from JSON numbers. |
| Clients | Async (reqwest, default; wasm via fetch) and blocking (ureq), both generated from one `operation_table!`. |
| Errors | A non-`Approved` `CommandStatus` is an `Error::Api` carrying the full envelope. This is unlike repay, where declines are not errors: in ACHQ a "Declined" is a gateway/validation refusal, not an issuer decision, because ACH outcomes arrive later via events. |
| Naming | The crate is `achq` (free on crates.io), with `achq-derive` and `xtask`. Workspace `achq-rs`. Repo assumed to be `lendsimple/achq-rs` (open item). |
| Toolchain | Edition 2024, resolver 3, MSRV 1.88, MIT, `#[non_exhaustive]` on every public type. Same as the siblings. |

## What the API tells us (subtleties to honor)

### Transport and protocol
- **One gateway URL serves every environment:** `https://www.speedchex.com/datalinks/transact.aspx`.
  - **Sandbox vs production is decided by credentials**, not by host. The public sandbox uses ProviderID `99`, `Provider_GateID`/`Provider_GateKey` `test`/`test`, MerchantID `2001`, and merchant gate `test`/`test`.
  - **"Development mode"** means production credentials plus `TestMode=On`: the gateway responds but stores nothing.
  - So `Environment` is not a sandbox/production pair. The plan is:
    - `Endpoints { gateway, api }`, defaulting to the speedchex URLs, with `Endpoints::custom(..)` for white-labels such as PaymentsXpress;
    - `Credentials::sandbox()` constructors;
    - a `test_mode(bool)` builder flag.
- **All fields go in the form body.** The ReadMe "Try it" curl puts credentials in the **query string**, along with a meaningless selector key (`&bankaccountpayment=`). The guide says POST *or* GET fields are accepted, and orkestra posts everything as form fields.
  - We send `ProviderID`, gate keys, `Command`, `CommandVersion=2.0` and the payload as `application/x-www-form-urlencoded` body fields. Secrets never appear in URLs (where they would be logged by proxies), and the query string stays empty.
- **`Accept: application/json`** is always sent. orkestra also sends `ResponseType=JSON`, which is undocumented. Whether it is needed (for example, whether the response is XML without it) is checked against the sandbox (open item).
- **HTTP status is meaningless on the gateway.** It is presumably always 200, and success or failure is in the envelope. A non-2xx response becomes `Error::Status`.
- **Response envelope (JSON), the same shape for every gateway command except events:**
  ```json
  {"CommandStatus":"Approved","Description":"Command Successful.  Approved.","ErrorInformation":null,
   "ExpressVerify":{"Status":null,"Code":null,"Description":null},"ResponseData":null,
   "ACHQToken":"achq-sandbox-…","Provider_TransactionID":"21321312","TransAct_ReferenceID":"14469220","ResponseCode":"000"}
  ```
  - `CommandStatus` ∈ `Approved | Declined | Error`, with `Other` as a fallback.
  - `ResponseCode` is a **string**: `"000"` on the wire, but `0` in the docs table. It is parsed into `ResponseCode(u16)` with named constants for the 33 documented codes (100 invalid credentials … 609 invalid merchant configuration).
  - The response key is spelled **`TransAct_ReferenceID`**, but the docs table says `Transact_ReferenceID`. We deserialize with both as aliases.
  - **`ResponseData` is polymorphic:** `null` for payment commands, an array for settlements, an object for settlement details. It is modeled as a type parameter, `Envelope<D>`.
  - The settlement-details sample **omits `ResponseCode`**, so every envelope field is `Option`/defaulted.
  - orkestra fixtures show extra legacy keys (`AVS`, `CVN`, `PaymentKey`, `AuthorizationCode`). Unknown fields are ignored.
  - A bad-credentials response is `CommandStatus:"Declined"`, `ResponseCode:"100"`, `ErrorInformation:"Provider_Credentials"`, so `ErrorInformation` names the offending field. `ApiError::field()` exposes it.
- **Every command gets its own `TransAct_ReferenceID`.** Update, cancel and hold return a **new** reference ID, not the payment's. The payment's ID is the one from create (and refund creates a new credit, which gets its own).
  - The types must make this hard to misuse: `PaymentCreated { payment_id: PaymentId, token, .. }` is distinct from `CommandReceipt { command_id: CommandId }`. Docs warn: "store the `PaymentId` from create; don't overwrite it with receipts".
- **Event tracking returns CSV, not JSON.** Each row has 10 or 11 fields. The docs list 11 (Return Code *and* Return Explanation); orkestra fixtures have 10.
  - The response ends with a trailer row, `Command Response,<Status>,<Code>,<Description>,<ErrorInfo>,<RefId>,,,`, which the docs don't mention.
  - Parse with the `csv` crate in `flexible` mode with no headers. The trailer is the envelope. If the body starts with `{`, decode it as a JSON error envelope (to be confirmed in the sandbox).
- **Onboarding API (`https://www.speedchex.com/api`):**
  - **Requests:** JSON (camelCase) `POST /onboarding` and `POST /onboarding/fees`, with auth in **headers** `ProviderID`, `Provider_GateID`, `Provider_GateKey`.
  - **Responses:** 200 is `{id|feeProfileId, status, apiRequestId, apiRequestTimeStamp}`. 400 is `ErrorResponse {errorCode, message, additionalInformation, apiRequestId, apiRequestTimeStamp}`.
  - **Negotiation:** it advertises JSON, XML and text/plain, so `Accept: application/json` is required.
- **IP allow-listing:** platforms may be IP-filtered (response code 109, "Invalid Source IP"). This is documented in the README. It matters especially for wasm, where browser traffic will fail.

### Commands and fields
| Op | `Command` | Creds | Notable fields |
|---|---|---|---|
| Create token | `ECheck.CreateACHQToken` | merchant | `CheckType`, `AccountType`, `RoutingNumber`, `AccountNumber` (the fragment marks only the first two required; all four are, per the guide) |
| Create payment (bank / token) | `ECheck.ProcessPayment` | merchant | Billing block, `SECCode`, `PaymentDirection`, `DeliveryWindow`, `Amount`, plus either bank fields + `Create_ACHQToken`, or `AccountToken` + `TokenSource` |
| Update | `ECheck.Update` | merchant | `Transact_ReferenceID` + any create field |
| Cancel | `ECheck.Void` | merchant | `Transact_ReferenceID` |
| Hold | `ECheck.Hold` | merchant | `Transact_ReferenceID` (the guide's sample wrongly shows `ECheck.Void`) |
| Refund | `ECheck.Refund` | merchant | **`TransAct_ReferenceID`** (capital A, in this op only), optional `Amount` (blank = full refund), `DateScheduled`, IDs, `Description` |
| Account status | `ECheck.VerifyPlus` | merchant | `RoutingNumber`, `AccountNumber` |
| Account ID (owner auth) | `ECheck.VerifyPlus` + `OwnerAuth=On` | merchant | + `CustomerName`/`Company`, address, `Phone Number`(sic, open item), `TaxID` |
| Get settlements | `ECheckReports.GetSettlements` | **platform only** | `DateFrom`, `DateTo` (ISO accepted), optional `MerchantID` |
| Settlement details | `ECheckReports.GetSettlementItems` | platform only | `SettlementID` (UUID), `PageSize` ≤ 500, `PageNumber` |
| Event tracking | `ECheckReports.StatusTrackingQuery` | platform only | `TrackingDate` (`MMDDYYYY` or `MM/DD/YYYY`), optional `MerchantID` |
| Create merchant | JSON `POST /onboarding` | platform (headers) | Nested `MerchantOnboardingRequest` |
| Create fee profile | JSON `POST /onboarding/fees` | platform (headers) | `CreateFeeRequest` |

- **Wire names are sent exactly as documented per op**, including the refund `TransAct_` spelling. ASP.NET form binding is probably case-insensitive; this is verified once in the sandbox and recorded.
- **The guides add fields the fragments omit:**
  - `Run_ExpressVerify` (Yes/No; "Required" in the guides, absent from the fragments). Optional, sent only if set; open item.
  - `Billing_Company`: required if `CheckType=Business`.
  - `Billing_CustomerID` (≤ 20).
  - `TestMode` (On).
- **Conditional rules, validated client-side:**
  - `Billing_Company` is required when `CheckType=Business`.
  - `Customer_IPAddress` is required only when `SECCode=WEB`. The fragment says it is always required; the guide says otherwise, and we follow the guide.
  - Token payments need `TokenSource`.
- **Max lengths** (guide tables): CustomerName 80, Company 80, Address1 70, Address2 40, City 70, State 30, Zip 10 (`#####` or `#####-####`), Country 2, Phone 20, Email 80, Description 100, IDs 50, RoutingNumber 9, AccountNumber 30, AccountToken 100, Customer_IPAddress 15. The last means **IPv4 only**, so `std::net::Ipv4Addr` is typed, and IPv6 is an open item.
- **Enumerations:**
  - `PaymentDirection` FromCustomer|ToCustomer;
  - `SecCode` PPD|CCD|WEB|TEL;
  - `DeliveryWindow` FirstAvailable|Standard (Same-Day needs merchant enablement);
  - `CheckType` Personal|Business;
  - `AccountType` Checking|Savings;
  - `TokenSource` ACHQ|Plaid|MX;
  - Yes/No and On/Off booleans.
  - All of these are `string_enum!` with `Other`.
- **Dates come in five formats:**
  - `DateScheduled` / dob: `MM/DD/YYYY`;
  - `TrackingDate`: `MMDDYYYY`;
  - settlement `DateFrom`/`DateTo`: ISO `YYYY-MM-DD`;
  - responses `EffectiveDate`/`ProcessDate`: `2023-12-01T00:00:00` (no offset);
  - event timestamps: `MM/DD/YYYY HH:MM:SS` in **US Central** time.
  - Requests take `time::Date`, and each op formats it. Responses use `PrimitiveDateTime`, documented as the gateway's local time; no tz database dependency.
- **Settlement responses:**
  - `Amount` is a JSON **number**, read losslessly as with repay's `Amount`.
  - The list endpoint uses `TransferDirection` but the details parent uses `PaymentDirection` (both Credit|Debit). Both become a `TransferDirection` enum, read through an alias.
  - The sample `MerchantID` is `" 2001"` (leading space), so it is trimmed.
  - `SettlementType` is a free-text enum ("Settlement for ACH Debits", "Offset for ACH Credits", "Charge Back for Late Disputes/Returns", "Offset Return"), with `Other`.
  - Item `MerchantReferenceID` drops the underscore that the request field `Merchant_ReferenceID` has.
- **Events and statuses:**
  - **Events:** Created, Cancelled, Failed Verification, Submitted, Returned-NSF, Returned-Other, Cleared, Charged Back, Held by Merchant, Held by Processor, Hold Released. The samples also show the undocumented **Authorization**, and `Rejected` as an event name.
  - **Statuses:** Scheduled, Cancelled, Rejected, In-Process (docs say "In-process"), Returned-NSF, Returned-Other, Cleared, Charged Back, Merchant Hold, Processor Hold, Hold Released, plus the undocumented **Authorized**. orkestra's fixtures put "Failed Verification" in the *status* column, so the docs and samples disagree on which column holds which name.
  - Both columns are `string_enum!` with `Other`, accept either spelling, and **compare case-insensitively** ("In-process" vs "In-Process").
  - `PaymentStatus` exposes `is_final()` and `is_returned()`, which help a platform's state machine.
  - Return codes are `R01…`, a `ReturnCode` `string_enum!` with the NACHA descriptions from the docs table, plus `Other`.
- **Verification results (`ExpressVerify` / `OwnerAuth`):**
  - Status is POS|NEG|UNK|ERR. Codes are 4 characters (`1111`, `RT03`, `CA11`, `0`).
  - The fields are `AccountCreated`, `AccountLastSeen` and `BankName`.
  - `VerificationCode` is a `string_enum!` carrying the documented meaning.
  - **There is no response sample for VerifyPlus.** The `OwnerAuth` object's placement and casing are captured from the sandbox before the type is finalized (Stage 5).
  - Whether a NEG verification comes back as `Approved` + NEG (data) or as `Declined` (error) is also captured. The design returns it as data if `CommandStatus` is Approved.
- **Duplicate and idempotency semantics:**
  - Code 102 is "Duplicate Command Not Processed"; 107 is "Non-Unique Reference/Transaction ID". So `Provider_TransactionID` acts as a de-facto idempotency key.
  - We **never auto-retry**. The README recommends always setting `provider_transaction_id`, and `ApiError::is_duplicate()` is provided.
- **Lifecycle constraints:**
  - Update, cancel and hold work only while the payment is `Scheduled` (or `Merchant Hold`).
  - Refund works only when `Cleared`.
  - Hold has no documented "release". Whether `ECheck.Update` releases it is an open item.
  - Cut-off times (9:00, 11:15, 14:15, 19:00 ET) are documented; they are not enforced.
- **Onboarding schema quirks:**
  - **Strings for numbers:** `ownershipPercent` (≤3), phone fields exactly 10 digits, `taxId`/`ssn` 9 digits, `zip` ≤5, and `dob` `MM/DD/YYYY`. These get validators: `digits(n)` and `us_state_name_or_code`.
  - **Fragment `maxLength` bugs:** `BusinessEntity.address2` has `maxLength: 9` and `transferProcessorToken` has `maxLength: 10`. The latter is impossible for Plaid tokens. Both are overridden with reasons.
  - **Conditional requirements:**
    - charge limits are required if `enableCharges`, and payout limits if `enablePayouts`;
    - at least one of `customerTypeConsumer`/`customerTypeBusiness` must be set;
    - at least one `enablePPD|CCD|WEB|TEL` must be set;
    - `feeProfileId` XOR `feeProfile`;
    - transfers and billing take either a bank account or a token.
    - These map to container `one_of` / `rule` checks and source enums.
  - `industryCategory`/`specificIndustry` must be a pair from the 150-row table. They are generated as `Industry` (an enum of valid pairs) with `category()` and `specific()`, and serialized into two fields.
  - Fee amounts are `number/double`, so they are `Amount` as numbers. `percentFee` is a percentage, so it is `Decimal`.
  - **Highly sensitive fields:** `ssn`, `taxId`, `dob`, account numbers and tokens are `#[achq(sensitive)]`.

## Architecture

### Workspace layout
```
achq-rs/
├── Cargo.toml                # workspace: achq, achq-derive, xtask; edition 2024, MSRV 1.88
├── .cargo/config.toml        # alias xtask; env ACHQ_UI_TESTS=1
├── .github/workflows/{ci,release}.yml, dependabot.yml
├── deny.toml  LICENSE  CHANGELOG.md  RELEASING.md  README.md  implementation.md
├── scripts/test-packaged-crate.sh
├── schema/
│   ├── README.md             # source URL, fetch date, SHA-256 per file; "Updating"
│   ├── gateway/*.json        # 12 extracted OpenAPI fragments (one per reference page)
│   ├── onboarding.json       # merged onboarding OpenAPI 3.0.1 components
│   ├── industry-categories.md
│   └── overrides.toml        # every intentional deviation, each with a reason
├── achq-derive/src/{lib,attrs,model,form}.rs
├── achq/
│   ├── src/
│   │   ├── lib.rs            # docs, re-exports, `extern crate self as achq`, README doctests
│   │   ├── macros.rs         # string_enum!, string_id!, yes_no/on_off helpers
│   │   ├── types/{amount,date,ids,ip}.rs
│   │   ├── validate.rs       # Validate, Violation, Path, rules::{digits, zip, email, ...}
│   │   ├── form.rs           # FormWriter (percent-encoding, field order kept), ToForm trait
│   │   ├── credentials.rs    # ProviderCredentials, MerchantCredentials, ::sandbox()
│   │   ├── config.rs         # Endpoints, Config, redaction, tracing span, common_builder_methods!
│   │   ├── envelope.rs       # Envelope<D>, CommandStatus, ResponseCode, ExpressVerify
│   │   ├── operation.rs      # Operation trait, Scope, Body, decode kinds
│   │   ├── operations.rs     # operation_table! + impl_operations!
│   │   ├── protocol.rs       # sans-IO encode/decode, Context
│   │   ├── transport.rs      # HttpRequest/Response, Transport, ReqwestTransport
│   │   ├── blocking.rs       # blocking Transport, UreqTransport, blocking Client
│   │   ├── client.rs         # Client<T>, Merchant<'_, T>, resource handles
│   │   ├── error.rs          # Error, ApiError, OnboardingError, TransportError
│   │   ├── payments.rs       # PaymentRequest, PaymentSource, Billing, Update/Refund, results
│   │   ├── tokens.rs  insights.rs  settlements.rs  events.rs (CSV parser)
│   │   └── onboarding/{mod,models,industry}.rs   # industry.rs is @generated
│   ├── examples/{tokenize_and_charge,refund,settlements,events_poll,onboard_merchant,charge_blocking}.rs
│   └── tests/{common/mod.rs, async_client, blocking_client, custom_transport, requests,
│              responses, events_csv, conformance, model_derive, derive_ui, logging, sandbox}.rs
│              + tests/data/{responses/*.json, events/*.csv}, tests/snapshots/, tests/ui/
└── xtask/src/{main,fetch,industry,record}.rs
```

### Dependencies (`achq`)
- **Runtime:**
  - `bon`, `serde` + `serde_json`, `rust_decimal` (macros), `time` (parsing, formatting, macros);
  - `secrecy`, `thiserror` 2, `tracing`, `csv`;
  - `reqwest` (optional, default-features off), `ureq` (optional);
  - `achq-derive` pinned `=x.y.z`.
- **Features:** `default = ["reqwest", "rustls", "onboarding"]`, plus `ureq`, `native-tls` and `onboarding`.
- **Dev:** `insta`, `mockito`, `tokio`, `tracing-subscriber`, `trybuild`, `pretty_assertions`.
- **Not used, deliberately:**
  - `serde_urlencoded`: field order, `Yes`/`No` mapping, `Billing_` prefix flattening and redaction are easier with our own `FormWriter` + derive.
  - `regex`: hand-written rules instead.
  - `chrono-tz`: Central time is documented, not converted.
  - `wasm-bindgen`: reqwest's wasm backend is enough, as in the siblings.

### Layer 1: Runtime types and the derive DSL
- **`string_enum!`, `string_id!`:** copied from repay-rs `macros.rs`, plus a `case_insensitive` arm for event and status names.
  - IDs: `PaymentId` and `CommandId` (both `TransAct_ReferenceID`, distinct types on purpose), `ProviderTransactionId`, `MerchantReferenceId`, `MerchantId`, `SettlementId`, `AchqToken` (sensitive Debug), `FeeProfileId`, `OnboardingId`.
- **`Amount(Decimal)`:**
  - **Form encoding:** a fixed 2-decimal string.
  - **JSON:** serialized as a number for onboarding fees; deserialized from a number or a string without float rounding (repay's technique).
  - **Validation:** ≥ 0 and ≤ 2 decimal places. Direction is carried by `PaymentDirection`, never by sign.
- **`#[derive(Model)]` in `achq-derive`:**
  - **Redacted `Debug` and `Validate`:** repay's attributes `sensitive`, `required`, `min_length`, `max_length`, `rule`, and container-level `one_of`.
  - **Form encoding (new):** `impl ToForm`, which writes fields in declaration order into a `FormWriter`.
  - **Form attributes:** `form = "Wire_Name"`, `flatten` (nested struct, optionally `prefix = "Billing_"`), `yes_no` / `on_off` (bool → Yes/No or On/Off), `date = "mdy_slash" | "mdy" | "iso"`, and `skip_form`.
  - **`FIELDS` metadata:** `const FIELDS: &[FieldMeta { wire, required, max_length }]`. It feeds the conformance test, so the derive itself is the source of truth that gets diffed against the vendored fragments.
- **Validation rules (`validate::rules`):**
  - `digits(n)`, `routing_number`, `zip`, `email`;
  - `us_state`, which accepts a code or a full name, matching the onboarding description;
  - `percent_0_100`, and `mmddyyyy` for the onboarding dob string.
  - `routing_number` checks 9 digits **plus the ABA mod-10 checksum**. `123123123` passes, so the sandbox still works.

### Layer 2: Generated data (`cargo xtask codegen [--check]`)
- The only generated file is `onboarding/industry.rs`. It is built from `schema/industry-categories.md` into an `industry_table!` invocation, then `Industry` with its `category()`/`specific()` pairs, emitted with prettyplease and the `@generated` header.
- **`cargo xtask fetch`:**
  1. Reads `llms.txt`.
  2. Downloads the reference pages.
  3. Extracts the ```` ```json ```` blocks to `schema/gateway/*.json` and `schema/onboarding.json`.
  4. Refreshes `schema/README.md` with the date and SHA-256.
- **`cargo xtask drift`:** re-fetches and diffs the result against the vendored files. It is not CI-gated, because the network is flaky; it is run on each release, as in `RELEASING.md`.
- **`cargo xtask record`:**
  - Runs every command against the public sandbox and writes scrubbed responses to `achq/tests/data/responses/`.
  - Before writing, tokens become `achq-sandbox-…` and reference IDs stay; nothing else is sensitive in the sandbox.
  - It **refuses** any ProviderID other than `99` unless `--i-know` is passed.
- **`tests/conformance.rs`:** for each op, it compares `Request::FIELDS` (wire name, required, max_length) to the fragment's properties. Every mismatch must be listed in `overrides.toml` under `[gateway.<op>]` with a `reason`, and a stale override fails the test, as in repay-rs.

### Layer 3: Operations DSL and sans-IO core
```rust
pub trait Operation {
    type Response;
    const NAME: &'static str;        // "payments.create"
    const KIND: Kind;                // Gateway { command: "ECheck.ProcessPayment", scope: Scope::Merchant, extra: &[("OwnerAuth","On")] }
                                     // | Events | Onboarding { path: "/onboarding" }
    fn validate(&self) -> Result<(), ValidationError>;
    #[doc(hidden)] fn write_body(&self, form: &mut FormWriter) -> Result<(), Error>;   // or json_body
    #[doc(hidden)] fn decode(resp: &HttpResponse) -> Result<Self::Response, Error>;
}
```
- **`operation_table!` (hand-written, one entry per op), consumed by `impl_operations!` and `async_handles!`/`blocking_handles!`:**
  ```rust
  create => CreatePayment {
      resource: payments, kind: gateway "ECheck.ProcessPayment", scope: merchant, extra: [],
      request: $crate::payments::PaymentRequest, response: envelope $crate::payments::PaymentCreated,
  }
  account_id => VerifyOwner {
      resource: insights, kind: gateway "ECheck.VerifyPlus", scope: merchant, extra: [("OwnerAuth", "On")],
      request: $crate::insights::OwnerAuthRequest, response: envelope $crate::insights::OwnerVerification,
  }
  query => TrackEvents { resource: events, kind: events, scope: platform, request: $crate::events::EventQuery, response: csv }
  create_merchant => CreateMerchant { resource: onboarding, kind: json POST "/onboarding", scope: platform, cfg: "onboarding", ... }
  ```
- **`protocol::encode(op, &Context) -> HttpRequest`:**
  1. Validate (configurable).
  2. Write the credentials: provider always, merchant when `scope: merchant`.
  3. Write `Command`, `CommandVersion=2.0`, `TestMode=On` if configured, the `extra` pairs, then the op's fields.
  4. Set `Content-Type: application/x-www-form-urlencoded`, `Accept: application/json` and `User-Agent: achq-rs/<ver>`.
- **`protocol::decode::<O>(&HttpResponse)`:**
  - Non-2xx is `Error::Status`.
  - Envelope ops parse `Envelope<D>`. Unless the status is `Approved`, the result is `Error::Api(Box<ApiError { envelope }>)`; otherwise the envelope is mapped into `O::Response` (for example `PaymentCreated::from_envelope`).
  - The CSV op is described under Events.
  - Onboarding: a 200 is typed, a 400 is `Error::Onboarding(ErrorResponse)`, and anything else is `Error::Status`.
- **Handles:**
  - `client.merchant(creds).payments().create(&req)`, `.update(id, &changes)`, `.cancel(id)`, `.hold(id)`, `.refund(id, &RefundRequest)`;
  - `.tokens().create(&BankAccount)`;
  - `.insights().account_status(&acct)`, `.account_owner(&req)`;
  - `client.settlements().list(&SettlementQuery)`, `.details(id, page)`;
  - `client.events().query(date)` / `.query_merchant(date, merchant_id)`;
  - `client.onboarding().create_merchant(&req)`, `.create_fee_profile(&fees)`.

### Layer 4: Clients and transports
- `transport.rs` and `blocking.rs` are ported from repay-rs nearly verbatim: `HttpRequest`/`HttpResponse`, the cfg-split `Send` bounds, `ReqwestTransport`, `UreqTransport` (100 MiB cap), and `TransportError { timeout }`.
- `SECRET_HEADERS` covers `Provider_GateKey`, which onboarding sends as a header.
- **`Client<T = ReqwestTransport>`:**
  - It holds `Arc<T>` and `Arc<Config>`.
  - The constructors are `Client::new(ProviderCredentials)`, `Client::builder(creds)` and `Client::with_transport(creds, t)`.
  - The builder takes `.endpoints(..)`, `.test_mode(bool)`, `.timeout(..)`, `.user_agent(..)` and `.validate_requests(bool)`.
- **`Merchant<'a, T>`:** `{ client: &'a Client<T>, creds: MerchantCredentials }` (cheap), with its own `execute`.
- **Logging:**
  - A `tracing::debug_span!("achq", operation = O::NAME, command)`.
  - DEBUG shows the URL and command. TRACE shows the form body with every sensitive key redacted: gate keys, `AccountNumber`, `AccountToken`, `ACHQToken`, `TaxID`, `ssn`, `dob`, account and processor tokens. Routing numbers stay visible.

### Errors
```rust
#[non_exhaustive] pub enum Error {
    Api(Box<ApiError>),                 // CommandStatus Declined/Error (envelope or CSV trailer)
    #[cfg(feature = "onboarding")] Onboarding(Box<OnboardingError>), // 400 ErrorResponse
    Validation(ValidationError),
    Encode(serde_json::Error),          // onboarding JSON
    Decode { source: DecodeError, status: u16, body: String },   // JSON or CSV, body excerpt 4 KiB
    UnexpectedContentType { status: u16, content_type: String },
    Status { status: u16, body: String },
    Transport(TransportError),
}
```
- `ApiError` offers `command_status()`, `code() -> ResponseCode`, `description()`, `field()` (from `ErrorInformation`), `express_verify()` (why a verification rejected the payment), `reference_id()` (to quote to ACHQ support), and `is_duplicate()` / `is_credentials()` / `is_validation()` (150–152, 209/210/212) / `is_retryable()` (600/601/603 only, documented as "maybe").
- Display: `Declined [100] Invalid Gateway Credentials (Provider_Credentials)`.

### Ergonomics (the DSL users touch)
```rust
let client = achq::Client::new(ProviderCredentials::sandbox());
let merchant = client.merchant(MerchantCredentials::sandbox());

let payment = PaymentRequest::builder()
    .amount(dec!(42.00))
    .direction(PaymentDirection::FromCustomer)      // or PaymentRequest::charge(..) / ::payout(..)
    .sec_code(SecCode::Web)
    .customer(Billing::builder().name("Bob Loblaw").address1("123 Street").city("Town")
        .state("NY").zip("12345").email("bob@example.com").phone("555-867-5309").build())
    .customer_ip(Ipv4Addr::new(203, 0, 113, 9))
    .source(PaymentSource::bank(BankAccount::checking("123123123", "12345678").personal()).tokenize())
    // or PaymentSource::token("achq-sandbox-…", TokenSource::Achq)
    .provider_transaction_id("order-1234")
    .build();
let created = merchant.payments().create(&payment).await?;
let id: PaymentId = created.payment_id;
let token: Option<AchqToken> = created.token;
```
- **`PaymentSource`:**
  - It is an enum, so the bank-vs-token alternatives can't both be set.
  - `BankAccount` has `checking`/`savings` constructors plus `.personal()`/`.business(company)`. `business` sets `Billing_Company`, which removes that conditional rule from the user's concerns.
- **`UpdatePayment`:** every field is optional (bon). Its `source` is `Option<PaymentSource>`, so changing the bank account is typed.
- **`RefundRequest::full()` / `::partial(amount)`:** blank `Amount` means a full refund, and that becomes explicit in the API.
- **`EventQuery`:** `::on(date)` and `.merchant(id)`. `EventBatch` has `events: Vec<PaymentEvent>` and `trailer: Envelope<()>`. `PaymentEvent` has `payment_id`, `provider_transaction_id`, `merchant_id`, `event`, `occurred_at`, `status`, `return_code`, `return_explanation` and `verification`.
- **`SettlementDetailsPage`:** `has_next_page()` and `next_page()` build the next request. There is no async stream, which keeps wasm simple.

## Stages

## Stage 1: Workspace, runtime types, derive
**Goal**:
- A workspace skeleton copied from repay-rs: Cargo files, `.cargo/config.toml`, `.gitattributes`, `.gitignore`, deny.toml, LICENSE copies, CHANGELOG, RELEASING, CI (fmt, clippy, test, msrv, minimal-versions, wasm, docs, deny).
- `macros.rs` (`string_enum!` with a case-insensitive arm, `string_id!`), `Amount`, the date-format helpers, the ID newtypes, `validate` with its rules.
- `achq-derive` `#[derive(Model)]`: redacted Debug, Validate, `ToForm` (form/flatten/prefix/yes_no/on_off/date), `FIELDS` metadata.
- `FormWriter` (RFC 3986 encoding for form bodies, with `+` for spaces as browsers do; we pick `%20`, which ASP.NET accepts, and note it).

**Success Criteria**:
- `cargo test --workspace` and clippy `-D warnings` are clean on native and on `wasm32-unknown-unknown`/`wasm32-wasip2` with `--no-default-features`.

**Tests**:
- `Amount`: `42` → `"42.00"`, a scale > 2 is rejected, a negative is rejected, a JSON number `55300.19` is read losslessly.
- Dates: each of the five formats round-trips.
- `routing_number` checksum (`123123123` passes, `123456789` fails).
- Enum `Other` and case-insensitive parsing (`In-process`).
- Derive: flatten with prefix, field order, yes_no, redaction, `FIELDS`.
- trybuild UI errors.

**Status**: Complete

**Notes (as built)**:
- **Workspace:** `achq` and `achq-derive` only; `xtask` and its cargo alias join in Stage 2. RELEASING.md moves to Stage 6, as there is nothing to release yet.
- **CI:** jobs fmt (plus the LICENSE copies), clippy (default and all features), test (3 OSes), msrv 1.88, minimal-versions, wasm (both targets, the library alone), docs and deny. The transport feature matrix comes in Stage 4, and codegen, package, package-tests and docs-rs in later stages.
- **Form attributes:** ~~`yes_no` / `on_off` / `date = "..."` attributes~~ became one general attribute, `with = "name"`. It names a function in `achq::form::with` (`yes_no`, `on_off`, `mdy_slash`, `mdy`, `iso_date`); an unknown name fails on the attribute's string.
- **`FormValue` diagnostic:** `#[diagnostic::on_unimplemented]` suggests `with = "..."` when a field's type has no single encoding (a `bool`, a `Date`).
- **Form field rules:**
  - Each field of a `#[achq(form)]` struct needs exactly one of `form = "..."`, `flatten` or `skip_form`.
  - A form attribute on a struct without `#[achq(form)]` is an error.
  - Wire names may contain inner spaces, for the documented `Phone Number`.
- **Space encoding:** ~~`%20` for spaces~~. `FormWriter::encode` follows the WHATWG `application/x-www-form-urlencoded` serializer: alphanumerics and `*-._` stay as they are, a space becomes `+`, everything else becomes `%XX`. That is what the content type specifies and what ASP.NET decodes.
- **Sensitivity per pair:** `FormWriter` records it per pair (`push_sensitive`, from `#[achq(sensitive)]`). `redacted()` writes `[REDACTED]` unencoded for log readability, and `Debug` shows the redacted body. This replaces a global list of sensitive keys for form bodies.
- **Field metadata:** ~~`const FIELDS`~~ became `ToForm::form_fields(prefix, &mut Vec<FieldMeta>)`, because prefixed names are built at runtime. `FieldMeta { wire, required, max_length, sensitive }` is `#[non_exhaustive]`. A field counts as `required` when it is not an `Option`, or when it is marked `required`.
- **Cross-field checks:** the new container attribute `check = "method"` (it may repeat) runs after the field and `one_of` checks. `validate::violation` became public (doc-hidden) so check methods can report at the current path.
- **IDs:**
  - `string_id!` trims whitespace on deserialize (for `" 2001"`).
  - Its `sensitive:` arm hides the value in `Debug`; it is used for `AchqToken`.
  - `int_id!` was dropped.
- **Enums:** `string_enum!` takes a leading `case_insensitive;`. It is `#[allow(unused_macros)]` until Stage 3 uses it.
- **Dates:** `types::LocalDateTime` covers both response formats and is written as ISO, with fractional seconds only when present. Request dates are `time::Date`, formatted with `with`. The onboarding `dob` and `apiRequestTimeStamp` are deferred to Stage 5.
- **`Amount`:** sent as `rescale(2)` after `round_dp(2)`; validation already guarantees at most 2 dp. JSON keeps repay's f64-number behaviour.
- **Validation rules:** `digits`, `routing_number` (ABA checksum), `zip`, `email` and `us_state` (the 50 states, DC, AS, GU, PR, VI by code or name, matching the onboarding list). `no_constraints!` also covers `u16`, `u32`, `Date`, `Ipv4Addr` and `IpAddr`.
- **UI tests:** 17 cases, repay's 10 plus `form_field_missing_name`, `form_vec_field`, `prefix_without_flatten`, `form_and_flatten_together`, `unknown_with`, `form_attribute_without_form` and `value_without_form_value`.
- **Test counts:** 41 unit tests, plus `model_derive` (7), `form_derive` (6), `derive_ui` (1) and 2 doctests.
- **Verified:** fmt, clippy `-D warnings` (default and all features), tests, MSRV 1.88, minimal versions, wasm32-unknown-unknown and wasm32-wasip2 clippy, docs `-D warnings`, and cargo-deny.

## Stage 2: Schema vendoring, conformance, industry codegen
**Goal**:
- `xtask fetch` and `drift`, populating `schema/` along with its README (URL, date, SHA-256).
- `xtask codegen [--check]` for `Industry`.
- `tests/conformance.rs` and `overrides.toml`, seeded with the known deviations: `Run_ExpressVerify`, `Billing_Company`, `Billing_CustomerID`, `Customer_IPAddress` conditional, token `AccountNumber` required, `Phone Number`, the onboarding `address2`/`transferProcessorToken` lengths, and `TrackingDate` required.

**Success Criteria**:
- `cargo xtask codegen --check` passes in CI.
- The conformance test fails on an unlisted or stale deviation.

**Tests**:
- Insta snapshot of the parsed industry table, with a row-count assertion.
- Conformance self-test using a deliberately wrong fixture struct.

**Status**: Complete

**Notes (as built)**:
- **Sequencing:** ~~seeding `overrides.toml` with the known deviations~~ moves to Stage 3. The deviations belong to request types that don't exist yet, and the stale-override check rejects an entry with nothing to match.
  - Stage 2 lists all 12 gateway fragments under `unregistered`, and the check passes with an empty registry.
  - **Stage 3 must:** register each request type in `xtask/src/conformance.rs` (`("bankpayment", fields::<PaymentRequest>())`), remove it from `unregistered`, and add its `[gateway.<op>]` overrides (`missing` / `extra` / `required`, each with a reason).
- **Where conformance lives:** ~~`achq/tests/conformance.rs`~~ became `xtask/src/conformance.rs`. It runs as xtask tests under `cargo test --workspace` and as `cargo xtask conformance`, which CI's codegen job also runs.
  - xtask is never packaged, so it can read `schema/`; a test in `achq` would break the packaged-crate tests.
  - It compares request-body properties and their `required` flags. The gateway fragments have no `maxLength`, so lengths aren't compared.
  - Query parameters such as `OwnerAuth` are left to the Stage 3 operation table.
- **How xtask depends on achq:** with `default-features = false`. This way the generated `onboarding` module is never part of the tool that generates it, and a broken `industry.rs` can't stop `cargo xtask codegen` from replacing it. We hit exactly that while bootstrapping the file.
- **Layout of `schema/`:**
  - `gateway/*.json` (12 files);
  - `onboarding/{merchant-onboarding,post_onboarding-fees}.json`, kept as two byte-exact files rather than ~~a merged `onboarding.json`~~. A test asserts that their shared `ErrorResponse` is identical;
  - `industry-categories.md`, with its front matter removed;
  - `README.md`, holding the source, date and SHA-256 per file;
  - `overrides.toml`.
- **xtask tasks:**
  - `fetch` writes only the files that changed and is idempotent.
  - `drift` reports the properties, query parameters and component schemas that were added, removed or changed, "reformatted" or "other changes" otherwise, and any **new reference page** in `llms.txt` that isn't vendored or on the `NOT_API` list.
  - ~~`xtask record`~~ remains in Stage 5, as planned.
- **Industry codegen:** produces `achq/src/onboarding/industry.rs`, an `industry_table!` invocation written as plain text, since prettyplease can't format a macro body.
  - The hand-written macro in `onboarding/mod.rs` generates the two enums:
    - `IndustryCategory`: 13 variants, with `ALL`, `name()` and `industries()`;
    - `Industry`: 150 variants, with `ALL`, `name()` and `category()`.
  - Both have `Display` and a `FromStr` that ignores case, with an `UnknownIndustry` error.
  - They are `Copy` with no `Other`, as planned.
  - Variant names come from heck. The overrides in `[industry.names]` are `NftsDigitalCollectible`, `SaasAndSoftwareApplications` and `HeatingPlumbingAc`; a stale override or a name collision is a codegen error.
- **xtask dependencies:** `achq`, heck, serde, serde_json, sha2, toml and ureq. prettyplease, quote, syn and insta weren't needed, so the plan's insta snapshot became exact-string tests.
- **Features:** `onboarding` (on by default) gates the module. The CI clippy and test matrices add `--no-default-features`, wasm clippy covers `--features onboarding`, and a new `codegen` job runs `codegen --check` and `conformance`.
- **Test counts:** 46 achq unit tests (5 onboarding) and 29 xtask tests.
- **Verified:** the same matrix as Stage 1, plus `xtask drift` clean, `codegen --check`, `conformance`, and the nightly docs.rs build.

## Stage 3: Envelope, operations DSL, sans-IO protocol, errors
**Goal**:
- `Envelope<D>`, `CommandStatus`, `ResponseCode` (constants plus `description()`), `ExpressVerify`.
- The `Operation` trait, `operation_table!` with `impl_operations!`, `Context`, `protocol::encode`/`decode`, `Error`/`ApiError`.
- Every gateway request/response model: payments, tokens, insights, settlements.
- The event CSV parser, which handles the trailer, 10 or 11 columns, a quoted explanation, an empty body, and a JSON error body.

**Success Criteria**:
- Each operation encodes to a golden request (insta), and each documented sample decodes.

**Tests**:
- `requests.rs`: an insta snapshot per op, with secrets shown as `<redacted>`.
- `responses.rs`: every doc sample in the guides and every orkestra fixture, including "000" vs "0", `TransAct_` vs `Transact_`, the missing `ResponseCode`, `" 2001"` and `TransferDirection`/`PaymentDirection`.
- `events_csv.rs`: the full doc sample and the orkestra samples, plus the error trailer, an unknown event, and an `Authorization` row.
- Declined bad-credentials → `Error::Api` with `field() == Some("Provider_Credentials")`.

**Status**: Complete

**Notes (as built)**:
- **Operations:** 11 operations in 5 resources, from a hand-written `operation_table!` in `operations.rs`.
  - The resources are tokens, payments, insights, settlements and events.
  - The table is consumed by `impl_operations!`. It is also `pub(crate)` re-exported for the Stage 4 handles, with `#[allow(unused_imports)]` until then.
  - Each operation is `Name<'a>`, with private ID fields, a borrowed body and `new(ids…, &body)`.
  - Table syntax: `command`, `scope: Merchant|Platform`, `extra: [("OwnerAuth", "On")]`, `fields: { payment_id: PaymentId = "Transact_ReferenceID", }`, `body: [T]` and `response: T`.
  - Responses decode through a hidden `Decode` trait, which `Vec<Settlement>` also implements, so no response-kind syntax is needed.
- **The `Operation` trait:** `Response`, `NAME`, `COMMAND`, `SCOPE`, the hidden `EXTRA`, `validate`, the hidden `write_form`, `form_fields` (for conformance) and `decode`.
- **`protocol`:**
  - **`encode`:** validates, and adds a violation `merchant credentials: are required for payments.cancel` for a merchant-scoped op without a merchant.
  - **`encode_unvalidated`:** returns `HttpRequest` directly, since it cannot fail.
  - **`protocol::form(op, ctx)`:** the public `FormWriter`, for logging the redacted body in Stage 4.
  - **Body order:** credentials, then `Command`, `CommandVersion=2.0`, `TestMode=On` (when set), the extras and the op fields. Nothing goes in the URL; `ResponseType=JSON` is not sent.
- **JSON detection:** JSON is recognized by the first non-whitespace byte `{`, whatever the `Content-Type`. A mislabeled JSON body is still read (tested), and an HTML page becomes `UnexpectedContentType`.
- **`UnexpectedContentType` fields:** it gained `expected` ("JSON" or "CSV") and `body` (an excerpt), which the plan didn't have.
- **`Envelope<D>`:**
  - Every field is optional, plus `other` (via `#[serde(flatten)]`).
  - `TransAct_ReferenceID` and its alias `Transact_ReferenceID` both work alongside the flatten; a unit test pins this.
  - The data is parsed once as a `Value`, then mapped with `map_data`.
- **`ResponseCode(u16)`:** 31 named constants (the documented codes) with `description()`, displayed as 3 digits.
- **`ApiError`:** wraps the envelope and the HTTP status. Display: `Declined [100] Invalid Gateway Credentials (Provider_Credentials)`, which falls back to the documented description when the gateway sends none.
- **Credentials:**
  - `ProviderCredentials` and `MerchantCredentials`, each with `sandbox()`, holding the key in a `SecretString` (re-exported).
  - `Endpoints { gateway, api }` with `Endpoints::custom`, and the constants `GATEWAY_URL` and `API_URL`.
- **Models:** in `bank`, `payments`, `tokens`, `insights`, `settlements` and `events`. Requests are `Model` + `bon::Builder` + `#[achq(form)]`.
  - **`PaymentSource`:** an enum with hand-written `ToForm` and `Validate`. Its `form_fields` lists both kinds' fields, and xtask filters them per fragment.
  - **`AccountToken`:** a new sensitive ID.
  - **Account numbers:** only their length (30) is checked. A NACHA account may contain letters, so the planned `digits` rule was dropped.
  - **Cross-field checks** (`check = …`): a business account needs `Billing_Company`; WEB needs `Customer_IPAddress`; the IP must be ≤ 15 characters, which in practice means IPv4 (the IPv6 open item remains); an update needs at least one change.
  - **Response rows** (`Settlement`, `SettlementItem`, `SettlementDetailsPage`) are all `Option`, as planned. Success types (`PaymentCreated`, `CommandReceipt`, `TokenCreated`) require their reference ID, or `Error::Decode`.
  - **`AccountVerification` and `OwnerVerification`** keep the whole `Envelope<IgnoredAny>`. `OwnerAuth` is read from a top-level key, pending the Stage 5 sandbox run.
- **Events:**
  - The CSV `Command Response` trailer becomes the envelope; a declined trailer is `Error::Api`.
  - Rows of exactly 10 fields use orkestra's layout; anything else uses the documented 11.
  - Rows with fewer than 6 fields, or a bad event time, are `Error::Decode` with the line number.
  - The enums match case-insensitively and include the undocumented `Authorization`/`Authorized`.
  - `ReturnCode` has the 13 documented R-codes.
- **Ergonomic constructors**, pulled forward from Stage 5 because they were trivial: `BankAccount::checking/savings/business/personal/last_four`, `PaymentSource::bank/token/achq_token/tokenize`, `RefundRequest::full/partial`, `SettlementQuery::between/merchant`, `SettlementDetailsQuery::new/page`, `EventQuery::on/merchant`, and `TokenRequest: From<BankAccount>`.
- **Derive fix:** the fields of an **optional flattened struct** are now described as optional. Conformance caught this: `PaymentUpdate`'s `Option<PaymentSource>` showed its account fields as required. The Stage 1 `form_derive` expectation that pinned the bug was corrected.
- **Conformance:** all 12 fragments are registered and `unregistered` is empty. `schema/overrides.toml` holds 18 differences, each with the guide that documents it (required/extra on bankpayment, tokenizedpayment, updatepayment, banktoken and eventmonitor).
- **Fixtures (`tests/data/README.md` lists the provenance):**
  - 9 guide samples; two had JSON typos repaired (a missing and a stray `}`);
  - 3 orkestra-derived envelopes;
  - synthesized: an Express Verify reject, an IIS page, and four event edge cases;
  - the 89-row docs event sample.
- **Tests:**
  - 81 unit tests;
  - integration: `requests` 19 (15 insta snapshots, checked against the guides' curl examples), `responses` 16, `events_csv` 7, `form_derive` 6, `model_derive` 7, `derive_ui` 1;
  - xtask 30;
  - 8 doctests.
  - The `value_without_form_value` UI snapshot was regenerated, because rustc's list of implementors grew.
- **Verified:** the same matrix as Stage 2 (fmt, clippy ×3, tests with all features and none, MSRV, minimal versions, both wasm targets with and without onboarding, docs and docs.rs, deny, codegen, conformance, drift).

## Stage 4: Async + blocking clients, transports, wasm
**Goal**:
- Port `transport.rs`, `blocking.rs` and `client.rs` from repay-rs; add `Merchant` scoping and the handles generated from the table.
- The tracing span and redaction.

**Success Criteria**:
- Both clients pass the shared `client_cases!` table against mockito, which asserts on the form body, including credential fields, `Command` and `CommandVersion`.
- The custom transport works with no features.
- Futures are `Send` on native.
- wasm clippy passes with `--features reqwest`.

**Tests**:
- `async_client.rs` / `blocking_client.rs`: every op, plus timeout, connection refused, non-2xx, and HTML returned with a 200 (an IIS error page → `UnexpectedContentType`).
- `logging.rs`: no gate key, account number, token or SSN appears at TRACE.
- `custom_transport.rs`.

**Status**: Complete

**Notes (as built)**:
- **The credentials model, made type-safe:**
  - **`Client<T = ReqwestTransport>`** is built from `ProviderCredentials` and carries only the platform resources: `settlements()` and `events()`.
  - **`client.merchant(MerchantCredentials)` returns an owned `Merchant<T>`**, a client clone with the merchant in its context that shares the transport `Arc`. It carries `payments()`, `tokens()` and `insights()`, and can be stored or moved into tasks. This replaces the planned ~~`Merchant<'_, T>`~~.
  - A merchant command can't be reached through a handle without a merchant. Through `Client::execute` it is the Stage 3 validation error, and nothing is sent (tested).
  - The blocking module mirrors this: `blocking::{Client, Merchant, ClientBuilder}`.
- **Table syntax:** each resource now declares its scope (`tokens => Tokens in Merchant { … }`), and a unit test checks that every command's scope matches its resource's. `resource_handles!([Client, Merchant, Transport, async|blocking])` puts each accessor on `Merchant` or `Client` by that scope.
- **Generated handles:** each is `Copy`, holds `&'a Merchant<T>` or `&'a Client<T>`, and has one method per command. A method takes the ID fields (`impl Into<PaymentId>`) and then `&body`, for example `merchant.payments().cancel("14477175")` and `client.events().query(&EventQuery::on(date))`.
  - A nested helper macro inside `concat!` names the owner type in the docs. `concat!` expands it eagerly.
- **Transports:** ported from repay. `Method` has only Get and Post. `UreqTransport` caps responses at 16 MiB.
- **Logging:**
  - TRACE request bodies come from `protocol::form(op, ctx).redacted()`, so the per-field sensitivity flags decide what is hidden: gate keys, account numbers, tokens and tax IDs. Routing numbers stay visible.
  - Response bodies are redacted by key (`ACHQToken`, `AccountToken`, `AccountNumber`, `TaxID`, case-insensitive); a non-JSON body (event CSV) is summarised.
  - The span is `achq` with `operation` and `command`.
- **Builder settings:** `endpoints`, `test_mode`, `timeout`, `user_agent`, `validate_requests`. `DEFAULT_TIMEOUT` (60 s) is re-exported.
- **Features:** `default = ["reqwest", "rustls", "onboarding"]`, plus `ureq` and `native-tls`.
  - CI clippy covers `""`, `--no-default-features`, `ureq,rustls`, `reqwest,native-tls` and `--all-features`.
  - CI tests cover all features, no default features, and `ureq,rustls`.
  - CI wasm clippy covers reqwest (fetch) on wasm32-unknown-unknown.
- **Tests:**
  - `tests/common/mod.rs` is a mockito gateway that matches the form content type, `Accept` and the `achq-rs/` user agent and captures the form. `client_cases!` runs 11 cases, one per command, each asserting the command, its version and the credential fields (merchant fields only for merchant commands).
  - `async_client` has 20 tests: the cases, plus field order, test mode, a declined command, a 503, an HTML page, a timeout, a refused connection, an invalid command not sent, and futures being `Send`.
  - `blocking_client` has 17.
  - `custom_transport` has 9 and runs with no features and no tokio: settings reach the request, merchants share the transport, merchant commands need a merchant, validation can be turned off, transport errors pass through, and `Debug` hides the keys.
  - `logging` has 2: across both clients, the logs carry the operation and command and the redacted fields, and neither gate key, the account number, the tax ID nor the returned token ever appears.
  - In total, 230 tests pass with all features.
- **Crate docs:** they now open with a client example and list the features.
- **Verified:** fmt, clippy ×5, tests ×3 feature sets, MSRV 1.88, minimal versions, wasm (5 combinations), docs and docs.rs, deny, codegen, conformance, drift.

## Stage 5: Onboarding, ergonomics, sandbox
**Goal**:
- The `onboarding` feature: JSON models with the derive (camelCase serde), header auth, conditional rules (`one_of`/source enums), `Industry`, and `OnboardingError`.
- Ergonomic constructors (`PaymentRequest::charge/payout`, `BankAccount::checking().business()`, `RefundRequest::full/partial`, `EventQuery::on`).
- `xtask record`. Run it once against the sandbox, then **resolve the open items below** (VerifyPlus shape, `ResponseType`, NEG semantics, error bodies on events) and finalize the insight types from the recorded fixtures.
- `tests/sandbox.rs` (`#[ignore]`, `ACHQ_SANDBOX=1`):
  1. token → payment (bank, tokenize) → update amount → hold → cancel;
  2. a token payment;
  3. an Express Verify reject (`987654321`);
  4. account status, and account ID with the `999999999` owner failure;
  5. settlements over the last 30 days;
  6. events for today.
  - All of it uses fabricated data only, because merchant 2001's data is public.

**Success Criteria**:
- The recorded fixtures decode through the strict tests.
- The sandbox suite passes locally.
- Onboarding requests are validated against `schema/onboarding.json` by the conformance test.

**Tests**:
- Onboarding golden requests and the 200/400 decode.
- The conditional-rule violations.
- Sandbox suite (manual).

**Status**: Not Started

## Stage 6: Hardening, docs, release prep
**Goal**:
- A README in the sibling structure:
  - tagline, features table, "Charging a bank account", "Tokens", "Payment lifecycle & IDs" (the `PaymentId` vs `CommandId` warning), "Tracking payment status" (recommended polling schedule: 9 AM, 1 PM, 7 PM and 12 AM ET);
  - Settlements, Onboarding, Errors, Other HTTP clients;
  - WebAssembly, with a strong warning: gate keys must never ship to browsers, and IP allow-listing makes browser calls fail anyway;
  - Logging, Development, License.
- Examples, crate docs, README doctests, CHANGELOG `[Unreleased]`, `scripts/test-packaged-crate.sh`, and the CI package/package-tests/docs-rs/codegen jobs plus release.yml.

**Success Criteria**:
- Every CI job is green.
- `cargo publish --dry-run --workspace` passes, and the packaged-crate tests pass.

**Tests**:
- README doctests compile.
- Examples build under their required features.

**Status**: Not Started

## Verification (end-to-end)
1. `cargo fmt --check && cargo clippy --workspace --all-targets --all-features -- -D warnings`
2. `cargo test --workspace --all-features`, then again with `--no-default-features` and with `--no-default-features --features ureq,rustls`
3. `cargo clippy -p achq --lib --target wasm32-unknown-unknown --no-default-features --features reqwest -- -D warnings` (and `wasm32-wasip2` with no features)
4. `cargo xtask codegen --check`, then `cargo xtask drift` (manual, needs network)
5. `ACHQ_SANDBOX=1 cargo test -p achq --all-features --test sandbox -- --ignored --nocapture`
6. `cargo run -p achq --example tokenize_and_charge` against the sandbox credentials
7. `./scripts/test-packaged-crate.sh` and `cargo publish --dry-run --workspace`

## Open items to resolve during implementation (not blocking)
- **`ResponseType=JSON` (orkestra):** is it needed for a JSON response when it is omitted? Does `Accept` suffice?
- **`ECheck.VerifyPlus` response shape:** the placement and casing of `OwnerAuth`, the date format of `AccountCreated`/`AccountLastSeen`, and whether NEG is `Approved` or `Declined`.
- **`Phone Number`** (with a space) in the bank-account-ID fragment: is the wire name `Phone`, `PhoneNumber` or `Phone Number`? Its type is integer, max 10.
- **Error bodies:** does the event query return a JSON envelope or a CSV `Command Response` row? Are the CSV fields quoted when the return explanation contains commas?
- **Case sensitivity:** `Transact_ReferenceID` vs `TransAct_ReferenceID` on requests. Does the gateway accept either?
- **`Run_ExpressVerify`:** required or optional, and what the default is when omitted. This interacts with the merchant-level rulesets ("First time WEB", etc.).
- **Releasing a hold:** is there a command, or does `ECheck.Update` release it?
- **`Customer_IPAddress`:** IPv4 only (max length 15)? Confirm before typing it as `Ipv4Addr`; otherwise use `IpAddr` and validate the length.
- **Onboarding:** the sandbox behaviour of `/api/onboarding` for ProviderID 99, and whether `status` values are enumerable.
- **Repository:** `lendsimple/achq-rs`? This sets the badge and CI URLs.
- **Webhooks:** the docs said "Webhooks … are up next" (Feb 2023), but none are documented. Out of scope; revisit if ACHQ publishes them.

---

## How this plan was derived (for reviewers)
- **Docs:** every page listed in `https://developers.achq.com/llms.txt` was downloaded as Markdown, and the embedded OpenAPI fragments were extracted.
- **Reference implementation:** `~/Web/repay-rs` and `~/Web/authorizenet-rs` were studied for layout, macros, transport, error, CI and README conventions. The files reused or ported nearly verbatim are `repay/src/{macros,transport,blocking,client,config,validate,query}.rs`, `repay/src/types/amount.rs`, `repay-derive/src/*`, `repay/tests/common/mod.rs`, `.github/workflows/*`, `deny.toml`, `RELEASING.md` and `scripts/test-packaged-crate.sh`.
- **Open source:** `orkestra/orkestra-transactor` (PHP) was found; it provides the CSV trailer, credentials-in-body evidence and envelope fixtures.
