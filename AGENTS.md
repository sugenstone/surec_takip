# AGENTS.md

## Instructions for AI Coding Agents

> This file is the operational instruction set for any AI agent
> modifying this repository.

# 1. Required Reading Order

Before writing code, read:

1.  `AGENTS.md`
2.  `AI_CODING_AGENT_MASTER_PLAN.md`
3.  `DATABASE_SCHEMA.md`
4.  `API_CONTRACT.md`
5.  `UX_UI_SPEC.md`
6.  existing `README.md`
7.  relevant source files, migrations, and tests

Do not begin implementation based only on a user prompt when repository
documentation already defines the feature.

# 2. Source of Truth Priority

If documents conflict, use this order:

``` text
1. Explicit latest human instruction
2. AGENTS.md operational rules
3. AI_CODING_AGENT_MASTER_PLAN.md product/implementation order
4. DATABASE_SCHEMA.md persistence rules
5. API_CONTRACT.md transport/API rules
6. UX_UI_SPEC.md interface behavior
7. Existing implementation
```

If a conflict would cause a breaking architectural change, stop and
document the conflict before proceeding.

# 3. Core Product Rules

Never violate these:

-   General-purpose platform; no industry-specific core logic.
-   Multi-tenant from the foundation.
-   Backend enforces tenant isolation.
-   Backend enforces authorization.
-   Modular monolith first.
-   PostgreSQL is source of truth.
-   Realtime is event-driven.
-   Timers do not write every second.
-   Time is represented by sessions.
-   Important state changes are auditable.
-   Routine mistakes are recoverable where feasible.
-   Templates are versioned/snapshotted.
-   Worker UX stays simple.
-   Billing never blocks an employee in the middle of a critical task
    without an explicit product policy.
-   WebSocket is not the sole source of truth.
-   No silent concurrent overwrites.
-   No secrets in repository/logs.

# 4. Implementation Sequence

Follow the master plan phases in order.

Current high-level order:

``` text
01 Repository + CI
02 Multi-tenancy
03 Auth + RBAC
04 Projects + Sections + Cards
05 Dynamic Properties
06 Processes + Steps
07 Dependencies
08 Time Sessions
09 Realtime + Outbox
10 Employee UX
11 TV Display
12 Templates
13 Checklist / Forms / Approvals
14 Audit + Undo + Restore
15 Notifications + Email
16 Files + Comments
17 Search + Views + Bulk
18 Import / Export
19 Reporting
20 Optional Customer Module
21 Resource Foundation
22 PWA / Offline Foundation
23 Billing / Subscription
24 Automations
25 API / Webhooks
26 Tenant Customization
27 Advanced Operations
```

Do not jump to attractive later features while foundational acceptance
criteria are failing.

# 5. Task Execution Protocol

For every task:

## Step A --- Inspect

-   Read relevant docs.
-   Inspect current implementation.
-   Identify existing patterns.
-   Identify migrations/API/tests affected.

## Step B --- Plan

Write a short internal implementation plan: - files to change, - schema
impact, - API impact, - permission impact, - realtime impact, - audit
impact, - undo/delete impact, - tests.

Do not create unnecessary abstractions.

## Step C --- Implement

Implement the smallest complete vertical slice.

## Step D --- Verify

Run: - formatter, - linter, - type checks, - unit tests, - integration
tests, - relevant E2E tests.

## Step E --- Review

Check: - tenant scoping, - permissions, - validation, - concurrency, -
error states, - accessibility, - responsive behavior, - realtime
consistency.

## Step F --- Document

Update: - OpenAPI if API changed, - schema docs if architecture
changed, - README/setup if developer workflow changed, - migrations, -
tests.

Only then proceed.

# 6. Do Not Fake Completion

Never: - mark TODO code as complete, - return mocked success from
unfinished backend logic, - disable tests to get green CI, - swallow
errors, - use `unwrap()`/panic in normal Rust request paths without
strong justification, - bypass authorization temporarily, - hard-code a
tenant/user ID, - replace a failing integration with a frontend-only
workaround.

If blocked, report the actual blocker.

# 7. Rust Backend Standards

Preferred stack: - Axum - Tokio - SQLx - Serde - Tracing

Rules: - Keep HTTP handlers thin. - Domain/application services contain
business rules. - Repositories/data-access remain tenant-aware. - Use
typed errors. - Map domain errors to stable API error codes. - Use
transactions for multi-step state changes. - State transition functions
must validate current state. - Do not let generic PATCH handlers bypass
domain commands. - Avoid unnecessary trait/DI complexity. - Prefer
explicit code until abstraction is justified by repeated patterns.

