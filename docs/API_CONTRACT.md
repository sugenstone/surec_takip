# API_CONTRACT.md

## Multi-Tenant Workflow & Operations Platform

> Companion to `AI_CODING_AGENT_MASTER_PLAN.md` and
> `DATABASE_SCHEMA.md`. This document defines API conventions and the
> first implementation contract between SvelteKit clients and the
> Rust/Axum backend.

# 1. API principles

-   REST for request/response business operations.
-   WebSocket for realtime server events.
-   OpenAPI is the canonical machine-readable HTTP contract.
-   API version prefix: `/api/v1`.
-   JSON request/response bodies unless uploading/downloading files.
-   Server is authoritative.
-   Every tenant-owned endpoint enforces authenticated membership and
    permission checks.
-   Never accept `tenant_id` as trusted authorization input.
-   Prefer command endpoints for meaningful state transitions instead of
    generic PATCH operations that bypass domain rules.
-   All timestamps are ISO-8601 UTC.
-   All IDs are opaque strings to clients.
-   Use idempotency keys for retry-sensitive commands.
-   Use optimistic concurrency/revision on collaborative editable
    entities.
-   Do not expose internal database errors.

# 2. Standard response conventions

Successful resource:

``` json
{
  "data": {
    "id": "..."
  }
}
```

Collection:

``` json
{
  "data": [],
  "meta": {
    "next_cursor": null
  }
}
```

Error:

``` json
{
  "error": {
    "code": "DEPENDENCY_BLOCKED",
    "message": "This task cannot start yet.",
    "details": {},
    "request_id": "..."
  }
}
```

Validation error:

``` json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Some fields are invalid.",
    "details": {
      "fields": {
        "title": ["Required"]
      }
    },
    "request_id": "..."
  }
}
```

# 3. HTTP status conventions

``` text
200 OK                  read/update/command success
201 Created             resource created
202 Accepted            asynchronous operation accepted
204 No Content          successful no-body operation
400 Bad Request         malformed request
401 Unauthorized        not authenticated
403 Forbidden           authenticated but not permitted
404 Not Found           absent OR intentionally hidden cross-tenant resource
409 Conflict            revision/state/idempotency conflict
422 Unprocessable       domain/validation rule failure
429 Too Many Requests   rate limited
500 Internal Error      unexpected server failure
```

Do not reveal whether a foreign-tenant UUID exists. Return the same safe
not-found behavior.

# 4. Authentication

## Foundation operational probes

Before authentication/domain endpoints are implemented, the server exposes only:

```text
GET /api/v1/health   200 while the HTTP process is live
GET /api/v1/ready    200 when PostgreSQL SELECT 1 succeeds; otherwise 503
```

These probes intentionally require no tenant context or permission and return no
tenant data, credentials, version details or database errors. Success uses
`{"data":{"status":"ok"}}` / `{"data":{"status":"ready"}}`.
Readiness checks connectivity, not schema version; deployment must run migration
apply/verify separately before admitting application traffic.

Every response carries a server-generated `x-request-id` and `Cache-Control:
no-store`. Structured errors include the same ID. Unknown routes return
`RESOURCE_NOT_FOUND` (404), unsupported methods `METHOD_NOT_ALLOWED` (405),
database readiness failure `SERVICE_NOT_READY` (503). Probe callers can retry
after recovery; these read-only calls require no idempotency key.

The generated foundation contract is `packages/contracts/openapi.json` and its
TypeScript schema is `packages/contracts/src/schema.d.ts`; drift is checked
against Rust definitions. Future endpoints below remain unimplemented until
their phase and must never return fabricated success.

Sessions use a secure HTTP-only cookie (`platform_session`).

``` text
POST /api/v1/auth/login
POST /api/v1/auth/logout
GET  /api/v1/auth/me
POST /api/v1/auth/password/forgot
POST /api/v1/auth/password/reset
POST /api/v1/auth/email/verify
```

Implementation status (users/sessions foundation): `login`, `logout` and
`me` are implemented with Argon2id password hashing; the opaque session
token is stored server-side only as a SHA-256 digest. Login, logout and
`me` are the only implemented endpoints above; `password/forgot`,
`password/reset` and `email/verify` wait for the email infrastructure
phase and their token models will be contracted before implementation.
All credential failures (unknown email, wrong password, disabled
account) return `401 AUTH_INVALID_CREDENTIALS` with an identical body so
account existence is not revealed.

Example `GET /auth/me`:

``` json
{
  "data": {
    "user": {
      "id": "...",
      "display_name": "..."
    },
    "organizations": [
      {
        "id": "...",
        "name": "...",
        "role_summary": ["..."]
      }
    ]
  }
}
```

# 5. Tenant/workspace context

Preferred URL context:

``` text
/api/v1/organizations/{organization_id}/...
/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/...
```

The URL selects context; authentication/membership authorizes it.

Endpoints:

``` text
GET  /organizations
POST /organizations
GET  /organizations/{organization_id}
PATCH /organizations/{organization_id}

GET  /organizations/{organization_id}/workspaces
POST /organizations/{organization_id}/workspaces
GET  /organizations/{organization_id}/workspaces/{workspace_id}
PATCH /organizations/{organization_id}/workspaces/{workspace_id}
```

