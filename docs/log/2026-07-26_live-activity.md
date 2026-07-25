# 2026-07-26 — Live-activity rows are per failover attempt

## Decision

The admin live-activity view opens one row per **upstream attempt**, not per client request.

A request whose primary upstream fails over produces two rows: the upstream that gave up (settled `Error` with the status that triggered the advance) and the upstream that served. The row is opened in `proxy::failover::forward` immediately alongside the existing `crate::metrics::record_proxied_request` call, so the activity view and the Prometheus metrics count the same events.

## Why

The recording hook was originally written against `proxy::forward`, when a request dispatched to exactly one upstream and provider/model were unambiguous labels. Ordered cross-provider failover (#218/#224) moved `forward` into `proxy/failover.rs` and turned that single dispatch into a loop over a route chain, which made "which provider served this request?" a question with more than one answer.

Per-request rows would have to pick one provider for the whole request, and the only provider known when the row opens is the *first* one tried. On any request that fails over, the operator would see the request attributed to an upstream that did not serve it — the failover would be invisible in the exact view meant to show what the gateway is doing. Per-attempt rows make the failover legible, and they align the view with metrics, which were already per-attempt.

The cost is that request counts in the activity view exceed client request counts when failover fires. That is accepted: the view is an operator's live picture of upstream traffic, not a billing or client-facing counter.

## Implementation boundaries

- Row settlement happens exactly once per attempt, at the terminal edge:
  - streaming responses hand an `ActivityFinish` to the SSE observer, which settles on the stream's real outcome (completed / upstream cut / client disconnect) rather than on the response head;
  - buffered and early-error responses have no stream lifetime, so the caller settles directly from the response status;
  - an attempt that advances settles its own row before the loop continues.
- Relaying a remembered response after chain exhaustion passes no activity handle: that attempt's row was already settled when it advanced, and must not be reopened or double-settled.
- `count_tokens` never reaches the failover loop, so token-estimation calls stay out of the view — matching the pre-existing metrics gate.
- The store is allocated only when `[server.admin]` is present at boot, and is `None` otherwise; hooks no-op on `None`.
- No route is registered yet. `GET /admin/activity` is named in comments as the intended consumer but does not exist, so this work changes no observable behavior and issue #214 remains open.

## Notes

The rebase also replaced an `std::env::set_var`-based admin test helper with a per-test `tokens_file`. Env mutation is process-global and raced other tests in the same binary regardless of key name; it was observed failing `auth::tests::resolves_chatgpt_account_token_env_verbatim_with_account_id` intermittently.

## Links

- Issue: https://github.com/pleaseai/shunt/issues/214
- Failover design this had to be reconciled with: `.please/docs/decisions/0002-ordered-upstreams-failover.md`, `docs/upstreams-failover.md`, issues #218 / #224
- Plans parked on `wip/local-plans-and-test-flake`: `docs/plans/live-activity/TICKETS.md`