Suggested conceptual layers:

``` text
HTTP
↓
Application / Commands / Queries
↓
Domain Rules
↓
Persistence / External Adapters
```

Do not turn this into ceremony-heavy enterprise architecture.

# 8. SvelteKit Frontend Standards

Rules: - TypeScript required. - Reusable UI primitives live in shared UI
package/lib. - Page components should not contain large amounts of
API/business logic. - Centralize API client behavior. - Use generated
API types where practical. - Handle loading/empty/error states. -
Preserve local draft on recoverable save failure. - Realtime updates
should update stores/query state predictably. - Avoid global stores for
state that can remain local. - Mobile employee experience must be tested
at narrow widths. - Admin interfaces must support keyboard use. - Never
key UI behavior off human-readable error message strings; use stable
error codes.

# 9. Database Rules

Before creating a migration: - verify table does not already exist, -
verify feature phase is active, - add foreign keys, - add required
tenant scoping, - add indexes justified by query patterns, - consider
soft deletion, - consider uniqueness under tenant.

Never: - use floating point for money, - store passwords/tokens/API keys
raw, - store uploaded file bytes in PostgreSQL, - trust client tenant
ID, - create a giant JSONB blob for core domain data, - physically
delete audit history through normal product actions.

# 10. Multi-Tenant Checklist

Every tenant-owned query/mutation must answer:

``` text
How is tenant determined?
How is membership verified?
How is workspace scope verified?
What permission is required?
Can a guessed foreign UUID reveal anything?
```

Cross-tenant requests should generally behave like not-found, not reveal
resource existence.

Add integration tests whenever a new tenant-owned resource type is
introduced.

# 11. Permission Checklist

For each endpoint/action: - define permission key, - define scope, -
enforce backend, - optionally hide/disable frontend control, - ensure
realtime event does not reveal unauthorized data.

Permission changes should take effect promptly.

# 12. Domain Command Rules

Use explicit commands for meaningful transitions:

``` text
start
pause
resume
complete
reopen
approve
return_for_correction
move
restore
publish
```

Do not expose arbitrary state mutation like:

``` json
{"status":"completed"}
```

when business rules must run.

# 13. Time Tracking Rules

Time tracking is financially/operationally important.

Required invariants: - one session has one start, - stop cannot precede
start, - paused time is not active time, - no per-second database
writes, - corrections are audited, - switching tasks is transactional, -
duplicate requests are idempotent, - client timer is display-only;
server data is authoritative.

# 14. Dependency Rules

-   No cycles.
-   No self-dependency.
-   Same tenant.
-   Explain blocked state.
-   Completing prerequisite recomputes dependents.
-   Unlock + event creation must be transactionally safe.
-   Override requires permission and reason.

# 15. Realtime Rules

Use domain events + transactional outbox.

Never: - emit a critical event before DB commit, - assume WebSocket
delivery is guaranteed, - broadcast all tenant data to all sockets, -
send timer ticks every second.

Clients must refetch after reconnect/gap/stale revision.

# 16. Audit Rules

Audit important mutations including: - assignment, - status
transitions, - dependency override, - permission/role changes, - time
corrections, - deletion/restoration, - template publication, - billing
changes.

Audit entries are append-only under normal product behavior.

# 17. Undo / Delete Rules

Before implementing delete: 1. Is it soft-delete? 2. Who can restore? 3.
Does deleting parent affect children? 4. What happens to active work? 5.
What does realtime show? 6. What is audited? 7. Is immediate Undo
available?

Never cascade-delete important operational history casually.

# 18. API Rules

-   Prefix `/api/v1`.
-   OpenAPI updated with endpoint changes.
-   Stable machine-readable error codes.
-   Cursor pagination for large mutable collections.
-   Idempotency for retry-sensitive commands.
-   Revision/optimistic concurrency for collaborative edits.
-   Do not expose DB internals.
-   Cross-tenant IDs do not leak existence.
-   Heavy async jobs return `202`.

# 19. Error Handling

Every feature must define: - validation error, - unauthorized, -
forbidden, - not found, - domain blocked, - conflict, - network/server
failure, - retry behavior.

Frontend messages are human-readable; API codes are stable.

# 20. UX Rules for Agents

Before adding a visible control ask:

> Does this user need this control on this screen for their current
> task?

If no, hide it behind secondary/advanced UI.

Employee primary UI should remain:

``` text
My Tasks
Current Task
Start
Pause
Resume
Complete
```

Do not expose admin workflow configuration to employees.

# 21. Accessibility Rules