Implementation status (workspaces foundation): workspace `POST` (create),
`GET` (list) and single `GET` are implemented under the organization path.
Access model: a workspace is visible/accessible only when the caller has
BOTH a valid organization membership for the parent organization AND a valid
workspace membership, joined to non-deleted workspace/organization rows —
organization membership alone does NOT grant workspace access. Creation is
restricted to organization members and is atomic: workspace + creator
workspace membership commit in one transaction (201 returns workspace +
membership). Workspace slugs are unique per organization (case-insensitive
citext); a taken slug inside that organization returns 422
`VALIDATION_ERROR` with `details.fields.slug = ["Already taken"]`, while the
same slug may exist in other organizations. Parent-child paths are
authoritative: `GET .../workspaces/{id}` requires the workspace to belong to
the organization in the path; mismatched combinations — even for a user who
is a member of both organizations — foreign organizations, unknown or
malformed ids, deleted memberships, deleted workspaces and deleted
organizations all return the same 404 `RESOURCE_NOT_FOUND`. `PATCH` and
member management remain unimplemented until their phases. `/auth/me` is
unchanged: workspaces are fetched per organization via the list endpoint.

Implementation status (organizations foundation): `POST /organizations`,
`GET /organizations` and `GET /organizations/{organization_id}` are
implemented. Creation is authenticated and atomic: the organization and the
creator membership commit in one transaction (201 returns organization +
membership). `name` (1–200 chars) is required; optional `slug` is normalized
to `[a-z0-9-]` with Turkish transliteration — an unusable slug is a 422 field
error, a taken slug (case-insensitive citext UNIQUE) returns 422
`VALIDATION_ERROR` with `details.fields.slug = ["Already taken"]`. Slugs are
public identifiers, never authorization boundaries; authorization uses the
immutable organization id. List and single-get return only organizations
where the caller has an active, non-deleted membership joined to a
non-deleted organization. Non-member, unknown id, malformed id, deleted
membership and deleted organization all return the same 404
`RESOURCE_NOT_FOUND` (no existence leak). `PATCH`, workspaces and member
management remain unimplemented until their phases. `GET /auth/me`
`organizations` now carries the caller's visible organizations
(id/name/slug, empty `role_summary` until RBAC).

# 6. Invitations and memberships

``` text
GET    /organizations/{org}/members
GET    /organizations/{org}/members
POST   /organizations/{org}/invitations
DELETE /organizations/{org}/invitations/{invitation_id}
POST   /invitations/{token}/accept

Implementation status (invitations foundation): all four endpoints are
implemented with the accept route moved to `POST /api/v1/invitations/accept`
with the token in the JSON body (never in a URL — request logs must not
capture invitation secrets). Creation requires the `members:invite`
permission at organization scope (Owner only by default); responses follow
401/404/403 ordering. The raw invitation token is returned exactly ONCE in
the creation response (email delivery is a later phase) and never appears in
list responses or logs; only its SHA-256 digest is stored. Acceptance is an
identity-bound atomic transaction: authenticated user + citext-equal account
email + valid pending token. Every acceptance failure returns the same
generic `422 INVITATION_INVALID` — expired, revoked, consumed, tampered and
wrong-identity cases are indistinguishable. Accepted invitations grant
exactly the tenant's built-in `member` role (no client-chosen roles, no
Owner invitations, no implicit workspace membership). Re-inviting a pending
recipient revokes the old token and issues a new one (one live token per
tenant+email). Acceptance never resurrects old privileges; an already-active
member keeps existing grants untouched.

PATCH  /organizations/{org}/members/{user_id}
DELETE /organizations/{org}/members/{user_id}
```

Never return invitation secrets after creation.

# 7. Teams

``` text
GET    /organizations/{org}/workspaces/{ws}/teams
POST   /organizations/{org}/workspaces/{ws}/teams
GET    /organizations/{org}/workspaces/{ws}/teams/{team_id}
PATCH  /organizations/{org}/workspaces/{ws}/teams/{team_id}
DELETE /organizations/{org}/workspaces/{ws}/teams/{team_id}

POST   /.../teams/{team_id}/members
DELETE /.../teams/{team_id}/members/{user_id}
```

# 8. Roles and permissions

``` text
GET  /organizations/{org}/permissions
GET  /organizations/{org}/roles
POST /organizations/{org}/roles
GET  /organizations/{org}/roles/{role_id}
PATCH /organizations/{org}/roles/{role_id}
DELETE /organizations/{org}/roles/{role_id}

PUT /organizations/{org}/members/{user_id}/roles
```

Role mutation payloads explicitly contain permission keys/scopes.

Implementation status (RBAC foundation): `GET /organizations/{org}/permissions`
and `GET /organizations/{org}/roles` are implemented as eligibility (any
active member) read-only catalog endpoints. Role create/update/delete and
role assignment endpoints remain unimplemented until their phase; assignment
paths are exercised through service-level tests. `POST .../workspaces` now
requires the `workspaces:create` permission at organization scope: an
eligible member without the permission receives `403 PERMISSION_DENIED`
(stable code, generic message, no role internals); non-members keep the
uniform 404. Eligibility reads (organization/workspace lists and gets) stay
membership-based — RBAC never weakens tenant isolation. Authorization is
permission-key based; role names/labels are never authorization inputs and
built-in role names are stable identifiers, not translations.

