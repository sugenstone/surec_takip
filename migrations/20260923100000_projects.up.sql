-- Canonical projects (STEP 17, ADR 0011). A project belongs to EXACTLY ONE
-- workspace of EXACTLY ONE organization (tenant root): the composite FK
-- (tenant_id, workspace_id) -> workspaces(tenant_id, id) makes a cross-tenant
-- project row impossible to store at all, mirroring workspace_memberships.
CREATE TABLE projects (
  id uuid PRIMARY KEY,
  tenant_id uuid NOT NULL REFERENCES organizations (id),
  workspace_id uuid NOT NULL,
  name text NOT NULL,
  slug citext NOT NULL,
  description text NULL,
  -- Deliberately small V1 lifecycle (ADR 0011); soft deletion stays separate.
  status text NOT NULL DEFAULT 'active',
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz NULL,
  -- workspace_id is globally unique (workspaces.id uuid PK), so slug
  -- uniqueness per workspace implies per tenant; citext gives the
  -- case-insensitive comparison for free.
  CONSTRAINT projects_workspace_slug_key UNIQUE (workspace_id, slug),
  CONSTRAINT projects_tenant_workspace_fk
    FOREIGN KEY (tenant_id, workspace_id) REFERENCES workspaces (tenant_id, id),
  CONSTRAINT projects_name_length CHECK (char_length(name) BETWEEN 1 AND 200),
  CONSTRAINT projects_slug_length CHECK (char_length(slug::text) BETWEEN 1 AND 64),
  CONSTRAINT projects_status_check CHECK (status IN ('active', 'completed', 'archived'))
);

-- Hot path: the workspace project list filters deleted rows and orders by
-- creation (newest first), then id for deterministic pagination.
CREATE INDEX projects_workspace_created_idx ON projects (workspace_id, created_at DESC, id);

-- ---------------------------------------------------------------------------
-- Permission catalog extension (stable machine keys, real enforcement points
-- only, STEP 17).
-- ---------------------------------------------------------------------------
INSERT INTO permissions (id, key, description) VALUES
  (gen_random_uuid(), 'projects:create', 'Create projects inside a workspace'),
  (gen_random_uuid(), 'projects:update', 'Update project content and lifecycle'),
  (gen_random_uuid(), 'projects:archive', 'Transition a project into the archived status');

-- ---------------------------------------------------------------------------
-- Existing built-in Owner roles receive the new grants at organization scope
-- (the step 13 security decision, ADR 0008): extending the Owner GRANT LIST
-- through migration is allowed; assigning the Owner ROLE to users is NOT.
-- No INSERT into membership_roles happens here.
-- ---------------------------------------------------------------------------
INSERT INTO role_permissions (role_id, permission_id, scope)
SELECT r.id, p.id, 'organization'
FROM roles r
JOIN permissions p
  ON p.key IN ('projects:create', 'projects:update', 'projects:archive')
WHERE r.is_system AND r.name = 'owner' AND r.deleted_at IS NULL;