For new UI: - keyboard reachable, - visible focus, - semantic labels, -
no color-only state, - accessible dialogs, - appropriate touch target, -
reduced motion respected, - timer should not cause screen reader
announcement every second.

# 22. Testing Requirements

A change is incomplete without appropriate tests.

### Unit

Use for: - pure domain logic, - permission evaluation, - dependency
evaluation, - time calculations, - entitlement rules.

### Integration

Use for: - database behavior, - tenant isolation, - transactions, -
outbox, - state transitions, - template snapshots.

### E2E

Use for critical user journeys.

Do not overuse E2E for logic better tested below the UI.

# 23. Mandatory Critical E2E Flow

Keep this scenario passing:

``` text
Admin creates project
→ section
→ card
→ process/template
→ assignment

Employee sees task
→ starts

Admin sees active state
TV sees live timer

Employee pauses
→ resumes
→ completes

Dependent step unlocks
Notification appears
Audit history is correct

Authorized user reopens accidental completion
History preserves both actions
```

# 24. Code Quality Gates

Before reporting a task complete, run relevant commands for:

``` text
Rust format
Rust clippy/lint
Rust tests
SQL migration checks
TypeScript typecheck
Frontend lint
Frontend unit tests
Playwright critical flow where affected
```

If repository scripts exist, use those instead of inventing alternate
commands.

# 25. Performance Rules

Do not prematurely optimize, but never introduce obvious scale problems.

Avoid: - N+1 queries, - loading unbounded lists, - timer polling every
second, - full-page refetch after every small realtime event, -
broadcasting irrelevant events, - huge card-detail payloads.

Use pagination and targeted includes.

# 26. Security Rules

Never: - log passwords/tokens, - return raw provider secrets, - expose
raw API keys after creation, - trust file MIME from client alone, -
allow unrestricted object-storage URLs, - interpolate SQL, - accept
unsigned billing webhooks, - rely on frontend permission checks.

Validate file size/type and authorize download.

# 27. Billing Rules

Billing uses entitlements.

Wrong:

``` text
if plan == professional
```

Correct:

``` text
entitlements.can("tv_display")
entitlements.limit("users")
```

Payment provider is an adapter, not the source of product authorization
truth.

Operational workers should not be interrupted by routine billing UI.

# 28. Templates

Published template versions are immutable.

Changing a template:

``` text
v3 published
→ create v4 draft
→ edit
→ publish v4
```

Existing live work remains on its recorded version unless explicitly
migrated.

# 29. Comments and Documentation

Comment **why**, not obvious syntax.

Document: - domain invariants, - unusual concurrency choices, -
security-sensitive logic, - non-obvious SQL, - external provider
behavior.

Avoid large stale comment blocks.

# 30. Dependency Policy

Before adding a package/crate: - verify standard library/current
dependencies cannot reasonably solve it, - prefer mature maintained
dependencies, - avoid overlapping libraries, - document major
architectural dependencies.

Do not add a framework merely to save a few lines.

# 31. Git / Change Scope

Keep changes focused.

Do not: - reformat unrelated code, - rename unrelated modules, - rewrite
architecture while fixing a small bug, - mix large refactor with feature
unless necessary.

If a refactor is required, separate it conceptually and test it.

# 32. Feature Completion Checklist

Before marking any feature complete:

``` text
[ ] Correct tenant scope
[ ] Correct permission
[ ] Validation
[ ] Domain rules
[ ] Migration
[ ] API contract
[ ] Loading state
[ ] Empty state
[ ] Error state
[ ] Responsive UI
[ ] Accessibility
[ ] Realtime impact
[ ] Audit impact
[ ] Undo/delete impact
[ ] Concurrency
[ ] Tests
[ ] Documentation
```

Not every box applies to every feature, but each must be considered.

# 33. V1 Priority Protection

If schedule pressure occurs, protect these first:

``` text
tenant security
auth/RBAC
projects/sections/cards
processes/steps
assignments
dependencies
time sessions
employee UX
realtime
TV
templates
audit/undo
notifications
```

Defer before compromising foundations:

``` text
AI
advanced dashboards
complex automations
deep offline
QR
resource planning
advanced billing promotions
formula/rollup properties
SSO
```

# 34. When Requirements Are Ambiguous

Use these defaults:

1.  Preserve tenant isolation.
2.  Preserve audit/history.
3.  Preserve recoverability.
4.  Preserve simple employee UX.
5.  Choose the least complex implementation that does not block the
    documented roadmap.
6.  Do not invent industry-specific assumptions.
7.  If a decision is expensive to reverse, surface it for human
    confirmation.

# 35. Agent Progress Reporting