# 9. Projects

Implemented V1 surface (ADR 0011):

``` text
GET    /organizations/{org}/workspaces/{ws}/projects
POST   /organizations/{org}/workspaces/{ws}/projects
GET    /organizations/{org}/workspaces/{ws}/projects/{project_id}
PATCH  /organizations/{org}/workspaces/{ws}/projects/{project_id}
```

- Reads are eligibility-based (active org + workspace membership); V1 has no
  `projects:read`.
- Mutations: `projects:create` (POST), `projects:update` (PATCH), and
  `projects:archive` additionally when a PATCH sets `status: "archived"`.
- Status lifecycle: `active | completed | archived`; allowed transitions
  active→completed, active→archived, completed→active, completed→archived,
  archived→active (+ identity). `archived → completed` is rejected.
- Slug conflicts map to `VALIDATION_ERROR` with `details.fields.slug`.
- `DELETE` and `/restore` are deferred until product semantics are defined;
  `deleted_at` is schema infrastructure only.
- Every ProjectPublic (list and detail) embeds `progress` — the derived
  aggregate `{completed, active, total, percent|null}` over ALL counted
  process definitions in the project (ADR 0017).

Planned later (documented shape, not yet implemented):

``` text
DELETE /organizations/{org}/workspaces/{ws}/projects/{project_id}
POST   /organizations/{org}/workspaces/{ws}/projects/{project_id}/restore
```

List filters may include (later phases):

``` text
status
priority
due_before
due_after
search
cursor
limit
sort
```

Collaborative mutations include `expected_revision`; resource responses expose
`revision`. Use this body convention consistently, including move commands.

# 10. Sections

Implemented V1 surface (ADR 0012):

``` text
GET    /.../projects/{project_id}/sections
POST   /.../projects/{project_id}/sections
GET    /.../projects/{project_id}/sections/{section_id}
PATCH  /.../projects/{project_id}/sections/{section_id}
```

- Reads are eligibility-based (V1 has no `sections:read`).
- Mutations: `sections:create` (POST), `sections:update` (PATCH — including
  reparent/reorder), and `sections:archive` additionally when a PATCH sets
  `status: "archived"`.
- Tree response is a FLAT ordered list (one query; roots first, siblings by
  `(position, id)`); clients assemble the tree.
- PATCH `parent_section_id` is three-state: absent = keep, `null` = move to
  root, UUID = reparent inside the same project. Cycle attempts (self or
  descendant parent) map to `VALIDATION_ERROR` with
  `details.fields.parent_section_id`.
- Status lifecycle: `active | archived` both directions; sections never
  "complete".
- `DELETE`/`restore`, dedicated `move`/`duplicate` commands and
  `bulk-create` are planned later phases (move/reorder are expressible via
  PATCH today; `expected_revision` arrives with collaborative editing).
- Every SectionPublic embeds `progress` — the leaf-weighted aggregate over
  the section's ENTIRE recursive subtree, not a child-percentage average
  (ADR 0017).

Planned later (documented shape, not yet implemented):

``` text
DELETE /.../sections/{section_id}
POST   /.../sections/{section_id}/restore
POST   /.../sections/{section_id}/duplicate
POST   /.../projects/{project_id}/sections/bulk-create
```

# 10.1 Work Items (implemented — STEP 19)

Base: `/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items`.

| Method | Suffix | Input | Success |
| --- | --- | --- | --- |
| GET | collection | none | 200 `{data: WorkItemPublic[]}` |
| POST | collection | name, optional slug | 201 `{data: WorkItemPublic}` |
| GET | `/{work_item_id}` | none | 200 bare WorkItemPublic |
| PATCH | `/{work_item_id}` | optional name, slug, position, status | 200 `{data: WorkItemPublic}` |

Public fields: id, organization_id, workspace_id, project_id, section_id,
name, slug, position, status, progress. Scope and initial active status are
server-derived; unknown DTO fields (including identity/move fields) are
rejected.

Reads require both memberships and the exact parent route chain. Archived or
deleted work items and archived/deleted project/direct section are invisible
(404). List order `(position,id)`; create appends; position is a non-negative
integer. Slug namespace is section-local and retained after archive/deletion.

Create requires `work_items:create`; all PATCH requires `work_items:update`;
setting archived additionally requires `work_items:archive`. Active/completed
are manual labels; archive is terminal for this V1 API, retained in storage.
No DELETE/restore/move/Process endpoint exists here. Existing Projects/Sections
archive behavior is unchanged.

Error ordering: 401 → 404 → 403 → 422 field/domain validation. Structurally
invalid JSON/type/unknown-field requests follow existing 400 VALIDATION_ERROR
convention after eligibility and base permission. Mutation revalidation is
inside a transaction with parent/membership/grant locks. Field keys are name,
slug, position, status. No raw DB errors. No revision or pagination protocol
is introduced in this slice; see [ADR 0013](decisions/0013-work-items-domain-foundation.md).

