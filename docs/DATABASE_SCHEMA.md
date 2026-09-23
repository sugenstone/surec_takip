# DATABASE_SCHEMA.md

## Multi-Tenant Workflow & Operations Platform

> Companion to `AI_CODING_AGENT_MASTER_PLAN.md`. This document defines
> the target relational model and migration rules. Implement tables only
> when their corresponding phase is reached; do not create the whole
> future schema prematurely.

## 1. Database conventions

-   PostgreSQL is the source of truth.
-   Use UUIDv7 for primary IDs where supported by the application layer.
-   Persist timestamps as `timestamptz` in UTC.
-   Use `snake_case`.
-   Tenant-owned rows must contain `tenant_id` unless ownership is
    unambiguously inherited and every access path still enforces tenant
    scope.
-   Prefer explicit foreign keys and unique constraints.
-   Use soft delete (`deleted_at`) for recoverable business entities.
-   Do not use JSONB as a replacement for relational modeling. JSONB is
    appropriate for flexible settings, event payloads, provider
    metadata, and version snapshots.
-   Money must never use floating point. Store minor units
    (`amount_minor bigint`) plus ISO currency, or use a deliberate
    numeric strategy.
-   Durations should be derived from timestamps where possible; cached
    duration fields must have a clear source of truth.
-   Every migration must be reversible when practical and safe for
    existing data.
-   Never trust a client-supplied `tenant_id`; derive tenant context
    from authenticated membership.

## 2. Common column pattern

Typical tenant entity:

``` sql
id uuid primary key,
tenant_id uuid not null,
created_at timestamptz not null default now(),
updated_at timestamptz not null default now(),
deleted_at timestamptz null,
created_by uuid null,
updated_by uuid null
```

Not every table needs every audit column. Join/event tables may use a
smaller set.

## 3. Identity and tenancy

### users

``` text
id uuid PK
email citext UNIQUE NOT NULL
password_hash text NULL
display_name text NOT NULL
email_verified_at timestamptz NULL
status text NOT NULL
locale text NULL
timezone text NULL
created_at
updated_at
```

### sessions

``` text
id uuid PK
user_id FK users
token_hash text UNIQUE
expires_at
revoked_at NULL
ip_metadata jsonb NULL
user_agent text NULL
created_at
```

### organizations

`organizations.id` is the canonical tenant ID.

``` text
id uuid PK
name text NOT NULL
slug citext UNIQUE
status text NOT NULL
default_timezone text NOT NULL
default_locale text NULL
default_currency char(3) NULL
settings jsonb NOT NULL default {}
created_at
updated_at
deleted_at
```

### organization_memberships

``` text
id uuid PK
tenant_id FK organizations
user_id FK users
status text NOT NULL
joined_at NULL
created_at
updated_at
deleted_at timestamptz NULL
UNIQUE (tenant_id, user_id)
```

Membership `status` values are `active` and `deleted` (soft delete). The
UNIQUE constraint is intentionally hard rather than partial: at most one
membership row per (tenant, user) ever exists, including soft-deleted rows;
rejoining reactivates the same row instead of inserting a duplicate, which
makes duplicate active memberships impossible under concurrency. Deleted
memberships and deleted organizations never grant visibility.

### workspaces

``` text
id uuid PK
tenant_id FK organizations
name text NOT NULL
slug citext NOT NULL
settings jsonb default {}
created_at
updated_at
deleted_at
UNIQUE (tenant_id, slug)
```

### workspace_memberships

``` text
id uuid PK
tenant_id FK organizations
workspace_id composite FK -> workspaces(tenant_id, id)
user_id FK users
status text NOT NULL
created_at
updated_at
deleted_at timestamptz NULL
UNIQUE (workspace_id, user_id)
```

Workspace membership `status` values are `active` and `deleted` (soft
delete), mirroring organization memberships. The UNIQUE constraint is hard
(partial değil): one membership row per (workspace, user) ever exists and
rejoining reactivates that row (ADR 0006/0007). The composite foreign key
`(tenant_id, workspace_id)` referencing `workspaces (tenant_id, id)` makes a
cross-tenant membership row impossible to store: the claimed tenant must
match the workspace's actual tenant. Whether the user is still a valid
organization member cannot be expressed by constraints; every access path
therefore joins organization membership validity at read time, so a stale
workspace membership grants nothing once the organization membership is
gone or the organization is deleted.