When completing a development task, report concisely:

``` text
Implemented
- ...

Migrations
- ...

API
- ...

Tests
- ...

Remaining / Risks
- ...
```

Do not claim tests passed unless they were actually executed.

# 36. First Agent Mission

If repository is new, begin exactly here:

``` text
1. Initialize monorepo.
2. Configure SvelteKit + TypeScript frontend.
3. Configure Rust/Axum server.
4. Configure PostgreSQL migration tooling.
5. Configure Docker local environment.
6. Configure CI quality gates.
7. Implement users/sessions.
8. Implement organizations/memberships.
9. Implement workspaces/memberships.
10. Implement tenant context middleware.
11. Write cross-tenant security tests.
12. Do not continue until they pass.
13. Implement roles/permissions.
14. Build minimal app shell.
15. Implement projects.
16. Implement recursive sections + cycle prevention.
17. Implement cards + soft delete/restore.
18. Verify Phase 1–4 acceptance criteria.
19. Only then begin Dynamic Properties.
```

# 37. Final Agent Instruction

> Build the product in vertical, tested slices. Never trade tenant
> security, workflow correctness, time accuracy, auditability, or user
> clarity for development speed.

> The system may be powerful internally. It must remain simple
> externally.

---

# 38. Localization / i18n Agent Rules

The product is multilingual from the first implementation.

Mandatory rules:

- Default UI language: `tr-TR`.
- Initial supported languages: Turkish and English.
- Never hard-code user-facing text in Svelte components.
- Use translation keys for navigation, buttons, dialogs, validation messages, empty states, errors, statuses, notifications, and email templates.
- Keep API/domain/event identifiers language-neutral and stable.
- Realtime events must not contain pre-translated sentences as the canonical event representation.
- Resolve language from user preference, then organization default, then `tr-TR`.
- Use locale-aware date/number/currency formatting.
- Store timestamps in UTC.
- Keep terminology overrides separate from translations.
- Do not rename source-code/domain concepts when a tenant changes terminology.
- New visible UI is incomplete until Turkish and English translation entries exist.
- Tests for critical screens should detect missing translation keys where practical.

Before marking a frontend feature complete, verify it in Turkish and ensure English rendering does not break layout.


# Repository Documentation Locations

The canonical documentation layout is:

```text
/
├── AGENTS.md
└── docs/
    ├── AI_CODING_AGENT_MASTER_PLAN.md
    ├── DATABASE_SCHEMA.md
    ├── API_CONTRACT.md
    └── UX_UI_SPEC.md
```

Always read the files from these paths before implementation.

---

# 39. Canonical Repository Layout

Use the following documentation locations:

```text
platform/
├── AGENTS.md
├── README.md
├── docs/
│   ├── AI_CODING_AGENT_MASTER_PLAN.md
│   ├── DATABASE_SCHEMA.md
│   └── API_CONTRACT.md
│   └── UX_UI_SPEC.md
├── apps/
│   ├── web/
│   ├── server/
│   └── worker/
├── packages/
│   ├── ui/
│   └── contracts/
├── migrations/
├── infrastructure/
├── docker/
├── compose.yml
└── .env.example
```

`AGENTS.md` must remain at repository root. The five canonical specification documents belong under `docs/`.

If future documentation is added, organize it under `docs/architecture`, `docs/product`, or `docs/decisions` as appropriate. Do not move the canonical documents without updating this file and all references.

---

# 40. README Maintenance Rule

`README.md` is the repository's living developer guide and must stay synchronized with the real implementation.

The coding agent must update `README.md` in the **same task/change set** whenever any of the following changes:

- repository structure,
- installation/setup procedure,
- prerequisites,
- frontend/backend/worker development commands,
- ports or local service addresses,
- Docker/Compose workflow,
- environment variables,
- database migration commands or workflow,
- test commands,
- lint/format/type-check commands,
- CI workflow relevant to developers,
- deployment workflow,
- important developer tooling or local development behavior.

Mandatory rules:

1. **Never add an unverified command to README.**
2. A command may be documented only after it has been run successfully in the relevant environment or is generated/defined by the repository and verified as valid.
3. Remove or correct obsolete commands immediately when implementation changes.
4. Keep `.env.example` and the README environment-variable documentation consistent.
5. Never put real secrets, passwords, tokens, API keys, private URLs, or production credentials in README.
6. If a new developer should know something to run the project successfully, document it.
7. If documentation and implementation disagree, fix the documentation in the same task unless the implementation itself is incorrect.
8. Do not turn README into the architecture specification. Detailed architecture belongs under `docs/`; README should link to the canonical documents.
9. Before reporting a setup/tooling/infrastructure task complete, follow the documented README instructions from a clean or equivalent environment where practical and verify they work.
10. If a command cannot be verified, clearly mark it as pending in development notes rather than presenting it as a working README instruction.