# 10.2 Processes (implemented — STEP 20)

Base: `/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}/processes`.

| Method | Suffix | Input | Success |
| --- | --- | --- | --- |
| GET | collection | none | 200 `{data: ProcessPublic[]}` |
| POST | collection | name, optional slug, description, is_required (default true) | 201 `{data: ProcessPublic}` |
| PATCH | `/reorder` | `process_ids: uuid[]` (every active process exactly once) | 200 `{data: ProcessPublic[]}` |
| GET | `/{process_id}` | none | 200 bare ProcessPublic |
| PATCH | `/{process_id}` | optional name, slug, description (empty clears), is_required, status | 200 `{data: ProcessPublic}` |
| PUT | `/{process_id}/assignment` | `{"user_id": uuid \| null}` | 200 `{data: ProcessPublic}` |

Public fields: id, organization_id, workspace_id, project_id, section_id,
work_item_id, name, slug, description, position, is_required, status,
`assignee` (`{id, display_name, eligible}` or `null`).

Assignment (STEP 21C, ADR 0018): `assignee` is the current responsible user —
responsibility metadata, NOT authorization. `PUT .../assignment` requires
`processes:assign` (Owner-only system grant; Member never receives it).
`user_id: null` unassigns; a uuid must satisfy eligibility at write time
(active user + active organization + workspace membership), otherwise a
generic `422 VALIDATION_ERROR` on field `user_id` — foreign or unknown ids
are indistinguishable. A stale assignee (membership later revoked) remains
readable with `eligible: false`; the row is never auto-cleared. There is no
self-claim path in V1.

Reads require both memberships and the exact parent route chain through an
active section, non-archived project and visible work item; there is no
global `/processes/{id}` route. Archived/deleted processes are invisible (404,
same fingerprint as unknown/foreign/wrong-parent). List order `(position,id)`;
create appends. `position` is not writable through PATCH — ordering changes
only through `/reorder`, which rejects missing, extra, duplicate, unknown,
foreign or archived ids with one `process_ids` 422. Status accepts only the
configuration values `active|archived`; execution states (`running`,
`completed`, …) are 422. Slug namespace is work-item-local and retained after
archive/deletion.

Create requires `processes:create`; all per-process PATCH requires
`processes:update`; setting archived additionally requires
`processes:archive`; reorder requires only `processes:reorder`. Archive is
terminal in this V1 API (no DELETE/restore). Unknown DTO fields, including
all scope ids, `position` and execution fields, are rejected with 400.

Error ordering: 401 → 404 → 403 → 422. Mutations revalidate inside a
transaction (project → section/memberships → work item → grants → process
rows). No revision or pagination protocol; see
[ADR 0015](decisions/0015-process-domain-foundation.md).

## 10.2a Workspace member directory (implemented — STEP 21C)

`GET /api/v1/organizations/{organization_id}/workspaces/{workspace_id}/members`
→ `200 {data: WorkspaceMemberPublic[]}` where `WorkspaceMemberPublic` is
`{id, display_name}` — eligible assignable users only (active user + active
org + workspace membership), ordered by display name. Access requires
ordinary workspace access (both memberships); `processes:assign` is NOT
required for reads. No email, roles, or membership internals are exposed.

# 10.3 Process executions (implemented — STEP 21A)

Runtime attempt records — separate from the definitions above (ADR 0016).
Base for transitions:
`.../work-items/{work_item_id}/processes/{process_id}/executions`.

| Method | Path | Input | Success |
| --- | --- | --- | --- |
| GET | `.../work-items/{work_item_id}/executions` | none | 200 `{data: ExecutionPublic[], server_time}` |
| POST | `.../processes/{process_id}/executions` | optional `start_reason` | 201 `{data: ExecutionPublic, server_time}` |
| POST | `.../processes/{process_id}/executions/{execution_id}/complete` | empty `{}` | 200 `{data: ExecutionPublic, server_time}` |
| POST | `.../processes/{process_id}/executions/{execution_id}/cancel` | optional `cancel_reason` | 200 `{data: ExecutionPublic, server_time}` |

Public fields: id, tenant/scoped ids, `attempt_no`, `status`
(`active|completed|cancelled`), `started_at`, `completed_at`,
`cancelled_at`, `start_reason`, `cancel_reason`, the actor ids
`started_by_user_id`/`completed_by_user_id`/`cancelled_by_user_id`, and
`assignee_user_id` — the immutable responsibility snapshot written at START
(ADR 0018): who the process was assigned to when this attempt began. It is
never client-writable and never rewritten by later reassignment; it is
distinct from actor ids (a supervisor may start work assigned to someone
else).
`server_time` is the DB clock at response time so clients can anchor the
display-only timer without trusting their own clock.

