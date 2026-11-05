# ACHQ API documentation, vendored

ACHQ publishes no single API specification. Each page of its API reference embeds
an OpenAPI fragment describing that page's one operation, and every page is served
as Markdown at its URL with `.md` appended (indexed by
<https://developers.achq.com/llms.txt>). These files are cut out of those pages, so
that builds and checks never depend on the network.

| File | Source | Fetched | SHA-256 |
|---|---|---|---|
| `gateway/banktoken.json` | <https://developers.achq.com/reference/banktoken.md> | 2026-11-05 | `902010d3d74a7de5b6f37492b866829fa0a2725b8b0998e75865f5e33497e8c4` |
| `gateway/tokenizedpayment.json` | <https://developers.achq.com/reference/tokenizedpayment.md> | 2026-11-05 | `273283ebb2647bddc1be4330bcbfb648791fa3459d51831950b6801d575eef04` |
| `gateway/bankpayment.json` | <https://developers.achq.com/reference/bankpayment.md> | 2026-11-05 | `80cfa43033e2f04843be78163aa5408635659135ef388051704d17841cb7de94` |
| `gateway/updatepayment.json` | <https://developers.achq.com/reference/updatepayment.md> | 2026-11-05 | `2a69a52ecb42b432127ed2e5848f7cbf8fc463d19dfbb1852cc6de10f1b02a65` |
| `gateway/cancelpayment.json` | <https://developers.achq.com/reference/cancelpayment.md> | 2026-11-05 | `29fa8e9edb247eb16f428e15ae79a859a2504acf3b4d119cea66bfe5e2bec4f5` |
| `gateway/holdpayment.json` | <https://developers.achq.com/reference/holdpayment.md> | 2026-11-05 | `e264f4ac5c59a58bdf07643c7101d96e40c76ad746b13dd1268aea65a2beb080` |
| `gateway/refundpayment.json` | <https://developers.achq.com/reference/refundpayment.md> | 2026-11-05 | `bb71ab1fa34976b06497976d2b6c775ada7132bbec5324c00903a5cabc9c0752` |
| `gateway/get-settlements.json` | <https://developers.achq.com/reference/get-settlements.md> | 2026-11-05 | `d36d7344ed69763a7729a2b9dbf7590c9a1ab8fabff7c177b445c585c579cebb` |
| `gateway/settlement-details.json` | <https://developers.achq.com/reference/settlement-details.md> | 2026-11-05 | `c60ff30b1c6cbbfc42fdd0198c84670a63cf1a4ec361e9d96809c59ed0acd46d` |
| `gateway/eventmonitor.json` | <https://developers.achq.com/reference/eventmonitor.md> | 2026-11-05 | `ddf363142e25fd0e4889173882ab89fb4c70d5323c6e813af0d1c11718cc0bca` |
| `gateway/bank-account-status.json` | <https://developers.achq.com/reference/bank-account-status.md> | 2026-11-05 | `af8f8b3dee9f2217172b1fd89bf10c06a99b9ec9efb57898fe9c3cf16616f1fa` |
| `gateway/bankaccountid.json` | <https://developers.achq.com/reference/bankaccountid.md> | 2026-11-05 | `2d78de7af624a42332986058eb812b8706200c9052f9eebddd9b76ad2e4650e4` |
| `onboarding/merchant-onboarding.json` | <https://developers.achq.com/reference/merchant-onboarding.md> | 2026-11-05 | `72b49c03624c14d38334d46d8c8d51c2c6f05b4293f9cf6fee78c924ac5e4cb0` |
| `onboarding/post_onboarding-fees.json` | <https://developers.achq.com/reference/post_onboarding-fees.md> | 2026-11-05 | `1bcc674e2defc2524e1cedb2477c363538918fb2cbc9bd194789b5140f94149e` |
| `industry-categories.md` | <https://developers.achq.com/reference/industry-categories.md> | 2026-11-05 | `4920b71a54d1547920d7f5cfdc890b82911cf70b1909479b52177b9fe4e12aec` |

- `gateway/*.json`: the gateway's form-encoded commands (OpenAPI 3.1), one per
  operation. They document request fields only, nearly all as strings, with no
  response schemas; the guides fill in the rest (see `implementation.md`).
- `onboarding/*.json`: the JSON onboarding API (OpenAPI 3.0.1), with full request
  and response schemas. Each fragment carries the components it uses; their shared
  `ErrorResponse` is identical.
- `industry-categories.md`: the page listing the onboarding API's industry
  pairings, without its front matter. `cargo xtask codegen` generates
  `achq/src/onboarding/industry.rs` from it.
- `overrides.toml`: every intentional difference between these files and the
  crate, each with its reason.

The JSON fragments are kept exactly as the pages embed them (see
`.gitattributes`).

## Updating

1. Compare the vendored files with the live documentation:

   ```sh
   cargo xtask drift
   ```

   It lists the properties, query parameters and schemas added, removed or
   changed in each file, and any new API reference page.

2. Download the pages that changed, which also updates the dates and checksums
   above:

   ```sh
   cargo xtask fetch
   ```

3. Regenerate the industry table with `cargo xtask codegen`, then run
   `cargo xtask conformance` and fix what it reports in the request types or in
   `overrides.toml`. Review the diff and record user-visible changes in
   `CHANGELOG.md`.
