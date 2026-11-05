# achq-derive

Derive macros for [`achq`](https://crates.io/crates/achq): the validation, redacted
`Debug` and form encoding of its request and response types. They call into
`achq`'s internals, so the two crates are released together with the same version;
depend on `achq` rather than on this crate.