Membership authorization requires both an active status and `deleted_at IS NULL`.
Soft deletion must never grant access. The existing uniqueness constraints remain:
rejoining/restoration operates on the same membership record, subject to authorized
commands and audit, rather than creating duplicate membership identities.

## 4. Teams, roles and permissions

### teams

``` text
id uuid PK
tenant_id
workspace_id NULL
name
description NULL
created_at
updated_at
deleted_at
```

### team_members

``` text
id uuid PK
tenant_id
team_id
user_id
created_at
UNIQUE (team_id, user_id)
```

### roles

``` text
id uuid PK
tenant_id NULL       # NULL only for immutable platform-provided role definitions if used
workspace_id NULL
name
description NULL
is_system boolean
created_at
updated_at
deleted_at
```

### permissions

Global catalog:

``` text
id uuid PK
key text UNIQUE       # cards:update, tasks:start, billing:manage
description text
```

### role_permissions

``` text
role_id
permission_id
scope text            # organization/workspace/team/self/assigned_projects
PRIMARY KEY (role_id, permission_id, scope)
```

### membership_roles

Assign roles to organization/workspace membership context:

``` text
id uuid PK
tenant_id            # composite FK -> roles(tenant_id, id)
user_id              # composite FK -> organization_memberships(tenant_id, user_id)
role_id
workspace_id NULL    # composite FK -> workspaces(tenant_id, id); NULL = org-wide
created_at
```

Assignments are physically deleted when the membership is deactivated
(ADR 0008): reactivation never resurrects old privileges. The
`(tenant_id, user_id)` FK prevents assigning roles to non-members, and
authorization always joins ACTIVE memberships as defense in depth.

## 5. Projects, sections and cards

### projects

Implemented by migration `007_projects` (ADR 0011). `slug` replaces the
earlier `public_code` concept and reuses the shared slug normalization
(citext, parent-scoped uniqueness); `status` is the deliberately small V1
lifecycle (`active | completed | archived`, DB CHECK). Future columns
(priority/starts_at/due_at/revision/created_by/updated_by) arrive with their
own phases; adding them later is a non-breaking extension.

``` text
id uuid PK
tenant_id
workspace_id
name text (1..200)
slug citext (1..64)
description text NULL
status text NOT NULL default 'active'
  CHECK (status IN ('active','completed','archived'))
created_at
updated_at
deleted_at
FOREIGN KEY (tenant_id, workspace_id)
  REFERENCES workspaces (tenant_id, id)
UNIQUE (workspace_id, slug)          -- hard, case-insensitive (citext);
                                     -- workspace_id is globally unique, so
                                     -- this implies per-tenant uniqueness
INDEX (workspace_id, created_at DESC, id)  -- list hot path
```

### sections

Use adjacency-list hierarchy initially.

``` text
id uuid PK
tenant_id
project_id
parent_id FK sections NULL
name
description NULL
position numeric/int
revision bigint default 1
created_at
updated_at
deleted_at
```

Rules: - parent must belong to same tenant/project. - prevent
self-parenting and descendant cycles in application/domain logic. - add
indexes for `(tenant_id, project_id, parent_id, position)`. - if
recursive queries later become a proven bottleneck, evaluate closure
table/materialized path; do not prematurely add both.

### cards

``` text
id uuid PK
tenant_id
project_id
section_id NULL
public_code text
title
description NULL
system_status text
priority text
due_at NULL
revision bigint default 1
created_at
updated_at
deleted_at
created_by
updated_by
UNIQUE (tenant_id, public_code)
```

### card_relations

``` text
id uuid PK
tenant_id
source_card_id
target_card_id
relation_type text   # parent_child/related/blocks/etc.
created_at
created_by
UNIQUE (source_card_id, target_card_id, relation_type)
```

## 6. Dynamic property engine

### property_definitions

``` text
id uuid PK
tenant_id
workspace_id NULL
key text
name text
data_type text
description NULL
settings jsonb default {}
is_required boolean default false
is_archived boolean default false
created_at
updated_at
UNIQUE (tenant_id, workspace_id, key)
```

`data_type` supports:

``` text
short_text long_text integer decimal money percentage boolean
date datetime duration single_select multi_select user team
external_organization phone email url file image relation
```

### property_options

``` text
id uuid PK
tenant_id
property_definition_id
label
value
position
is_archived
UNIQUE (property_definition_id, value)
```

