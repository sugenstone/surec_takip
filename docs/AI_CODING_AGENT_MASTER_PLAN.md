# AI Coding Agent Master Plan --- Multi-Tenant Workflow & Operations Platform

> **Purpose:** This file is the implementation contract for an AI coding
> agent. Follow the phases in order. Do not skip foundations to
> implement later features early. Keep the product industry-agnostic,
> simple for end users, and extensible for advanced organizations.

------------------------------------------------------------------------

## 0. Agent Operating Rules

1.  Read this entire document before changing code.
2.  Work **phase by phase** and **task by task** in the specified order.
3.  Before each phase:
    -   inspect the existing repository,
    -   identify what already exists,
    -   avoid duplicating working functionality,
    -   write/update migrations and tests first where practical.
4.  After each task:
    -   run formatting,
    -   run static/type checks,
    -   run unit/integration tests,
    -   fix failures before continuing.
5.  Do not introduce microservices, Kafka, Kubernetes, GraphQL, or other
    infrastructure unless a later requirement explicitly justifies it.
6.  Start as a **modular monolith**.
7.  Never hard-code industry terms such as kitchen countertop, textile,
    CNC, software, construction, etc. These are user data/templates, not
    core domain concepts.
8.  UX has priority over exposing technical complexity.
9.  Every destructive or important state-changing operation must be
    permission-checked, auditable, and recoverable where feasible.
10. All tenant-owned database access must be tenant-scoped on the
    backend. Frontend filtering is never a security boundary.
11. Prefer simple, maintainable code over clever abstractions.
12. Keep API contracts explicit and documented.
13. Use UTC in persistence. Convert to tenant/user timezone at
    presentation boundaries.
14. Do not calculate live timers by writing to the database every
    second.
15. Do not send email synchronously inside user-facing HTTP requests.
16. Any schema or architectural decision that conflicts with this
    document must be documented before changing the plan.
17. Keep `README.md`, migrations, API documentation, and this
    implementation checklist current.
18. Never silently overwrite concurrent user changes when a conflict can
    be detected.
19. Default behavior must work well without advanced configuration.
20. Build advanced functionality progressively; the simple workflow must
    remain simple.

------------------------------------------------------------------------

# 1. Product Definition

Build a general-purpose, multi-tenant SaaS platform for managing
projects, hierarchical work, cards, processes, process steps, employee
assignments, dependencies, live work timers, templates, dynamic
properties, notifications, permissions, subscriptions, reporting, and
live TV/operations displays.

The platform must support many industries without embedding any
industry's terminology into the core model.

### Core hierarchy

``` text
Platform
└── Organization / Tenant
    └── Workspace
        └── Project
            └── Section (recursive / unlimited nesting)
                └── Card
                    └── Process
                        └── Process Step
                            ├── Assignment(s)
                            ├── Dependencies
                            ├── Checklist / Forms
                            └── Time Sessions
```

Shared platform capabilities:

``` text
Users & Memberships
Roles & Permissions
Teams / Departments
Dynamic Properties
Templates
Realtime Events
Notifications
Email
Audit Log
Undo / Restore
Files
Search
Automations
Billing / Subscription
Reporting
API / Webhooks
```

------------------------------------------------------------------------

# 2. Primary UX Principles

The backend may be sophisticated. The user experience must not be.

### Employee experience

Primary loop:

``` text
My Tasks → Open Task → Start → Pause/Resume → Complete
```

An employee should not need to understand workflow graphs, templates,
tenant architecture, dependency engines, or database concepts.

### Admin experience

Primary loop:

``` text
Create/Select Project → Organize Sections → Create Cards →
Attach Process/Template → Assign Work → Monitor → Correct/Undo → Report
```

### TV experience

Primary purpose:

> Show every currently active process step with a live timer.

TV must require no manual refresh.

### UX rules

-   Frequent actions remain visible.
-   Advanced settings use progressive disclosure.
-   Prefer autosave for small edits.
-   Prefer "perform + Undo" over confirmation dialogs for low-risk
    reversible actions.
-   Use confirmation only for high-impact destructive operations.
-   Show clear reasons when an action is blocked.
-   Mobile employee UI must be usable one-handed.
-   A user should never need to understand the data model to complete
    routine work.
-   Realtime changes should appear without page refresh.
-   Admin, employee, and TV clients must converge on the same server
    state.
-   Provide loading, empty, error, offline, permission-denied, and
    conflict states intentionally.
-   Accessibility and keyboard navigation are required for admin UI.

------------------------------------------------------------------------

# 3. Recommended Technical Stack

## Frontend

-   SvelteKit
-   TypeScript
-   Tailwind CSS
-   shadcn-svelte and/or Bits UI
-   PWA support
-   Playwright for end-to-end tests

## Backend

-   Rust
-   Axum
-   Tokio
-   Serde
-   SQLx
-   Tracing
-   REST API
-   WebSocket for realtime communication

## Data & Infrastructure

-   PostgreSQL as the primary database from day one
-   Redis when needed for cache, distributed realtime/pub-sub, rate
    limiting, jobs, and locks
-   S3-compatible object storage for files
-   Docker for local and production packaging
-   Caddy or Nginx as reverse proxy
-   OpenAPI for API contract/documentation
-   OpenTelemetry-compatible observability
-   Error tracking integration

## Architecture

Use a modular monolith initially.

Suggested repository:

``` text
platform/
├── apps/
│   ├── web/
│   ├── server/
│   └── worker/
├── packages/
│   ├── ui/
│   └── contracts/
├── migrations/
├── infrastructure/
├── docs/
├── docker/
└── compose.yml
```

Suggested backend modules:

``` text
auth
organizations
workspaces
users
memberships
teams
roles
permissions
projects
sections
cards
properties
processes
process_steps
dependencies
assignments
time_tracking
templates
notifications
email
files
audit
search
billing
automations
webhooks
reports
```

------------------------------------------------------------------------

# 4. Non-Negotiable Data Architecture

Use UUIDv7 (or an equivalent sortable globally unique ID strategy) for
internal IDs.

Where appropriate, user-facing human-readable identifiers may coexist:

``` text
PRJ-00124
CARD-01842
```

Tenant-owned entities should generally contain:

``` text
id
tenant_id
created_at
updated_at
deleted_at
created_by
updated_by
```

Use soft deletion for recoverable business data unless there is a strong
reason not to.

Do not put the entire domain model into JSONB. Use relational
columns/tables for core relationships. JSONB is acceptable for flexible
configuration/settings where appropriate.

------------------------------------------------------------------------

# PHASE 1 --- Repository, Development Environment, and Quality Gates

## 1.1 Initialize repository

Create the monorepo structure.

## 1.2 Local infrastructure

Create Docker Compose for:

-   PostgreSQL
-   Redis only when a concrete cache/queue/pub-sub requirement is implemented
-   local S3-compatible storage if needed
-   backend
-   worker
-   frontend

## 1.3 Configuration

Create environment configuration with safe separation between:

-   development
-   test
-   production

Never commit secrets.

## 1.4 CI baseline

CI must run:

-   Rust formatting
-   Rust linting
-   Rust tests
-   TypeScript checks
-   frontend linting
-   frontend tests
-   database migration validation

### Exit criteria

The empty platform can be started locally with one command and CI is
green.

------------------------------------------------------------------------

# PHASE 2 --- Multi-Tenant Foundation

This phase must be completed before business modules.

## 2.1 Organization / Tenant

Implement:

``` text
organizations
organization_memberships
```

An organization represents a SaaS customer/company.

A user may belong to multiple organizations.

## 2.2 Workspace

Implement:

``` text
workspaces
workspace_memberships
```

One organization may have multiple workspaces.

Examples are intentionally generic: branches, factories, departments,
business units, or teams.

## 2.3 Tenant context middleware

Every authenticated request must resolve:

``` text
User
→ Organization
→ Workspace (when applicable)
→ Membership
→ Permissions
```

## 2.4 Tenant isolation

Every tenant-owned query must be scoped by `tenant_id`.

Evaluate PostgreSQL Row Level Security as defense in depth, but do not
rely on it as the only application-level authorization mechanism.

## 2.5 Tests

Required security tests:

-   User A cannot access Organization B.
-   A guessed UUID cannot cross tenant boundaries.
-   Workspace access does not imply access to another workspace.
-   Soft-deleted memberships cannot grant access.

### Exit criteria

Cross-tenant data access is blocked and covered by automated tests.

------------------------------------------------------------------------

# PHASE 3 --- Authentication, Users, Teams, Roles, Permissions

## 3.1 Authentication

Implement secure login/session handling.

Support architecture for:

-   password login
-   password reset
-   email verification
-   session revocation
-   optional future 2FA/SSO

## 3.2 Users and invitations

Admins can invite users by email.

Invitation lifecycle:

``` text
Invite → Email → Accept → Membership Created
```

## 3.3 Teams / departments

Implement teams.

A user can belong to multiple teams if needed.

Tasks may later be assigned to either a person or team.

## 3.4 RBAC

Do not hard-code only `admin` and `employee`.

Provide default roles such as:

``` text
Owner
Admin
Project Manager
Team Lead
Employee
Viewer
```

Allow custom roles.

Permission keys should resemble:

``` text
projects:view
projects:create
projects:update
projects:delete

cards:view
cards:create
cards:update
cards:delete

processes:view
processes:create
processes:update
processes:delete

tasks:start
tasks:pause
tasks:complete
tasks:reassign

users:invite
roles:manage
billing:manage
reports:view
```

Permissions may also have scope:

``` text
organization
workspace
assigned_projects
team
self
```

Frontend hiding is convenience only. Backend authorization is mandatory.

### Exit criteria

Role changes take effect immediately and permission tests cover critical
endpoints.

------------------------------------------------------------------------

# PHASE 3.5 --- Reusable SaaS Starter Extraction (Milestone)

This milestone makes the generic SaaS/tenancy/security foundation reusable
as the **"Sugenstone SaaS Starter"** so future products do not rebuild it
from scratch. Decision record:
`docs/decisions/0005-reusable-saas-starter.md`.

Position (binding):

``` text
Generic SaaS/Security Foundation
        ↓
Reusable SaaS Starter Extraction
        ↓
Project-Specific Domain Development
```

Protection rule:

> Project-specific domain development must not begin until the reusable
> starter extraction gate has been evaluated after completion of the
> generic multi-tenant/security foundation.

The generic foundation (users/sessions, authentication, organizations,
memberships, workspaces, workspace memberships, invitations, roles,
permissions, RBAC, tenant context, tenant isolation, cross-tenant
security tests) is NOT blocked by this rule — it is the rule's
precondition.

## 3.5.1 Gate conditions

Extraction may begin only when all of the following hold:

``` text
users + secure sessions implemented and tested
organizations + organization memberships
workspaces + workspace memberships
invitations
roles + permissions + RBAC
tenant context middleware
tenant isolation
cross-tenant security tests green
hosted CI green on the foundation commit
```

## 3.5.2 Extraction procedure (agent obligations at extraction time)

1. Review the repository for domain independence.
2. Identify any surec_takip-specific names, assumptions or dependencies
   that leaked into the generic foundation.
