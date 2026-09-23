-- STEP 19 / ADR 0013: non-recursive work directly inside a section.
CREATE TABLE work_items (
  id uuid PRIMARY KEY,
  tenant_id uuid NOT NULL REFERENCES organizations (id),
  workspace_id uuid NOT NULL,
  project_id uuid NOT NULL,
  section_id uuid NOT NULL,
  name text NOT NULL,
  slug citext NOT NULL,
  position integer NOT NULL,
  status text NOT NULL DEFAULT 'active',
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz NULL,
  CONSTRAINT work_items_section_fk
    FOREIGN KEY (tenant_id, workspace_id, project_id, section_id)
    REFERENCES sections (tenant_id, workspace_id, project_id, id),
  -- Hard uniqueness preserves the namespace after archive or soft deletion.
  CONSTRAINT work_items_section_slug_key UNIQUE (section_id, slug),
  CONSTRAINT work_items_name_length CHECK (char_length(btrim(name)) BETWEEN 1 AND 200),
  CONSTRAINT work_items_slug_length CHECK (char_length(slug::text) BETWEEN 1 AND 64),
  CONSTRAINT work_items_position_check CHECK (position >= 0),
  CONSTRAINT work_items_status_check CHECK (status IN ('active', 'completed', 'archived'))
);

-- Ordered section list; also supports max(position) for append.
CREATE INDEX work_items_section_order_idx ON work_items (section_id, position, id);

INSERT INTO permissions (id, key, description) VALUES
  (gen_random_uuid(), 'work_items:create', 'Create work items inside a section'),
  (gen_random_uuid(), 'work_items:update', 'Update work item content and status'),
  (gen_random_uuid(), 'work_items:archive', 'Archive a work item');

-- Extend existing system Owner grants, never assign roles to users.
INSERT INTO role_permissions (role_id, permission_id, scope)
SELECT r.id, p.id, 'organization'
FROM roles r
JOIN permissions p ON p.key IN ('work_items:create', 'work_items:update', 'work_items:archive')
WHERE r.is_system AND r.name = 'owner' AND r.deleted_at IS NULL;
