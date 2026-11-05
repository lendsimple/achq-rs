# Test fixtures

Where each fixture comes from. `recorded/` holds real responses from ACHQ's public
sandbox, written by `cargo xtask record` (see `recorded/README.md`); they replaced
the synthesized fixtures they cover.

## `responses/`

| File | Source |
|---|---|
| `token_created.json` | Sample response, guide *Tokenize a bank account* |
| `payment_bank.json` | Sample response, guide *Create a payment*; the closing `}` the guide omits is restored |
| `payment_token.json` | Sample response, guide *Use a token* |
| `update.json` | Sample response, guide *Update a payment* |
| `cancel.json` | Sample response, guide *Cancel a payment* |
| `hold.json` | Sample response, guide *Hold a payment* |
| `refund.json` | Sample response, guide *Refund a payment* |
| `settlements.json` | Sample response, guide *Get Settlements*; a stray extra `}` is removed |
| `settlement_details.json` | Sample response, guide *Get Settlement Details*, as published (no `ResponseCode`, `MerchantID` of `" 2001"`) |
| `declined_credentials.json` | The bad-credentials response in orkestra/orkestra-transactor's tests, with its legacy `AVS`, `CVN` and `PaymentKey` keys |
| `internal_error.json` | orkestra's internal-error response, with a documented code (600) in place of its placeholder |
| `legacy_keys.json` | orkestra's approved response, with `ResponseCode` `000` in place of its placeholder |
| `iis_error.html` | Synthesized: an ASP.NET error page |
| `onboarding_created.json` | Synthesized from the onboarding schema's `MerchantOnboardingResponse`; no application is ever submitted to the sandbox, so it is not recorded |

## `events/`

| File | Source |
|---|---|
| `docs_sample.csv` | Sample response, guide *Payment event tracking* (11 fields per row, no trailer) |
| `ten_fields_with_trailer.csv` | orkestra's event responses: 10 fields per row, then the `Command Response` row |
| `declined.csv` | Synthesized from orkestra's trailer format: a declined query |
| `returns.csv` | Synthesized: returns with codes, a quoted explanation with a comma, and a verification |
| `json_error.json` | Synthesized: an error answered as a JSON envelope |