3. Separate generic code from product domain code.
4. Make project name, product name, branding, application title and
   similar identity values easily configurable.
5. Prepare the starter's own `README.md`.
6. Prepare/adapt the starter's own `AGENTS.md`.
7. Create an example `PROJECT_BRIEF.md` template for new projects
   (Product, Purpose, Primary Users, Main Domain Entities, Initial
   Language, Additional Languages, Branding, Special Requirements).
8. Define the new-project startup procedure:

   ``` text
   Sugenstone SaaS Starter
   → create new repository from template
   → configure project identity/environment
   → complete PROJECT_BRIEF.md
   → coding agent reads AGENTS.md + PROJECT_BRIEF.md
   → agent plans project-specific domain
   → domain implementation begins
   ```

9. Run all generic tests in the starter.
10. Verify Docker clean-start.
11. Verify database migrations.
12. Run authentication and tenant-isolation security tests.
13. Verify GitHub Actions is green on the starter repository.
14. Verify the starter repository contains no secrets, production
    credentials, customer data, project-specific data, or
    surec_takip-specific assumptions.
15. Verify readiness for use as a GitHub Template Repository.

## 3.5.3 Boundaries and consequences

- The starter stays domain-independent; the excluded-domain list lives in
  the decision record and must be respected by generic phases.
- Extraction must not break this repository or its history; the product
  keeps developing here independently.
- Starter changes are not automatically propagated to derived products.
- The starter may adopt semantic versioning (v1.0.0, v1.1.0, ...); no
  versioning implementation happens now.

------------------------------------------------------------------------

# PHASE 4 --- Core Work Model

## 4.1 Projects

Implement generic projects.

Do not require a customer.

Project metadata may include dynamic properties later.

## 4.2 Recursive sections

Sections must support unlimited nesting.

Example:

``` text
Project
└── Section
    └── Section
        └── Section
```

Requirements:

-   create
-   rename
-   move
-   reorder
-   duplicate
-   bulk-create sibling sections
-   recursive navigation
-   tree view
-   card view

Prevent cycles when moving sections.

## 4.3 Cards

Replace industry-specific concepts such as "workmanship" or "product"
with the generic `Card`.

A card may represent:

-   work order
-   product
-   service
-   ticket
-   production item
-   maintenance job
-   deliverable

These are display semantics, not database types unless configured by
templates.

Card baseline:

``` text
id
tenant_id
project_id
section_id
title
description
system_status
priority
due_at
created_at
updated_at
deleted_at
```

## 4.4 Relationships

Design card-to-card relationships so later the system can support:

``` text
parent / child
related
blocks / blocked_by
```

### Exit criteria

Admins can build a project hierarchy and create/move/duplicate cards
without processes yet.

------------------------------------------------------------------------

# PHASE 5 --- Dynamic Property Engine

The system must remain industry-agnostic.

## 5.1 Property definitions

Implement reusable property definitions.

Supported types should be designed for:

``` text
short_text
long_text
integer
decimal
money
percentage
boolean
date
datetime
duration
single_select
multi_select
user
team
organization/customer reference
phone
email
url
file
image
relation
formula (later if necessary)
```

## 5.2 Property values

Keep core fields relational. Dynamic values use a typed property value
system.

## 5.3 Property blocks

Properties should render as configurable blocks on cards.

Admin can:

-   add property
-   remove property
-   reorder property
-   mark required
-   set default value
-   choose visibility

## 5.4 Display configuration

Allow properties to be marked for visibility in:

-   card summary
-   table
-   TV display
-   employee task
-   reports

### Exit criteria

Two cards can use completely different business fields without schema
changes.

------------------------------------------------------------------------

# PHASE 6 --- Process and Process Step Engine

## 6.1 Process

A card can contain zero or more processes.

## 6.2 Process steps

A process contains ordered process steps.

Each process step may contain:

``` text
name
description
order
system_status
priority
estimated_duration
default_assignee
current assignee(s)
required fields
checklist
completion rules
```

## 6.3 Employee assignment

Support:

-   user assignment
-   team assignment
-   default user
-   default team
-   reassignment

Default assignee should be replaceable per instance.

## 6.4 Status model

User-visible statuses may be configurable later, but maintain canonical
system states internally, e.g.:

``` text
not_started
ready
active
paused
completed
blocked
cancelled
awaiting_approval
```

This preserves reporting consistency.

### Exit criteria

An admin can attach processes and process steps to a card and assign
employees.

------------------------------------------------------------------------

# PHASE 7 --- Dependency / Workflow Engine

Do not implement dependencies merely with sequence numbers.

## 7.1 Process dependencies

A process may depend on one or more other processes.

## 7.2 Process-step dependencies

A process step may depend on one or more other steps.

Suggested tables:

``` text
process_dependencies
process_step_dependencies
```

## 7.3 Dependency rules

Support:

``` text
ALL prerequisites completed
ANY prerequisite completed
```

When unsatisfied:

``` text
hard_block
warn_only
```

## 7.4 Explain blocked state

Never show only a lock icon.

Show:

``` text
This task cannot start yet.

Waiting for:
- Step A
- Step B
```

## 7.5 Automatic unlocking

When a prerequisite completes:

1.  recompute dependents,
2.  change eligible items to `ready`,
3.  publish realtime event,
4.  create notifications if configured.

## 7.6 Override

Authorized roles may bypass a dependency.

Require a reason for override.

Audit:

``` text
User X bypassed prerequisite Y
Reason: ...
Timestamp: ...
```

## 7.7 Cycle detection

Prevent circular dependencies.

## 7.8 Workflow simulation

