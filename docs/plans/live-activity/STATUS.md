# STATUS — Admin live activity

Last updated: 2026-07-18

## Stage

The design contract is filed as [#214](https://github.com/pleaseai/shunt/issues/214),
the issue-aligned branch exists, and A1 is implemented and independently approved. B1 is now
ready to dispatch; later tickets remain dependency-blocked.

## Base state (clean before tickets)

- Repository: `pleaseai/shunt`
- Branch: `feat/214-admin-live-activity`
- Base commit: `3272daa feat(admin): show time-until-reset per quota window in pool dashboard (#206)`
- Matching issue search found no pre-existing activity-view issue.
- Architecture was explored by a ten-agent workflow and the ticket decomposition was attacked by
  external/independent reviewers before this plan was written.
- The plan is one feature branch and one PR so code, behavior spec, and user docs ship together.

## Ticket board

| Ticket | Stream | Status | depends-on | Wave |
|--------|--------|--------|------------|------|
| A1 | Bounded activity state | DONE | — | 1 |
| B1 | Request lifecycle | TODO | A1 | 2 |
| C1 | Admin activity API | TODO | B1 | 3 |
| C2 | Dashboard UI | TODO | C1 | 4 |
| D1 | Specification and docs | TODO | C2 | 5 |
| Q1 | Integrated verification | TODO | D1 | 6 |

## Dispatch waves

- Wave 1: A1 only.
- Wave 2: B1 after A1 is implemented and independently reviewed.
- Wave 3: C1 after lifecycle behavior is stable.
- Wave 4: C2 after the API schema is stable.
- Wave 5: D1 after observable UI behavior is stable; documentation remains in the same PR.
- Wave 6: Q1 independent whole-diff review and complete quality gates.

## Recent decisions

- 2026-07-18: Use one bounded queue; terminal records update in place. No separate unbounded
  active map.
- 2026-07-18: The store is created only when admin routes are enabled at boot and survives config
  reload, but never process restart.
- 2026-07-18: Store no request/session/account/content identity. Provider/model strings are
  treated as untrusted and capped.
- 2026-07-18: Poll the existing no-framework dashboard; do not add SSE, WebSockets, persistence,
  or frontend dependencies.
- 2026-07-18: Native tool-search verification and unrelated comparison cleanup require separate
  issues rather than expanding #214.

## Next action

Run `/plan next live-activity` to dispatch B1 to one implementer and then an independent reviewer.