A process has 0..N immutable attempts; at most one is `active`. START
inserts the next attempt (`attempt_no = MAX+1`) — retry is a new attempt,
never a resurrection. COMPLETE/CANCEL apply only to the `active` attempt
and make it terminal-immutable; repeating a terminal transition or
starting a second concurrent attempt is `409 STATE_CONFLICT`. Reasons are
trimmed, empty→NULL, ≤500 chars (`422` otherwise). Clients may not supply
scope ids, actor ids, timestamps, `attempt_no` or `status` — unknown
fields are 400.

Permissions are decomposed: `process_executions:start`,
`process_executions:complete`, `process_executions:cancel`; the read list
follows the existing work-item read eligibility (both memberships). Owner
and built-in Member both carry the three organization-scoped execution
grants — Member gains no administrative permissions, and workspace
membership is still required. Every transition revalidates the full
parent chain, both memberships and the permission inside the write
transaction. Error ordering: 401 → 404 → 403 → 400/422 → 409. Archiving
any ancestor (process, work item, section subtree, project) that still
contains an active execution fails `409`. See
[ADR 0016](decisions/0016-process-execution.md).

# 10.4 Derived progress (implemented — STEP 21B)

`ProjectPublic`, `SectionPublic` and `WorkItemPublic` — in every list and
detail response above — embed one shared `progress` object:

``` text
progress: {
  completed: integer,   -- counted definitions satisfying DONE
  active:    integer,   -- counted definitions with an active execution
  total:     integer,   -- all counted definitions in the scope
  percent:   integer|null  -- round-half-up int; NULL exactly when total = 0
}
```

A "counted" definition is `deleted_at IS NULL AND status = 'active'`
under reachable parents; DONE means "a completed attempt exists AND no
active attempt exists" — an active retry regresses a process to not-done.
Work Item aggregates its own definitions; Section and Project aggregates
are leaf-weighted over the whole subtree (never child-percentage
averages), so re-organizing sections cannot change a project's number.
Progress is computed per response in one SQL statement — never stored,
never client-calculated. See
[ADR 0017](decisions/0017-progress-engine.md).

# 11. Cards

``` text
GET    /.../projects/{project_id}/cards
POST   /.../projects/{project_id}/cards
GET    /.../cards/{card_id}
PATCH  /.../cards/{card_id}
DELETE /.../cards/{card_id}
POST   /.../cards/{card_id}/restore
POST   /.../cards/{card_id}/move
POST   /.../cards/{card_id}/duplicate
```

Card detail should be capable of returning/embedding selected related
data efficiently, but avoid a giant unbounded response. Prefer explicit
query includes:

``` text
?include=properties,process_summary,assignees
```

# 12. Dynamic properties

``` text
GET    /.../property-definitions
POST   /.../property-definitions
PATCH  /.../property-definitions/{property_id}
DELETE /.../property-definitions/{property_id}

GET /.../cards/{card_id}/properties
PUT /.../cards/{card_id}/properties/{property_id}
```

Server validates value against definition type/settings.

# 13. Processes

> Conceptual only; the implemented work-item-owned definition API is in
> section 10.2 (STEP 20). Restore, DELETE and steps are deferred.

``` text
GET    /.../cards/{card_id}/processes
POST   /.../cards/{card_id}/processes
GET    /.../processes/{process_id}
PATCH  /.../processes/{process_id}
DELETE /.../processes/{process_id}
POST   /.../processes/{process_id}/restore
POST   /.../processes/{process_id}/reorder
```

# 14. Process steps

``` text
GET    /.../processes/{process_id}/steps
POST   /.../processes/{process_id}/steps
GET    /.../process-steps/{step_id}
PATCH  /.../process-steps/{step_id}
DELETE /.../process-steps/{step_id}
POST   /.../process-steps/{step_id}/restore
POST   /.../processes/{process_id}/steps/reorder
```

Normal PATCH must not be able to arbitrarily set runtime state to
`completed`. Use command endpoints below.

# 15. Assignments

``` text
GET  /.../process-steps/{step_id}/assignments
POST /.../process-steps/{step_id}/assignments
POST /.../process-steps/{step_id}/reassign
```

Example:

``` json
{
  "assignee_type": "user",
  "assignee_id": "...",
  "reason": null
}
```

Assignment changes publish realtime events and may create notifications.

# 16. Dependencies

``` text
GET    /.../processes/{process_id}/dependencies
POST   /.../processes/{process_id}/dependencies
DELETE /.../process-dependencies/{dependency_id}

GET    /.../process-steps/{step_id}/dependencies
POST   /.../process-steps/{step_id}/dependencies
DELETE /.../process-step-dependencies/{dependency_id}

POST /.../process-steps/{step_id}/dependency-override
```

Cycle validation occurs before commit.

Blocked response example:

``` json
{
  "error": {
    "code": "DEPENDENCY_BLOCKED",
    "message": "This task cannot start yet.",
    "details": {
      "waiting_for": [
        {
          "id": "...",
          "name": "Previous Step",
          "required_state": "completed"
        }
      ]
    }
  }
}
```

# 17. Task runtime commands

These are the most important employee endpoints.

``` text
POST /.../process-steps/{step_id}/start
POST /.../process-steps/{step_id}/pause
POST /.../process-steps/{step_id}/resume
POST /.../process-steps/{step_id}/complete
POST /.../process-steps/{step_id}/reopen
```