Provide a future/admin utility to test workflow behavior before
publishing a template.

### Exit criteria

Complex DAG-like workflows work without cycles and locked tasks explain
why they are locked.

------------------------------------------------------------------------

# PHASE 8 --- Time Tracking Engine

This is a core subsystem.

## 8.1 Time sessions

Never calculate work time only from a single start/end pair.

Create separate sessions:

``` text
time_sessions

id
tenant_id
process_step_id
employee_id
started_at
stopped_at
stop_reason
created_at
```

Example:

``` text
14:00 Start
14:25 Pause
15:10 Resume
15:42 Pause
16:00 Resume
16:18 Complete

Active time = 25 + 32 + 18 = 75 minutes
```

Paused time is not active work time.

## 8.2 Commands

Employee actions:

``` text
Start
Pause
Resume
Complete
```

## 8.3 Single-active-task rule

By default, if an employee starts another task while one is active:

``` text
You are currently working on Task A.
Pause Task A and start Task B?
```

On confirmation:

1.  stop current session,
2.  create new session,
3.  emit both events transactionally.

Make this policy configurable later if necessary.

## 8.4 Stop reasons

Optional configurable stop reasons:

``` text
Break
Switching task
Waiting for material
Machine issue
Waiting for approval
Other
```

Tenant can decide whether a reason is required.

## 8.5 Corrections

Authorized users can correct erroneous sessions.

Never silently mutate history. Record before/after in audit log.

### Exit criteria

Active duration is correct across multiple pauses/resumes and survives
page refresh/server restart.

------------------------------------------------------------------------

# PHASE 9 --- Realtime Architecture

Realtime is a core requirement, not an enhancement.

## 9.1 Domain events

Introduce explicit domain events such as:

``` text
card.created
card.updated
assignment.created
assignment.changed
process_step.ready
process_step.started
process_step.paused
process_step.resumed
process_step.completed
process_step.reopened
dependency.unlocked
```

## 9.2 Transactional outbox

State changes and event creation must commit together.

Example:

``` text
DB transaction:
- update process step
- create time session
- write outbox event
COMMIT
```

A worker publishes/processes outbox events. Minimal outbox persistence is introduced
before the first command that requires transactional event creation; this phase
adds dispatch and client delivery, not delayed integrity for earlier commands.

## 9.3 WebSocket

Push relevant events to connected:

-   admin clients
-   employee clients
-   TV displays

Scope subscriptions by tenant/workspace/project as appropriate.

## 9.4 Timer strategy

Do **not** send a WebSocket message every second.

Server sends:

``` text
started_at
accumulated_active_duration
status
```

Client calculates:

``` text
displayed_duration =
accumulated_active_duration + (now - started_at)
```

When pause/complete/reassignment occurs, server sends a new
authoritative event.

Periodically resync clocks/state if necessary to avoid drift.

### Exit criteria

Two browser sessions and a TV display reflect state changes without
refresh.

------------------------------------------------------------------------

# PHASE 10 --- Employee Application

Build mobile-first.

## 10.1 Home

Employee sees:

``` text
Current Task
Recommended / Next Tasks
My Tasks
Today Completed
Notifications
```

## 10.2 Task card

Keep actions extremely simple:

``` text
START
PAUSE
RESUME
COMPLETE
```

## 10.3 Next-task assistance

The system may prioritize tasks based on objective data such as:

-   assigned to user
-   prerequisites satisfied
-   due date
-   configured priority

Show why an item is surfaced. Do not hide the rest of the task list.

## 10.4 Completion requirements

A task may require before completion:

-   required properties
-   checklist completion
-   form
-   photo/file
-   approval submission

Only show these requirements when relevant.

## 10.5 Handoff

Design support for task handoff:

``` text
Employee A → Employee B
note
time history preserved per employee
```

### Exit criteria

An employee can perform the normal workday flow without entering the
admin interface.

------------------------------------------------------------------------

# PHASE 11 --- Live TV / Operations Display

This is a first-class client.

## 11.1 Main view

Display every active process step.

Each card should support:

``` text
Project
Section path
Card
Process
Process Step
Assigned employee/team
Start time
LIVE ACTIVE TIMER
selected dynamic properties
```

## 11.2 Responsive grid

Automatically adapt:

``` text
1 active → 1 large card
2 active → 2 cards
4 active → 2×2
6 active → 3×2
etc.
```

Maintain legibility at TV viewing distance.

## 11.3 Status summary

Optional footer/header:

``` text
Active
Paused
Blocked
Completed Today
```

## 11.4 Paused items

Tenant may configure whether paused tasks:

-   disappear from main active grid,
-   move to a paused area,
-   remain visible with paused state.

## 11.5 Completion behavior

On completion, optionally show "Completed" briefly before removal.

## 11.6 Realtime

No page refresh.

### Exit criteria

Starting, pausing, resuming, completing, and reassigning a step updates
the TV in near realtime.

------------------------------------------------------------------------

# PHASE 12 --- Templates

Separate **card templates** from **process templates**.

## 12.1 Card templates

Define:

-   properties
-   property ordering
-   defaults
-   required fields
-   blocks
-   default process template(s)

## 12.2 Process templates

Define:

-   processes
-   steps
-   ordering
-   dependencies
-   default assignees
-   estimated durations
-   checklists
-   forms
-   completion rules

## 12.3 Snapshot/version semantics

When a template is assigned to a live card, create a versioned
snapshot/reference strategy that prevents future template edits from
unexpectedly mutating already-running work.

Example:

``` text
Template v1 → existing project continues on v1
Template updated → v2
New project → v2
```

Provide explicit upgrade/migration later if desired.

## 12.4 Draft/publish

