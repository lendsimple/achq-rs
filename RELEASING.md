# Releasing

`achq` and `achq-derive` are released together, with the same version. `achq` pins
`achq-derive` exactly (`=x.y.z`), because the derive generates calls into `achq`'s
internals.

Releases are published by the [Release workflow](.github/workflows/release.yml) when a
GitHub Release is published (not when a tag is pushed, and not for drafts). It checks
that the release's tag matches the crate version and that `CHANGELOG.md` has a section
for it, runs the codegen and conformance checks and the tests, and then runs
`cargo publish --workspace`, which publishes `achq-derive` first and skips the
unpublished `xtask`.

## Each release

1. Make sure `main` is green in CI.
2. Bump `version` in the workspace `Cargo.toml`, and the `achq-derive` requirement in
   `achq/Cargo.toml` to match.
3. In `CHANGELOG.md`, rename `[Unreleased]` to the new version with today's date, add
   an empty `[Unreleased]` section above it, and update the links at the bottom.
4. Check locally:

   ```sh
   cargo xtask drift                  # ACHQ's documentation still matches schema/
   cargo xtask conformance            # the request types still match it
   cargo test --workspace --all-features
   ./scripts/test-packaged-crate.sh   # the tests, run from the packaged crates
   cargo publish --dry-run --workspace
   ```

   And against ACHQ's public sandbox, which is shared and public (the tests send
   only made-up data):

   ```sh
   ACHQ_SANDBOX=1 cargo test -p achq --all-features --test sandbox -- --ignored
   ```

   If ACHQ's answers may have changed, refresh the recorded fixtures with
   `cargo xtask record` and review the diff. It keeps the one fee profile it created
   in the sandbox rather than creating another, and never submits a merchant
   application.

5. Commit and push:

   ```sh
   git commit -am "Release x.y.z"
   git push origin main
   ```

6. Publish a GitHub Release for the tag `vx.y.z` on `main`, either on the repository's
   Releases page or with `gh`, using the version's changelog section as the notes:

   ```sh
   awk '/^## \[x.y.z\]/{p=1; next} /^## \[|^\[.*\]: /{p=0} p' CHANGELOG.md > notes.md
   gh release create vx.y.z --target main --title "x.y.z" --notes-file notes.md
   ```

   Publishing the release starts the workflow. A release created by another workflow
   with the default `GITHUB_TOKEN` does not trigger it; GitHub suppresses that to
   prevent loops.
7. If the `crates-io` environment requires approval, approve the run in the Actions tab.

If the workflow publishes `achq-derive` but then fails on `achq`, fix the cause and
publish the rest by hand (`cargo publish -p achq`): re-running the workflow would fail
on the version that is already published.

## One-time setup

crates.io only allows trusted publishing (short-lived tokens, no stored secret) for
crates that already exist, so the first release needs an API token.

1. In the repository settings, create an environment named `crates-io`. Adding
   required reviewers makes every release wait for approval.
2. Create a crates.io API token with the `publish-new` and `publish-update` scopes, and
   add it to the `crates-io` environment as the secret `CARGO_REGISTRY_TOKEN`.
3. Release `0.1.0` as above. The workflow tries trusted publishing first; until it is
   configured that fails, and the workflow falls back to the secret.
4. On crates.io, for **each** of `achq` and `achq-derive`, open Settings → Trusted
   Publishing and add a GitHub publisher:
   - repository owner: `lendsimple`
   - repository name: `achq-rs`
   - workflow filename: `release.yml`
   - environment: `crates-io`
5. From then on the workflow uses trusted publishing even if the secret is still there.
   Delete the `CARGO_REGISTRY_TOKEN` secret and revoke the token on crates.io anyway, so
   no long-lived publish token is left around.
