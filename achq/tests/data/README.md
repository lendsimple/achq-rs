# Test fixtures

Where each fixture comes from. Stage 5 adds responses recorded from the public
sandbox (`cargo xtask record`), which will replace the synthesized ones.

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
| `express_verify_rejected.json` | Synthesized: a payment rejected for its account (code 301, `GS02`), the shape the sandbox's `987654321` account should produce |
| `iis_error.html` | Synthesized: an ASP.NET error page |

## `events/`

| File | Source |
|---|---|
| `docs_sample.csv` | Sample response, guide *Payment event tracking* (11 fields per row, no trailer) |
| `ten_fields_with_trailer.csv` | orkestra's event responses: 10 fields per row, then the `Command Response` row |
| `declined.csv` | Synthesized from orkestra's trailer format: a declined query |
| `returns.csv` | Synthesized: returns with codes, a quoted explanation with a comma, and a verification |
| `json_error.json` | Synthesized: an error answered as a JSON envelope |
