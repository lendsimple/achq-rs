//! Compile-fail checks: misuse of the derive must give a clear, well-placed error.
//!
//! The expected errors follow one rustc's wording, so this only runs in the
//! repository, where `.cargo/config.toml` sets `ACHQ_UI_TESTS`. From the published
//! crate it skips. It also skips without the `onboarding` feature: rustc lists
//! some of a trait's implementations in an error, and the feature adds some.

#[test]
fn derive_errors() {
    if std::env::var_os("ACHQ_UI_TESTS").is_none() {
        eprintln!("skipping: compile-error snapshots run only in the repository");
        return;
    }
    if !cfg!(feature = "onboarding") {
        eprintln!("skipping: the snapshots list implementations from the onboarding feature");
        return;
    }
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