Use `Idempotency-Key` for these commands.

### Start

Request:

``` json
{
  "expected_revision": 4,
  "switch_from_active_task": false
}
```

If employee already has an active task:

``` json
{
  "error": {
    "code": "ACTIVE_TASK_EXISTS",
    "message": "You already have an active task.",
    "details": {
      "active_step_id": "...",
      "can_switch": true
    }
  }
}
```

If user confirms switching:

``` json
{
  "expected_revision": 4,
  "switch_from_active_task": true
}
```

Server transaction: 1. pause current session, 2. start new session, 3.
update states, 4. write audit records, 5. write outbox events, 6.
commit.

### Pause

``` json
{
  "stop_reason_id": "...",
  "note": null,
  "expected_revision": 5
}
```

### Resume

``` json
{
  "expected_revision": 6
}
```

### Complete

``` json
{
  "expected_revision": 7,
  "completion_data": {}
}
```

Server validates checklist/forms/required fields/approval rules.

### Reopen

Requires permission and optional/required reason depending on policy.

``` json
{
  "reason": "Completed by mistake",
  "expected_revision": 8
}
```

# 18. My Tasks

Employee-focused endpoints should be optimized for simple UI.

``` text
GET /organizations/{org}/workspaces/{ws}/me/tasks
GET /organizations/{org}/workspaces/{ws}/me/active-task
GET /organizations/{org}/workspaces/{ws}/me/completed-today
```

Example task:

``` json
{
  "id": "...",
  "status": "ready",
  "title": "Process Step Name",
  "context": {
    "project": {"id": "...", "name": "..."},
    "section_path": ["...", "..."],
    "card": {"id": "...", "code": "CARD-01842", "title": "..."},
    "process": {"id": "...", "name": "..."}
  },
  "priority": "normal",
  "due_at": null,
  "blocked_reason": null,
  "timer": {
    "active": false,
    "accumulated_seconds": 0,
    "started_at": null
  }
}
```

# 19. Live operations / TV

``` text
GET /organizations/{org}/workspaces/{ws}/live-operations
```

Returns current active/optionally paused work.

TV does not poll every second. It receives authoritative start state and
calculates timer locally.

TV-specific display configuration:

``` text
GET /organizations/{org}/workspaces/{ws}/tv-settings
PATCH /organizations/{org}/workspaces/{ws}/tv-settings
```

# 20. WebSocket contract

Suggested endpoint:

``` text
GET /api/v1/realtime
```

Authentication uses the existing secure session or short-lived socket
token.

Client subscribes only to contexts it is authorized to view.

Server envelope:

``` json
{
  "event_id": "...",
  "event_type": "process_step.started",
  "occurred_at": "2026-09-21T14:00:00Z",
  "organization_id": "...",
  "workspace_id": "...",
  "entity": {
    "type": "process_step",
    "id": "..."
  },
  "data": {}
}
```

Important events:

``` text
project.created
project.updated
section.created
section.updated
card.created
card.updated
card.deleted
assignment.changed
process_step.ready
process_step.started
process_step.paused
process_step.resumed
process_step.completed
process_step.reopened
dependency.unlocked
notification.created
permission.changed
```

Realtime event is a hint/state update, not an authorization bypass.
Client may refetch authoritative data.

# 21. Timer payload contract

For active step:

``` json
{
  "status": "active",
  "accumulated_seconds": 2314,
  "started_at": "2026-09-21T14:25:10Z",
  "server_now": "2026-09-21T14:31:00Z"
}
```

Client computes:

``` text
accumulated_seconds + elapsed_since(started_at)
```

When paused/completed:

``` json
{
  "status": "paused",
  "accumulated_seconds": 2664,
  "started_at": null
}
```

Periodically resync using server time/state to limit drift.

# 22. Templates

``` text
GET    /.../card-templates
POST   /.../card-templates
GET    /.../card-templates/{id}
PATCH  /.../card-templates/{id}
POST   /.../card-templates/{id}/publish

GET    /.../process-templates
POST   /.../process-templates
GET    /.../process-templates/{id}
PATCH  /.../process-templates/{id}
POST   /.../process-templates/{id}/publish

POST /.../cards/{card_id}/apply-card-template
POST /.../cards/{card_id}/apply-process-template
```

Applying a template records the exact version used.

# 23. Checklists/forms/approvals

``` text
GET  /.../process-steps/{step_id}/checklist
PUT  /.../checklist-items/{item_id}/completion

GET  /.../process-steps/{step_id}/forms
POST /.../forms/{form_id}/submissions

POST /.../process-steps/{step_id}/submit-for-approval
POST /.../approvals/{approval_id}/approve
POST /.../approvals/{approval_id}/return
```

Approval decision requires authorization.

# 24. Audit and activity

``` text
GET /.../cards/{card_id}/activity
GET /.../process-steps/{step_id}/activity
GET /organizations/{org}/audit
```

Audit access requires explicit permission.

Audit results are paginated and filterable.

# 25. Trash / restore

``` text
GET  /organizations/{org}/workspaces/{ws}/trash
POST /.../trash/{entity_type}/{entity_id}/restore
```

