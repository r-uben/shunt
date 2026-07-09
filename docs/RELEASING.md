# Releasing shunt

Everything below is prepared but **not yet executed** — the repo is still private
(`chatbot-pf/shunt`) and nothing has been published. The plan: transfer to
`pleaseai/shunt`, make it public, then release. This is the go-live sequence.

## What is already in place

- **Dual license**: `LICENSE-MIT` + `LICENSE-APACHE` (`MIT OR Apache-2.0`), copyright
  Passion Factory, matching the other pleaseai OSS tools.
- **Cargo metadata**: the package publishes as **`shunt-gateway`** (the bare `shunt` name on
  crates.io is taken by an unrelated project); the library and binary are still named `shunt`,
  so `cargo install shunt-gateway` installs a `shunt` binary and no source changes were needed.
  `cargo publish --dry-run` passes; the tarball is trimmed via `include` (54 KiB compressed).
- **Release CI**: `.github/workflows/release.yml` triggers on `v*` tags — builds
  `shunt-darwin-arm64` / `shunt-darwin-x64` (macos-14) and `shunt-linux-x64` /
  `shunt-linux-arm64` (musl, static) on native runners, creates a GitHub release with the
  binaries + `SHA256SUMS`, then publishes `shunt-gateway` to crates.io. All third-party
  actions are pinned to full commit SHAs.
- **Homebrew formula draft**: `packaging/homebrew/shunt.rb`, following the
  `pleaseai/homebrew-tap` binary-release pattern used by `csp.rb` / `ask.rb`.

## Go-live sequence

1. **Make the repo public.** Preferably transfer `chatbot-pf/shunt` → `pleaseai/shunt`
   (matches the tap convention — every formula in `pleaseai/homebrew-tap` points at the
   `pleaseai` org — and the org's open-source standards). The transfer needs org admin
   permissions. If the repo ends up anywhere other than `pleaseai/shunt`, update
   `repository` in `Cargo.toml` and the URLs in `packaging/homebrew/shunt.rb`.
   Also update `README.md`'s "private, early" status line.
2. **Publish `shunt-gateway` v0.1.0 to crates.io manually** — Trusted Publishing can only
   be configured for a crate that already exists, so the first publish uses a personal
   API token (crates.io → Account Settings → API Tokens, scope `publish-new`):
   ```bash
   cargo publish --locked --token <token>
   ```
   The publishing account owns the crate; add other owners with `cargo owner --add`.
   Then set up OIDC for future releases:
   - crates.io → `shunt-gateway` → Settings → **Trusted Publishing** → add GitHub:
     repository `pleaseai/shunt`, workflow `release.yml`, environment `release`.
   - GitHub → repo Settings → Environments → create an environment named `release`
     (optionally with required reviewers / tag-only deployment branches).
   The `publish-crate` job in `release.yml` authenticates via
   `rust-lang/crates-io-auth-action` (`id-token: write`) — no long-lived token secret.
   Note: on the *first* tag push the job will fail (the crate was just published
   manually with the same version); that's expected and harmless.
3. **Set up the release-please GitHub App credentials.** Releases are cut by
   release-please (`.github/workflows/release-please.yml`): it opens a release PR from
   conventional commits, and merging that PR creates the `v<version>` tag + GitHub
   release. It authenticates with a GitHub App token (the default `GITHUB_TOKEN` would
   not trigger `release.yml` on the tag it pushes):
   - Create a GitHub App (org → Settings → Developer settings → GitHub Apps) with
     repository permissions **Contents: read & write** and **Pull requests: read &
     write**, and install it on `pleaseai/shunt`.
   - Set the `RELEASE_GITHUB_APP_CLIENT_ID` repository **variable** and the
     `RELEASE_GITHUB_APP_PRIVATE_KEY` repository **secret** (the app's PEM key).
4. **Merge the release PR.** release-please proposes `v0.1.0` (manifest starts at
   `0.0.0`; the `feat:` history bumps it to `0.1.0`, matching `Cargo.toml`). Merging
   tags `v0.1.0` and creates the GitHub release; the tag then triggers `release.yml`,
   which builds the four binaries and uploads them + `SHA256SUMS` to that release, and
   publishes `shunt-gateway` to crates.io via Trusted Publishing.
5. **Publish the formula.** Copy `packaging/homebrew/shunt.rb` into `pleaseai/homebrew-tap`
   as `shunt.rb`, fill in the four `sha256` values from the release's `SHA256SUMS` asset,
   and open a PR against the tap. Then:
   ```bash
   brew install pleaseai/tap/shunt
   ```

## Subsequent releases

Land conventional commits on `main`; release-please keeps a release PR up to date
(`feat:` → minor, `fix:` → patch, `feat!:`/`BREAKING CHANGE:` → major). Merge the PR
when ready — it bumps `Cargo.toml`/`Cargo.lock`, updates `CHANGELOG.md`, tags, and the
rest of the pipeline (binaries, crates.io) runs automatically. Then update `version` +
the four `sha256` values in the tap's `shunt.rb`. To force a specific version, add a
`Release-As: x.y.z` footer to a commit.

## Notes

- The linux binaries are **musl static** builds to avoid glibc version constraints. If a
  dependency ever grows a C dependency that breaks musl, switch the linux targets in
  `release.yml` to `-gnu` and accept the newer-glibc floor.
- `cargo install shunt-gateway` is the crates.io install path; homebrew is the
  binary path. Both produce a `shunt` binary.
- The `publish-crate` job runs after the GitHub release; if it fails (e.g. Trusted
  Publishing not yet configured, or the version already exists on crates.io), the
  release itself is unaffected — fix and `cargo publish` manually from the tag.
