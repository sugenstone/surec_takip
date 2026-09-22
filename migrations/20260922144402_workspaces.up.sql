-- Canonical workspaces (DATABASE_SCHEMA.md §3). Workspaces are NOT tenants:
-- every row belongs to exactly one organization (tenant root) via tenant_id.
CREATE TABLE workspaces (
  id uuid PRIMARY KEY,
  tenant_id uuid NOT NULL REFERENCES organizations (id),
  name text NOT NULL,
  slug citext NOT NULL,
  settings jsonb NOT NULL DEFAULT '{}',
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz NULL,
  -- Slug uniqueness is per tenant: two organizations may both use "production".
  CONSTRAINT workspaces_tenant_slug_key UNIQUE (tenant_id, slug),
  -- Composite FK target: lets workspace_memberships prove at the DB level that
  -- its tenant_id matches the tenant of the workspace it references.
  CONSTRAINT workspaces_tenant_id_key UNIQUE (tenant_id, id),
  CONSTRAINT workspaces_name_length CHECK (char_length(name) BETWEEN 1 AND 200),
  CONSTRAINT workspaces_slug_length CHECK (char_length(slug::text) BETWEEN 1 AND 64)
);

-- Canonical workspace_memberships plus deleted_at (canonical soft-delete
-- convention; lifecycle mirrors organization_memberships, see ADR 0006/0007).
-- The composite FK (tenant_id, workspace_id) -> workspaces(tenant_id, id)
-- makes a cross-tenant membership row (user of tenant A attached to a
-- workspace of tenant B) impossible to store at all.
CREATE TABLE workspace_memberships (
  id uuid PRIMARY KEY,
  tenant_id uuid NOT NULL REFERENCES organizations (id),
  workspace_id uuid NOT NULL,
  user_id uuid NOT NULL REFERENCES users (id),
  status text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz NULL,
  CONSTRAINT workspace_memberships_workspace_user_key UNIQUE (workspace_id, user_id),
  CONSTRAINT workspace_memberships_tenant_workspace_fk
    FOREIGN KEY (tenant_id, workspace_id) REFERENCES workspaces (tenant_id, id)
);

-- Hot path: resolve a user's workspaces per organization on every request.
CREATE INDEX workspace_memberships_user_id_idx ON workspace_memberships (user_id);
CREATE INDEX workspace_memberships_tenant_id_idx ON workspace_memberships (tenant_id);
