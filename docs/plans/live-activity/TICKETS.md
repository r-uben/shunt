# TICKETS — Admin live activity

Status keys: `TODO` · `WIP` · `DONE` · `BLOCKED`. `depends-on` gates dispatch.
Parallelizable = no shared files / no dependency. Each ticket is completed by one
implementer and then checked by an independent reviewer before commit.

Design contract: [#214](https://github.com/pleaseai/shunt/issues/214).
All tickets in this graph land together in one focused PR so code, behavior spec,
and user documentation cannot drift.

## Frozen scope

The opt-in admin dashboard gains a bounded, in-memory operational view of inference
requests on `/v1/messages` and the inbound Codex Responses endpoint. It may retain only
a synthetic request id, bounded provider/model labels, adapter/protocol, timestamps and
durations, HTTP status, terminal outcome, and upstream-reported streaming token counts.

It must never retain prompts, responses, reasoning, tool arguments/results, request bodies
or headers, credentials, account identities, or raw/derived session ids. The view is
instance-wide for administrators, resets on restart, and is not an audit log. Persistence,
session grouping, SSE/WebSocket updates, and per-request retry/failover/continuation data
are non-goals.

## Stream A — Bounded activity state

### TICKET-A1 — Add the privacy-safe activity store · DONE · depends-on: none · wave 1
**Problem:** Existing metrics discard request-level state, so the admin surface has no safe,
bounded source for active and recent rows.

**Do:** Add a process-lifetime activity module backed by one bounded in-memory queue. Records
transition from active to terminal in place and use a synthetic process-local id unrelated to
client/session identifiers. Bound both row count and copied provider/model strings. Make every
recording operation synchronous, non-panicking, and best-effort. Add the store to `AppState`
parallel to the other process-lifetime stores, construct it only when `[server.admin]` was
present at boot, preserve it across hot reload, and make hooks no-op when absent.

**Files:** `src/activity.rs` (new), `src/lib.rs`, `src/server.rs`

**Done when:**
- The queue cannot exceed its named capacity even when all retained rows remain active.
- Oldest-first eviction and active-to-terminal updates are unit tested.
- Provider/model display values are UTF-8-safely capped at a named maximum.
- Lock poisoning or an evicted id cannot fail an inference request.
- No activity store is allocated when admin routes are disabled at boot.
- Reloading config preserves the same store when admin routes were enabled at boot.
- The record type has no content, header, credential, account, or session field.

## Stream B — Request lifecycle

### TICKET-B1 — Record Messages and inbound Codex lifecycles · TODO · depends-on: A1 · wave 2
**Problem:** The store needs exact lifecycle updates without buffering streams, parsing SSE a
second time, or leaving stale active rows on errors and cancellations.

**Do:** Start a row only after routing resolves trusted provider/adapter metadata; exclude
`count_tokens` and document that pre-routing/auth/policy rejections are outside v1. Cover both
`/v1/messages` and inbound Codex. Extend the existing `stream_metrics` observer so one SSE
classification and parser drives response-header latency, TTFT, final outcome, and reported
streaming token usage. Finalize non-SSE responses, adapter errors, upstream cuts, and client
disconnects exactly once. Use a small lifecycle handle/guard so cancellation before response
headers cannot strand a row. Do not capture request/response headers; “header latency” means
only elapsed time until the response headers arrive.

**Files:** `src/proxy.rs`, `src/codex_endpoint.rs`, `src/stream_metrics.rs`,
`src/stream_metrics/tests.rs`

**Done when:**
- Focused tests cover successful SSE for both protocols, non-SSE responses, adapter failure,
  upstream cut, client disconnect/drop, and exactly-once finalization.
- Streaming behavior remains byte-for-byte pass-through and uses the existing bounded SSE
  parser; no second parser or buffering path is introduced.
- `count_tokens`, pre-routing failures, and requests handled while admin is disabled create no
  rows.
- A request-declared streaming flag cannot disagree with response classification: the response
  `Content-Type` is classified once by `stream_metrics`.
- Only upstream-reported streaming usage is recorded; missing/non-streaming usage remains absent.
- Existing aggregate Sentry/OpenTelemetry metrics remain unchanged.

## Stream C — Authenticated admin surface

### TICKET-C1 — Expose the read-only activity API · TODO · depends-on: B1 · wave 3
**Problem:** Administrators need a secure machine-readable snapshot, and privacy guarantees
need end-to-end tests at the HTTP boundary.

**Do:** Add a focused admin submodule and register authenticated `GET /admin/activity` only in
the existing opt-in admin route tree. Return active and recent rows newest-first from the bounded
snapshot, with secure no-store JSON headers and the existing header-token or browser-session
authentication. Add a dedicated integration-test file with sentinel content, credential-like
headers, account markers, and a known session id to prove none appear in JSON. Test the
instance-wide admin visibility contract without introducing user/session grouping.

**Files:** `src/admin/activity.rs` (new), `src/admin/mod.rs`, `tests/admin_activity.rs` (new)

**Done when:**
- The endpoint is absent without `[server.admin]`, rejects unauthenticated callers, and accepts
  both supported admin authentication modes.
- JSON distinguishes active and terminal rows and reports absent measurements as null/omitted
  according to one documented schema.
- Privacy regression tests prove request content, headers, credentials, account identity, and
  raw/derived session ids are absent from the response.
- Responses retain the admin API's existing `Cache-Control: no-store`, `nosniff`, CSP/auth, and
  no-CORS posture.
- A poisoned activity lock yields a safe empty/unavailable snapshot rather than an inference or
  admin-process panic.

### TICKET-C2 — Render the polling activity table · TODO · depends-on: C1 · wave 4
**Problem:** The API is not useful to an operator without a compact view matching the current
admin dashboard.

**Do:** Add a “Live activity” card to the existing server-rendered dashboard. Display a textual
active/recent summary and columns for state, provider, model, age, HTTP status, header latency,
TTFT, and reported input/output tokens. Poll with the existing fetch/DOM style, pause while the
document is hidden, resume immediately when visible, render all untrusted values through
`textContent`, and provide accessible empty/error/loading states without color-only meaning.
Avoid a frontend framework, SSE, WebSockets, or new dependencies.

**Files:** `src/admin/html.rs`

**Done when:**
- The dashboard renders active, completed, error-event, upstream-cut, and client-disconnect
  states as text; unavailable values render as `—`.
- Polling does not overlap requests, pauses while hidden, and resumes on visibility change.
- Provider/model values never reach `innerHTML`; existing CSP needs no relaxation.
- The summary uses a restrained `aria-live` region while the changing table does not spam screen
  readers.
- Focused source/render assertions cover the endpoint, columns, safe DOM insertion, visibility
  behavior, and empty/error states; no browser-test dependency is added solely for this feature.

## Stream D — Specification and user documentation

### TICKET-D1 — Specify and document the activity surface · TODO · depends-on: C2 · wave 5
**Problem:** The new endpoint and retention/privacy behavior are observable and must ship with a
stable specification and user guidance in the same PR.

**Do:** Add the next milestone behavior spec and update the existing admin guide with the table,
endpoint, field meanings, retention limits, admin-wide visibility, privacy exclusions, and
“operational aid, not audit log” warning. Update the comparison only to remove the now-false
claim that Shunt has no live-traffic view. Confirm README, other `docs/`, site reference pages,
and generated wiki surfaces were considered; do not hand-edit `wiki/` or `site/dist/`.

**Files:** `docs/m13-admin-activity-surface.md` (new),
`site/src/content/docs/guides/admin-remote-provisioning.md`, `docs/comparison.md`

**Done when:**
- The M13 spec freezes the API schema, lifecycle states, bounds, trust boundary, and non-goals.
- User docs explain how to read the table and authenticate to the endpoint.
- The docs explicitly disclose that an administrator sees activity across all instance users,
  rows may be evicted, and data disappears on restart.
- `docs/comparison.md` changes only the activity/observability claim; unrelated stale citations
  and tool-search claims remain for separate work.
- `cd site && npm run build` succeeds and no generated `wiki/` files are edited.

## Stream Q — Integrated verification

### TICKET-Q1 — Run independent final review and quality gates · TODO · depends-on: D1 · wave 6
**Problem:** Request lifecycle and admin metadata cross concurrency, privacy, and browser trust
boundaries that individual ticket tests do not fully validate together.

**Do:** Have a reviewer independent from the implementers inspect the complete diff for
correctness, privacy/security, scope, and documentation drift. Resolve verified findings without
expanding into deferred observability features. Run the repository's complete quality gates and
record observed results for the PR.

**Files:** No planned product files; verified fixes may touch only files already owned by A1–D1.

**Done when:**
- Independent review confirms bounded memory, no sensitive-data path, exact-once lifecycle
  completion, unchanged streaming bytes, and authenticated/no-store browser exposure.
- `cargo fmt --all --check` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- `cargo test --all-features --workspace` passes.
- `cd site && npm run build` passes.
- The PR test plan reports observed results and references `Closes #214`.

## Deferred work outside this plan

These require separate issues/design contracts and are not dispatchable from this graph:

- Live-probe native Responses `tool_search`, then separately decide whether to change its default.
- Repair stale comparison citations and translated continuation-probe contradictions beyond the
  one activity claim changed by D1.
- Per-row retries, failovers, account selection, or Codex continuation hit/fallback.
- Persistent history, billing, per-user/session grouping, or push-based dashboard updates.