Templates should support:

``` text
Draft → Published → Archived
```

### Exit criteria

A reusable process can be assigned repeatedly without corrupting
existing instances when its template changes.

------------------------------------------------------------------------

# PHASE 13 --- Checklist, Forms, Completion Rules, Approval

## 13.1 Checklist

Do not force every small action to become a process step.

A process step may contain a checklist.

## 13.2 Forms

Admin can create forms containing generic field blocks.

## 13.3 Completion rules

Examples:

``` text
Checklist must be complete
Required photo must exist
Required field must have value
Form must be submitted
Approval must be received
```

## 13.4 Approval workflow

A step may transition:

``` text
Active
→ Submitted for Approval
→ Approved
```

or:

``` text
→ Returned for Correction
```

Dependencies may require `approved`, not merely `completed`.

### Exit criteria

Quality/control workflows can be represented without custom code.

------------------------------------------------------------------------

# PHASE 14 --- Audit, Undo, Delete, Restore, Version History

This is mandatory for UX and trust.

Minimal append-only audit persistence accompanies the first audited mutation in
earlier phases. This phase completes audit/history UI and broader recovery tools;
it does not defer audit of role changes, deletion/restoration or time commands.

## 14.1 Audit log

Record important events:

``` text
who
tenant
workspace
entity type
entity id
action
before
after
timestamp
reason (when relevant)
```

## 14.2 Soft delete

Normal deletion should generally set `deleted_at`.

## 14.3 Undo

After reversible actions show:

``` text
Deleted. Undo
```

or:

``` text
Task completed. Undo
```

## 14.4 Trash

Admins can inspect and restore deleted business objects subject to
retention policy.

## 14.5 Reopen/correct employee mistakes

If an employee accidentally completes a task, allow permitted
undo/reopen.

Do not erase the original event; append corrective history.

## 14.6 Version history

For important card/configuration changes, support viewing prior values
and later restoring a prior version where safe.

### Exit criteria

A mistaken routine action can be corrected without database intervention
and its history remains visible.

------------------------------------------------------------------------

# PHASE 15 --- Notifications and Email

## 15.1 Notification engine

Notifications are derived from events.

Potential triggers:

``` text
task assigned
task reassigned
task ready
task overdue
deadline approaching
task completed
task paused
approval requested
approval returned
mention
file added
project completed
billing issue
```

## 15.2 Channels

Initial:

``` text
in_app
email
```

Design channel abstraction for future additions.

## 15.3 Preferences

Users can configure non-mandatory notification channels.

Organization admins may define mandatory critical rules.

## 15.4 Rule model

Conceptually:

``` text
WHEN event
IF conditions
SEND to recipients
VIA channels
```

Recipients may be:

``` text
specific user
assignee
team lead
project manager
role
```

## 15.5 Email worker

Email is asynchronous.

Use jobs/outbox. Add retry and failure tracking.

## 15.6 Digest

Design for:

``` text
morning digest
end-of-day digest
```

to prevent notification fatigue.

### Exit criteria

Assigning a task can produce an in-app notification immediately and an
email asynchronously without delaying the API request.

------------------------------------------------------------------------

# PHASE 16 --- Files and Comments

## 16.1 Files

Store bytes in S3-compatible storage, metadata in PostgreSQL.

Files may attach to:

-   project
-   card
-   process
-   process step
-   form submission
-   comment

Enforce tenant-scoped authorization on download.

## 16.2 Comments

Add threaded/simple comments to cards and/or steps.

Support `@mention`.

Mention produces notification.

## 16.3 Activity timeline

Show a human-readable timeline combining relevant events:

``` text
16:42 Task completed
16:31 3 photos added
15:54 Work started
15:42 Assignee changed
14:17 Due date changed
```

### Exit criteria

Operational communication can remain attached to the work rather than
being lost externally.

------------------------------------------------------------------------

# PHASE 17 --- Search, Saved Views, and Bulk Operations

## 17.1 Global search

Search across permitted:

-   projects
-   sections
-   cards
-   human-readable IDs
-   selected dynamic properties
-   users

## 17.2 Command palette

Provide `Ctrl/Cmd + K` for fast navigation/actions.

## 17.3 Views

Support progressively:

``` text
List
Table
Cards
Tree
Kanban
Calendar
Timeline
Active Operations
TV
```

Do not implement all at once if schedule is constrained. Tree, cards,
list/table, active operations, and TV are higher priority.

## 17.4 Filters

Generic filter builder:

``` text
property/status/date/assignee
operator
value
AND/OR
```

## 17.5 Saved views

Users/admins can save filtered views.

## 17.6 Bulk actions

Examples:

``` text
assign
add process
change property
change due date
move
archive
delete
```

Bulk operations must be audited.

### Exit criteria

Large organizations do not need to navigate manually through thousands
of cards.

------------------------------------------------------------------------

# PHASE 18 --- Import / Export

## 18.1 CSV/Excel import

Create mapping flow:

``` text
Source column → Platform field/property
```

Preview before commit.

## 18.2 Bulk creation

Support generating repetitive structures such as many sibling
sections/cards.

## 18.3 Export

Design exports for:

-   CSV/Excel
-   report-friendly formats later
-   tenant data portability

### Exit criteria

Organizations can onboard existing operational data without manual
re-entry.

------------------------------------------------------------------------

# PHASE 19 --- Reporting and Operational Analytics

Focus on operational facts, not opaque employee scoring.

## 19.1 Metrics

Examples:

``` text
active work duration
waiting duration
paused duration
cycle time
estimated vs actual
completed count
blocked count
overdue count
```

Break down by:

``` text
project
section
card
process
step
team
employee
resource (future)
```

