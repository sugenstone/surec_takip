-- STEP 20 / ADR 0015: ordered process DEFINITIONS inside a work item.
-- Configuration only: no execution/runtime columns (started/finished,
-- timers, assignees, progress). Future execution records reference these
-- rows by id instead of mutating them.

-- Composite FK target: lets processes prove at the DB level that their full
-- (tenant, workspace, project, section) chain matches the work item row.
ALTER TABLE work_items
  ADD CONSTRAINT work_items_scope_id_key
    UNIQUE (tenant_id, workspace_id, project_id, section_id, id);

CREATE TABLE processes (
  id uuid PRIMARY KEY,
  tenant_id uuid NOT NULL REFERENCES organizations (id),
  workspace_id uuid NOT NULL,
  project_id uuid NOT NULL,
  section_id uuid NOT NULL,
  work_item_id uuid NOT NULL,
  name text NOT NULL,
  slug citext NOT NULL,
  description text NULL,
  position integer NOT NULL,
  -- Definition fact only; completion enforcement/progress derive from future
  -- execution records, never from a stored percentage.
  is_required boolean NOT NULL DEFAULT true,
  -- Configuration lifecycle, deliberately NOT an execution state.
  status text NOT NULL DEFAULT 'active',
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  deleted_at timestamptz NULL,
  CONSTRAINT processes_work_item_fk
    FOREIGN KEY (tenant_id, workspace_id, project_id, section_id, work_item_id)
    REFERENCES work_items (tenant_id, workspace_id, project_id, section_id, id),
  -- Hard uniqueness preserves the namespace after archive or soft deletion.
  CONSTRAINT processes_work_item_slug_key UNIQUE (work_item_id, slug),
  CONSTRAINT processes_name_length CHECK (char_length(btrim(name)) BETWEEN 1 AND 200),
  CONSTRAINT processes_slug_length CHECK (char_length(slug::text) BETWEEN 1 AND 64),
  CONSTRAINT processes_description_length CHECK (description IS NULL OR char_length(description) <= 2000),
  CONSTRAINT processes_position_check CHECK (position >= 0),
  CONSTRAINT processes_status_check CHECK (status IN ('active', 'archived'))
);

-- Active definitions have a strict order: two visible processes can never
-- share a position. Archived/deleted rows leave the index and keep their
-- historical position. Reorder writes in two phases to avoid transient
-- conflicts (ADR 0015).
CREATE UNIQUE INDEX processes_active_position_unique
  ON processes (work_item_id, position)
  WHERE deleted_at IS NULL AND status = 'active';

-- Ordered list per work item; also supports max(position) for append.
CREATE INDEX processes_work_item_order_idx ON processes (work_item_id, position, id);

INSERT INTO permissions (id, key, description) VALUES
  (gen_random_uuid(), 'processes:create', 'Create process definitions inside a work item'),
  (gen_random_uuid(), 'processes:update', 'Update process definition content'),
  (gen_random_uuid(), 'processes:archive', 'Archive a process definition'),
  (gen_random_uuid(), 'processes:reorder', 'Reorder process definitions inside a work item');

-- Extend existing system Owner grants, never assign roles to users.
INSERT INTO role_permissions (role_id, permission_id, scope)
SELECT r.id, p.id, 'organization'
FROM roles r
JOIN permissions p
  ON p.key IN ('processes:create', 'processes:update', 'processes:archive', 'processes:reorder')
WHERE r.is_system AND r.name = 'owner' AND r.deleted_at IS NULL;
