# STATUS — Gemini Code Assist and Antigravity providers

Last updated: 2026-07-22

## Stage

Implementation is complete on `feat/gemini-provider` and published as commit `6e3cdad`.

- Tracking issue: [#233](https://github.com/pleaseai/shunt/issues/233)
- Pull request: [#234](https://github.com/pleaseai/shunt/pull/234)
- Socket Security passed; Greptile and Cubic reviews are pending.
- All implementation tickets for this PR are done. Multi-account pooling remains future work.

## Verification

Observed on 2026-07-22:

- `cargo fmt --all --check` passed.
- `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `cargo test --all-features --workspace` passed.
- `git diff --check` passed.
- `claude-gemini-3.5-flash-via-antigravity` routed to `agy --model gemini-3.5-flash` and returned HTTP 200 with the expected sentinel.
- `claude-gemini-3.6-flash-via-antigravity` routed to `agy --model gemini-3.6-flash` and returned HTTP 200 with the expected sentinel.

The Antigravity evidence proves the exact model slug shunt supplies to `agy`; it does not independently attest Google's internal serving identity.

## Recent decisions

- Native Code Assist and Antigravity are separate providers because they use different transports, authentication, model namespaces, and capacity behavior.
- The native provider reuses `~/.gemini/oauth_creds.json`. A valid access token works directly; shunt-side refresh requires `SHUNT_GOOGLE_CLIENT_ID` and `SHUNT_GOOGLE_CLIENT_SECRET`, otherwise the operator reruns `gemini login`.
- Google OAuth client credentials are not committed. GitHub blocked the initial unpublished commit through push protection; the commit was amended to remove the credentials instead of bypassing protection.
- Native Gemini tool-result correlation, non-streaming endpoint selection, terminal SSE closure, thinking-config nesting, and host-independent Antigravity tests were fixed before publication.
- Multi-account pooling is outside PR #234 and remains future hardening.

## Outstanding TODOs

- Triage and address any actionable findings from PR #234.
- Rerun the required quality gates after any review-driven changes.
- Keep unrelated local work out of this PR:
  - `src/auth/codex/auth.rs`
  - `docs/plans/live-activity/`
  - `docs/plans/usage-bars/`

## Next action

Check Greptile and Cubic results on PR #234, triage verified findings, and update the branch only when a finding requires a change.