## 19.2 Waiting time

Track and expose:

``` text
Total elapsed
Active work
Waiting/paused
```

This is important for bottleneck analysis.

## 19.3 Stop-reason analysis

Example:

``` text
72 hours waiting for material
31 hours machine issue
18 hours approval waiting
```

## 19.4 Dashboards

Role-specific defaults:

Employee:

``` text
Current task
Next tasks
Today completed
```

Team lead:

``` text
Team active
Blocked
Paused
Approvals
```

Manager:

``` text
Projects
Overdue
Bottlenecks
Operational summary
```

### Exit criteria

Managers can understand where work time and waiting time are being
spent.

------------------------------------------------------------------------

# PHASE 20 --- Customer / Organization Records (Optional Module)

A customer is not mandatory for every card/project.

Implement a generic external organization/contact module that can be
disabled.

Potential fields:

``` text
organization name
contacts
phone
email
billing metadata
notes
related projects
```

Projects/cards may reference a customer record through a normal
relation/property.

Do not make the core hierarchy depend on customer existence.

------------------------------------------------------------------------

# PHASE 21 --- Resources (Future-Ready)

Prepare architecture for assignment beyond people.

Possible resources:

``` text
Employee
Team
Machine
Vehicle
Workstation
Room/Location
```

A step could later record:

``` text
Employee: X
Machine: Y
```

Do not overbuild this in V1, but avoid schema choices that make it
impossible.

------------------------------------------------------------------------

# PHASE 22 --- PWA and Offline Strategy

## 22.1 PWA

Employee app should be installable as a PWA.

## 22.2 Offline

Do not implement naïve offline state mutation.

Design an explicit queued-command model for future/offline support.

Example:

``` text
Employee taps Pause offline
→ client records command + client timestamp
→ reconnect
→ server validates and reconciles
→ conflicts surfaced
```

Offline timer actions require special care because time is business
data.

### Exit criteria

PWA installability works; deeper offline mutation may be phased if
necessary.

------------------------------------------------------------------------

# PHASE 23 --- Billing and Subscription

Billing belongs to organization/tenant, not individual operational
cards.

## 23.1 Models

``` text
plans
plan_features
subscriptions
subscription_items
entitlements
usage_counters
invoices
payments
billing_events
coupons (later)
add_ons (later)
```

## 23.2 Feature entitlements

Never scatter logic such as:

``` text
if plan == "professional"
```

Use:

``` text
feature.tv_display = true
feature.api_access = false

limit.users = 25
limit.storage_bytes = ...
limit.automation_runs = ...
```

## 23.3 Billing cycles

Support architecture for:

``` text
monthly
yearly
```

## 23.4 Trial

Support configurable trial.

## 23.5 Grace period

Payment failure must not abruptly stop workers during active operations.

Suggested flow:

``` text
Payment failed
→ notify billing admins
→ retry / grace period
→ restricted mode if unresolved
```

Operational data remains safe.

## 23.6 Provider abstraction

Keep internal subscription/entitlement models independent of a single
payment provider.

## 23.7 Platform admin

Create a separate platform administration security domain for:

``` text
organizations
plans
subscriptions
payments
feature flags
email health
jobs
system health
```

Tenant admins must never receive platform-admin capabilities.

### Exit criteria

Feature access is entitlement-driven and changing a plan does not
require code changes.

------------------------------------------------------------------------

# PHASE 24 --- Automation Engine

Build after core events are stable.

Concept:

``` text
WHEN
event occurs

IF
conditions match

THEN
actions
```

Example:

``` text
WHEN task becomes overdue
IF priority = critical
THEN notify project manager
```

Potential actions:

``` text
send notification
send email
change status
assign user/team
set property
open next process
create card
call webhook
```

Every automation run must be logged and idempotency considered.

------------------------------------------------------------------------

# PHASE 25 --- API and Webhooks

## 25.1 Public API

Expose tenant-scoped API keys with permissions.

## 25.2 Webhooks

Events may include:

``` text
card.created
card.updated
task.started
task.completed
project.completed
```

Requirements:

-   signing secret
-   retries
-   delivery history
-   idempotency
-   disable failing endpoint after policy threshold
-   tenant scoping

------------------------------------------------------------------------

# PHASE 26 --- SaaS Customization

Tenant settings may include:

``` text
name
logo
language
timezone
date format
currency
week start
working hours
email settings
terminology
```

Terminology aliases are important.

Internally:

``` text
Card
```

Tenant UI may call it:

``` text
Work Order
Order
Job
Production Item
Ticket
```

Do not rename database concepts based on tenant terminology.

------------------------------------------------------------------------

# PHASE 27 --- Advanced Operational Features

Implement only after core stability.

Candidates:

-   recurring work
-   shift schedules
-   leave/availability
-   SLA rules
-   advanced approvals
-   resource capacity
-   QR/barcode
-   saved dashboards
-   formula properties
-   rollups/lookups
-   template migration tools
-   task handoff enhancements
-   recurring notification summaries

QR/barcode should use stable public-safe identifiers, not expose
sensitive internal structure.

------------------------------------------------------------------------

# 28. Realtime Consistency and Conflict Rules

The system is collaborative.

Requirements:

1.  Server is authoritative.
2.  Optimistic UI is allowed when safe.
3.  Realtime events reconcile clients.
4.  Use version/revision numbers or equivalent optimistic concurrency
    for editable entities.
5.  If Admin A edits due date and Admin B edits assignee, field-level or
    merge-friendly updates should preserve both where possible.
6.  If both edit the same protected field concurrently, surface a
    conflict instead of silently overwriting.