Permanent deletion, if exposed at all, is a separate high-privilege
action with retention rules.

# 26. Notifications

``` text
GET   /organizations/{org}/notifications
POST  /organizations/{org}/notifications/{id}/read
POST  /organizations/{org}/notifications/read-all

GET   /organizations/{org}/notification-preferences
PUT   /organizations/{org}/notification-preferences
```

Notification payloads include a safe deep-link descriptor rather than
requiring frontend string parsing.

# 27. Files

Upload flow should support direct-to-object-storage presigned upload
when appropriate.

``` text
POST /.../files/upload-intent
POST /.../files/{file_id}/complete-upload
GET  /.../files/{file_id}
GET  /.../files/{file_id}/download
DELETE /.../files/{file_id}
```

The backend validates: - membership, - permission, - tenant ownership, -
file size/type policy, - attachment target.

# 28. Comments and mentions

``` text
GET    /.../cards/{card_id}/comments
POST   /.../cards/{card_id}/comments
PATCH  /.../comments/{comment_id}
DELETE /.../comments/{comment_id}
```

Mentions are parsed/validated server-side from explicit user IDs, not
only display-name text.

# 29. Search

``` text
GET /organizations/{org}/workspaces/{ws}/search?q=...
```

Return grouped results:

``` json
{
  "data": {
    "projects": [],
    "sections": [],
    "cards": [],
    "users": []
  }
}
```

Only authorized resources may appear.

# 30. Saved views

``` text
GET    /.../saved-views
POST   /.../saved-views
PATCH  /.../saved-views/{id}
DELETE /.../saved-views/{id}
```

# 31. Bulk operations

Use preview for consequential bulk actions.

``` text
POST /.../cards/bulk/preview
POST /.../cards/bulk/execute
```

Preview response:

``` json
{
  "data": {
    "matched": 148,
    "allowed": 143,
    "blocked": 5,
    "warnings": []
  }
}
```

Execution requires an operation token/idempotency key from preview where
appropriate.

# 32. Import/export

``` text
POST /.../imports
POST /.../imports/{id}/mapping
POST /.../imports/{id}/preview
POST /.../imports/{id}/commit
GET  /.../imports/{id}

POST /.../exports
GET  /.../exports/{id}
```

Heavy jobs return `202 Accepted`.

# 33. Reporting

``` text
GET /.../reports/operations
GET /.../reports/time
GET /.../reports/bottlenecks
```

Filters:

``` text
date range
project
section
team
employee
process
step
status
```

Reports return operational facts; do not embed opaque employee
performance scoring.

# 34. Billing and subscription

Organization owner/billing-admin endpoints:

``` text
GET  /organizations/{org}/billing/subscription
GET  /organizations/{org}/billing/entitlements
GET  /organizations/{org}/billing/usage
GET  /organizations/{org}/billing/invoices

POST /organizations/{org}/billing/checkout
POST /organizations/{org}/billing/change-plan
POST /organizations/{org}/billing/cancel
```

Provider webhook is outside tenant-authenticated namespace:

``` text
POST /api/v1/billing/webhooks/{provider}
```

Requirements: - verify signature, - idempotently process provider event
ID, - persist billing event, - update subscription, - recompute
entitlements, - emit internal event.

Feature checks use entitlements, not plan-name string comparisons.

# 35. Automations

``` text
GET    /.../automations
POST   /.../automations
PATCH  /.../automations/{id}
DELETE /.../automations/{id}
POST   /.../automations/{id}/test
GET    /.../automations/{id}/runs
```

Automation actions execute asynchronously and idempotently.

# 36. Public API keys and webhooks

``` text
GET    /organizations/{org}/api-keys
POST   /organizations/{org}/api-keys
DELETE /organizations/{org}/api-keys/{id}

GET    /organizations/{org}/webhooks
POST   /organizations/{org}/webhooks
PATCH  /organizations/{org}/webhooks/{id}
DELETE /organizations/{org}/webhooks/{id}
GET    /organizations/{org}/webhooks/{id}/deliveries
```

Raw API key is shown once at creation.

Webhook deliveries are signed.

# 37. Pagination

Prefer cursor pagination for large mutable collections.

Example:

``` text
?limit=50&cursor=opaque_cursor
```

Response:

``` json
{
  "data": [],
  "meta": {
    "next_cursor": "..."
  }
}
```

Do not expose database offsets as a long-term pagination contract for
high-volume feeds.

# 38. Filtering and sorting

Use predictable query parameters for simple filters.

For complex saved-view filtering, define a typed filter JSON schema
shared by frontend/backend contract generation.

Never concatenate client filter strings into SQL.

# 39. Optimistic concurrency

Editable resource response:

``` json
{
  "id": "...",
  "revision": 12
}
```

Mutation:

``` json
{
  "title": "New title",
  "expected_revision": 12
}
```

Conflict:

``` json
{
  "error": {
    "code": "REVISION_CONFLICT",
    "message": "This item changed after you opened it.",
    "details": {
      "current_revision": 13
    }
  }
}
```

