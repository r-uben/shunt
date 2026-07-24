# Plan creation — live activity

Date: 2026-07-18
Issue: [#214](https://github.com/pleaseai/shunt/issues/214)
Branch: `feat/214-admin-live-activity`

## What was decided

Create one focused implementation plan for a bounded, privacy-safe live request view in the
opt-in admin dashboard. The feature covers Messages and inbound Codex requests and reuses the
existing streaming observer for TTFT, outcomes, and reported token usage.

The plan deliberately excludes persistence, session/user grouping, content capture, push updates,
and per-request retry/failover/continuation correlation. Native tool-search verification and
unrelated comparison-document repairs require separate design contracts.

## Why

Shunt already exposes aggregate telemetry and account-pool health, but an operator cannot see
active/recent inference requests without external infrastructure. A bounded in-memory table is
the smallest useful addition that does not turn Shunt into an APM or audit platform.

## Review evidence

- A ten-agent workflow traced lifecycle hooks, admin integration, security/privacy, metrics reuse,
  tool-search status, and minimal UI, then ran independent correctness, security, and scope reviews.
- The first store proposal was corrected from separate/unbounded active and recent structures to
  one bounded queue.
- The ticket graph was attacked by external/independent reviewers. Their accepted corrections
  include process-lifetime store placement, admin-disabled no-op behavior, capped untrusted labels,
  focused admin/test modules, exact lifecycle acceptance criteria, a new M13 specification, and
  separation of tool-search work from #214.

## Blockers

None. A1 is ready for `/plan next live-activity`.