7.  Deleted entities received through realtime should
    disappear/transition appropriately.
8.  Permission changes must invalidate relevant client capabilities
    promptly.

------------------------------------------------------------------------

# 29. Error Recovery Rules

For every major action define:

``` text
success
validation error
permission error
dependency blocked
concurrency conflict
network failure
server failure
undo behavior
audit behavior
```

Never leave a user unsure whether an action succeeded.

Use idempotency keys for actions where retries could duplicate important
business events.

------------------------------------------------------------------------

# 30. Security Checklist

Mandatory:

-   tenant isolation
-   backend RBAC
-   secure password/session handling
-   CSRF strategy appropriate to auth model
-   XSS-safe rendering
-   SQL parameterization
-   rate limiting
-   upload validation
-   signed/private file access
-   audit logs
-   secret management
-   backups
-   restore testing
-   email verification
-   session revocation
-   secure headers
-   dependency scanning
-   logging without leaking secrets
-   platform-admin separation
-   billing webhook signature validation
-   API key hashing/storage strategy

------------------------------------------------------------------------

# 31. Testing Strategy

## Unit tests

Especially:

-   dependency evaluation
-   cycle detection
-   time calculation
-   permission evaluation
-   entitlement evaluation
-   status transitions
-   notification rules

## Integration tests

Especially:

-   tenant isolation
-   transactional outbox
-   start/pause/resume/complete
-   dependency unlock
-   assignment changes
-   undo/reopen
-   template snapshot behavior
-   billing entitlement changes

## End-to-end tests

Critical scenario:

``` text
Admin logs in
→ creates project
→ creates section/card
→ attaches process template
→ assigns employee
→ employee receives task
→ employee starts task
→ admin sees active state
→ TV shows task + live timer
→ employee pauses
→ TV updates
→ employee resumes
→ timer continues active-time total
→ employee completes
→ dependent task unlocks
→ notification appears
→ audit log contains full history
```

Also test:

``` text
Employee completes by mistake
→ Undo/Reopen
→ state restored
→ original action and correction remain in audit history
```

------------------------------------------------------------------------

# 32. Performance Principles

-   Paginate large collections.
-   Avoid N+1 queries.
-   Index `tenant_id` plus common filter columns.
-   Index active time sessions.
-   Index dependency lookup paths.
-   Do not broadcast tenant events globally.
-   Do not persist timer ticks.
-   Use background processing for email, webhooks, digests, heavy
    exports, and automation.
-   Add Redis only where it solves a demonstrated architectural need.
-   Measure before optimizing.

------------------------------------------------------------------------

# 33. Suggested Initial Database Entities

This is a starting inventory, not permission to create all tables before
their phase.

``` text
users
sessions

organizations
organization_memberships

workspaces
workspace_memberships

teams
team_members

roles
permissions
role_permissions
membership_roles

projects
sections
cards
card_relations

property_definitions
property_options
property_values

processes
process_steps
process_dependencies
process_step_dependencies

assignments
time_sessions
stop_reasons

checklists
checklist_items
forms
form_fields
form_submissions

approvals

card_templates
card_template_versions
process_templates
process_template_versions

comments
mentions
files

notifications
notification_preferences
notification_rules

audit_events
entity_versions
outbox_events

saved_views

plans
plan_features
subscriptions
entitlements
usage_counters
invoices
payments
billing_events

automation_rules
automation_runs

api_keys
webhook_endpoints
webhook_deliveries
```

------------------------------------------------------------------------

# 34. V1 Scope --- Build This First

Do **not** attempt the entire roadmap before shipping a usable V1.

V1 should contain:

1.  Multi-tenant organizations
2.  Workspaces
3.  Authentication
4.  User invitations
5.  Basic RBAC
6.  Projects
7.  Recursive sections
8.  Cards
9.  Basic dynamic properties
10. Processes
11. Process steps
12. User/team assignment
13. Default assignee
14. Process/step dependencies
15. Start / Pause / Resume / Complete
16. Accurate time sessions
17. Employee "My Tasks"
18. Realtime WebSocket updates
19. Live TV display with second-by-second client timer
20. Process templates
21. Card templates
22. Audit log
23. Soft delete / restore
24. Basic Undo/Reopen
25. In-app notifications
26. Email notifications
27. Basic file attachments
28. Search
29. Basic dashboard
30. Basic subscription/entitlement foundation
31. Responsive/PWA-ready frontend
32. Critical automated tests

### V1 success definition

A new company can:

``` text
Register
→ create organization/workspace
→ invite employees
→ define roles
→ create project
→ create nested sections
→ create cards
→ attach a reusable process
→ assign process steps
→ enforce dependencies
→ employee starts work
→ TV/admin sees live timer
→ employee pauses/switches/resumes
→ employee completes
→ next dependent work unlocks
→ notifications are delivered
→ mistakes can be corrected
→ all important changes are audited
```

If this flow is excellent, V1 is successful.

------------------------------------------------------------------------

# 35. V2 Candidates

After V1 stability:

-   advanced forms
-   approvals
-   advanced notification rules
-   saved views
-   bulk operations
-   CSV/Excel import
-   richer reporting
-   stop-reason analytics
-   customer/contact module
-   recurring work
-   enhanced billing
-   coupons/add-ons
-   automation engine
-   public API
-   webhooks
-   QR/barcode
-   advanced offline queue
-   resource/machine assignment
-   shift/availability
-   SLA
-   formula/rollup properties
-   dashboard builder

------------------------------------------------------------------------

# 36. V3 Candidates