Frontend offers refresh/reconcile rather than silently overwriting.

# 40. Idempotency

Commands at risk of duplicate side effects accept:

``` text
Idempotency-Key: <opaque unique value>
```

Examples: - task start/pause/resume/complete, - invitation, - bulk
execute, - import commit, - checkout creation.

Same key + same actor/tenant/route returns the original outcome or safe
equivalent.

# 41. Permission behavior

Every endpoint maps to explicit permissions.

Examples:

``` text
GET projects            (eligibility: active memberships; V1 has no read key)
POST projects           projects:create
PATCH project           projects:update
PATCH project -> archived   projects:update + projects:archive
DELETE project          projects:delete (deferred; not yet implemented)

POST step/start         tasks:start
POST step/pause         tasks:pause
POST step/complete      tasks:complete
POST step/reopen        tasks:reopen

GET audit               audit:view
POST invitation         users:invite
POST billing/change     billing:manage
```

Scope evaluation occurs after permission lookup.

# 42. Domain error codes

Maintain stable machine-readable codes.

Initial catalog:

``` text
AUTH_REQUIRED
AUTH_INVALID_CREDENTIALS
EMAIL_NOT_VERIFIED
MEMBERSHIP_REQUIRED
PERMISSION_DENIED
RESOURCE_NOT_FOUND
VALIDATION_ERROR
REVISION_CONFLICT
DEPENDENCY_BLOCKED
DEPENDENCY_CYCLE
ACTIVE_TASK_EXISTS
TASK_NOT_ASSIGNED
INVALID_STATE_TRANSITION
COMPLETION_REQUIREMENTS_NOT_MET
APPROVAL_REQUIRED
IDEMPOTENCY_CONFLICT
FEATURE_NOT_AVAILABLE
USAGE_LIMIT_REACHED
RATE_LIMITED
UPLOAD_REJECTED
INTERNAL_ERROR
```

Frontend behavior should key off `code`, never parse human message
strings.

# 43. OpenAPI and generated contracts

Rust backend owns the HTTP contract.

Generate/maintain OpenAPI.

Frontend should consume generated TypeScript API types/client where
practical.

CI must detect accidental contract drift.

Do not manually duplicate dozens of request/response interfaces in two
languages if generation can safely prevent drift.

# 44. Realtime + REST recovery rule

WebSocket is not the sole source of truth.

On: - reconnect, - sequence gap, - unknown event, - stale revision,

the client refetches affected REST resources.

This makes realtime resilient instead of fragile.

# 45. First API implementation order

The coding agent must implement HTTP contracts in this sequence:

``` text
01 auth/me/login/logout
02 organizations
03 workspaces
04 memberships/invitations
05 roles/permissions
06 projects
07 sections
08 cards
09 properties
10 processes
11 process steps
12 assignments
13 dependencies
14 task runtime commands
15 My Tasks
16 realtime WebSocket
17 live operations/TV
18 templates
19 audit/restore
20 notifications
21 files
22 search
23 billing entitlement foundation
```

Do not expose unfinished future endpoints as fake stubs unless
explicitly needed for contract-first development.

# 46. Critical end-to-end contract

The following must work before V1 is accepted:

``` text
Admin:
POST project
POST section
POST card
POST/apply process template
POST assignment

Employee:
GET /me/tasks
POST step/start

Server:
creates time session
writes audit
writes outbox event
pushes WebSocket event

Admin/TV:
receives active task
renders live timer locally

Employee:
POST pause
POST resume
POST complete

Server:
closes sessions accurately
validates completion
unlocks dependent step
creates notification
pushes realtime events

Admin:
can inspect activity/audit

Authorized user:
can reopen an accidental completion

All clients:
converge without manual refresh
```

------------------------------------------------------------------------

## Final API rule

> Use REST to express authoritative business commands and queries; use
> WebSocket to propagate changes. Never let generic CRUD endpoints
> bypass workflow, permission, dependency, timing, audit, or tenant
> rules.

---

# 47. Localization / Locale Contract

User-facing clients resolve language from:

```text
User preference
→ Organization default
→ tr-TR
```

Initial supported locales:

```text
tr-TR
en
```

Relevant preference endpoints may include:

```text
GET   /api/v1/me/preferences
PATCH /api/v1/me/preferences

GET   /api/v1/organizations/{org}/settings
PATCH /api/v1/organizations/{org}/settings
```

Example user preference:

```json
{
  "locale": "tr-TR",
  "timezone": "Europe/Istanbul"
}
```

Example organization settings:

```json
{
  "default_locale": "tr-TR",
  "default_timezone": "Europe/Istanbul",
  "default_currency": "TRY",
  "terminology": {
    "card": "İş Emri"
  }
}
```

API error `code` values, event types, permission keys, status codes, and field names remain language-neutral.

Example:

```json
{
  "error": {
    "code": "DEPENDENCY_BLOCKED",
    "message": "Bu görev henüz başlatılamaz."
  }
}
```

Clients must branch on `code`, never on localized `message`.

For WebSocket events, transmit canonical structured data. Clients render localized UI text.

Notification/email generation must resolve the recipient language; persisted event identity remains canonical.