Before completing any task, explicitly ask:

```text
Did this change affect how a developer installs, configures, runs,
tests, migrates, builds, or deploys the project?

YES → update and verify README.md.
NO  → no README change required.
```

A task that changes developer workflow but leaves `README.md` stale is **not complete**.

---

# 41. Design System Compliance

`docs/DESIGN_SYSTEM.md` is the canonical visual-design contract for the product.

When implementing or modifying user-facing UI, the coding agent must read and follow both:

```text
docs/UX_UI_SPEC.md
docs/DESIGN_SYSTEM.md
```

Their responsibilities are different:

```text
UX_UI_SPEC.md      → behavior, flows, interaction and screen requirements
DESIGN_SYSTEM.md   → visual language, tokens, components and consistency
```

Mandatory rules:

1. Do not invent a separate visual language for individual pages.
2. Use semantic design tokens instead of scattering hard-coded colors, spacing, radii, shadows, or status colors through feature components.
3. Reuse shared UI primitives where appropriate.
4. Keep feature-specific composed components close to their feature; do not turn `packages/ui` into a dumping ground.
5. Every new user-facing screen must account for relevant loading, empty, error, disabled, permission and realtime states.
6. Every new visible UI must work in Turkish and English.
7. Validate text expansion; do not size controls only for one language.
8. Every major screen must be responsive for its intended device class.
9. Employee execution screens prioritize touch usability and simplicity.
10. Admin screens may use higher information density without becoming visually cluttered.
11. TV mode is a dedicated high-legibility interface and must not simply reuse the normal admin layout.
12. Use status color semantically and never as the sole carrier of meaning.
13. Maintain keyboard accessibility, visible focus and accessible labels.
14. Respect dark mode and reduced-motion behavior where applicable.
15. Do not add decorative gradients, excessive shadows, glass effects, oversized typography or animation unless the design system explicitly calls for them.
16. When a reusable visual pattern is introduced, prefer extending the shared design system rather than creating competing page-local variants.
17. If implementation requires intentionally deviating from `DESIGN_SYSTEM.md`, document the reason and update the design documentation when the new rule should become canonical.

Before reporting a UI task complete, verify:

```text
Does it follow UX_UI_SPEC.md?
Does it follow DESIGN_SYSTEM.md?
Does it reuse the correct shared components/tokens?
Does it work in Turkish and English?
Does it work at the intended responsive sizes?
Are loading/empty/error/permission states handled?
Is keyboard/focus behavior correct?
Does dark mode remain coherent?
```

A user-facing feature that is functionally complete but visually inconsistent with the design system is **not complete**.

---

# 42. Mandatory Incremental Verification

Errors must not accumulate until the end of a task. This section is
binding for every change.

Working rule:

```text
Implement → run the cheapest relevant verification → fix failures → continue
```

While a type check, compiler, lint, migration check, or relevant test is
failing, do not start the next feature unit. Do not batch verification to
the end of a large task.

## TypeScript / Svelte

1. After changing a logical unit, run the relevant typecheck before
   continuing to the next unit.
2. A symbol used at runtime (`instanceof`, `new`, static access, function
   or class values) must never be imported with `import type`. Type-only
   imports are for types only.
3. Never silence errors with `any`, `@ts-ignore`, `@ts-expect-error`,
   ESLint disable comments, or by weakening `tsconfig` strictness. Fix the
   cause instead.

## Rust

At the relevant stages of a change, run:

```text
cargo fmt --all --check
relevant cargo tests
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

A change is not ready to build upon while any of these fails.

## Database / migrations

A migration or query change is not done until the relevant PostgreSQL
integration tests pass against a real database. Do not move to the next
layer before they run green. Integration tests are never silently
skipped.

## Pipelines must not mask failures

When piping command output (for example into `tail` or `grep`), the
pipeline's exit status is the last command's status. Verify every gate by
its real exit code. Reporting success from piped output alone is a
verification failure, not a pass.

## Pre-push quality gate

Before pushing, the fast local gate must pass:

```text
npm run setup:hooks   # once per clone: activates .githooks/pre-push
```

The hook runs `npm run check:prepush`: frontend format check, lint,
typecheck, unit tests, Rust fmt, clippy, tests and OpenAPI contract
drift. Heavy Docker builds, E2E and Docker smoke stay in CI and are run
in the full local verification flow instead.