-   SSO
-   enterprise security controls
-   advanced audit retention
-   advanced workflow designer
-   workflow simulation UI
-   resource capacity planning
-   deeper integrations
-   AI-assisted querying/configuration
-   AI-assisted import mapping
-   advanced tenant branding
-   enterprise billing/contracts

------------------------------------------------------------------------

# 37. AI Features --- Safety Rule for Actions

AI may later assist with:

``` text
“Show overdue work in Project X.”
“Summarize today's blocked tasks.”
“Create a draft project from this spreadsheet.”
```

For AI-generated mutations, always preview consequential changes.

Example:

``` text
148 cards will be created
6 process templates will be attached
23 users will receive assignments

[Confirm]
```

AI must not silently perform large destructive or bulk mutations.

------------------------------------------------------------------------

# 38. Implementation Order Summary

The coding agent must follow this order:

``` text
01 Repository + CI
02 Multi-tenancy
03 Auth + RBAC
-- MILESTONE: Reusable SaaS Starter Extraction (Phase 3.5) --
04 Projects + Sections + Cards
05 Dynamic Properties
06 Processes + Steps
07 Dependencies
08 Time Sessions
09 Realtime + Outbox
10 Employee UX
11 TV Display
12 Templates
13 Checklist/Forms/Approvals
14 Audit + Undo + Restore
15 Notifications + Email
16 Files + Comments
17 Search + Views + Bulk
18 Import/Export
19 Reporting
20 Optional Customer Module
21 Resource Foundation
22 PWA/Offline Foundation
23 Billing/Subscriptions
24 Automations
25 API/Webhooks
26 Tenant Customization
27 Advanced Operations
```

Do not move Billing, AI, automation, or advanced reporting ahead of the
core employee work loop.

------------------------------------------------------------------------

# 39. Definition of Done for Every Feature

A feature is not complete until:

-   database migration exists where required,
-   backend tenant scoping is enforced,
-   backend authorization is enforced,
-   validation exists,
-   API behavior is documented,
-   frontend success state exists,
-   frontend loading state exists,
-   frontend empty state exists where relevant,
-   frontend error state exists,
-   realtime implications are handled,
-   audit implications are handled,
-   undo/delete implications are considered,
-   automated tests cover critical behavior,
-   responsive behavior is checked,
-   no industry-specific assumption was introduced.

------------------------------------------------------------------------

# 40. First Coding Sprint --- Exact Agent Instructions

The agent should begin here.

## Step 1

Create the monorepo and local Docker development environment.

## Step 2

Create PostgreSQL migration infrastructure.

## Step 3

Implement `users`, `organizations`, `organization_memberships`,
`workspaces`, and `workspace_memberships`.

## Step 4

Implement authentication and tenant-context middleware.

## Step 5

Write tenant-isolation integration tests **before proceeding**.

## Step 6

Implement roles, permissions, and membership role assignment.

## Step 7

Create the initial SvelteKit shell:

``` text
Login
Organization switcher
Workspace switcher
Sidebar
Top navigation
Account menu
```

Keep the UI minimal and polished.

## Step 8

Implement Projects.

## Step 9

Implement recursive Sections with cycle prevention.

## Step 10

Implement Cards.

## Step 11

Build Project → Section → Card navigation using both tree and
card-oriented views.

## Step 12

Stop and verify:

``` text
Can two organizations coexist safely?
Can a user switch organizations?
Can a user switch workspaces?
Can permissions block unauthorized mutations?
Can a project contain infinitely nested sections in the model?
Can cards be created/moved/soft-deleted/restored?
Does the UI remain simple?
Are all tests green?
```

Only after all answers are yes should the agent start the Dynamic
Property Engine.

------------------------------------------------------------------------

# 41. Product Principle to Preserve Throughout Development

> **Simple when you start; powerful when you need it.**

The system should be capable of complex workflows, but the employee
should usually experience:

``` text
My Task
→ Start
→ Pause if necessary
→ Resume
→ Complete
```

The manager should gain the complexity only when needed.

The TV should answer one question instantly:

> **What work is active right now, who is doing it, and for how long?**

Every architecture and UX decision should be evaluated against these
principles.

---

# 42. Localization / Internationalization (i18n)

Internationalization is a foundation requirement, not a later retrofit.

## Language policy

- The initial and default product language is **Turkish (`tr-TR`)**.
- English (`en`) must also be supported from the initial architecture.
- Additional languages must be addable without changing domain logic or rebuilding screen structure.
- Never hard-code user-facing text directly inside UI components.
- All user-facing strings must use translation keys.

Example:

```text
task.start
task.pause
task.resume
task.complete
```

Turkish:

```text
Başlat
Duraklat
Devam Et
Tamamla
```

English:

```text
Start
Pause
Resume
Complete
```

## Language resolution

Use this preference order:

```text
User language preference
→ Organization default language
→ Platform default: tr-TR
```

## Separate language, terminology and locale

These are different concerns:

```text
Language:
Turkish / English / ...

Terminology:
Card → Kart / İş Emri / Sipariş / Job
Process → Süreç / İş Akışı / Workflow

Locale:
date format
number format
currency
timezone
week start
```

Tenant terminology customization must not rename internal database/domain concepts.

## Formatting

Use locale-aware formatting for:

- dates,
- times,
- numbers,
- decimal separators,
- currency,
- percentages,
- durations where localized labels are used.

Persist timestamps in UTC and render in user/tenant timezone.

## Realtime / notifications / email

Realtime events carry machine-readable event types and data, not pre-translated UI sentences.

Notifications and email are rendered in the recipient's resolved language.

## Acceptance

V1 must be usable completely in Turkish. Switching to English must change all core navigation, employee task actions, system statuses, validation messages, notification text, and core email templates without page-specific hard-coded replacements.