### property_values

Use typed nullable columns rather than a single opaque value.

``` text
id uuid PK
tenant_id
property_definition_id
entity_type text
entity_id uuid
text_value NULL
integer_value NULL
decimal_value NULL
boolean_value NULL
date_value NULL
datetime_value NULL
uuid_value NULL
json_value NULL
created_at
updated_at
UNIQUE (property_definition_id, entity_type, entity_id)
```

For multi-select/relation types, use normalized child tables if
query/reporting requirements justify it rather than packing large arrays
into JSON.

## 7. Processes and process steps

### processes

``` text
id uuid PK
tenant_id
card_id
name
description NULL
system_status
position
estimated_duration_seconds NULL
template_version_id NULL
revision bigint default 1
created_at
updated_at
deleted_at
```

### process_steps

``` text
id uuid PK
tenant_id
process_id
name
description NULL
system_status
position
priority
estimated_duration_seconds NULL
default_assignee_type NULL   # user/team
default_assignee_id NULL
completion_policy jsonb default {}
revision bigint default 1
created_at
updated_at
deleted_at
```

Canonical status values should be application constants:

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

User-defined labels map onto canonical states; they do not replace them.

## 8. Dependencies

### process_dependencies

``` text
id uuid PK
tenant_id
process_id
depends_on_process_id
requirement text       # completed/approved/etc.
mode text              # hard_block/warn_only
created_at
UNIQUE (process_id, depends_on_process_id)
```

### process_step_dependencies

``` text
id uuid PK
tenant_id
process_step_id
depends_on_step_id
requirement text
mode text
created_at
UNIQUE (process_step_id, depends_on_step_id)
```

If ALL/ANY grouping is needed, add `dependency_groups` and group members
rather than encoding complex Boolean logic into strings.

Application rules: - same tenant. - no self-dependency. - no cycles. -
dependency changes on active workflows require explicit
validation/audit.

## 9. Assignments

### assignments

``` text
id uuid PK
tenant_id
process_step_id
assignee_type text      # user/team; future resource
assignee_id uuid
is_primary boolean
assigned_by uuid
assigned_at
ended_at NULL
created_at
```

Keep assignment history instead of overwriting old assignments.

## 10. Time tracking

### time_sessions

``` text
id uuid PK
tenant_id
process_step_id
employee_id
started_at timestamptz
stopped_at timestamptz NULL
stop_reason_id NULL
corrected_from_id NULL
created_at
created_by
```

Constraints/rules: - `stopped_at >= started_at`. - active session means
`stopped_at IS NULL`. - enforce single active task per employee when
tenant policy requires it. - use a partial unique index where compatible
with policy or enforce transactionally. - never update this table every
second. - corrections must be audited.

### stop_reasons

``` text
id uuid PK
tenant_id
workspace_id NULL
name
category NULL
is_active
position
```

## 11. Checklists, forms and approvals

### checklists

``` text
id uuid PK
tenant_id
process_step_id
title
created_at
```

### checklist_items

``` text
id uuid PK
tenant_id
checklist_id
label
position
is_required
completed_at NULL
completed_by NULL
```

### forms

``` text
id uuid PK
tenant_id
workspace_id NULL
name
version
status
created_at
updated_at
```

### form_fields

``` text
id uuid PK
tenant_id
form_id
field_type
label
settings jsonb
position
required
```

### form_submissions

``` text
id uuid PK
tenant_id
form_id
process_step_id NULL
submitted_by
payload jsonb
submitted_at
```

Form definitions should be structured; submission payload may use JSONB
with server validation against the form version.

### approvals

``` text
id uuid PK
tenant_id
process_step_id
requested_by
approver_type
approver_id
status
decision_by NULL
decision_at NULL
reason NULL
created_at
```

## 12. Templates and versioning

### card_templates

``` text
id uuid PK
tenant_id
workspace_id NULL
name
status
current_version integer
created_at
updated_at
deleted_at
```

### card_template_versions

``` text
id uuid PK
tenant_id
card_template_id
version integer
snapshot jsonb
created_at
created_by
UNIQUE (card_template_id, version)
```

### process_templates / process_template_versions

Same versioned pattern.

Published versions are immutable. Editing a published template creates a
new draft/version.

Live process/card instances must not unexpectedly mutate when templates
change.

## 13. Comments, mentions and files

### comments

