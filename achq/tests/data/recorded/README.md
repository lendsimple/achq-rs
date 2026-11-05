# Recorded sandbox responses

Written by `cargo xtask record` from ACHQ's public sandbox (ProviderID 99,
merchant 2001), with made-up data. Rerun it to refresh them.

Recorded on 2026-11-05.

| File | Outcome |
|---|---|
| `token_created.json` | HTTP 200, decoded: TokenCreated { token: AchqToken([REDACTED]), command_id: CommandId("36438689"), provider_transaction_id: Some(ProviderTransactionId("achq-rs-… |
| `payment_bank.json` | HTTP 200, decoded: PaymentCreated { payment_id: PaymentId("36438690"), token: Some(AchqToken([REDACTED])), provider_transaction_id: Some(ProviderTransactionId("… |
| `payment_achq_token.json` | HTTP 200, decoded: PaymentCreated { payment_id: PaymentId("36438691"), token: Some(AchqToken([REDACTED])), provider_transaction_id: Some(ProviderTransactionId("… |
| `payment_plaid_token.json` | HTTP 200, decoded: PaymentCreated { payment_id: PaymentId("36438692"), token: None, provider_transaction_id: Some(ProviderTransactionId("achq-rs-18e544ed04e9066… |
| `payment_express_verify_reject.json` | HTTP 200, error: Declined [202] Failed Express Verify |
| `update.json` | HTTP 200, decoded: CommandReceipt { command_id: CommandId("36438694"), provider_transaction_id: None } |
| `hold.json` | HTTP 200, decoded: CommandReceipt { command_id: CommandId("36438695"), provider_transaction_id: None } |
| `cancel.json` | HTTP 200, decoded: CommandReceipt { command_id: CommandId("36438696"), provider_transaction_id: None } |
| `cancel_again.json` | HTTP 200, decoded: CommandReceipt { command_id: CommandId("36438697"), provider_transaction_id: None } |
| `refund_not_cleared.json` | HTTP 200, error: Error [103] Transaction Cannot Be Modified (Scheduled) |
| `cancel_unknown.json` | HTTP 200, error: Error [105] Invalid TransAct_ReferenceID |
| `account_status.json` | HTTP 200, decoded: AccountVerification { command_id: CommandId("36438700"), express_verify: ExpressVerify { status: Some(Positive), code: Some(PassAv), descript… |
| `account_owner.json` | HTTP 200, decoded: OwnerVerification { command_id: CommandId("36438701"), express_verify: ExpressVerify { status: Some(Positive), code: Some(PassAv), descriptio… |
| `account_owner_fails.json` | HTTP 200, decoded: OwnerVerification { command_id: CommandId("36438702"), express_verify: ExpressVerify { status: Some(Positive), code: Some(PassAv), descriptio… |
| `settlements.json` | HTTP 200, decoded: [Settlement { settlement_id: Some(SettlementId("7f14e5cd-a6ea-4d15-aea8-c5c6675e663b")), merchant_id: Some(MerchantId("2001")), settlement_ty… |
| `settlement_details.json` | HTTP 200, decoded: SettlementDetailsPage { settlement: Settlement { settlement_id: Some(SettlementId("7f14e5cd-a6ea-4d15-aea8-c5c6675e663b")), merchant_id: Some… |
| `events_today.csv` | HTTP 200, decoded: EventBatch { events: [PaymentEvent { payment_id: PaymentId("36403598"), provider_transaction_id: Some(ProviderTransactionId("01a1a36d-9958-70… |
| `events_yesterday.csv` | HTTP 200, decoded: EventBatch { events: [PaymentEvent { payment_id: PaymentId("36395105"), provider_transaction_id: Some(ProviderTransactionId("01a19e49-2e40-77… |
| `events_bad_credentials.txt` | HTTP 200, error: Error [100] Invalid Gateway Credentials (Provider_Credentials) |
| `cancel_bad_credentials.json` | HTTP 200, error: Error [100] Invalid Gateway Credentials (Provider_Credentials) |
| `onboarding_invalid` | onboarding_invalid.json: HTTP 400, application/json; charset=utf-8 |
| `fee_profile_created.json` | kept from an earlier run |
