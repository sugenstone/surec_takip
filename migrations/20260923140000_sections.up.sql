-- Canonical sections (STEP 18, ADR 0012). ONE generic recursive entity:
-- "Block", "Floor", "Apartment" etc. are user-chosen names, never types.
-- Adjacency list (parent_section_id) with composite-FK tenant/project
-- integrity, mirroring the workspace/project patterns from 004/007.

-- Composite FK target: lets sections prove at the DB level that their
-- (tenant, workspace, project) triple matches the project row it claims.
ALTER TABLE projects
  ADD CONSTRAINT projects_tenant_workspace_id_key UNIQUE (tenant_id, workspace_id, id);

CREATE TABLE sections (
  id uuid PRIMARY KEY,
  tenant_id uuid NOT NULL REFERENCES organizations (id),
  workspace_id uuid NOT NULL,
  project_id uuid NOT NULL,
  parent_section_id uuid NULL,
  name text NOT NULL,
  slug citext NOT NULL,
  -- Sibling ordering; NOT unique per (parent, position) by design (ADR 0012):
  -- ordering stays deterministic via (position, id) and concurrent appends
  -- cannot corrupt state. Drag/drop and bulk generation reuse this primitive.
  position integer NOT NULL,
  -- Structural lifecycle only: sections never "complete" (no work yet).
  status text NOT NULL DEFAULT 'active',
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz NULL,
  -- Composite FK target for the self-reference below.
  CONSTRAINT sections_tenant_workspace_project_id_key
    UNIQUE (tenant_id, workspace_id, project_id, id),
  -- Project integrity: a section can only be stored under the project it
  -- claims, within that project's real tenant/workspace pair.
  CONSTRAINT sections_project_fk
    FOREIGN KEY (tenant_id, workspace_id, project_id)
    REFERENCES projects (tenant_id, workspace_id, id),
  -- Parent integrity: a stored parent ALWAYS carries the SAME
  -- tenant/workspace/project triple as the child, so cross-project and
  -- cross-tenant parenting is impossible at the storage layer.
  CONSTRAINT sections_parent_fk
    FOREIGN KEY (tenant_id, workspace_id, project_id, parent_section_id)
    REFERENCES sections (tenant_id, workspace_id, project_id, id),
  CONSTRAINT sections_name_length CHECK (char_length(name) BETWEEN 1 AND 200),
  CONSTRAINT sections_slug_length CHECK (char_length(slug::text) BETWEEN 1 AND 64),
  CONSTRAINT sections_position_check CHECK (position >= 0),
  CONSTRAINT sections_status_check CHECK (status IN ('active', 'archived'))
);

-- Slug uniqueness is SIBLING-scoped (ADR 0012): "Apartment 1" may exist under
-- Floor 1 and Floor 2. PostgreSQL UNIQUE treats NULLs as distinct, so roots
-- and children get separate partial unique indexes (005 membership_roles
-- pattern). Hard uniqueness: soft deletion does not free a sibling slug.
CREATE UNIQUE INDEX sections_child_slug_unique
  ON sections (project_id, parent_section_id, slug)
  WHERE parent_section_id IS NOT NULL;
CREATE UNIQUE INDEX sections_root_slug_unique
  ON sections (project_id, slug)
  WHERE parent_section_id IS NULL;

-- Hot path: one flat ordered query per project tree (query count = 1).
CREATE INDEX sections_project_order_idx
  ON sections (project_id, parent_section_id, position, id);

-- ---------------------------------------------------------------------------
-- Permission catalog extension (real enforcement points only, STEP 18).
-- ---------------------------------------------------------------------------
INSERT INTO permissions (id, key, description) VALUES
  (gen_random_uuid(), 'sections:create', 'Create sections inside a project'),
  (gen_random_uuid(), 'sections:update', 'Update, reorder and reparent sections'),
  (gen_random_uuid(), 'sections:archive', 'Transition a section into the archived status');

-- Existing built-in Owner roles receive the new grants at organization scope
-- (the step 13/17 security decision): extending the Owner GRANT LIST through
-- migration is allowed; assigning the Owner ROLE to users is NOT. No
-- membership_roles writes happen here.
INSERT INTO role_permissions (role_id, permission_id, scope)
SELECT r.id, p.id, 'organization'
FROM roles r
JOIN permissions p
  ON p.key IN ('sections:create', 'sections:update', 'sections:archive')
WHERE r.is_system AND r.name = 'owner' AND r.deleted_at IS NULL;