``` text
id uuid PK
tenant_id
entity_type
entity_id
author_id
parent_comment_id NULL
body
created_at
updated_at
deleted_at
```

### mentions

``` text
id uuid PK
tenant_id
comment_id
mentioned_user_id
created_at
UNIQUE (comment_id, mentioned_user_id)
```

### files

``` text
id uuid PK
tenant_id
storage_provider
object_key
original_filename
mime_type
size_bytes
sha256 NULL
uploaded_by
created_at
deleted_at
```

### file_links

``` text
id uuid PK
tenant_id
file_id
entity_type
entity_id
purpose NULL
created_at
```

Do not store large file bytes in PostgreSQL.

## 14. Notifications

### notifications

``` text
id uuid PK
tenant_id
recipient_user_id
event_type
title
body
entity_type NULL
entity_id NULL
payload jsonb
read_at NULL
dismissed_at NULL
created_at
```

### notification_preferences

``` text
id uuid PK
tenant_id
user_id
event_type
in_app_enabled
email_enabled
digest_mode NULL
UNIQUE (tenant_id, user_id, event_type)
```

### notification_rules

``` text
id uuid PK
tenant_id
workspace_id NULL
name
event_type
conditions jsonb
recipients jsonb
channels jsonb
enabled
created_at
updated_at
```

## 15. Audit, versioning and outbox

### audit_events

Append-only.

``` text
id uuid PK
tenant_id
workspace_id NULL
actor_user_id NULL
entity_type
entity_id
action
before_data jsonb NULL
after_data jsonb NULL
reason NULL
request_id NULL
created_at
```

Do not soft-delete audit events through normal product operations.

### entity_versions

Use only for entities that need restoreable historical snapshots.

``` text
id uuid PK
tenant_id
entity_type
entity_id
version
snapshot jsonb
created_by
created_at
UNIQUE (entity_type, entity_id, version)
```

### outbox_events

``` text
id uuid PK
tenant_id
event_type
aggregate_type
aggregate_id
payload jsonb
occurred_at
available_at
processed_at NULL
attempt_count default 0
last_error NULL
```

State change and outbox insert must occur in the same PostgreSQL
transaction.

## 16. Saved views

### saved_views

``` text
id uuid PK
tenant_id
workspace_id
owner_user_id NULL
name
entity_type
view_type
filters jsonb
sorting jsonb
columns jsonb
visibility text
created_at
updated_at
```

## 17. Billing

### plans

Platform-level:

``` text
id uuid PK
code UNIQUE
name
status
billing_metadata jsonb
created_at
updated_at
```

### plan_features

``` text
id uuid PK
plan_id
feature_key
enabled
limit_value bigint NULL
config jsonb NULL
UNIQUE (plan_id, feature_key)
```

### subscriptions

``` text
id uuid PK
tenant_id UNIQUE
plan_id
provider
provider_customer_id NULL
provider_subscription_id NULL
status
billing_cycle
trial_ends_at NULL
current_period_start NULL
current_period_end NULL
grace_ends_at NULL
created_at
updated_at
```

### entitlements

Materialized/overridden effective tenant entitlement:

``` text
id uuid PK
tenant_id
feature_key
enabled
limit_value bigint NULL
source text
valid_until NULL
UNIQUE (tenant_id, feature_key)
```

### usage_counters

``` text
id uuid PK
tenant_id
metric_key
period_start
period_end
value bigint
updated_at
UNIQUE (tenant_id, metric_key, period_start, period_end)
```

### invoices / payments / billing_events

Store normalized business fields plus provider IDs/metadata. Provider
webhooks must be idempotent.

## 18. Automations and webhooks

### automation_rules

``` text
id uuid PK
tenant_id
workspace_id NULL
name
event_type
conditions jsonb
actions jsonb
enabled
created_at
updated_at
```

### automation_runs

``` text
id uuid PK
tenant_id
automation_rule_id
source_event_id
status
result jsonb
started_at
finished_at NULL
UNIQUE (automation_rule_id, source_event_id)
```

### api_keys

``` text
id uuid PK
tenant_id
name
key_prefix
key_hash
permissions jsonb
last_used_at NULL
expires_at NULL
revoked_at NULL
created_at
```

Never store raw API keys after creation.

### webhook_endpoints

``` text
id uuid PK
tenant_id
url
secret_hash/encrypted_secret
subscribed_events jsonb
enabled
created_at
updated_at
```

