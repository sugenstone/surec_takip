-- Canonical RBAC model (DATABASE_SCHEMA.md §4). Authorization evaluates
-- PERMISSIONS; role names/labels are never authorization inputs.
CREATE TABLE permissions (
  id uuid PRIMARY KEY,
  key text NOT NULL,
  description text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT permissions_key_key UNIQUE (key)
);

-- Tenant-owned roles (workspace_id reserved for future workspace-specific
-- custom roles; NULL = organization-level role). is_system marks seeded
-- built-in roles; their stable lowercase names are identifiers, not
-- translations.
CREATE TABLE roles (
  id uuid PRIMARY KEY,
  tenant_id uuid NOT NULL REFERENCES organizations (id),
  workspace_id uuid NULL,
  name text NOT NULL,
  description text NULL,
  is_system boolean NOT NULL DEFAULT false,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz NULL,
  CONSTRAINT roles_tenant_name_key UNIQUE (tenant_id, name),
  -- Composite FK target so membership_roles cannot claim a role of another
  -- tenant while asserting this one.
  CONSTRAINT roles_tenant_id_key UNIQUE (tenant_id, id),
  -- A workspace-specific role must belong to the same tenant as its
  -- workspace (NULL workspace_id = organization-level role, FK skipped).
  CONSTRAINT roles_tenant_workspace_fk
    FOREIGN KEY (tenant_id, workspace_id) REFERENCES workspaces (tenant_id, id)
);

-- Permission grants per role; scope declares the authority class
-- ('organization' | 'workspace'; further canonical scopes arrive with their
-- domains). A permission may be granted at several scopes.
CREATE TABLE role_permissions (
  role_id uuid NOT NULL REFERENCES roles (id),
  permission_id uuid NOT NULL REFERENCES permissions (id),
  scope text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT role_permissions_pk PRIMARY KEY (role_id, permission_id, scope),
  CONSTRAINT role_permissions_scope_check CHECK (scope IN ('organization', 'workspace'))
);

-- Role assignments. workspace_id NULL = organization-wide assignment (its
-- grants apply across the tenant, including workspaces); non-NULL =
-- workspace-scoped assignment and MUST belong to the same tenant (composite
-- FK). (tenant_id, user_id) references the membership row, so an assignment
-- can only target a real membership; authorization additionally requires the
-- membership to be ACTIVE at evaluation time (defense in depth).
-- Lifecycle: assignments are physically deleted on membership deactivation
-- (ADR 0008) — reactivation never resurrects old privileges.
CREATE TABLE membership_roles (
  id uuid PRIMARY KEY,
  tenant_id uuid NOT NULL,
  user_id uuid NOT NULL,
  role_id uuid NOT NULL,
  workspace_id uuid NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT membership_roles_tenant_role_fk
    FOREIGN KEY (tenant_id, role_id) REFERENCES roles (tenant_id, id),
  CONSTRAINT membership_roles_tenant_workspace_fk
    FOREIGN KEY (tenant_id, workspace_id) REFERENCES workspaces (tenant_id, id),
  CONSTRAINT membership_roles_membership_fk
    FOREIGN KEY (tenant_id, user_id)
    REFERENCES organization_memberships (tenant_id, user_id),
  -- Pre-grant prevention: a workspace-scoped assignment can only exist for a
  -- user who actually holds that workspace membership (NULL workspace_id =
  -- organization-wide assignment, this FK is skipped).
  CONSTRAINT membership_roles_ws_membership_fk
    FOREIGN KEY (workspace_id, user_id)
    REFERENCES workspace_memberships (workspace_id, user_id)
);

-- One assignment per (tenant, user, role, workspace-scope). PostgreSQL
-- UNIQUE treats NULLs as distinct, so organization-wide assignments (NULL
-- workspace) get their own partial uniqueness.
CREATE UNIQUE INDEX membership_roles_org_unique
  ON membership_roles (tenant_id, user_id, role_id)
  WHERE workspace_id IS NULL;
CREATE UNIQUE INDEX membership_roles_workspace_unique
  ON membership_roles (tenant_id, user_id, role_id, workspace_id)
  WHERE workspace_id IS NOT NULL;
CREATE INDEX membership_roles_user_idx ON membership_roles (user_id);
CREATE INDEX membership_roles_tenant_user_idx ON membership_roles (tenant_id, user_id);

-- ---------------------------------------------------------------------------
-- Initial permission catalog (stable machine keys; extend only with real
-- enforcement).
-- ---------------------------------------------------------------------------
INSERT INTO permissions (id, key, description) VALUES
  (gen_random_uuid(), 'workspaces:create', 'Create workspaces inside the organization');

-- ---------------------------------------------------------------------------
-- Backfill for organizations that predate RBAC (ADR 0006 convention: the
-- earliest ACTIVE membership per organization is the deterministic bootstrap
-- owner). No guessing beyond documented convention; every other member keeps
-- membership eligibility without privileges.
-- ---------------------------------------------------------------------------
INSERT INTO roles (id, tenant_id, name, description, is_system)
SELECT gen_random_uuid(), o.id, 'owner',
       'Built-in owner role (all current tenant management permissions)', true
FROM organizations o;

INSERT INTO roles (id, tenant_id, name, description, is_system)
SELECT gen_random_uuid(), o.id, 'member',
       'Built-in member role (no additional permissions yet)', true
FROM organizations o;

INSERT INTO role_permissions (role_id, permission_id, scope)
SELECT r.id, p.id, 'organization'
FROM roles r
CROSS JOIN permissions p
WHERE r.is_system AND r.name = 'owner' AND p.key = 'workspaces:create';

-- NO automatic Owner backfill: the schema holds no authoritative creator
-- record, and "earliest active membership" is a guess that imported data can
-- violate (privilege escalation). Pre-RBAC organizations receive the built-in
-- roles and grants, and require an EXPLICIT trusted bootstrap via
-- `npm run user:grant-owner -- <organization_id> <email>` (ADR 0008).