### webhook_deliveries

``` text
id uuid PK
tenant_id
webhook_endpoint_id
event_id
attempt
status
http_status NULL
response_excerpt NULL
next_attempt_at NULL
created_at
```

## 19. Indexing baseline

At minimum evaluate indexes for:

``` text
(tenant_id, id)
(tenant_id, workspace_id)
(tenant_id, project_id)
(tenant_id, project_id, parent_id)
(tenant_id, section_id)
(tenant_id, system_status)
(tenant_id, due_at)
(process_id, position)
(process_step_id)
(employee_id, stopped_at)
(recipient_user_id, read_at)
(outbox_events.processed_at, available_at)
```

Use partial indexes for active/non-deleted rows where query patterns
justify them.

## 20. Deletion rules

Default behavior: - Organization deletion is a controlled lifecycle, not
a simple row delete. - Project/section/card/process/step: soft delete. -
Time sessions: preserve; corrections are explicit. - Audit events:
preserve according to retention policy. - Files: soft unlink first;
physical object deletion may occur asynchronously after retention. -
Billing records: retain as legally/operationally required. - Templates:
archive instead of destructive delete when referenced.

## 21. Concurrency

Entities commonly edited by multiple admins should carry
`revision bigint`.

Update pattern:

``` sql
UPDATE cards
SET title = $1, revision = revision + 1
WHERE id = $2
  AND tenant_id = $3
  AND revision = $4;
```

Zero updated rows indicates conflict or absence; distinguish securely.

## 22. Migration order for V1

Phase 1 infrastructure uses a reversible SQLx migration to install `citext`,
required by the identity schema. It creates no domain tables. Applied versions
and checksums are tracked in SQLx's `_sqlx_migrations` table. Embedded migrations
are rebuilt when the migrations directory changes. Never modify an applied
migration: append a new version. Rollback must not use CASCADE to erase dependent
business data; unsafe reversals require a forward corrective migration.

Implement domain migrations in this dependency order. These are logical steps,
not reserved migration filenames; infrastructure migrations may precede them.
Audit and outbox persistence must exist before the first mutation requiring them,
as mandated by AGENTS.md. Their UI/dispatch features remain in their planned phases.

``` text
001 users + sessions
002 organizations + organization_memberships
003 workspaces + workspace_memberships
004 audit_events + outbox_events (before audited membership/role/business mutations)
005 teams
006 roles + permissions
007 projects
008 sections
009 cards + card_relations
010 dynamic properties
011 processes + process_steps
012 dependencies
013 assignments
014 time_sessions + stop_reasons
015 template foundations
016 notifications
017 files
018 billing entitlement foundation
```

Do not create V2/V3 tables until their features are being implemented.

## 23. Database acceptance tests

Before V1 is considered complete, automated tests must prove:

-   cross-tenant IDs cannot be used to read or mutate another tenant;
-   recursive sections cannot create cycles;
-   dependencies cannot create cycles;
-   completing prerequisites unlocks eligible steps transactionally;
-   time sessions calculate active duration correctly;
-   duplicate retries do not create duplicate completion/events;
-   outbox event and business state commit atomically;
-   template updates do not mutate existing running instances;
-   soft-deleted entities are hidden from normal queries;
-   permission changes are respected;
-   billing entitlement lookup is tenant-specific;
-   concurrent updates do not silently overwrite protected state.

------------------------------------------------------------------------

## Final database rule

> The schema exists to protect business truth. UI convenience must never
> weaken tenant isolation, historical accuracy, time tracking,
> dependency integrity, or auditability.

---

# 24. Localization / Locale Persistence

Localization settings belong to preferences/configuration, not translated database column names.

Existing/foundation fields should support:

```text
users.locale
users.timezone

organizations.default_locale
organizations.default_timezone
organizations.default_currency
organizations.settings
```

Recommended organization settings include:

```json
{
  "week_start": "monday",
  "terminology": {}
}
```

`organizations.default_locale` is the single organization language preference;
do not duplicate it as `settings.default_language`.

If per-user preference grows beyond core user columns, introduce a user preferences table rather than repeatedly expanding unrelated business tables.

Do not store translated copies of canonical system states such as `active`, `paused`, or `completed`. Persist canonical codes and translate them at presentation time.

Tenant terminology overrides are configuration data. They must not alter table names, API field names, permission keys, event types, or canonical status values.
